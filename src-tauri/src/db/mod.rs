//! SQLCipher database: open, unlock, lock (ARCHITECTURE §9, ADR-0012, ADR-0013).
//!
//! One connection lives behind a mutex in app state and is `None` while locked. A wrong
//! passphrase fails closed: the key probe fails before anything else is read or written.

pub mod audit;
pub mod migrate;
pub mod repo;
pub mod settings;

use std::path::Path;

use rusqlite::{Connection, ErrorCode, OpenFlags};
use zeroize::Zeroizing;

use crate::config::DataPaths;
use crate::error::{AppError, AppResult};

pub struct Db {
    conn: Connection,
    passphrase: Zeroizing<String>,
    paths: DataPaths,
}

/// Never prints the passphrase: only the file the connection is bound to.
impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db")
            .field("path", &self.paths.db)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// Create `kept.db`; refuse if one already exists.
    CreateNew,
    /// Open an existing `kept.db`; refuse if none exists.
    Existing,
}

impl Db {
    /// Open (or create) the encrypted database, verify the key, apply pragmas, run pending
    /// migrations, and quick-check the file.
    pub fn open(paths: &DataPaths, passphrase: &str, mode: OpenMode) -> AppResult<Db> {
        validate_passphrase(passphrase)?;
        match mode {
            OpenMode::CreateNew if paths.db_exists() => {
                return Err(AppError::Conflict(format!(
                    "a database already exists at {}",
                    paths.db.display()
                )));
            }
            OpenMode::Existing if !paths.db_exists() => {
                return Err(AppError::validation(
                    "database",
                    format!("no database at {}", paths.db.display()),
                ));
            }
            _ => {}
        }
        paths.ensure_dirs()?;
        let mut conn = open_keyed(&paths.db, passphrase, mode == OpenMode::CreateNew)?;
        apply_pragmas(&conn)?;
        let report = migrate::run(&mut conn, paths, passphrase)?;
        if !report.applied.is_empty() {
            tracing::info!(
                from = report.from_version,
                to = report.to_version,
                backup = report.backup.is_some(),
                "schema migrated"
            );
        }
        quick_check(&conn)?;
        Ok(Db {
            conn,
            passphrase: Zeroizing::new(passphrase.to_owned()),
            paths: paths.clone(),
        })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn passphrase(&self) -> &str {
        &self.passphrase
    }

    pub fn paths(&self) -> &DataPaths {
        &self.paths
    }

    /// Change the passphrase in place with `PRAGMA rekey` (ADR-0012). The caller takes the
    /// fresh backup first; the write-ahead log is checkpointed so every page is rewritten.
    pub fn rekey(&mut self, new_passphrase: &str) -> AppResult<()> {
        validate_passphrase(new_passphrase)?;
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        self.conn.pragma_update(None, "rekey", new_passphrase)?;
        self.passphrase = Zeroizing::new(new_passphrase.to_owned());
        Ok(())
    }
}

/// Open a SQLCipher file with the given passphrase and prove the key by reading the schema.
/// Any other key leaves the file unreadable (`SQLITE_NOTADB`) and is reported as
/// `WrongPassphrase`.
pub fn open_keyed(path: &Path, passphrase: &str, create: bool) -> AppResult<Connection> {
    if !create && !path.is_file() {
        return Err(AppError::validation(
            "database",
            format!("no database at {}", path.display()),
        ));
    }
    // CREATE stays on even for an existing file: an ATTACHed database (a backup, the restore
    // staging copy) inherits the connection's flags and must be creatable. Existence of the
    // main file is decided above, not by SQLite.
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    conn.pragma_update(None, "key", passphrase)?;
    probe_key(&conn)?;
    Ok(conn)
}

/// Read-only open with a key, for inspecting backups. Never migrates.
pub fn open_keyed_readonly(path: &Path, passphrase: &str) -> AppResult<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "key", passphrase)?;
    probe_key(&conn)?;
    Ok(conn)
}

fn probe_key(conn: &Connection) -> AppResult<()> {
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::NotADatabase => {
            Err(AppError::WrongPassphrase)
        }
        Err(e) => Err(e.into()),
    }
}

/// A passphrase is always a passphrase: SQLCipher would read `x'…'` as a raw hex key.
pub fn validate_passphrase(passphrase: &str) -> AppResult<()> {
    if passphrase.is_empty() {
        return Err(AppError::validation(
            "passphrase",
            "a passphrase is required",
        ));
    }
    let t = passphrase.trim();
    let looks_like_raw_key =
        (t.starts_with("x'") || t.starts_with("X'")) && t.ends_with('\'') && t.len() > 3;
    if looks_like_raw_key {
        return Err(AppError::validation(
            "passphrase",
            "a passphrase may not have the form x'…' (SQLCipher raw-key syntax)",
        ));
    }
    Ok(())
}

pub fn apply_pragmas(conn: &Connection) -> AppResult<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    Ok(())
}

pub fn quick_check(conn: &Connection) -> AppResult<()> {
    let result: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(AppError::Internal(format!(
            "database quick_check failed: {result}"
        )))
    }
}

/// Full `PRAGMA integrity_check`; used by tests and maintenance, not on every open.
pub fn integrity_check(conn: &Connection) -> AppResult<()> {
    let result: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(AppError::Internal(format!(
            "database integrity_check failed: {result}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrase_rules() {
        assert!(validate_passphrase("").is_err());
        assert!(validate_passphrase("x'00ff'").is_err());
        assert!(validate_passphrase("X'00FF'").is_err());
        assert!(validate_passphrase("x'").is_ok());
        assert!(validate_passphrase("correct horse battery staple").is_ok());
        assert!(validate_passphrase("x'quoted' but longer phrase").is_ok());
    }
}
