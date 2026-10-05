//! Reconciliation (ARCHITECTURE §5.3, ADR-0021, ADR-0040): one statement balance per account
//! and period, roll-forward from the last balanced period, zero tolerance, the difference
//! explorer, and the trust status every figure on screen is marked with.

use chrono::Duration;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::{format_civil, now_rfc3339, parse_civil, CivilDate};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::account::{self, Account};
use crate::db::repo::batch::{self, QuarantineRow};
use crate::db::repo::ledger::{self, LedgerFilter, LedgerRow};
use crate::error::{AppError, AppResult};
use crate::money::{to_decimal_string, Cents};

/// Days on either side of a period the explorer looks at for rows that may belong to it.
pub const NEIGHBOUR_DAYS: i64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reconciliation {
    pub id: i64,
    pub account_id: i64,
    pub period_start: String,
    pub period_end: String,
    pub opening_cents: i64,
    pub statement_closing_cents: i64,
    pub statement_source: String,
    pub computed_closing_cents: i64,
    pub difference_cents: i64,
    pub status: String,
    pub balanced_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const COLS: &str = "id, account_id, period_start, period_end, opening_cents, statement_closing_cents, statement_source, computed_closing_cents, difference_cents, status, balanced_at, created_at, updated_at";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Reconciliation> {
    Ok(Reconciliation {
        id: r.get(0)?,
        account_id: r.get(1)?,
        period_start: r.get(2)?,
        period_end: r.get(3)?,
        opening_cents: r.get(4)?,
        statement_closing_cents: r.get(5)?,
        statement_source: r.get(6)?,
        computed_closing_cents: r.get(7)?,
        difference_cents: r.get(8)?,
        status: r.get(9)?,
        balanced_at: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

/// The periods of an account, earliest first.
pub fn list(conn: &Connection, account_id: i64) -> AppResult<Vec<Reconciliation>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM reconciliation WHERE account_id = ?1 ORDER BY period_end, id"
    ))?;
    let rows = stmt
        .query_map([account_id], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Reconciliation> {
    conn.query_row(
        &format!("SELECT {COLS} FROM reconciliation WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "reconciliation",
        id,
    })
}

/// Σ posted leaf rows of an account with `posted_date` in `[start, end]`.
pub fn posted_sum_between(
    conn: &Connection,
    account_id: i64,
    start: &str,
    end: &str,
) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf
         WHERE account_id = ?1 AND status = 'posted' AND posted_date BETWEEN ?2 AND ?3",
        params![account_id, start, end],
        |r| r.get(0),
    )?)
}

/// Where the period after `prior` (the last balanced period, if any) starts and opens.
struct Base {
    start: String,
    opening_cents: i64,
}

fn base_after(acct: &Account, prior: Option<&Reconciliation>) -> AppResult<Base> {
    Ok(match prior {
        Some(p) => Base {
            start: format_civil(parse_civil(&p.period_end)? + Duration::days(1)),
            opening_cents: p.statement_closing_cents,
        },
        None => Base {
            start: acct.opening_date.clone(),
            opening_cents: acct.opening_balance_cents,
        },
    })
}

/// The identity for one period: `computed = opening + Σ`, `difference = computed − statement`,
/// balanced iff the difference is exactly zero.
struct Figures {
    computed_cents: i64,
    difference_cents: i64,
    status: &'static str,
}

fn figures(
    conn: &Connection,
    account_id: i64,
    base: &Base,
    period_end: &str,
    statement_cents: i64,
) -> AppResult<Figures> {
    let sum = if base.start.as_str() <= period_end {
        posted_sum_between(conn, account_id, &base.start, period_end)?
    } else {
        0
    };
    let computed = Cents(base.opening_cents).checked_add(Cents(sum))?.0;
    let difference = Cents(computed).checked_sub(Cents(statement_cents))?.0;
    Ok(Figures {
        computed_cents: computed,
        difference_cents: difference,
        status: if difference == 0 { "balanced" } else { "off" },
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconInput {
    pub account_id: i64,
    pub period_end: String,
    pub statement_closing_cents: i64,
    pub statement_source: String,
}

/// Enter a statement balance for the period ending `period_end`. A new period follows the last
/// balanced one; an `off` period with the same end is re-entered; a balanced one is immutable
/// (delete it to redo it). Every period of the account is recomputed afterwards.
pub fn reconcile(
    conn: &Connection,
    cmd: &CommandRecord,
    input: &ReconInput,
) -> AppResult<Reconciliation> {
    let acct = account::get(conn, input.account_id)?;
    let end = parse_civil(&input.period_end)?;
    if input.statement_source != "user" && input.statement_source != "file" {
        return Err(AppError::validation(
            "statement_source",
            "must be user or file",
        ));
    }
    let periods = list(conn, acct.id)?;
    let existing = periods
        .iter()
        .find(|p| p.period_end == input.period_end)
        .cloned();
    if let Some(e) = &existing {
        if e.status == "balanced" {
            if e.statement_closing_cents == input.statement_closing_cents {
                return Ok(e.clone());
            }
            return Err(AppError::Conflict(format!(
                "the period ending {} is balanced; delete it to enter a different statement balance",
                e.period_end
            )));
        }
    }
    if let Some(later) = periods
        .iter()
        .find(|p| p.status == "balanced" && p.period_end > input.period_end)
    {
        return Err(AppError::validation(
            "period_end",
            format!(
                "a balanced period already ends later ({}); periods are entered in order",
                later.period_end
            ),
        ));
    }
    let prior = periods
        .iter()
        .filter(|p| p.status == "balanced" && p.period_end < input.period_end)
        .max_by(|a, b| a.period_end.cmp(&b.period_end))
        .cloned();
    let base = base_after(&acct, prior.as_ref())?;
    if end < parse_civil(&base.start)? {
        return Err(AppError::validation(
            "period_end",
            format!("the period would end before it starts ({})", base.start),
        ));
    }
    let f = figures(
        conn,
        acct.id,
        &base,
        &input.period_end,
        input.statement_closing_cents,
    )?;
    let now = now_rfc3339();
    let balanced_at = if f.status == "balanced" {
        Some(now.clone())
    } else {
        None
    };
    let id = match &existing {
        Some(e) => {
            conn.execute(
                "UPDATE reconciliation SET period_start = ?2, opening_cents = ?3, statement_closing_cents = ?4,
                   statement_source = ?5, computed_closing_cents = ?6, difference_cents = ?7, status = ?8,
                   balanced_at = ?9, updated_at = ?10 WHERE id = ?1",
                params![
                    e.id,
                    base.start,
                    base.opening_cents,
                    input.statement_closing_cents,
                    input.statement_source,
                    f.computed_cents,
                    f.difference_cents,
                    f.status,
                    balanced_at,
                    now
                ],
            )?;
            let after = get(conn, e.id)?;
            audit::record(
                conn,
                cmd,
                "reconciliation",
                e.id,
                Action::Update,
                Some(&serde_json::to_value(e)?),
                Some(&serde_json::to_value(&after)?),
            )?;
            e.id
        }
        None => {
            conn.execute(
                "INSERT INTO reconciliation (account_id, period_start, period_end, opening_cents, statement_closing_cents,
                   statement_source, computed_closing_cents, difference_cents, status, balanced_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
                params![
                    acct.id,
                    base.start,
                    input.period_end,
                    base.opening_cents,
                    input.statement_closing_cents,
                    input.statement_source,
                    f.computed_cents,
                    f.difference_cents,
                    f.status,
                    balanced_at,
                    now
                ],
            )?;
            let id = conn.last_insert_rowid();
            let after = get(conn, id)?;
            audit::record(
                conn,
                cmd,
                "reconciliation",
                id,
                Action::Insert,
                None,
                Some(&serde_json::to_value(&after)?),
            )?;
            id
        }
    };
    refresh_account(conn, cmd, acct.id)?;
    get(conn, id)
}

/// Remove the latest period of an account; later periods roll forward from earlier ones, so
/// an earlier period cannot go while a later one stands.
pub fn delete(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let r = get(conn, id)?;
    let periods = list(conn, r.account_id)?;
    if let Some(later) = periods.iter().find(|p| p.period_end > r.period_end) {
        return Err(AppError::validation(
            "id",
            format!(
                "the period ending {} rolls forward from this one; delete it first",
                later.period_end
            ),
        ));
    }
    audit::record(
        conn,
        cmd,
        "reconciliation",
        id,
        Action::Delete,
        Some(&serde_json::to_value(&r)?),
        None,
    )?;
    conn.execute("DELETE FROM reconciliation WHERE id = ?1", [id])?;
    refresh_account(conn, cmd, r.account_id)?;
    Ok(())
}

/// Recompute every period of an account against the ledger as it is now, in order, each one
/// rolling forward from the last balanced period before it. Returns how many rows changed.
pub fn refresh_account(
    conn: &Connection,
    cmd: &CommandRecord,
    account_id: i64,
) -> AppResult<usize> {
    let acct = account::get(conn, account_id)?;
    let mut prior: Option<Reconciliation> = None;
    let mut changed = 0;
    for p in list(conn, account_id)? {
        let base = base_after(&acct, prior.as_ref())?;
        let f = figures(
            conn,
            account_id,
            &base,
            &p.period_end,
            p.statement_closing_cents,
        )?;
        let mut after = p.clone();
        after.period_start = base.start;
        after.opening_cents = base.opening_cents;
        after.computed_closing_cents = f.computed_cents;
        after.difference_cents = f.difference_cents;
        after.status = f.status.into();
        if f.status == "balanced" {
            if after.balanced_at.is_none() {
                after.balanced_at = Some(now_rfc3339());
            }
        } else {
            after.balanced_at = None;
        }
        if after != p {
            after.updated_at = now_rfc3339();
            conn.execute(
                "UPDATE reconciliation SET period_start = ?2, opening_cents = ?3, computed_closing_cents = ?4,
                   difference_cents = ?5, status = ?6, balanced_at = ?7, updated_at = ?8 WHERE id = ?1",
                params![
                    after.id,
                    after.period_start,
                    after.opening_cents,
                    after.computed_closing_cents,
                    after.difference_cents,
                    after.status,
                    after.balanced_at,
                    after.updated_at
                ],
            )?;
            audit::record(
                conn,
                cmd,
                "reconciliation",
                p.id,
                Action::Update,
                Some(&serde_json::to_value(&p)?),
                Some(&serde_json::to_value(&after)?),
            )?;
            changed += 1;
        }
        if after.status == "balanced" {
            prior = Some(after);
        }
    }
    Ok(changed)
}

/// Refresh every account that has periods; every writing transaction calls this before it
/// commits, so a cached status is never stale (ARCHITECTURE §5.3).
pub fn refresh_all(conn: &Connection, cmd: &CommandRecord) -> AppResult<usize> {
    let mut stmt =
        conn.prepare("SELECT DISTINCT account_id FROM reconciliation ORDER BY account_id")?;
    let ids: Vec<i64> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut changed = 0;
    for id in ids {
        changed += refresh_account(conn, cmd, id)?;
    }
    Ok(changed)
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplorerRow {
    #[serde(flatten)]
    pub row: LedgerRow,
    pub running_cents: i64,
}

/// What could explain a difference: the period's posted rows with a running balance, posted
/// rows just outside it, pending rows, and rows still held in quarantine for the account.
#[derive(Debug, Clone, Serialize)]
pub struct DifferenceExplorer {
    pub reconciliation: Reconciliation,
    pub in_period: Vec<ExplorerRow>,
    pub before: Vec<LedgerRow>,
    pub after: Vec<LedgerRow>,
    pub pending: Vec<LedgerRow>,
    pub quarantine: Vec<QuarantineRow>,
    pub neighbour_days: i64,
}

fn range_filter(account_id: i64, from: CivilDate, to: CivilDate, status: &str) -> LedgerFilter {
    LedgerFilter {
        account_ids: vec![account_id],
        date_from: Some(format_civil(from)),
        date_to: Some(format_civil(to)),
        status: Some(status.to_string()),
        ..Default::default()
    }
}

pub fn explorer(conn: &Connection, id: i64) -> AppResult<DifferenceExplorer> {
    let r = get(conn, id)?;
    let start = parse_civil(&r.period_start)?;
    let end = parse_civil(&r.period_end)?;
    let rows = ledger::rows_asc(
        conn,
        &range_filter(r.account_id, start, end, "posted"),
        50_000,
    )?;
    let mut running = Cents(r.opening_cents);
    let mut in_period = Vec::with_capacity(rows.len());
    for row in rows {
        running = running.checked_add(Cents(row.amount_cents))?;
        in_period.push(ExplorerRow {
            row,
            running_cents: running.0,
        });
    }
    let before = ledger::rows_asc(
        conn,
        &range_filter(
            r.account_id,
            start - Duration::days(NEIGHBOUR_DAYS),
            start - Duration::days(1),
            "posted",
        ),
        1000,
    )?;
    let after = ledger::rows_asc(
        conn,
        &range_filter(
            r.account_id,
            end + Duration::days(1),
            end + Duration::days(NEIGHBOUR_DAYS),
            "posted",
        ),
        1000,
    )?;
    let pending = ledger::rows_asc(
        conn,
        &LedgerFilter {
            account_ids: vec![r.account_id],
            status: Some("pending".into()),
            ..Default::default()
        },
        1000,
    )?;
    let quarantine = batch::quarantine_pending_for_account(conn, r.account_id)?;
    Ok(DifferenceExplorer {
        reconciliation: r,
        in_period,
        before,
        after,
        pending,
        quarantine,
        neighbour_days: NEIGHBOUR_DAYS,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountTrust {
    pub account_id: i64,
    pub account_name: String,
    pub kind: String,
    /// Whether the account is in the hero's `available` set.
    pub contributes: bool,
    /// `reconciled | never_reconciled | stale | off`.
    pub status: String,
    pub latest_period_end: Option<String>,
    pub difference_cents: Option<i64>,
    pub days_since: Option<i64>,
    pub stale_after_days: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UntrustedAccount {
    pub account_id: i64,
    pub account_name: String,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeroTrust {
    pub trusted: bool,
    pub untrusted: Vec<UntrustedAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustReport {
    pub as_of: String,
    pub accounts: Vec<AccountTrust>,
    pub hero: HeroTrust,
}

/// Whether an account is in the hero's `available` set: the one definition `safe::contributes`
/// holds (cash kind, personal, not firewalled, not archived), so trust is judged over exactly
/// the accounts the figure sums.
pub fn contributes(acct: &Account) -> bool {
    super::safe::contributes(acct)
}

/// Trust per account as of `today`: `reconciled` iff the latest period is balanced and ended
/// within the stale window (account override, else the setting). The hero is trusted iff there
/// is at least one contributing account and every contributing account is reconciled.
pub fn trust(
    conn: &Connection,
    today: CivilDate,
    default_stale_days: i64,
) -> AppResult<TrustReport> {
    let mut accounts = Vec::new();
    let mut contributing = 0usize;
    for acct in account::list(conn)? {
        let stale_after = acct.recon_stale_after_days.unwrap_or(default_stale_days);
        let periods = list(conn, acct.id)?;
        let latest = periods.last();
        let (status, reason, days_since) = match latest {
            None => (
                "never_reconciled",
                "no statement balance entered yet".to_string(),
                None,
            ),
            Some(p) if p.status == "off" => (
                "off",
                format!(
                    "the period ending {} is off by {}",
                    p.period_end,
                    to_decimal_string(p.difference_cents)
                ),
                None,
            ),
            Some(p) => {
                let days = (today - parse_civil(&p.period_end)?).num_days();
                if days > stale_after {
                    (
                        "stale",
                        format!(
                            "last balanced through {}, {} days ago (window {} days)",
                            p.period_end, days, stale_after
                        ),
                        Some(days),
                    )
                } else {
                    (
                        "reconciled",
                        format!("balanced through {}, {} days ago", p.period_end, days),
                        Some(days),
                    )
                }
            }
        };
        let contributes = contributes(&acct);
        if contributes {
            contributing += 1;
        }
        accounts.push(AccountTrust {
            account_id: acct.id,
            account_name: acct.name.clone(),
            kind: acct.kind.clone(),
            contributes,
            status: status.into(),
            latest_period_end: latest.map(|p| p.period_end.clone()),
            difference_cents: latest.map(|p| p.difference_cents),
            days_since,
            stale_after_days: stale_after,
            reason,
        });
    }
    let untrusted: Vec<UntrustedAccount> = accounts
        .iter()
        .filter(|a| a.contributes && a.status != "reconciled")
        .map(|a| UntrustedAccount {
            account_id: a.account_id,
            account_name: a.account_name.clone(),
            status: a.status.clone(),
            reason: a.reason.clone(),
        })
        .collect();
    Ok(TrustReport {
        as_of: format_civil(today),
        accounts,
        hero: HeroTrust {
            trusted: contributing > 0 && untrusted.is_empty(),
            untrusted,
        },
    })
}
