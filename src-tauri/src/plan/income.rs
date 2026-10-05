//! Income streams and their receipts.

use chrono::Duration;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::{format_civil, now_rfc3339, parse_civil, CivilDate};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::txn;
use crate::error::{AppError, AppResult};
use crate::plan::occur::{self, IncomeRule};
use crate::plan::{MATCH_AFTER_DAYS, MATCH_BEFORE_DAYS, MATCH_LOOKBACK_DAYS};

pub const KINDS: [&str; 5] = ["base", "bonus", "rsu", "deferred_comp", "other"];
pub const CYCLES: [&str; 5] = ["weekly", "biweekly", "semimonthly", "monthly", "once"];
pub const CONFIDENCES: [&str; 3] = ["confirmed", "expected", "rumored"];
pub const WEEKEND_RULES: [&str; 3] = ["none", "previous_business_day", "next_business_day"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IncomeStream {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub cycle: String,
    pub anchor_date: String,
    pub semimonthly_day_1: Option<i64>,
    pub semimonthly_day_2: Option<i64>,
    pub expected_net_cents: i64,
    pub variability_cents: i64,
    pub confidence: String,
    pub weekend_rule: String,
    pub deposit_account_id: Option<i64>,
    pub match_payee_contains: Option<String>,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IncomeInput {
    pub name: String,
    pub kind: String,
    pub cycle: String,
    pub anchor_date: String,
    #[serde(default)]
    pub semimonthly_day_1: Option<i64>,
    #[serde(default)]
    pub semimonthly_day_2: Option<i64>,
    pub expected_net_cents: i64,
    #[serde(default)]
    pub variability_cents: i64,
    pub confidence: String,
    #[serde(default = "default_weekend_rule")]
    pub weekend_rule: String,
    #[serde(default)]
    pub deposit_account_id: Option<i64>,
    #[serde(default)]
    pub match_payee_contains: Option<String>,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_weekend_rule() -> String {
    "previous_business_day".to_string()
}

fn default_true() -> bool {
    true
}

const COLS: &str = "id, name, kind, cycle, anchor_date, semimonthly_day_1, semimonthly_day_2, expected_net_cents, variability_cents, confidence, weekend_rule, deposit_account_id, match_payee_contains, active, created_at, updated_at";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<IncomeStream> {
    Ok(IncomeStream {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        cycle: r.get(3)?,
        anchor_date: r.get(4)?,
        semimonthly_day_1: r.get(5)?,
        semimonthly_day_2: r.get(6)?,
        expected_net_cents: r.get(7)?,
        variability_cents: r.get(8)?,
        confidence: r.get(9)?,
        weekend_rule: r.get(10)?,
        deposit_account_id: r.get(11)?,
        match_payee_contains: r.get(12)?,
        active: r.get::<_, i64>(13)? != 0,
        created_at: r.get(14)?,
        updated_at: r.get(15)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<IncomeStream>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM income_stream ORDER BY active DESC, name"
    ))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<IncomeStream> {
    conn.query_row(
        &format!("SELECT {COLS} FROM income_stream WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "income_stream",
        id,
    })
}

fn validate(input: &IncomeInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::validation(
            "name",
            "an income stream needs a name",
        ));
    }
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::validation(
            "kind",
            format!("must be one of {}", KINDS.join(", ")),
        ));
    }
    if !CYCLES.contains(&input.cycle.as_str()) {
        return Err(AppError::validation(
            "cycle",
            format!("must be one of {}", CYCLES.join(", ")),
        ));
    }
    if !CONFIDENCES.contains(&input.confidence.as_str()) {
        return Err(AppError::validation(
            "confidence",
            "must be confirmed, expected or rumored",
        ));
    }
    if !WEEKEND_RULES.contains(&input.weekend_rule.as_str()) {
        return Err(AppError::validation(
            "weekend_rule",
            "must be none, previous_business_day or next_business_day",
        ));
    }
    parse_civil(&input.anchor_date)?;
    if input.expected_net_cents < 0 {
        return Err(AppError::validation(
            "expected_net_cents",
            "cannot be negative",
        ));
    }
    if input.variability_cents < 0 {
        return Err(AppError::validation(
            "variability_cents",
            "cannot be negative",
        ));
    }
    if input.cycle == "semimonthly" {
        for (field, value) in [
            ("semimonthly_day_1", input.semimonthly_day_1),
            ("semimonthly_day_2", input.semimonthly_day_2),
        ] {
            match value {
                Some(d) if (1..=31).contains(&d) => {}
                _ => {
                    return Err(AppError::validation(
                        field,
                        "a semimonthly cycle needs two days 1–31",
                    ))
                }
            }
        }
    }
    Ok(())
}

fn needle(input: &IncomeInput) -> Option<String> {
    input
        .match_payee_contains
        .as_ref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

pub fn create(
    conn: &Connection,
    cmd: &CommandRecord,
    input: &IncomeInput,
) -> AppResult<IncomeStream> {
    validate(input)?;
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO income_stream (name, kind, cycle, anchor_date, semimonthly_day_1, semimonthly_day_2, expected_net_cents,
           variability_cents, confidence, weekend_rule, deposit_account_id, match_payee_contains, active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
        params![
            input.name.trim(),
            input.kind,
            input.cycle,
            input.anchor_date.trim(),
            input.semimonthly_day_1,
            input.semimonthly_day_2,
            input.expected_net_cents,
            input.variability_cents,
            input.confidence,
            input.weekend_rule,
            input.deposit_account_id,
            needle(input),
            i64::from(input.active),
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    let created = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "income_stream",
        id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&created)?),
    )?;
    Ok(created)
}

pub fn update(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    input: &IncomeInput,
) -> AppResult<IncomeStream> {
    validate(input)?;
    let before = get(conn, id)?;
    conn.execute(
        "UPDATE income_stream SET name = ?2, kind = ?3, cycle = ?4, anchor_date = ?5, semimonthly_day_1 = ?6, semimonthly_day_2 = ?7,
           expected_net_cents = ?8, variability_cents = ?9, confidence = ?10, weekend_rule = ?11, deposit_account_id = ?12,
           match_payee_contains = ?13, active = ?14, updated_at = ?15 WHERE id = ?1",
        params![
            id,
            input.name.trim(),
            input.kind,
            input.cycle,
            input.anchor_date.trim(),
            input.semimonthly_day_1,
            input.semimonthly_day_2,
            input.expected_net_cents,
            input.variability_cents,
            input.confidence,
            input.weekend_rule,
            input.deposit_account_id,
            needle(input),
            i64::from(input.active),
            now_rfc3339()
        ],
    )?;
    let after = get(conn, id)?;
    if after != before {
        audit::record(
            conn,
            cmd,
            "income_stream",
            id,
            Action::Update,
            Some(&serde_json::to_value(&before)?),
            Some(&serde_json::to_value(&after)?),
        )?;
    }
    Ok(after)
}

/// Expected civil dates inside `[from, to]`.
pub fn occurrences(
    stream: &IncomeStream,
    from: CivilDate,
    to: CivilDate,
) -> AppResult<Vec<CivilDate>> {
    occur::income_occurrences(
        &IncomeRule {
            cycle: &stream.cycle,
            anchor: parse_civil(&stream.anchor_date)?,
            semimonthly_day_1: stream.semimonthly_day_1,
            semimonthly_day_2: stream.semimonthly_day_2,
            weekend_rule: &stream.weekend_rule,
        },
        from,
        to,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    pub income_stream_id: i64,
    pub due_date: String,
    pub txn_id: i64,
    pub matched_by: String,
}

fn receipt_from_row(r: &rusqlite::Row) -> rusqlite::Result<Receipt> {
    Ok(Receipt {
        income_stream_id: r.get(0)?,
        due_date: r.get(1)?,
        txn_id: r.get(2)?,
        matched_by: r.get(3)?,
    })
}

pub fn receipts(conn: &Connection, stream_id: i64) -> AppResult<Vec<Receipt>> {
    let mut stmt = conn.prepare(
        "SELECT income_stream_id, due_date, txn_id, matched_by FROM income_receipt WHERE income_stream_id = ?1 ORDER BY due_date",
    )?;
    let rows = stmt
        .query_map([stream_id], receipt_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn all_receipts(conn: &Connection) -> AppResult<Vec<Receipt>> {
    let mut stmt = conn.prepare(
        "SELECT income_stream_id, due_date, txn_id, matched_by FROM income_receipt ORDER BY income_stream_id, due_date",
    )?;
    let rows = stmt
        .query_map([], receipt_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn receipt_for(
    conn: &Connection,
    stream_id: i64,
    due_date: &str,
) -> AppResult<Option<Receipt>> {
    Ok(conn
        .query_row(
            "SELECT income_stream_id, due_date, txn_id, matched_by FROM income_receipt WHERE income_stream_id = ?1 AND due_date = ?2",
            params![stream_id, due_date],
            receipt_from_row,
        )
        .optional()?)
}

/// Record that `txn_id` is the receipt of the occurrence due on `due_date`.
pub fn record_receipt(
    conn: &Connection,
    cmd: &CommandRecord,
    stream_id: i64,
    due_date: &str,
    txn_id: i64,
    matched_by: &str,
) -> AppResult<Receipt> {
    get(conn, stream_id)?;
    parse_civil(due_date)?;
    let row = txn::get(conn, txn_id)?;
    if row.amount_cents <= 0 {
        return Err(AppError::validation("txn_id", "a receipt is an inflow"));
    }
    if receipt_for(conn, stream_id, due_date)?.is_some() {
        return Err(AppError::Conflict(format!(
            "the occurrence due {due_date} already has a receipt"
        )));
    }
    let used: i64 = conn.query_row(
        "SELECT count(*) FROM income_receipt WHERE txn_id = ?1",
        [txn_id],
        |r| r.get(0),
    )?;
    if used > 0 {
        return Err(AppError::Conflict(format!(
            "row {txn_id} is already a receipt"
        )));
    }
    conn.execute(
        "INSERT INTO income_receipt (income_stream_id, due_date, txn_id, matched_by) VALUES (?1, ?2, ?3, ?4)",
        params![stream_id, due_date, txn_id, matched_by],
    )?;
    let receipt = Receipt {
        income_stream_id: stream_id,
        due_date: due_date.to_string(),
        txn_id,
        matched_by: matched_by.to_string(),
    };
    audit::record(
        conn,
        cmd,
        "income_receipt",
        txn_id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&receipt)?),
    )?;
    Ok(receipt)
}

pub fn remove_receipt(
    conn: &Connection,
    cmd: &CommandRecord,
    stream_id: i64,
    due_date: &str,
) -> AppResult<()> {
    let Some(receipt) = receipt_for(conn, stream_id, due_date)? else {
        return Err(AppError::validation(
            "due_date",
            format!("no receipt is recorded for {due_date}"),
        ));
    };
    audit::record(
        conn,
        cmd,
        "income_receipt",
        receipt.txn_id,
        Action::Delete,
        Some(&serde_json::to_value(&receipt)?),
        None,
    )?;
    conn.execute(
        "DELETE FROM income_receipt WHERE income_stream_id = ?1 AND due_date = ?2",
        params![stream_id, due_date],
    )?;
    Ok(())
}

/// The closest unmatched inflow to `due` on the deposit account (any account when unset): payee
/// contains the match text when one is set, amount within expected ± variability, posted within
/// the window.
fn candidate_receipt(
    conn: &Connection,
    stream: &IncomeStream,
    due: CivilDate,
) -> AppResult<Option<i64>> {
    let from = format_civil(due - Duration::days(MATCH_BEFORE_DAYS));
    let to = format_civil(due + Duration::days(MATCH_AFTER_DAYS));
    let low = stream
        .expected_net_cents
        .saturating_sub(stream.variability_cents);
    let high = stream
        .expected_net_cents
        .saturating_add(stream.variability_cents);
    let needle = stream.match_payee_contains.clone().unwrap_or_default();
    let id: Option<i64> = conn
        .query_row(
            "SELECT t.id FROM txn_leaf t
             WHERE t.amount_cents BETWEEN ?1 AND ?2 AND t.amount_cents > 0
               AND t.posted_date BETWEEN ?3 AND ?4
               AND (?5 IS NULL OR t.account_id = ?5)
               AND (?6 = '' OR instr(t.payee_norm, ?6) > 0)
               AND NOT EXISTS (SELECT 1 FROM income_receipt r WHERE r.txn_id = t.id)
             ORDER BY abs(julianday(t.posted_date) - julianday(?7)), t.posted_date, t.id
             LIMIT 1",
            params![
                low,
                high,
                from,
                to,
                stream.deposit_account_id,
                needle,
                format_civil(due)
            ],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id)
}

/// Match receipts for every active stream's occurrences in the lookback window; returns how many
/// were recorded.
pub fn match_receipts(
    conn: &Connection,
    cmd: &CommandRecord,
    today: CivilDate,
) -> AppResult<usize> {
    let mut matched = 0;
    for stream in list(conn)?.into_iter().filter(|s| s.active) {
        let from = today - Duration::days(MATCH_LOOKBACK_DAYS);
        let to = today + Duration::days(MATCH_BEFORE_DAYS);
        for due in occurrences(&stream, from, to)? {
            let due_s = format_civil(due);
            if receipt_for(conn, stream.id, &due_s)?.is_some() {
                continue;
            }
            if let Some(txn_id) = candidate_receipt(conn, &stream, due)? {
                record_receipt(conn, cmd, stream.id, &due_s, txn_id, "heuristic")?;
                matched += 1;
            }
        }
    }
    Ok(matched)
}
