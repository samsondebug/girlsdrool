//! Encrypted backups via `sqlcipher_export` (ADR-0012, ADR-0046): the copy is re-encrypted
//! under the passphrase given, which may differ from the live one. Daily copies rotate on
//! unlock; manual, pre-migration and pre-restore copies are kept until the person removes them.
//! Every copy this module writes is verified (row counts per table) before it is logged.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::config::DataPaths;
use crate::dates::{format_civil, now_rfc3339, parse_civil, CivilDate};
use crate::db::open_keyed_readonly;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    Daily,
    Manual,
    PreMigration,
    PreRestore,
}

impl BackupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackupKind::Daily => "daily",
            BackupKind::Manual => "manual",
            BackupKind::PreMigration => "pre_migration",
            BackupKind::PreRestore => "pre_restore",
        }
    }
}

/// Write a complete encrypted copy of the open database to `dest`, keyed with `passphrase`.
/// Refuses to overwrite.
pub fn export_encrypted(conn: &Connection, dest: &Path, passphrase: &str) -> AppResult<()> {
    if dest.exists() {
        return Err(AppError::Conflict(format!(
            "backup target already exists: {}",
            dest.display()
        )));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
    }
    // `sqlcipher_export` copies schema and rows but not `user_version`, which the migration
    // runner reads first; the copy carries it explicitly so it opens as a database, not as a
    // file that looks unmigrated.
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    conn.execute(
        "ATTACH DATABASE ?1 AS kept_backup KEY ?2",
        params![dest.to_string_lossy(), passphrase],
    )?;
    let exported = conn.execute_batch(&format!(
        "SELECT sqlcipher_export('kept_backup'); PRAGMA kept_backup.user_version = {version};"
    ));
    let detached = conn.execute_batch("DETACH DATABASE kept_backup;");
    if let Err(e) = exported {
        let _ = std::fs::remove_file(dest);
        return Err(e.into());
    }
    detached?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TableCount {
    pub table: String,
    pub rows: i64,
}

/// Row counts for every user table, ordered by name. Used to verify a backup or restore.
pub fn table_counts(conn: &Connection) -> AppResult<Vec<TableCount>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(names.len());
    for table in names {
        // table names come from sqlite_master, not from user input
        let rows: i64 = conn.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| {
            r.get(0)
        })?;
        out.push(TableCount { table, rows });
    }
    Ok(out)
}

/// Open a backup read-only with its passphrase and count its rows per table.
pub fn inspect(path: &Path, passphrase: &str) -> AppResult<Vec<TableCount>> {
    let conn = open_keyed_readonly(path, passphrase)?;
    table_counts(&conn)
}

/// Record a backup in `backup_log` (requires schema v1 or later).
pub fn log_backup(
    conn: &Connection,
    path: &Path,
    kind: BackupKind,
    verified: bool,
) -> AppResult<()> {
    let bytes = std::fs::metadata(path)
        .map(|m| i64::try_from(m.len()).unwrap_or(i64::MAX))
        .map_err(|e| AppError::io(path, e))?;
    conn.execute(
        "INSERT INTO backup_log (path, kind, bytes, verified, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            path.to_string_lossy(),
            kind.as_str(),
            bytes,
            i64::from(verified),
            now_rfc3339()
        ],
    )?;
    Ok(())
}

/// `backups/kept-YYYY-MM-DD.db`: one per civil day.
pub fn daily_path(paths: &DataPaths, today: CivilDate) -> PathBuf {
    paths
        .backups
        .join(format!("kept-{}.db", format_civil(today)))
}

/// Only files named exactly like a daily copy take part in rotation; everything else in the
/// folder (manual, pre-migration, pre-restore copies, the person's own files) is left alone.
fn is_daily_file(name: &str) -> bool {
    name.len() == "kept-YYYY-MM-DD.db".len()
        && name.starts_with("kept-")
        && name.ends_with(".db")
        && parse_civil(&name[5..15]).is_ok()
}

/// A copy opens with its passphrase and holds the same row counts as the live database.
pub fn verify(path: &Path, passphrase: &str, live: &[TableCount]) -> AppResult<bool> {
    Ok(inspect(path, passphrase)? == live)
}

/// The daily copy for `today`, unless one exists: export, verify, log, then drop the oldest
/// daily copies beyond `keep`. Returns the path written.
pub fn daily(
    conn: &Connection,
    paths: &DataPaths,
    passphrase: &str,
    today: CivilDate,
    keep: i64,
) -> AppResult<Option<PathBuf>> {
    let dest = daily_path(paths, today);
    if dest.exists() {
        return Ok(None);
    }
    export_encrypted(conn, &dest, passphrase)?;
    let verified = verify(&dest, passphrase, &table_counts(conn)?)?;
    log_backup(conn, &dest, BackupKind::Daily, verified)?;
    rotate(paths, keep)?;
    Ok(Some(dest))
}

/// Remove the oldest daily copies so that at most `keep` remain. Returns what was removed.
pub fn rotate(paths: &DataPaths, keep: i64) -> AppResult<Vec<PathBuf>> {
    let keep = usize::try_from(keep).unwrap_or(1).max(1);
    let entries = std::fs::read_dir(&paths.backups).map_err(|e| AppError::io(&paths.backups, e))?;
    let mut daily: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| AppError::io(&paths.backups, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_daily_file(&name) && entry.path().is_file() {
            daily.push((name, entry.path()));
        }
    }
    daily.sort();
    let mut removed = Vec::new();
    while daily.len() > keep {
        let (_, path) = daily.remove(0);
        std::fs::remove_file(&path).map_err(|e| AppError::io(&path, e))?;
        removed.push(path);
    }
    Ok(removed)
}

/// A backup row as the Settings screen lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BackupEntry {
    pub id: i64,
    pub path: String,
    pub kind: String,
    pub bytes: i64,
    pub verified: bool,
    pub created_at: String,
    /// Whether the file is still where the log says (rotation and the person remove files).
    pub exists: bool,
}

/// Every logged backup, newest first.
pub fn list_log(conn: &Connection) -> AppResult<Vec<BackupEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, kind, bytes, verified, created_at FROM backup_log ORDER BY id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(BackupEntry {
            id: r.get(0)?,
            path: r.get(1)?,
            kind: r.get(2)?,
            bytes: r.get(3)?,
            verified: r.get::<_, i64>(4)? == 1,
            created_at: r.get(5)?,
            exists: false,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        let mut entry = row?;
        entry.exists = Path::new(&entry.path).is_file();
        out.push(entry);
    }
    Ok(out)
}

/// A manual copy under the live passphrase: `backups/kept-manual-<stamp>.db`, verified and
/// logged. Returns its log entry.
pub fn manual(
    conn: &Connection,
    paths: &DataPaths,
    passphrase: &str,
    stamp: &str,
) -> AppResult<BackupEntry> {
    let dest = paths.backups.join(format!("kept-manual-{stamp}.db"));
    export_encrypted(conn, &dest, passphrase)?;
    let verified = verify(&dest, passphrase, &table_counts(conn)?)?;
    log_backup(conn, &dest, BackupKind::Manual, verified)?;
    list_log(conn)?
        .into_iter()
        .find(|e| Path::new(&e.path) == dest)
        .ok_or_else(|| AppError::Internal("the backup was written but not logged".into()))
}

/// A file-name stamp for manual and pre-restore copies: the UTC instant without separators.
pub fn stamp() -> String {
    now_rfc3339().replace([':', '-'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_daily_names_rotate() {
        assert!(is_daily_file("kept-2026-10-05.db"));
        assert!(!is_daily_file("kept-manual-20261005T081545Z.db"));
        assert!(!is_daily_file("kept-pre-v5-20261005T081545Z.db"));
        assert!(!is_daily_file("kept-2026-13-05.db"));
        assert!(!is_daily_file("kept.db"));
    }
}
