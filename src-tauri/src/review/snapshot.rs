//! Snapshots (ADR-0027): once per civil day on unlock, on demand, and a `plan` at review
//! completion. They feed trends only and are never read for a current figure.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::cash::safe;
use crate::dates::{format_civil, now_rfc3339, CivilDate};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::account;
use crate::debt;
use crate::error::{AppError, AppResult};
use crate::venture;

pub const KINDS: [&str; 3] = ["daily", "on_demand", "plan"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountBalance {
    pub account_id: i64,
    pub name: String,
    pub kind: String,
    pub balance_cents: i64,
}

/// What a snapshot carries beyond its columns: every account's balance, and for a `plan` the
/// forecast closings the person committed to.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Detail {
    #[serde(default)]
    pub accounts: Vec<AccountBalance>,
    #[serde(default)]
    pub days: Vec<crate::forecast::Point>,
    #[serde(default)]
    pub review_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub id: i64,
    pub taken_at: String,
    pub civil_date: String,
    pub kind: String,
    pub safe_cents: i64,
    pub available_cents: i64,
    pub earmarks_cents: i64,
    pub obligations_cents: i64,
    pub buffer_cents: i64,
    pub trusted: bool,
    pub total_debt_cents: i64,
    pub informal_remaining_cents: i64,
    pub venture_cap_used_cents: i64,
    pub detail: Detail,
}

const COLS: &str = "id, taken_at, civil_date, kind, safe_cents, available_cents, earmarks_cents, obligations_cents, buffer_cents, trusted, total_debt_cents, informal_remaining_cents, venture_cap_used_cents, detail_json";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Snapshot> {
    let detail: String = r.get(13)?;
    Ok(Snapshot {
        id: r.get(0)?,
        taken_at: r.get(1)?,
        civil_date: r.get(2)?,
        kind: r.get(3)?,
        safe_cents: r.get(4)?,
        available_cents: r.get(5)?,
        earmarks_cents: r.get(6)?,
        obligations_cents: r.get(7)?,
        buffer_cents: r.get(8)?,
        trusted: r.get::<_, i64>(9)? == 1,
        total_debt_cents: r.get(10)?,
        informal_remaining_cents: r.get(11)?,
        venture_cap_used_cents: r.get(12)?,
        detail: serde_json::from_str(&detail).unwrap_or_default(),
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Snapshot>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM snapshot ORDER BY civil_date, taken_at, id"
    ))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Snapshot> {
    conn.query_row(
        &format!("SELECT {COLS} FROM snapshot WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "snapshot",
        id,
    })
}

pub fn daily_for(conn: &Connection, day: CivilDate) -> AppResult<Option<Snapshot>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM snapshot WHERE kind = 'daily' AND civil_date = ?1"),
            [format_civil(day)],
            from_row,
        )
        .optional()?)
}

/// Take a snapshot of today's figures: the hero's terms, total debt, informal remaining, venture
/// cap used and every account's balance. `extra` adds the plan's closings or the review id.
pub fn take(
    conn: &Connection,
    cmd: &CommandRecord,
    today: CivilDate,
    kind: &str,
    extra: Detail,
) -> AppResult<Snapshot> {
    if !KINDS.contains(&kind) {
        return Err(AppError::validation(
            "kind",
            "must be daily, on_demand or plan",
        ));
    }
    if kind == "daily" && daily_for(conn, today)?.is_some() {
        return Err(AppError::Conflict(format!(
            "a daily snapshot for {} already exists",
            format_civil(today)
        )));
    }
    let hero = safe::safe_to_spend(conn, today)?;
    let debts = debt::totals(conn, today)?;
    let ventures = venture::summary(conn, today)?;
    let mut accounts = Vec::new();
    for a in account::list(conn)? {
        accounts.push(AccountBalance {
            account_id: a.id,
            name: a.name.clone(),
            kind: a.kind.clone(),
            balance_cents: safe::posted_balance_as_of(conn, &a, today)?,
        });
    }
    let detail = Detail {
        accounts,
        days: extra.days,
        review_id: extra.review_id,
    };
    let detail_json = serde_json::to_string(&detail)?;
    let taken_at = now_rfc3339();
    conn.execute(
        "INSERT INTO snapshot (taken_at, civil_date, kind, safe_cents, available_cents, earmarks_cents, obligations_cents,
                               buffer_cents, trusted, total_debt_cents, informal_remaining_cents, venture_cap_used_cents, detail_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            taken_at,
            format_civil(today),
            kind,
            hero.safe_cents,
            hero.terms.available.cents,
            hero.terms.earmarks.cents,
            hero.terms.obligations.cents,
            hero.terms.buffer.cents,
            i64::from(hero.trust.hero.trusted),
            debts.total_debt_cents,
            debts.informal_remaining_cents,
            ventures.total_cap_used_cents,
            detail_json,
        ],
    )?;
    let snap = get(conn, conn.last_insert_rowid())?;
    let after = serde_json::json!({
        "id": snap.id, "kind": kind, "civil_date": snap.civil_date, "safe_cents": snap.safe_cents,
        "total_debt_cents": snap.total_debt_cents, "venture_cap_used_cents": snap.venture_cap_used_cents,
    });
    audit::record(
        conn,
        cmd,
        "snapshot",
        snap.id,
        Action::Insert,
        None,
        Some(&after),
    )?;
    Ok(snap)
}

/// The nightly snapshot, implemented as once per civil day on unlock (ADR-0027).
pub fn take_daily_if_missing(
    conn: &Connection,
    cmd: &CommandRecord,
    today: CivilDate,
) -> AppResult<Option<Snapshot>> {
    if daily_for(conn, today)?.is_some() {
        return Ok(None);
    }
    Ok(Some(take(conn, cmd, today, "daily", Detail::default())?))
}

/// One point per civil day for the trends: the latest snapshot of each day.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrendPoint {
    pub civil_date: String,
    pub kind: String,
    pub safe_cents: i64,
    pub available_cents: i64,
    pub total_debt_cents: i64,
    pub informal_remaining_cents: i64,
    pub venture_cap_used_cents: i64,
    pub trusted: bool,
}

pub fn trends(conn: &Connection) -> AppResult<Vec<TrendPoint>> {
    let mut out: Vec<TrendPoint> = Vec::new();
    for s in list(conn)? {
        let point = TrendPoint {
            civil_date: s.civil_date.clone(),
            kind: s.kind.clone(),
            safe_cents: s.safe_cents,
            available_cents: s.available_cents,
            total_debt_cents: s.total_debt_cents,
            informal_remaining_cents: s.informal_remaining_cents,
            venture_cap_used_cents: s.venture_cap_used_cents,
            trusted: s.trusted,
        };
        match out.last_mut() {
            Some(last) if last.civil_date == point.civil_date => *last = point,
            _ => out.push(point),
        }
    }
    Ok(out)
}
