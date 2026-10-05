//! Restore from an encrypted backup (ARCHITECTURE §6.6, ADR-0012, ADR-0046): the backup is
//! opened with its own passphrase, exported into a staging folder inside the data folder under
//! the live passphrase, migrated there, and compared with the live database (row counts per
//! table and the hero as of today). Only an explicit confirmation swaps the files, and a
//! verified pre-restore copy of the live database is taken first.

use std::path::{Path, PathBuf};

use serde::Serialize;
use zeroize::Zeroizing;

use crate::cash::safe;
use crate::config::DataPaths;
use crate::dates::{format_civil, CivilDate};
use crate::db::{migrate, open_keyed, Db, OpenMode};
use crate::error::{AppError, AppResult};
use crate::export::backup::{self, BackupKind, TableCount};

pub const STAGING_DIR: &str = "restore-staging";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableDiff {
    pub table: String,
    pub live_rows: i64,
    pub backup_rows: i64,
}

/// What the person sees before confirming: every table side by side and the hero number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreComparison {
    pub backup_path: String,
    pub staged_dir: String,
    pub as_of: String,
    pub tables: Vec<TableDiff>,
    pub tables_differ: usize,
    pub hero_live_cents: i64,
    pub hero_backup_cents: i64,
    pub hero_same: bool,
    /// The backup's schema version as found and after migration in the staging folder.
    pub schema_before: i64,
    pub schema_after: i64,
}

pub fn staging_paths(paths: &DataPaths) -> DataPaths {
    DataPaths::new(paths.root.join(STAGING_DIR))
}

/// Remove the staging folder if one is there (a cancelled or abandoned restore).
pub fn discard(paths: &DataPaths) -> AppResult<()> {
    let dir = paths.root.join(STAGING_DIR);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| AppError::io(&dir, e))?;
    }
    Ok(())
}

pub fn is_staged(paths: &DataPaths) -> bool {
    staging_paths(paths).db_exists()
}

fn side_by_side(live: &[TableCount], copy: &[TableCount]) -> Vec<TableDiff> {
    let mut names: Vec<&str> = live
        .iter()
        .chain(copy.iter())
        .map(|t| t.table.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    let rows =
        |set: &[TableCount], name: &str| set.iter().find(|t| t.table == name).map_or(0, |t| t.rows);
    names
        .into_iter()
        .map(|name| TableDiff {
            table: name.to_string(),
            live_rows: rows(live, name),
            backup_rows: rows(copy, name),
        })
        .collect()
}

/// Open `backup` with its passphrase, stage a copy under the live passphrase, migrate it and
/// compare it with the live database. Nothing about the live database changes.
pub fn stage(
    live: &Db,
    backup_file: &Path,
    backup_passphrase: &str,
    today: CivilDate,
) -> AppResult<RestoreComparison> {
    if !backup_file.is_file() {
        return Err(AppError::validation(
            "backup",
            format!("no file at {}", backup_file.display()),
        ));
    }
    // Opened read-write (never created): an attached database inherits the connection's
    // flags, and the staging copy is written through an ATTACH. Nothing writes to the backup.
    let source = open_keyed(backup_file, backup_passphrase, false)?;
    let schema_before = migrate::current_version(&source)?;
    if schema_before > migrate::latest_version() {
        return Err(AppError::Migration(format!(
            "the backup is schema v{schema_before}; this build knows up to v{}",
            migrate::latest_version()
        )));
    }
    let paths = live.paths();
    discard(paths)?;
    let staged = staging_paths(paths);
    staged.ensure_dirs()?;
    backup::export_encrypted(&source, &staged.db, live.passphrase())?;
    drop(source);
    let copy = Db::open(&staged, live.passphrase(), OpenMode::Existing)?;
    let schema_after = migrate::current_version(copy.conn())?;
    let live_counts = backup::table_counts(live.conn())?;
    let copy_counts = backup::table_counts(copy.conn())?;
    let tables = side_by_side(&live_counts, &copy_counts);
    let tables_differ = tables
        .iter()
        .filter(|t| t.live_rows != t.backup_rows)
        .count();
    let hero_live_cents = safe::safe_to_spend(live.conn(), today)?.safe_cents;
    let hero_backup_cents = safe::safe_to_spend(copy.conn(), today)?.safe_cents;
    drop(copy);
    Ok(RestoreComparison {
        backup_path: backup_file.display().to_string(),
        staged_dir: staged.root.display().to_string(),
        as_of: format_civil(today),
        tables,
        tables_differ,
        hero_live_cents,
        hero_backup_cents,
        hero_same: hero_live_cents == hero_backup_cents,
        schema_before,
        schema_after,
    })
}

fn sidecar(db: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", db.display()))
}

/// Before a swap: the verified pre-restore copy of the live database, `backups/kept-pre-restore-<stamp>.db`.
/// Nothing else changes, so a failure here leaves the live database open and untouched.
pub fn prepare(live: &Db, stamp: &str) -> AppResult<PathBuf> {
    let paths = live.paths();
    let staged = staging_paths(paths);
    if !staged.db_exists() {
        return Err(AppError::Conflict(
            "nothing is staged: open a backup and compare it first".into(),
        ));
    }
    let staged_wal = sidecar(&staged.db, "-wal");
    if staged_wal.is_file()
        && std::fs::metadata(&staged_wal)
            .map(|m| m.len() > 0)
            .unwrap_or(true)
    {
        return Err(AppError::Conflict(
            "the staged copy still has a write-ahead log; stage it again".into(),
        ));
    }
    let dest = paths.backups.join(format!("kept-pre-restore-{stamp}.db"));
    backup::export_encrypted(live.conn(), &dest, live.passphrase())?;
    let live_counts = backup::table_counts(live.conn())?;
    if !backup::verify(&dest, live.passphrase(), &live_counts)? {
        let _ = std::fs::remove_file(&dest);
        return Err(AppError::Internal(
            "the pre-restore copy did not verify; nothing was changed".into(),
        ));
    }
    Ok(dest)
}

/// Replace the live database with the staged copy after `prepare`. The live database is
/// closed, the files are swapped, the result is reopened under the live passphrase and logs
/// the pre-restore copy. If the restored file does not open, the pre-restore copy is put back
/// so the data folder is never left without a readable database.
pub fn swap(live: Db, pre_restore: &Path) -> AppResult<Db> {
    let paths = live.paths().clone();
    let passphrase = Zeroizing::new(live.passphrase().to_owned());
    let staged = staging_paths(&paths);
    if !staged.db_exists() {
        return Err(AppError::Conflict(
            "nothing is staged: open a backup and compare it first".into(),
        ));
    }
    if !pre_restore.is_file() {
        return Err(AppError::Conflict(format!(
            "the pre-restore copy {} is missing; prepare again",
            pre_restore.display()
        )));
    }
    live.conn()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(live);
    for suffix in ["-wal", "-shm"] {
        let side = sidecar(&paths.db, suffix);
        if side.exists() {
            std::fs::remove_file(&side).map_err(|e| AppError::io(&side, e))?;
        }
    }
    std::fs::rename(&staged.db, &paths.db).map_err(|e| AppError::io(&paths.db, e))?;
    let reopened = match Db::open(&paths, &passphrase, OpenMode::Existing) {
        Ok(db) => db,
        Err(e) => {
            let put_back = std::fs::copy(pre_restore, &paths.db);
            return Err(AppError::Internal(format!(
                "the restored database did not open ({e}); the pre-restore copy at {} {}",
                pre_restore.display(),
                match put_back {
                    Ok(_) => "was put back",
                    Err(_) => "could not be put back: copy it over kept.db by hand",
                }
            )));
        }
    };
    backup::log_backup(reopened.conn(), pre_restore, BackupKind::PreRestore, true)?;
    discard(&paths)?;
    tracing::info!(backup = %pre_restore.display(), "database restored from backup");
    Ok(reopened)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_by_side_lists_every_table_once_with_missing_as_zero() {
        let live = vec![
            TableCount {
                table: "account".into(),
                rows: 7,
            },
            TableCount {
                table: "txn".into(),
                rows: 104,
            },
        ];
        let copy = vec![
            TableCount {
                table: "txn".into(),
                rows: 100,
            },
            TableCount {
                table: "zzz_new".into(),
                rows: 1,
            },
        ];
        let diff = side_by_side(&live, &copy);
        assert_eq!(diff.len(), 3);
        assert_eq!(diff[0].table, "account");
        assert_eq!((diff[0].live_rows, diff[0].backup_rows), (7, 0));
        assert_eq!((diff[1].live_rows, diff[1].backup_rows), (104, 100));
        assert_eq!((diff[2].live_rows, diff[2].backup_rows), (0, 1));
    }
}
