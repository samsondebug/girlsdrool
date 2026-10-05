//! Ledger rows: insert, system-field updates (imports), user edits, splits, tags. Every write
//! records an audit row with the full before/after row.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::{now_rfc3339, parse_civil};
use crate::db::audit::{self, Action, CommandRecord};
use crate::error::{AppError, AppResult};
use crate::import::csv::{flag_bit, RowStatus, FLAG_NEEDS_REVIEW, FLAG_PAYMENT_APP_UNKNOWN};
use crate::import::normalize::payee_norm;
use crate::money;

// user_edited bits (ARCHITECTURE §3.5)
pub const UE_PAYEE_NORM: i64 = 1;
pub const UE_MEMO: i64 = 2;
pub const UE_CATEGORY: i64 = 4;
pub const UE_TAGS: i64 = 8;
pub const UE_VENTURE: i64 = 16;
pub const UE_FLAGS: i64 = 32;
pub const UE_EFFECTIVE_DATE: i64 = 64;
pub const UE_STATUS: i64 = 128;
pub const UE_AMOUNT: i64 = 256;
pub const UE_POSTED_DATE: i64 = 512;
pub const UE_ACCOUNT: i64 = 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxnRecord {
    pub id: i64,
    pub account_id: i64,
    pub parent_id: Option<i64>,
    pub posted_date: String,
    pub effective_date: String,
    pub amount_cents: i64,
    pub payee_raw: String,
    pub payee_norm: String,
    pub memo: String,
    pub category_id: Option<i64>,
    pub import_batch_id: Option<i64>,
    pub source_row_hash: Option<String>,
    pub external_id: Option<String>,
    pub classification: String,
    pub rule_id: Option<i64>,
    pub heuristic_code: Option<String>,
    pub user_edited: i64,
    pub status: String,
    pub transfer_link_id: Option<i64>,
    pub refund_link_id: Option<i64>,
    pub venture_id: Option<i64>,
    pub flags: i64,
    pub created_at: String,
    pub updated_at: String,
    /// Tag names, kept in the audit JSON so undo can restore them.
    #[serde(default)]
    pub tags: Vec<String>,
}

pub const COLS: &str = "id, account_id, parent_id, posted_date, effective_date, amount_cents, payee_raw, payee_norm, memo, category_id, import_batch_id, source_row_hash, external_id, classification, rule_id, heuristic_code, user_edited, status, transfer_link_id, refund_link_id, venture_id, flags, created_at, updated_at";

pub fn from_row(r: &rusqlite::Row) -> rusqlite::Result<TxnRecord> {
    Ok(TxnRecord {
        id: r.get(0)?,
        account_id: r.get(1)?,
        parent_id: r.get(2)?,
        posted_date: r.get(3)?,
        effective_date: r.get(4)?,
        amount_cents: r.get(5)?,
        payee_raw: r.get(6)?,
        payee_norm: r.get(7)?,
        memo: r.get(8)?,
        category_id: r.get(9)?,
        import_batch_id: r.get(10)?,
        source_row_hash: r.get(11)?,
        external_id: r.get(12)?,
        classification: r.get(13)?,
        rule_id: r.get(14)?,
        heuristic_code: r.get(15)?,
        user_edited: r.get(16)?,
        status: r.get(17)?,
        transfer_link_id: r.get(18)?,
        refund_link_id: r.get(19)?,
        venture_id: r.get(20)?,
        flags: r.get(21)?,
        created_at: r.get(22)?,
        updated_at: r.get(23)?,
        tags: Vec::new(),
    })
}

pub fn tags_of(conn: &Connection, txn_id: i64) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM txn_tag x JOIN tag t ON t.id = x.tag_id WHERE x.txn_id = ?1 ORDER BY t.name",
    )?;
    let rows = stmt
        .query_map([txn_id], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<TxnRecord> {
    let mut rec = conn
        .query_row(
            &format!("SELECT {COLS} FROM txn WHERE id = ?1"),
            [id],
            from_row,
        )
        .optional()?
        .ok_or(AppError::NotFound { entity: "txn", id })?;
    rec.tags = tags_of(conn, id)?;
    Ok(rec)
}

pub fn children(conn: &Connection, parent_id: i64) -> AppResult<Vec<TxnRecord>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM txn WHERE parent_id = ?1 ORDER BY id"
    ))?;
    let mut rows = stmt
        .query_map([parent_id], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    for row in &mut rows {
        row.tags = tags_of(conn, row.id)?;
    }
    Ok(rows)
}

/// What an import inserts.
#[derive(Debug, Clone)]
pub struct NewImportedTxn<'a> {
    pub account_id: i64,
    pub import_batch_id: i64,
    pub posted_date: &'a str,
    pub effective_date: &'a str,
    pub amount_cents: i64,
    pub payee_raw: &'a str,
    pub memo: &'a str,
    pub status: RowStatus,
    pub external_id: Option<&'a str>,
    pub source_row_hash: &'a str,
    pub flags: i64,
}

pub fn insert_imported(
    conn: &Connection,
    cmd: &CommandRecord,
    new: &NewImportedTxn,
) -> AppResult<TxnRecord> {
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO txn (account_id, posted_date, effective_date, amount_cents, payee_raw, payee_norm, memo, import_batch_id,
                          source_row_hash, external_id, classification, user_edited, status, flags, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'unclassified', 0, ?11, ?12, ?13, ?13)",
        params![
            new.account_id,
            new.posted_date,
            new.effective_date,
            new.amount_cents,
            new.payee_raw,
            payee_norm(new.payee_raw),
            new.memo,
            new.import_batch_id,
            new.source_row_hash,
            new.external_id,
            new.status.as_str(),
            new.flags,
            now
        ],
    )?;
    let rec = get(conn, conn.last_insert_rowid())?;
    audit::record(
        conn,
        cmd,
        "txn",
        rec.id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&rec)?),
    )?;
    Ok(rec)
}

/// A better observation of an existing row (ADR-0017): system fields only, each one skipped
/// when its `user_edited` bit is set. Returns the names of the fields that changed.
pub struct SystemUpdate<'a> {
    pub posted_date: &'a str,
    pub effective_date: &'a str,
    pub payee_raw: &'a str,
    pub memo: &'a str,
    pub status: RowStatus,
    pub external_id: Option<&'a str>,
    pub source_row_hash: &'a str,
}

pub fn update_system_fields(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    up: &SystemUpdate,
) -> AppResult<Vec<String>> {
    let before = get(conn, id)?;
    let mut after = before.clone();
    let mut changed = Vec::new();
    let mut set = |name: &str, bit: i64, apply: &mut dyn FnMut(&mut TxnRecord)| {
        if before.user_edited & bit == 0 {
            let mut probe = after.clone();
            apply(&mut probe);
            if probe != after {
                after = probe;
                changed.push(name.to_string());
            }
        }
    };
    set("posted_date", UE_POSTED_DATE, &mut |t| {
        t.posted_date = up.posted_date.to_string()
    });
    set("effective_date", UE_EFFECTIVE_DATE, &mut |t| {
        t.effective_date = up.effective_date.to_string()
    });
    set("status", UE_STATUS, &mut |t| {
        t.status = up.status.as_str().to_string()
    });
    set("payee_norm", UE_PAYEE_NORM, &mut |t| {
        t.payee_norm = payee_norm(up.payee_raw)
    });
    set("memo", UE_MEMO, &mut |t| t.memo = up.memo.to_string());
    // identity fields are always the latest observation
    if after.payee_raw != up.payee_raw {
        after.payee_raw = up.payee_raw.to_string();
        changed.push("payee_raw".into());
    }
    if after.external_id.as_deref() != up.external_id {
        after.external_id = up.external_id.map(str::to_string);
        changed.push("external_id".into());
    }
    if after.source_row_hash.as_deref() != Some(up.source_row_hash) {
        after.source_row_hash = Some(up.source_row_hash.to_string());
        changed.push("source_row_hash".into());
    }
    if changed.is_empty() {
        return Ok(changed);
    }
    after.updated_at = now_rfc3339();
    write_all_columns(conn, &after)?;
    audit::record(
        conn,
        cmd,
        "txn",
        id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(changed)
}

/// Write every mutable column of a record (used by system updates, user edits and undo).
pub fn write_all_columns(conn: &Connection, t: &TxnRecord) -> AppResult<()> {
    conn.execute(
        "UPDATE txn SET account_id = ?1, parent_id = ?2, posted_date = ?3, effective_date = ?4, amount_cents = ?5, payee_raw = ?6,
         payee_norm = ?7, memo = ?8, category_id = ?9, import_batch_id = ?10, source_row_hash = ?11, external_id = ?12,
         classification = ?13, rule_id = ?14, heuristic_code = ?15, user_edited = ?16, status = ?17, transfer_link_id = ?18,
         refund_link_id = ?19, venture_id = ?20, flags = ?21, updated_at = ?22 WHERE id = ?23",
        params![
            t.account_id, t.parent_id, t.posted_date, t.effective_date, t.amount_cents, t.payee_raw, t.payee_norm, t.memo,
            t.category_id, t.import_batch_id, t.source_row_hash, t.external_id, t.classification, t.rule_id, t.heuristic_code,
            t.user_edited, t.status, t.transfer_link_id, t.refund_link_id, t.venture_id, t.flags, t.updated_at, t.id
        ],
    )?;
    set_tag_rows(conn, t.id, &t.tags)?;
    Ok(())
}

fn set_tag_rows(conn: &Connection, txn_id: i64, tags: &[String]) -> AppResult<()> {
    conn.execute("DELETE FROM txn_tag WHERE txn_id = ?1", [txn_id])?;
    for name in tags {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        conn.execute("INSERT OR IGNORE INTO tag (name) VALUES (?1)", [name])?;
        let tag_id: i64 = conn.query_row(
            "SELECT id FROM tag WHERE name = ?1 COLLATE NOCASE",
            [name],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO txn_tag (txn_id, tag_id) VALUES (?1, ?2)",
            params![txn_id, tag_id],
        )?;
    }
    Ok(())
}

/// A user's edit. Every field set here flips its `user_edited` bit so imports never undo it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TxnPatch {
    #[serde(default)]
    pub payee_norm: Option<String>,
    #[serde(default)]
    pub memo: Option<String>,
    #[serde(default, deserialize_with = "crate::db::repo::double_option")]
    pub category_id: Option<Option<i64>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "crate::db::repo::double_option")]
    pub venture_id: Option<Option<i64>>,
    #[serde(default)]
    pub flags: Option<i64>,
    #[serde(default)]
    pub effective_date: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

pub fn apply_user_patch(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    patch: &TxnPatch,
) -> AppResult<TxnRecord> {
    let before = get(conn, id)?;
    let mut after = before.clone();
    if let Some(p) = &patch.payee_norm {
        let p = p.trim();
        if p.is_empty() {
            return Err(AppError::validation(
                "payee_norm",
                "a payee cannot be empty",
            ));
        }
        after.payee_norm = p.to_string();
        after.user_edited |= UE_PAYEE_NORM;
    }
    if let Some(m) = &patch.memo {
        after.memo = m.trim().to_string();
        after.user_edited |= UE_MEMO;
    }
    if let Some(c) = patch.category_id {
        if let Some(cid) = c {
            let exists: i64 =
                conn.query_row("SELECT count(*) FROM category WHERE id = ?1", [cid], |r| {
                    r.get(0)
                })?;
            if exists == 0 {
                return Err(AppError::NotFound {
                    entity: "category",
                    id: cid,
                });
            }
        }
        after.category_id = c;
        after.classification = "manual".into();
        after.rule_id = None;
        after.heuristic_code = None;
        after.user_edited |= UE_CATEGORY;
        // a categorised row leaves the review queue, unless a firewall acknowledgment is pending
        if c.is_some() && !super::link::awaits_firewall_ack(conn, &after)? {
            after.flags &= !(i64::from(FLAG_NEEDS_REVIEW) | i64::from(FLAG_PAYMENT_APP_UNKNOWN));
        }
    }
    if let Some(tags) = &patch.tags {
        let mut cleaned: Vec<String> = tags
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        cleaned.sort();
        cleaned.dedup();
        after.tags = cleaned;
        after.user_edited |= UE_TAGS;
    }
    if let Some(v) = patch.venture_id {
        if let Some(vid) = v {
            let exists: i64 =
                conn.query_row("SELECT count(*) FROM venture WHERE id = ?1", [vid], |r| {
                    r.get(0)
                })?;
            if exists == 0 {
                return Err(AppError::NotFound {
                    entity: "venture",
                    id: vid,
                });
            }
        }
        after.venture_id = v;
        after.user_edited |= UE_VENTURE;
    }
    if let Some(flags) = patch.flags {
        if !(0..=127).contains(&flags) {
            return Err(AppError::validation("flags", "unknown flag bits"));
        }
        after.flags = flags;
        after.user_edited |= UE_FLAGS;
    }
    if let Some(d) = &patch.effective_date {
        parse_civil(d)?;
        after.effective_date = d.trim().to_string();
        after.user_edited |= UE_EFFECTIVE_DATE;
    }
    if let Some(s) = &patch.status {
        if s != "pending" && s != "posted" {
            return Err(AppError::validation("status", "must be pending or posted"));
        }
        after.status = s.clone();
        after.user_edited |= UE_STATUS;
    }
    if after == before {
        return Ok(before);
    }
    after.updated_at = now_rfc3339();
    write_all_columns(conn, &after)?;
    audit::record(
        conn,
        cmd,
        "txn",
        id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(after)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitPart {
    pub amount_cents: i64,
    #[serde(default)]
    pub category_id: Option<i64>,
    #[serde(default)]
    pub memo: String,
}

/// Split a row into child rows whose amounts sum to the parent's (`money_sum_conserves`).
/// Children inherit account, dates and status; the parent keeps its amount and stops counting.
pub fn split(
    conn: &Connection,
    cmd: &CommandRecord,
    parent_id: i64,
    parts: &[SplitPart],
) -> AppResult<Vec<TxnRecord>> {
    let parent = get(conn, parent_id)?;
    if parent.parent_id.is_some() {
        return Err(AppError::validation(
            "parent_id",
            "a split child cannot be split again",
        ));
    }
    if parts.len() < 2 {
        return Err(AppError::validation(
            "parts",
            "a split needs at least two parts",
        ));
    }
    let total = money::sum(parts.iter().map(|p| money::Cents(p.amount_cents)))?;
    if total.0 != parent.amount_cents {
        return Err(AppError::validation(
            "parts",
            format!(
                "parts sum to {} but the row is {}",
                money::to_decimal_string(total.0),
                money::to_decimal_string(parent.amount_cents)
            ),
        ));
    }
    if !children(conn, parent_id)?.is_empty() {
        return Err(AppError::Conflict(
            "the row is already split; unsplit it first".into(),
        ));
    }
    let now = now_rfc3339();
    let mut out = Vec::new();
    for part in parts {
        conn.execute(
            "INSERT INTO txn (account_id, parent_id, posted_date, effective_date, amount_cents, payee_raw, payee_norm, memo, category_id,
                              classification, user_edited, status, venture_id, flags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
            params![
                parent.account_id,
                parent.id,
                parent.posted_date,
                parent.effective_date,
                part.amount_cents,
                parent.payee_raw,
                parent.payee_norm,
                part.memo.trim(),
                part.category_id,
                if part.category_id.is_some() { "manual" } else { "unclassified" },
                UE_AMOUNT | if part.category_id.is_some() { UE_CATEGORY } else { 0 },
                parent.status,
                parent.venture_id,
                parent.flags,
                now
            ],
        )?;
        let child = get(conn, conn.last_insert_rowid())?;
        audit::record(
            conn,
            cmd,
            "txn",
            child.id,
            Action::Insert,
            None,
            Some(&serde_json::to_value(&child)?),
        )?;
        out.push(child);
    }
    Ok(out)
}

pub fn unsplit(conn: &Connection, cmd: &CommandRecord, parent_id: i64) -> AppResult<usize> {
    let kids = children(conn, parent_id)?;
    if kids.is_empty() {
        return Err(AppError::validation("parent_id", "the row is not split"));
    }
    for child in &kids {
        if child.transfer_link_id.is_some() || child.refund_link_id.is_some() {
            return Err(AppError::Conflict(format!(
                "split part {} is linked; unlink it first",
                child.id
            )));
        }
        audit::record(
            conn,
            cmd,
            "txn",
            child.id,
            Action::Delete,
            Some(&serde_json::to_value(child)?),
            None,
        )?;
        conn.execute("DELETE FROM txn_tag WHERE txn_id = ?1", [child.id])?;
        conn.execute("DELETE FROM txn WHERE id = ?1", [child.id])?;
    }
    Ok(kids.len())
}

/// Delete a row (only ever an undo of an import or a split; never a user-facing delete).
pub fn delete_row(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let before = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "txn",
        id,
        Action::Delete,
        Some(&serde_json::to_value(&before)?),
        None,
    )?;
    detach_plan_links(conn, cmd, id)?;
    conn.execute("DELETE FROM txn_tag WHERE txn_id = ?1", [id])?;
    conn.execute("DELETE FROM txn WHERE id = ?1", [id])?;
    Ok(())
}

/// A row that goes takes its receipt, payment and earmark-entry links with it (audited).
fn detach_plan_links(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let receipt: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT income_stream_id, due_date, matched_by FROM income_receipt WHERE txn_id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((stream_id, due_date, matched_by)) = receipt {
        audit::record(
            conn,
            cmd,
            "income_receipt",
            id,
            Action::Delete,
            Some(
                &serde_json::json!({ "income_stream_id": stream_id, "due_date": due_date, "txn_id": id, "matched_by": matched_by }),
            ),
            None,
        )?;
        conn.execute("DELETE FROM income_receipt WHERE txn_id = ?1", [id])?;
    }
    let payment: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT obligation_id, due_date, matched_by FROM obligation_payment WHERE txn_id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((obligation_id, due_date, matched_by)) = payment {
        audit::record(
            conn,
            cmd,
            "obligation_payment",
            id,
            Action::Delete,
            Some(
                &serde_json::json!({ "obligation_id": obligation_id, "due_date": due_date, "txn_id": id, "matched_by": matched_by }),
            ),
            None,
        )?;
        conn.execute("DELETE FROM obligation_payment WHERE txn_id = ?1", [id])?;
    }
    conn.execute(
        "UPDATE earmark_entry SET txn_id = NULL WHERE txn_id = ?1",
        [id],
    )?;
    Ok(())
}

/// Flag bits from names (for commands that take flag names).
pub fn flags_from_names(names: &[String]) -> AppResult<i64> {
    let mut bits = 0u32;
    for n in names {
        bits |= flag_bit(n)
            .ok_or_else(|| AppError::validation("flags", format!("unknown flag {n:?}")))?;
    }
    Ok(i64::from(bits))
}
