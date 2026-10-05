//! Ventures: the record a rule or a row can point at. The rollups live in `venture/` (M7).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::{now_rfc3339, parse_civil};
use crate::db::audit::{self, Action, CommandRecord};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Venture {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub cash_cap_cents: i64,
    pub time_budget_hours: Option<i64>,
    pub milestone: String,
    pub milestone_date: Option<String>,
    pub stop_condition: String,
    pub archived: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VentureInput {
    pub name: String,
    pub status: String,
    pub cash_cap_cents: i64,
    #[serde(default)]
    pub time_budget_hours: Option<i64>,
    #[serde(default)]
    pub milestone: String,
    #[serde(default)]
    pub milestone_date: Option<String>,
    #[serde(default)]
    pub stop_condition: String,
}

const COLS: &str = "id, name, status, cash_cap_cents, time_budget_hours, milestone, milestone_date, stop_condition, archived, created_at";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Venture> {
    Ok(Venture {
        id: r.get(0)?,
        name: r.get(1)?,
        status: r.get(2)?,
        cash_cap_cents: r.get(3)?,
        time_budget_hours: r.get(4)?,
        milestone: r.get(5)?,
        milestone_date: r.get(6)?,
        stop_condition: r.get(7)?,
        archived: r.get::<_, i64>(8)? == 1,
        created_at: r.get(9)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Venture>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM venture ORDER BY archived, name"
    ))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Venture> {
    conn.query_row(
        &format!("SELECT {COLS} FROM venture WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "venture",
        id,
    })
}

fn validate(input: &VentureInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::validation("name", "a venture needs a name"));
    }
    if !["fund", "freeze", "kill"].contains(&input.status.as_str()) {
        return Err(AppError::validation(
            "status",
            "must be fund, freeze or kill",
        ));
    }
    if input.cash_cap_cents < 0 {
        return Err(AppError::validation(
            "cash_cap_cents",
            "the cash cap cannot be negative",
        ));
    }
    if let Some(h) = input.time_budget_hours {
        if h < 0 {
            return Err(AppError::validation(
                "time_budget_hours",
                "cannot be negative",
            ));
        }
    }
    if let Some(d) = &input.milestone_date {
        parse_civil(d)?;
    }
    Ok(())
}

pub fn create(conn: &Connection, cmd: &CommandRecord, input: &VentureInput) -> AppResult<Venture> {
    validate(input)?;
    conn.execute(
        "INSERT INTO venture (name, status, cash_cap_cents, time_budget_hours, milestone, milestone_date, stop_condition, archived, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
        params![
            input.name.trim(),
            input.status,
            input.cash_cap_cents,
            input.time_budget_hours,
            input.milestone.trim(),
            input.milestone_date,
            input.stop_condition.trim(),
            now_rfc3339()
        ],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            AppError::validation("name", format!("a venture named {:?} already exists", input.name.trim()))
        }
        other => other.into(),
    })?;
    let v = get(conn, conn.last_insert_rowid())?;
    audit::record(
        conn,
        cmd,
        "venture",
        v.id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&v)?),
    )?;
    Ok(v)
}

pub fn update(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    input: &VentureInput,
) -> AppResult<Venture> {
    let before = get(conn, id)?;
    validate(input)?;
    conn.execute(
        "UPDATE venture SET name = ?1, status = ?2, cash_cap_cents = ?3, time_budget_hours = ?4, milestone = ?5, milestone_date = ?6, stop_condition = ?7 WHERE id = ?8",
        params![
            input.name.trim(),
            input.status,
            input.cash_cap_cents,
            input.time_budget_hours,
            input.milestone.trim(),
            input.milestone_date,
            input.stop_condition.trim(),
            id
        ],
    )?;
    let after = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "venture",
        id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(after)
}
