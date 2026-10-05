//! Earmarks: reserved cents against a funding account. Remaining is derived from entries, never
//! stored twice (spec: "funding status is derived, not a second copy of the balance").

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::{format_civil, now_rfc3339, parse_civil, CivilDate};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::{account, txn};
use crate::error::{AppError, AppResult};
use crate::money::Cents;

pub const KINDS: [&str; 3] = ["obligation", "sinking_fund", "emergency_reserve"];
pub const SCHEDULES: [&str; 4] = ["none", "monthly", "per_paycheck", "by_date"];
pub const ENTRY_KINDS: [&str; 3] = ["fund", "release", "adjust"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Earmark {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub funding_account_id: i64,
    pub obligation_id: Option<i64>,
    pub target_cents: i64,
    pub target_date: Option<String>,
    pub schedule: String,
    pub schedule_amount_cents: Option<i64>,
    pub schedule_day: Option<i64>,
    pub schedule_income_stream_id: Option<i64>,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
    /// Σ every entry: what the earmark holds today, before any schedule projection.
    pub held_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EarmarkInput {
    pub name: String,
    pub kind: String,
    pub funding_account_id: i64,
    #[serde(default)]
    pub obligation_id: Option<i64>,
    pub target_cents: i64,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default = "default_schedule")]
    pub schedule: String,
    #[serde(default)]
    pub schedule_amount_cents: Option<i64>,
    #[serde(default)]
    pub schedule_day: Option<i64>,
    #[serde(default)]
    pub schedule_income_stream_id: Option<i64>,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_schedule() -> String {
    "none".to_string()
}

fn default_true() -> bool {
    true
}

const COLS: &str = "e.id, e.name, e.kind, e.funding_account_id, e.obligation_id, e.target_cents, e.target_date, e.schedule, e.schedule_amount_cents, e.schedule_day, e.schedule_income_stream_id, e.active, e.created_at, e.updated_at, (SELECT coalesce(SUM(amount_cents), 0) FROM earmark_entry WHERE earmark_id = e.id)";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Earmark> {
    Ok(Earmark {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        funding_account_id: r.get(3)?,
        obligation_id: r.get(4)?,
        target_cents: r.get(5)?,
        target_date: r.get(6)?,
        schedule: r.get(7)?,
        schedule_amount_cents: r.get(8)?,
        schedule_day: r.get(9)?,
        schedule_income_stream_id: r.get(10)?,
        active: r.get::<_, i64>(11)? != 0,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
        held_cents: r.get(14)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Earmark>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM earmark e ORDER BY e.active DESC, e.kind, e.name"
    ))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Earmark> {
    conn.query_row(
        &format!("SELECT {COLS} FROM earmark e WHERE e.id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "earmark",
        id,
    })
}

fn validate(conn: &Connection, input: &EarmarkInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::validation("name", "an earmark needs a name"));
    }
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::validation(
            "kind",
            "must be obligation, sinking_fund or emergency_reserve",
        ));
    }
    if (input.kind == "obligation") != input.obligation_id.is_some() {
        return Err(AppError::validation(
            "obligation_id",
            "an obligation earmark names its obligation; other kinds do not",
        ));
    }
    if !SCHEDULES.contains(&input.schedule.as_str()) {
        return Err(AppError::validation(
            "schedule",
            "must be none, monthly, per_paycheck or by_date",
        ));
    }
    if input.target_cents < 0 {
        return Err(AppError::validation("target_cents", "cannot be negative"));
    }
    if let Some(a) = input.schedule_amount_cents {
        if a < 0 {
            return Err(AppError::validation(
                "schedule_amount_cents",
                "cannot be negative",
            ));
        }
    }
    if let Some(d) = &input.target_date {
        parse_civil(d)?;
    }
    match input.schedule.as_str() {
        "monthly"
            if !matches!(input.schedule_day, Some(1..=31))
                || input.schedule_amount_cents.is_none() =>
        {
            return Err(AppError::validation(
                "schedule_day",
                "a monthly schedule needs a day 1–31 and an amount",
            ));
        }
        "per_paycheck"
            if input.schedule_income_stream_id.is_none()
                || input.schedule_amount_cents.is_none() =>
        {
            return Err(AppError::validation(
                "schedule_income_stream_id",
                "a per-paycheck schedule names its income stream and an amount",
            ));
        }
        "by_date" if input.target_date.is_none() => {
            return Err(AppError::validation(
                "target_date",
                "a by-date schedule needs the target date",
            ));
        }
        _ => {}
    }
    account::get(conn, input.funding_account_id)?;
    Ok(())
}

pub fn create(conn: &Connection, cmd: &CommandRecord, input: &EarmarkInput) -> AppResult<Earmark> {
    validate(conn, input)?;
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO earmark (name, kind, funding_account_id, obligation_id, target_cents, target_date, schedule, schedule_amount_cents,
           schedule_day, schedule_income_stream_id, active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
        params![
            input.name.trim(),
            input.kind,
            input.funding_account_id,
            input.obligation_id,
            input.target_cents,
            input.target_date,
            input.schedule,
            input.schedule_amount_cents,
            input.schedule_day,
            input.schedule_income_stream_id,
            i64::from(input.active),
            now
        ],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            AppError::validation("obligation_id", "that obligation already has an earmark")
        }
        other => other.into(),
    })?;
    let id = conn.last_insert_rowid();
    let created = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "earmark",
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
    input: &EarmarkInput,
) -> AppResult<Earmark> {
    validate(conn, input)?;
    let before = get(conn, id)?;
    conn.execute(
        "UPDATE earmark SET name = ?2, kind = ?3, funding_account_id = ?4, obligation_id = ?5, target_cents = ?6, target_date = ?7,
           schedule = ?8, schedule_amount_cents = ?9, schedule_day = ?10, schedule_income_stream_id = ?11, active = ?12, updated_at = ?13
         WHERE id = ?1",
        params![
            id,
            input.name.trim(),
            input.kind,
            input.funding_account_id,
            input.obligation_id,
            input.target_cents,
            input.target_date,
            input.schedule,
            input.schedule_amount_cents,
            input.schedule_day,
            input.schedule_income_stream_id,
            i64::from(input.active),
            now_rfc3339()
        ],
    )?;
    let after = get(conn, id)?;
    if after != before {
        audit::record(
            conn,
            cmd,
            "earmark",
            id,
            Action::Update,
            Some(&serde_json::to_value(&before)?),
            Some(&serde_json::to_value(&after)?),
        )?;
    }
    Ok(after)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub id: i64,
    pub earmark_id: i64,
    pub entry_date: String,
    pub kind: String,
    pub amount_cents: i64,
    pub txn_id: Option<i64>,
    pub note: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntryInput {
    pub entry_date: String,
    pub kind: String,
    pub amount_cents: i64,
    #[serde(default)]
    pub txn_id: Option<i64>,
    #[serde(default)]
    pub note: String,
}

const ENTRY_COLS: &str = "id, earmark_id, entry_date, kind, amount_cents, txn_id, note, created_at";

fn entry_from_row(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        earmark_id: r.get(1)?,
        entry_date: r.get(2)?,
        kind: r.get(3)?,
        amount_cents: r.get(4)?,
        txn_id: r.get(5)?,
        note: r.get(6)?,
        created_at: r.get(7)?,
    })
}

pub fn entries(conn: &Connection, earmark_id: i64) -> AppResult<Vec<Entry>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ENTRY_COLS} FROM earmark_entry WHERE earmark_id = ?1 ORDER BY entry_date, id"
    ))?;
    let rows = stmt
        .query_map([earmark_id], entry_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_entry(conn: &Connection, id: i64) -> AppResult<Entry> {
    conn.query_row(
        &format!("SELECT {ENTRY_COLS} FROM earmark_entry WHERE id = ?1"),
        [id],
        entry_from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "earmark_entry",
        id,
    })
}

/// Record money set aside (`fund`, positive), released (`release`, negative) or corrected
/// (`adjust`, either sign); an entry may point at the ledger row that moved it.
pub fn add_entry(
    conn: &Connection,
    cmd: &CommandRecord,
    earmark_id: i64,
    input: &EntryInput,
) -> AppResult<Entry> {
    get(conn, earmark_id)?;
    parse_civil(&input.entry_date)?;
    if !ENTRY_KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::validation(
            "kind",
            "must be fund, release or adjust",
        ));
    }
    match input.kind.as_str() {
        "fund" if input.amount_cents <= 0 => {
            return Err(AppError::validation(
                "amount_cents",
                "a funding is a positive amount",
            ));
        }
        "release" if input.amount_cents >= 0 => {
            return Err(AppError::validation(
                "amount_cents",
                "a release is a negative amount",
            ));
        }
        "adjust" if input.amount_cents == 0 => {
            return Err(AppError::validation(
                "amount_cents",
                "an adjustment cannot be zero",
            ));
        }
        _ => {}
    }
    if let Some(t) = input.txn_id {
        txn::get(conn, t)?;
    }
    conn.execute(
        "INSERT INTO earmark_entry (earmark_id, entry_date, kind, amount_cents, txn_id, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            earmark_id,
            input.entry_date.trim(),
            input.kind,
            input.amount_cents,
            input.txn_id,
            input.note.trim(),
            now_rfc3339()
        ],
    )?;
    let id = conn.last_insert_rowid();
    let entry = get_entry(conn, id)?;
    audit::record(
        conn,
        cmd,
        "earmark_entry",
        id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&entry)?),
    )?;
    Ok(entry)
}

pub fn delete_entry(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let entry = get_entry(conn, id)?;
    audit::record(
        conn,
        cmd,
        "earmark_entry",
        id,
        Action::Delete,
        Some(&serde_json::to_value(&entry)?),
        None,
    )?;
    conn.execute("DELETE FROM earmark_entry WHERE id = ?1", [id])?;
    Ok(())
}

/// Σ entries dated on or before `as_of` (checked arithmetic).
pub fn remaining(conn: &Connection, earmark_id: i64, as_of: CivilDate) -> AppResult<i64> {
    let mut stmt = conn.prepare(
        "SELECT amount_cents FROM earmark_entry WHERE earmark_id = ?1 AND entry_date <= ?2",
    )?;
    let amounts = stmt
        .query_map(params![earmark_id, format_civil(as_of)], |r| {
            r.get::<_, i64>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut total = Cents::ZERO;
    for a in amounts {
        total = total.checked_add(Cents(a))?;
    }
    Ok(total.0)
}

/// Entry ids dated on or before `as_of`, for the hero's drill-down.
pub fn entry_ids(conn: &Connection, earmark_id: i64, as_of: CivilDate) -> AppResult<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT id FROM earmark_entry WHERE earmark_id = ?1 AND entry_date <= ?2 ORDER BY entry_date, id")?;
    let ids = stmt
        .query_map(params![earmark_id, format_civil(as_of)], |r| {
            r.get::<_, i64>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}
