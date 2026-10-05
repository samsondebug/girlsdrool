//! Encrypted backups via `sqlcipher_export` (ADR-0012): the copy is re-encrypted under the
//! passphrase given, which may differ from the live one.

use std::path::Path;

use rusqlite::{params, Connection};

use crate::dates::now_rfc3339;
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
    conn.execute(
        "ATTACH DATABASE ?1 AS kept_backup KEY ?2",
        params![dest.to_string_lossy(), passphrase],
    )?;
    let exported = conn.execute_batch("SELECT sqlcipher_export('kept_backup');");
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
