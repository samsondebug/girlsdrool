//! Versioned SQL migrations (ARCHITECTURE §8, ADR-0011): embedded files, one transaction each,
//! checksums verified on every open, forward-only, pre-migration backup.

use std::path::PathBuf;

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::config::DataPaths;
use crate::dates::now_rfc3339;
use crate::error::{AppError, AppResult};
use crate::export::backup;

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

/// Every migration this build knows, in order. A committed file is never edited: a mistake is
/// fixed by the next migration.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("../../migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "institution_profiles",
        sql: include_str!("../../migrations/0002_institution_profiles.sql"),
    },
    Migration {
        version: 3,
        name: "category_seeds",
        sql: include_str!("../../migrations/0003_category_seeds.sql"),
    },
    Migration {
        version: 4,
        name: "debt_payment_account",
        sql: include_str!("../../migrations/0004_debt_payment_account.sql"),
    },
    Migration {
        version: 5,
        name: "ofx_profile",
        sql: include_str!("../../migrations/0005_ofx_profile.sql"),
    },
];

pub fn latest_version() -> i64 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

/// SHA-256 of the migration text with line endings normalised, so a Windows checkout and a
/// Linux checkout of the same file agree.
pub fn checksum(sql: &str) -> String {
    hex::encode(Sha256::digest(sql.replace("\r\n", "\n").as_bytes()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub from_version: i64,
    pub to_version: i64,
    pub applied: Vec<i64>,
    pub backup: Option<PathBuf>,
}

pub fn current_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// The newest migration `schema_migration` records, when the table exists: the version a copy
/// made without `user_version` (a backup taken by an earlier build) really is.
fn recorded_version(conn: &Connection) -> AppResult<Option<i64>> {
    let has_table: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migration'",
        [],
        |r| r.get(0),
    )?;
    if has_table == 0 {
        return Ok(None);
    }
    Ok(
        conn.query_row("SELECT MAX(version) FROM schema_migration", [], |r| {
            r.get::<_, Option<i64>>(0)
        })?,
    )
}

/// Verify checksums of applied migrations, back up before changing anything, apply pending
/// migrations in order, and check foreign keys afterwards.
pub fn run(
    conn: &mut Connection,
    paths: &DataPaths,
    passphrase: &str,
) -> AppResult<MigrationReport> {
    let mut current = current_version(conn)?;
    if current == 0 {
        if let Some(recorded) = recorded_version(conn)? {
            conn.pragma_update(None, "user_version", recorded)?;
            current = recorded;
            tracing::info!(
                version = recorded,
                "user_version restored from schema_migration"
            );
        }
    }
    let latest = latest_version();
    if current > latest {
        return Err(AppError::Migration(format!(
            "database schema is v{current} but this build knows up to v{latest}; update Kept"
        )));
    }
    verify_applied(conn, current)?;

    let pending: Vec<&Migration> = MIGRATIONS.iter().filter(|m| m.version > current).collect();
    let mut backup_path = None;
    let mut verified = false;
    if !pending.is_empty() && current > 0 {
        let stamp = now_rfc3339().replace([':', '-'], "");
        let dest = paths.backups.join(format!("kept-pre-v{latest}-{stamp}.db"));
        backup::export_encrypted(conn, &dest, passphrase)?;
        verified = backup::verify(&dest, passphrase, &backup::table_counts(conn)?)?;
        backup_path = Some(dest);
    }

    let applied = apply_pending(conn)?;

    if let Some(dest) = &backup_path {
        backup::log_backup(conn, dest, backup::BackupKind::PreMigration, verified)?;
    }

    foreign_key_check(conn)?;
    Ok(MigrationReport {
        from_version: current,
        to_version: latest,
        applied,
        backup: backup_path,
    })
}

fn verify_applied(conn: &Connection, current: i64) -> AppResult<()> {
    if current == 0 {
        return Ok(());
    }
    let mut stmt =
        conn.prepare("SELECT version, name, sha256 FROM schema_migration ORDER BY version")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for expected in MIGRATIONS.iter().filter(|m| m.version <= current) {
        let Some((_, name, stored)) = rows.iter().find(|(v, _, _)| *v == expected.version) else {
            return Err(AppError::Migration(format!(
                "schema is v{current} but migration v{} ({}) is not recorded as applied",
                expected.version, expected.name
            )));
        };
        if *stored != checksum(expected.sql) {
            return Err(AppError::Migration(format!(
                "migration v{} ({name}) differs from the one this database was built with; \
                 refusing to open",
                expected.version
            )));
        }
    }
    for (version, name, _) in &rows {
        if !MIGRATIONS.iter().any(|m| m.version == *version) {
            return Err(AppError::Migration(format!(
                "database records migration v{version} ({name}) unknown to this build"
            )));
        }
    }
    Ok(())
}

/// Apply every migration newer than the database's version, without the backup step (used by
/// `run` after it has taken the backup, and by tests on fresh in-memory databases).
pub fn apply_pending(conn: &mut Connection) -> AppResult<Vec<i64>> {
    let current = current_version(conn)?;
    let mut applied = Vec::new();
    for m in MIGRATIONS.iter().filter(|m| m.version > current) {
        apply(conn, m)?;
        applied.push(m.version);
    }
    Ok(applied)
}

fn apply(conn: &mut Connection, m: &Migration) -> AppResult<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(m.sql)
        .map_err(|e| AppError::Migration(format!("v{} ({}): {e}", m.version, m.name)))?;
    tx.execute(
        "INSERT INTO schema_migration (version, name, sha256, applied_at) VALUES (?1, ?2, ?3, ?4)",
        params![m.version, m.name, checksum(m.sql), now_rfc3339()],
    )?;
    tx.pragma_update(None, "user_version", m.version)?;
    tx.commit()?;
    tracing::info!(version = m.version, name = m.name, "migration applied");
    Ok(())
}

pub fn foreign_key_check(conn: &Connection) -> AppResult<()> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let violations = stmt.query_map([], |r| r.get::<_, String>(0))?.count();
    if violations == 0 {
        Ok(())
    } else {
        Err(AppError::Migration(format!(
            "{violations} foreign key violation(s) after migration"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_strictly_increasing_from_one() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(m.version, i as i64 + 1, "migration {} out of order", m.name);
        }
    }

    #[test]
    fn checksum_ignores_line_ending_style() {
        assert_eq!(checksum("a\r\nb\r\n"), checksum("a\nb\n"));
        assert_ne!(checksum("a\nb\n"), checksum("a\nc\n"));
    }

    #[test]
    fn migrations_do_not_manage_their_own_transactions() {
        for m in MIGRATIONS {
            let code: String = m
                .sql
                .lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
                .to_ascii_uppercase();
            assert!(!code.contains("BEGIN;"), "{} opens a transaction", m.name);
            assert!(
                !code.contains("COMMIT;"),
                "{} commits a transaction",
                m.name
            );
        }
    }
}
