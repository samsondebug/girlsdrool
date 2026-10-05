//! Import batches and the quarantine (ARCHITECTURE §6.2).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::now_rfc3339;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportBatch {
    pub id: i64,
    pub command_id: i64,
    pub file_sha256: String,
    pub file_name: String,
    pub account_id: i64,
    pub profile_id: i64,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub rows_read: i64,
    pub inserted: i64,
    pub updated: i64,
    pub skipped: i64,
    pub quarantined: i64,
    pub dedup_report_json: String,
    pub created_at: String,
    pub undone_at: Option<String>,
}

const COLS: &str = "id, command_id, file_sha256, file_name, account_id, profile_id, date_from, date_to, rows_read, inserted, updated, skipped, quarantined, dedup_report_json, created_at, undone_at";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<ImportBatch> {
    Ok(ImportBatch {
        id: r.get(0)?,
        command_id: r.get(1)?,
        file_sha256: r.get(2)?,
        file_name: r.get(3)?,
        account_id: r.get(4)?,
        profile_id: r.get(5)?,
        date_from: r.get(6)?,
        date_to: r.get(7)?,
        rows_read: r.get(8)?,
        inserted: r.get(9)?,
        updated: r.get(10)?,
        skipped: r.get(11)?,
        quarantined: r.get(12)?,
        dedup_report_json: r.get(13)?,
        created_at: r.get(14)?,
        undone_at: r.get(15)?,
    })
}

pub fn get(conn: &Connection, id: i64) -> AppResult<ImportBatch> {
    conn.query_row(
        &format!("SELECT {COLS} FROM import_batch WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "import_batch",
        id,
    })
}

pub fn list(conn: &Connection, limit: usize) -> AppResult<Vec<ImportBatch>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM import_batch ORDER BY id DESC LIMIT ?1"
    ))?;
    let limit = i64::try_from(limit).map_err(|_| AppError::Overflow)?;
    let rows = stmt
        .query_map([limit], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// An earlier, not-undone batch of the same file for the same account and profile.
pub fn find_same_file(
    conn: &Connection,
    account_id: i64,
    profile_id: i64,
    sha256: &str,
) -> AppResult<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM import_batch WHERE account_id = ?1 AND profile_id = ?2 AND file_sha256 = ?3 AND undone_at IS NULL
             ORDER BY id LIMIT 1",
            params![account_id, profile_id, sha256],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn insert(
    conn: &Connection,
    command_id: i64,
    sha256: &str,
    file_name: &str,
    account_id: i64,
    profile_id: i64,
    rows_read: usize,
) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO import_batch (command_id, file_sha256, file_name, account_id, profile_id, rows_read, inserted, updated, skipped, quarantined, dedup_report_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, 0, 0, '{}', ?7)",
        params![command_id, sha256, file_name, account_id, profile_id, i64::try_from(rows_read).map_err(|_| AppError::Overflow)?, now_rfc3339()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Counts written onto a batch when it finishes.
#[derive(Debug, Clone, Copy, Default)]
pub struct BatchTotals<'a> {
    pub date_from: Option<&'a str>,
    pub date_to: Option<&'a str>,
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    pub quarantined: usize,
}

pub fn finish(
    conn: &Connection,
    id: i64,
    totals: &BatchTotals,
    report_json: &str,
) -> AppResult<()> {
    let n = |v: usize| i64::try_from(v).map_err(|_| AppError::Overflow);
    conn.execute(
        "UPDATE import_batch SET date_from = ?1, date_to = ?2, inserted = ?3, updated = ?4, skipped = ?5, quarantined = ?6, dedup_report_json = ?7 WHERE id = ?8",
        params![
            totals.date_from,
            totals.date_to,
            n(totals.inserted)?,
            n(totals.updated)?,
            n(totals.skipped)?,
            n(totals.quarantined)?,
            report_json,
            id
        ],
    )?;
    Ok(())
}

pub fn mark_undone(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE import_batch SET undone_at = ?1 WHERE id = ?2",
        params![now_rfc3339(), id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuarantineRow {
    pub id: i64,
    pub import_batch_id: i64,
    pub account_id: i64,
    pub row_json: String,
    pub source_row_hash: String,
    pub suspected_txn_id: Option<i64>,
    pub similarity_bps: i64,
    pub reason: String,
    pub resolution: String,
    pub resolved_txn_id: Option<i64>,
    pub resolved_at: Option<String>,
}

const QCOLS: &str = "id, import_batch_id, account_id, row_json, source_row_hash, suspected_txn_id, similarity_bps, reason, resolution, resolved_txn_id, resolved_at";

fn q_from_row(r: &rusqlite::Row) -> rusqlite::Result<QuarantineRow> {
    Ok(QuarantineRow {
        id: r.get(0)?,
        import_batch_id: r.get(1)?,
        account_id: r.get(2)?,
        row_json: r.get(3)?,
        source_row_hash: r.get(4)?,
        suspected_txn_id: r.get(5)?,
        similarity_bps: r.get(6)?,
        reason: r.get(7)?,
        resolution: r.get(8)?,
        resolved_txn_id: r.get(9)?,
        resolved_at: r.get(10)?,
    })
}

pub fn quarantine_get(conn: &Connection, id: i64) -> AppResult<QuarantineRow> {
    conn.query_row(
        &format!("SELECT {QCOLS} FROM import_quarantine WHERE id = ?1"),
        [id],
        q_from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "import_quarantine",
        id,
    })
}

pub fn quarantine_pending(conn: &Connection) -> AppResult<Vec<QuarantineRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {QCOLS} FROM import_quarantine WHERE resolution = 'pending' ORDER BY id"
    ))?;
    let rows = stmt
        .query_map([], q_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Rows still held for review on one account (the difference explorer lists them).
pub fn quarantine_pending_for_account(
    conn: &Connection,
    account_id: i64,
) -> AppResult<Vec<QuarantineRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {QCOLS} FROM import_quarantine WHERE account_id = ?1 AND resolution = 'pending' ORDER BY id"
    ))?;
    let rows = stmt
        .query_map([account_id], q_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn quarantine_for_batch(conn: &Connection, batch_id: i64) -> AppResult<Vec<QuarantineRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {QCOLS} FROM import_quarantine WHERE import_batch_id = ?1 ORDER BY id"
    ))?;
    let rows = stmt
        .query_map([batch_id], q_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn quarantine_pending_by_hash(
    conn: &Connection,
    account_id: i64,
    hash: &str,
) -> AppResult<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM import_quarantine WHERE account_id = ?1 AND source_row_hash = ?2 AND resolution = 'pending'",
            params![account_id, hash],
            |r| r.get(0),
        )
        .optional()?)
}

/// A row held for review.
#[derive(Debug, Clone, Copy)]
pub struct NewQuarantine<'a> {
    pub batch_id: i64,
    pub account_id: i64,
    pub row_json: &'a str,
    pub hash: &'a str,
    pub suspected_txn_id: i64,
    pub similarity_bps: i64,
    pub reason: &'a str,
}

pub fn quarantine_insert(conn: &Connection, new: &NewQuarantine) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO import_quarantine (import_batch_id, account_id, row_json, source_row_hash, suspected_txn_id, similarity_bps, reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            new.batch_id,
            new.account_id,
            new.row_json,
            new.hash,
            new.suspected_txn_id,
            new.similarity_bps,
            new.reason
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn quarantine_resolve(
    conn: &Connection,
    id: i64,
    resolution: &str,
    resolved_txn_id: Option<i64>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE import_quarantine SET resolution = ?1, resolved_txn_id = ?2, resolved_at = ?3 WHERE id = ?4",
        params![resolution, resolved_txn_id, now_rfc3339(), id],
    )?;
    Ok(())
}
