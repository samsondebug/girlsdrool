//! The weekly review (ARCHITECTURE §5.10, ADR-0026, ADR-0045): a mode that walks the steps,
//! states the dependable surplus with its terms, and completes only with exactly three non-empty
//! actions, enforced in the completing transaction. Every figure a step shows is stored with the
//! review, so history is what the person saw, not what the engines would say today.

pub mod snapshot;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::cash::{recon, safe};
use crate::dates::{format_civil, now_rfc3339, parse_civil, CivilDate};
use crate::db::audit::{self, Action as AuditAction, CommandRecord};
use crate::db::repo::{account, ledger};
use crate::db::settings;
use crate::debt::{self, informal};
use crate::error::{AppError, AppResult};
use crate::forecast;
use crate::import::csv::{FLAG_BORROWING, FLAG_SECURITIES_SALE};
use crate::money::{mul_div_round, Cents};
use crate::plan::{earmark, income, obligation};
use crate::venture;

pub const STATUSES: [&str; 3] = ["in_progress", "completed", "abandoned"];
/// A review commits exactly this many actions.
pub const ACTIONS: usize = 3;
pub const SURPLUS_INCOME_DAYS: i64 = 90;
pub const HORIZON_DAYS: i64 = 14;
const UNREVIEWED_SHOWN: usize = 25;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Term {
    pub name: String,
    pub monthly_cents: i64,
}

/// Monthly-equivalent surplus from rows only (§5.10).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Surplus {
    pub as_of: String,
    pub income_window_days: i64,
    pub income_window_cents: i64,
    pub income_receipts: usize,
    pub income_cents: i64,
    pub fixed_cents: i64,
    pub fixed_items: Vec<Term>,
    pub debt_service_cents: i64,
    pub debt_items: Vec<Term>,
    pub informal_schedule_12m_cents: i64,
    pub irregular_cents: i64,
    pub irregular_items: Vec<Term>,
    pub variable_cents: i64,
    pub surplus_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RowRef {
    pub txn_id: i64,
    pub account_name: String,
    pub posted_date: String,
    pub payee: String,
    pub amount_cents: i64,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BalanceLine {
    pub account_id: i64,
    pub name: String,
    pub kind: String,
    pub balance_cents: i64,
    pub trust: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BalancesStep {
    pub accounts: Vec<BalanceLine>,
    pub available_cents: i64,
    pub trusted: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnreviewedStep {
    pub count: usize,
    pub total_abs_cents: i64,
    pub rows: Vec<RowRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DueItem {
    pub obligation_id: i64,
    pub name: String,
    pub due_date: String,
    pub expected_cents: i64,
    pub overdue: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObligationsStep {
    pub count: usize,
    pub expected_cents: i64,
    pub items: Vec<DueItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanVarianceStep {
    pub plan_snapshot_id: Option<i64>,
    pub plan_date: Option<String>,
    pub plan_cents: Option<i64>,
    pub actual_cents: i64,
    pub variance_cents: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebtsStep {
    pub total_debt_cents: i64,
    pub informal_remaining_cents: i64,
    pub previous_review_id: Option<i64>,
    pub previous_total_debt_cents: Option<i64>,
    pub previous_informal_cents: Option<i64>,
    /// `total − previous_total` when there is a previous review.
    pub total_debt_delta_cents: Option<i64>,
    /// `informal_remaining − previous_informal` when there is a previous review.
    pub informal_delta_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VentureLine {
    pub venture_id: i64,
    pub name: String,
    pub status: String,
    pub cap_used_cents: i64,
    pub cap_cents: i64,
    pub utilization_bps: i64,
    pub alerts: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VenturesStep {
    pub cap_used_cents: i64,
    pub cap_cents: i64,
    pub ventures: Vec<VentureLine>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlagsStep {
    pub since: String,
    pub borrowing: Vec<RowRef>,
    pub securities_sale: Vec<RowRef>,
    pub firewall_unacknowledged: Vec<RowRef>,
    pub firewall_acknowledged: Vec<RowRef>,
}

/// What the walk showed, in order.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Steps {
    pub balances: BalancesStep,
    pub unreviewed: UnreviewedStep,
    pub obligations_14: ObligationsStep,
    pub plan_variance: PlanVarianceStep,
    pub debts: DebtsStep,
    pub ventures: VenturesStep,
    pub flags: FlagsStep,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewAction {
    pub id: i64,
    pub review_id: i64,
    pub position: i64,
    pub text: String,
    pub done: bool,
    pub done_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Review {
    pub id: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String,
    pub period_start: String,
    pub period_end: String,
    pub surplus_cents: Option<i64>,
    pub surplus: Option<Surplus>,
    pub steps: Steps,
    pub notes: String,
    pub actions: Vec<ReviewAction>,
    pub snapshot_id: Option<i64>,
}

// ---- surplus ----------------------------------------------------------------------------------

/// Monthly rules ×1, biweekly ×26/12, weekly ×52/12; annual and once are irregular, not fixed.
fn monthly_equivalent(ob: &obligation::Obligation) -> AppResult<i64> {
    Ok(match ob.due_rule.as_str() {
        "monthly_day" | "nth_weekday" => ob.expected_cents,
        "biweekly" => mul_div_round(i128::from(ob.expected_cents), 26, 12)?,
        "weekly" => mul_div_round(i128::from(ob.expected_cents), 52, 12)?,
        _ => 0,
    })
}

/// A sinking fund's schedule, monthly equivalent: per paycheck by its stream's cycle, monthly
/// as is, by date the gap spread over the months left.
fn sinking_monthly(conn: &Connection, em: &earmark::Earmark, today: CivilDate) -> AppResult<i64> {
    match em.schedule.as_str() {
        "per_paycheck" => {
            let (Some(stream_id), Some(amount)) =
                (em.schedule_income_stream_id, em.schedule_amount_cents)
            else {
                return Ok(0);
            };
            let per_year: i64 = match income::get(conn, stream_id)?.cycle.as_str() {
                "weekly" => 52,
                "biweekly" => 26,
                "semimonthly" => 24,
                "monthly" => 12,
                _ => 0,
            };
            mul_div_round(i128::from(amount), i128::from(per_year), 12)
        }
        "monthly" => Ok(em.schedule_amount_cents.unwrap_or(0)),
        "by_date" => {
            let Some(target) = &em.target_date else {
                return Ok(0);
            };
            let gap = Cents(em.target_cents)
                .checked_sub(Cents(earmark::remaining(conn, em.id, today)?.max(0)))?
                .0
                .max(0);
            let days = (parse_civil(target)? - today).num_days().max(1);
            let months = (days + 29) / 30;
            mul_div_round(i128::from(gap), 1, i128::from(months.max(1)))
        }
        _ => Ok(0),
    }
}

/// The dependable surplus as of `today` (§5.10): borrowing and asset sales cannot enter because
/// only `income_receipt` rows of confirmed streams count as income.
pub fn surplus(conn: &Connection, today: CivilDate) -> AppResult<Surplus> {
    let from = today - chrono::Duration::days(SURPLUS_INCOME_DAYS);
    let (income_window_cents, income_receipts): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(t.amount_cents), 0), COUNT(*) FROM income_receipt r
         JOIN txn t ON t.id = r.txn_id
         JOIN income_stream s ON s.id = r.income_stream_id
         WHERE s.confidence = 'confirmed' AND t.posted_date > ?1 AND t.posted_date <= ?2",
        params![format_civil(from), format_civil(today)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let income_cents = mul_div_round(
        i128::from(income_window_cents),
        30,
        i128::from(SURPLUS_INCOME_DAYS),
    )?;
    let mut fixed_items = Vec::new();
    let mut debt_items = Vec::new();
    let mut irregular_items = Vec::new();
    for ob in obligation::list(conn)?
        .into_iter()
        .filter(|o| o.status == "confirmed")
    {
        if ob.due_rule == "annual" {
            irregular_items.push(Term {
                name: ob.name.clone(),
                monthly_cents: mul_div_round(i128::from(ob.expected_cents), 1, 12)?,
            });
        } else if ob.due_rule == "once" {
            continue;
        } else if ob.kind == "debt_minimum" {
            debt_items.push(Term {
                name: ob.name.clone(),
                monthly_cents: monthly_equivalent(&ob)?,
            });
        } else {
            fixed_items.push(Term {
                name: ob.name.clone(),
                monthly_cents: monthly_equivalent(&ob)?,
            });
        }
    }
    for em in earmark::list(conn)?
        .into_iter()
        .filter(|e| e.active && e.kind == "sinking_fund")
    {
        let monthly = sinking_monthly(conn, &em, today)?;
        if monthly > 0 {
            irregular_items.push(Term {
                name: format!("{} (sinking fund)", em.name),
                monthly_cents: monthly,
            });
        }
    }
    let mut informal_12m = Cents::ZERO;
    for (_, _, _, cents) in
        informal::unpaid_due_between(conn, today, today, today + chrono::Duration::days(365))?
    {
        informal_12m = informal_12m.checked_add(Cents(cents))?;
    }
    let sum = |items: &[Term]| -> AppResult<i64> {
        let mut total = Cents::ZERO;
        for t in items {
            total = total.checked_add(Cents(t.monthly_cents))?;
        }
        Ok(total.0)
    };
    let fixed_cents = sum(&fixed_items)?;
    let debt_service_cents = Cents(sum(&debt_items)?)
        .checked_add(Cents(mul_div_round(i128::from(informal_12m.0), 1, 12)?))?
        .0;
    let irregular_cents = sum(&irregular_items)?;
    let mut variable = Cents::ZERO;
    for m in forecast::variable::model(conn, today)? {
        variable = variable.checked_add(Cents(m.per_30_days_cents))?;
    }
    let surplus_cents = Cents(income_cents)
        .checked_sub(Cents(fixed_cents))?
        .checked_sub(Cents(debt_service_cents))?
        .checked_sub(Cents(irregular_cents))?
        .checked_sub(variable)?
        .0;
    Ok(Surplus {
        as_of: format_civil(today),
        income_window_days: SURPLUS_INCOME_DAYS,
        income_window_cents,
        income_receipts: usize::try_from(income_receipts).unwrap_or(0),
        income_cents,
        fixed_cents,
        fixed_items,
        debt_service_cents,
        debt_items,
        informal_schedule_12m_cents: informal_12m.0,
        irregular_cents,
        irregular_items,
        variable_cents: variable.0,
        surplus_cents,
    })
}

// ---- steps --------------------------------------------------------------------------------------

fn row_ref(r: &ledger::LedgerRow, why: String) -> RowRef {
    RowRef {
        txn_id: r.id,
        account_name: r.account_name.clone(),
        posted_date: r.posted_date.clone(),
        payee: r.payee_raw.clone(),
        amount_cents: r.amount_cents,
        why,
    }
}

fn flagged_rows(conn: &Connection, since: &str, bits: u32) -> AppResult<Vec<RowRef>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, a.name, t.posted_date, t.payee_raw, t.amount_cents FROM txn_leaf t JOIN account a ON a.id = t.account_id
         WHERE (t.flags & ?1) <> 0 AND t.posted_date >= ?2 ORDER BY t.posted_date, t.id",
    )?;
    let rows = stmt
        .query_map(params![i64::from(bits), since], |r| {
            Ok(RowRef {
                txn_id: r.get(0)?,
                account_name: r.get(1)?,
                posted_date: r.get(2)?,
                payee: r.get(3)?,
                amount_cents: r.get(4)?,
                why: String::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn firewall_rows(conn: &Connection, since: &str, acknowledged: bool) -> AppResult<Vec<RowRef>> {
    let sql = if acknowledged {
        "SELECT t.id, a.name, t.posted_date, t.payee_raw, t.amount_cents, f.note FROM firewall_ack f
         JOIN txn_leaf t ON t.id = f.txn_id JOIN account a ON a.id = t.account_id
         WHERE substr(f.acknowledged_at, 1, 10) >= ?1 ORDER BY f.acknowledged_at, t.id"
    } else {
        "SELECT t.id, a.name, t.posted_date, t.payee_raw, t.amount_cents, '' FROM txn_leaf t JOIN account a ON a.id = t.account_id
         WHERE a.firewalled = 1 AND t.amount_cents < 0 AND t.posted_date >= ?1
           AND NOT EXISTS (SELECT 1 FROM firewall_ack f WHERE f.txn_id = t.id)
         ORDER BY t.posted_date, t.id"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([since], |r| {
            Ok(RowRef {
                txn_id: r.get(0)?,
                account_name: r.get(1)?,
                posted_date: r.get(2)?,
                payee: r.get(3)?,
                amount_cents: r.get(4)?,
                why: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Everything the walk shows as of `today`, with "since" = `period_start`.
pub fn steps(
    conn: &Connection,
    today: CivilDate,
    period_start: &str,
    previous: Option<&Review>,
) -> AppResult<Steps> {
    let cfg = settings::load(conn)?;
    let trust = recon::trust(conn, today, cfg.recon_stale_after_days)?;
    let hero = safe::safe_to_spend(conn, today)?;
    let mut accounts = Vec::new();
    for a in account::list(conn)?.into_iter().filter(|a| !a.archived) {
        let status = trust
            .accounts
            .iter()
            .find(|t| t.account_id == a.id)
            .map_or("never_reconciled".to_string(), |t| t.status.clone());
        accounts.push(BalanceLine {
            account_id: a.id,
            name: a.name.clone(),
            kind: a.kind.clone(),
            balance_cents: safe::posted_balance_as_of(conn, &a, today)?,
            trust: status,
        });
    }
    let queue = ledger::review_queue(conn, 5000)?;
    let mut total_abs = Cents::ZERO;
    for r in &queue {
        total_abs = total_abs.checked_add(Cents(r.amount_cents).checked_abs()?)?;
    }
    let unreviewed = UnreviewedStep {
        count: queue.len(),
        total_abs_cents: total_abs.0,
        rows: queue
            .iter()
            .take(UNREVIEWED_SHOWN)
            .map(|r| {
                row_ref(
                    r,
                    r.heuristic_code
                        .clone()
                        .map_or_else(|| r.classification.clone(), |h| format!("heuristic:{h}")),
                )
            })
            .collect(),
    };
    let up = safe::upcoming(conn, today, HORIZON_DAYS)?;
    let mut expected = Cents::ZERO;
    let mut items = Vec::new();
    for o in &up.obligations {
        expected = expected.checked_add(Cents(o.expected_cents))?;
        items.push(DueItem {
            obligation_id: o.obligation_id,
            name: o.name.clone(),
            due_date: o.due_date.clone(),
            expected_cents: o.expected_cents,
            overdue: o.overdue,
        });
    }
    let obligations_14 = ObligationsStep {
        count: items.len(),
        expected_cents: expected.0,
        items,
    };
    // the plan's balance entering today is its closing for yesterday; on the plan's first day
    // there is nothing to compare yet
    let plan = forecast::latest_plan(conn)?;
    let yesterday = format_civil(today - chrono::Duration::days(1));
    let plan_cents = plan
        .as_ref()
        .and_then(|p| p.days.iter().find(|d| d.date == yesterday).map(|d| d.cents));
    let plan_variance = PlanVarianceStep {
        plan_snapshot_id: plan.as_ref().map(|p| p.snapshot_id),
        plan_date: plan.as_ref().map(|p| p.civil_date.clone()),
        plan_cents,
        actual_cents: hero.terms.available.cents,
        variance_cents: match plan_cents {
            Some(c) => Some(Cents(hero.terms.available.cents).checked_sub(Cents(c))?.0),
            None => None,
        },
    };
    let totals = debt::totals(conn, today)?;
    let previous_total_debt_cents = previous.map(|p| p.steps.debts.total_debt_cents);
    let previous_informal_cents = previous.map(|p| p.steps.debts.informal_remaining_cents);
    let debts = DebtsStep {
        total_debt_cents: totals.total_debt_cents,
        informal_remaining_cents: totals.informal_remaining_cents,
        previous_review_id: previous.map(|p| p.id),
        previous_total_debt_cents,
        previous_informal_cents,
        total_debt_delta_cents: match previous_total_debt_cents {
            Some(c) => Some(Cents(totals.total_debt_cents).checked_sub(Cents(c))?.0),
            None => None,
        },
        informal_delta_cents: match previous_informal_cents {
            Some(c) => Some(
                Cents(totals.informal_remaining_cents)
                    .checked_sub(Cents(c))?
                    .0,
            ),
            None => None,
        },
    };
    let summary = venture::summary(conn, today)?;
    let ventures = VenturesStep {
        cap_used_cents: summary.total_cap_used_cents,
        cap_cents: summary.total_cap_cents,
        ventures: summary
            .ventures
            .iter()
            .map(|v| VentureLine {
                venture_id: v.venture.id,
                name: v.venture.name.clone(),
                status: v.venture.status.clone(),
                cap_used_cents: v.cap_used_cents,
                cap_cents: v.venture.cash_cap_cents,
                utilization_bps: v.cap_utilization_bps,
                alerts: v.alerts.clone(),
            })
            .collect(),
    };
    let flags = FlagsStep {
        since: period_start.to_string(),
        borrowing: flagged_rows(conn, period_start, FLAG_BORROWING)?,
        securities_sale: flagged_rows(conn, period_start, FLAG_SECURITIES_SALE)?,
        firewall_unacknowledged: firewall_rows(conn, period_start, false)?,
        firewall_acknowledged: firewall_rows(conn, period_start, true)?,
    };
    Ok(Steps {
        balances: BalancesStep {
            accounts,
            available_cents: hero.terms.available.cents,
            trusted: hero.trust.hero.trusted,
        },
        unreviewed,
        obligations_14,
        plan_variance,
        debts,
        ventures,
        flags,
    })
}

// ---- reviews ------------------------------------------------------------------------------------

const COLS: &str = "id, started_at, completed_at, status, period_start, period_end, surplus_cents, surplus_detail_json, steps_json, notes";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Review> {
    let surplus: Option<String> = r.get(7)?;
    let steps: String = r.get(8)?;
    Ok(Review {
        id: r.get(0)?,
        started_at: r.get(1)?,
        completed_at: r.get(2)?,
        status: r.get(3)?,
        period_start: r.get(4)?,
        period_end: r.get(5)?,
        surplus_cents: r.get(6)?,
        surplus: surplus.and_then(|s| serde_json::from_str(&s).ok()),
        steps: serde_json::from_str(&steps).unwrap_or_default(),
        notes: r.get(9)?,
        actions: Vec::new(),
        snapshot_id: None,
    })
}

fn actions_of(conn: &Connection, review_id: i64) -> AppResult<Vec<ReviewAction>> {
    let mut stmt = conn.prepare(
        "SELECT id, review_id, position, text, done, done_at FROM review_action WHERE review_id = ?1 ORDER BY position",
    )?;
    let rows = stmt
        .query_map([review_id], |r| {
            Ok(ReviewAction {
                id: r.get(0)?,
                review_id: r.get(1)?,
                position: r.get(2)?,
                text: r.get(3)?,
                done: r.get::<_, i64>(4)? == 1,
                done_at: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn snapshot_of(conn: &Connection, review_id: i64) -> AppResult<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM snapshot WHERE kind = 'plan' AND json_extract(detail_json, '$.review_id') = ?1 ORDER BY id DESC LIMIT 1",
            [review_id],
            |r| r.get(0),
        )
        .optional()?)
}

fn hydrate(conn: &Connection, mut review: Review) -> AppResult<Review> {
    review.actions = actions_of(conn, review.id)?;
    review.snapshot_id = snapshot_of(conn, review.id)?;
    Ok(review)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Review> {
    let review = conn
        .query_row(
            &format!("SELECT {COLS} FROM review WHERE id = ?1"),
            [id],
            from_row,
        )
        .optional()?
        .ok_or(AppError::NotFound {
            entity: "review",
            id,
        })?;
    hydrate(conn, review)
}

/// Newest first, with actions.
pub fn list(conn: &Connection) -> AppResult<Vec<Review>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM review ORDER BY started_at DESC, id DESC"
    ))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(|r| hydrate(conn, r)).collect()
}

pub fn current(conn: &Connection) -> AppResult<Option<Review>> {
    let review = conn
        .query_row(
            &format!(
                "SELECT {COLS} FROM review WHERE status = 'in_progress' ORDER BY id DESC LIMIT 1"
            ),
            [],
            from_row,
        )
        .optional()?;
    review.map(|r| hydrate(conn, r)).transpose()
}

pub fn last_completed(conn: &Connection) -> AppResult<Option<Review>> {
    let review = conn
        .query_row(
            &format!("SELECT {COLS} FROM review WHERE status = 'completed' ORDER BY completed_at DESC, id DESC LIMIT 1"),
            [],
            from_row,
        )
        .optional()?;
    review.map(|r| hydrate(conn, r)).transpose()
}

fn review_json(review: &Review) -> serde_json::Value {
    serde_json::json!({
        "id": review.id, "status": review.status, "period_start": review.period_start, "period_end": review.period_end,
        "surplus_cents": review.surplus_cents, "completed_at": review.completed_at, "notes": review.notes,
    })
}

fn store_figures(conn: &Connection, id: i64, steps: &Steps, surplus: &Surplus) -> AppResult<()> {
    conn.execute(
        "UPDATE review SET surplus_cents = ?2, surplus_detail_json = ?3, steps_json = ?4 WHERE id = ?1",
        params![
            id,
            surplus.surplus_cents,
            serde_json::to_string(surplus)?,
            serde_json::to_string(steps)?
        ],
    )?;
    Ok(())
}

/// Start the review: the period runs from the last completed review's end (else the first row
/// in the ledger, else today) to today. One review is in progress at a time.
pub fn start(conn: &Connection, cmd: &CommandRecord, today: CivilDate) -> AppResult<Review> {
    if let Some(open) = current(conn)? {
        return Err(AppError::Conflict(format!(
            "review {} is still in progress; complete or abandon it first",
            open.id
        )));
    }
    let previous = last_completed(conn)?;
    let period_start = match &previous {
        Some(p) => p.period_end.clone(),
        None => conn
            .query_row("SELECT MIN(posted_date) FROM txn", [], |r| {
                r.get::<_, Option<String>>(0)
            })?
            .unwrap_or_else(|| format_civil(today)),
    };
    let period_end = format_civil(today);
    conn.execute(
        "INSERT INTO review (started_at, completed_at, status, period_start, period_end, surplus_cents, surplus_detail_json, steps_json, notes)
         VALUES (?1, NULL, 'in_progress', ?2, ?3, NULL, NULL, '{}', '')",
        params![now_rfc3339(), period_start, period_end],
    )?;
    let id = conn.last_insert_rowid();
    let steps = steps(conn, today, &period_start, previous.as_ref())?;
    let surplus = surplus(conn, today)?;
    store_figures(conn, id, &steps, &surplus)?;
    let review = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "review",
        id,
        AuditAction::Insert,
        None,
        Some(&review_json(&review)),
    )?;
    Ok(review)
}

fn in_progress(conn: &Connection, id: i64) -> AppResult<Review> {
    let review = get(conn, id)?;
    if review.status != "in_progress" {
        return Err(AppError::Conflict(format!(
            "review {id} is {}; start a new one",
            review.status
        )));
    }
    Ok(review)
}

/// Recompute the steps and the surplus for the review in progress (its period end moves to today).
pub fn refresh(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    today: CivilDate,
) -> AppResult<Review> {
    let before = in_progress(conn, id)?;
    let previous = last_completed(conn)?;
    let steps = steps(conn, today, &before.period_start, previous.as_ref())?;
    let surplus = surplus(conn, today)?;
    conn.execute(
        "UPDATE review SET period_end = ?2 WHERE id = ?1",
        params![id, format_civil(today)],
    )?;
    store_figures(conn, id, &steps, &surplus)?;
    let after = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "review",
        id,
        AuditAction::Update,
        Some(&review_json(&before)),
        Some(&review_json(&after)),
    )?;
    Ok(after)
}

fn trimmed(actions: &[String]) -> Vec<String> {
    actions
        .iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect()
}

fn replace_actions(
    conn: &Connection,
    cmd: &CommandRecord,
    review_id: i64,
    actions: &[String],
) -> AppResult<()> {
    for old in actions_of(conn, review_id)? {
        conn.execute("DELETE FROM review_action WHERE id = ?1", [old.id])?;
        audit::record(
            conn,
            cmd,
            "review_action",
            old.id,
            AuditAction::Delete,
            Some(&serde_json::to_value(&old)?),
            None,
        )?;
    }
    for (i, text) in actions.iter().enumerate() {
        let position = i64::try_from(i + 1).map_err(|_| AppError::Overflow)?;
        conn.execute(
            "INSERT INTO review_action (review_id, position, text, done, done_at) VALUES (?1, ?2, ?3, 0, NULL)",
            params![review_id, position, text],
        )?;
        let id = conn.last_insert_rowid();
        audit::record(
            conn,
            cmd,
            "review_action",
            id,
            AuditAction::Insert,
            None,
            Some(
                &serde_json::json!({ "id": id, "review_id": review_id, "position": position, "text": text }),
            ),
        )?;
    }
    Ok(())
}

/// Save the draft actions (up to three) of the review in progress.
pub fn set_actions(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    actions: &[String],
) -> AppResult<Review> {
    in_progress(conn, id)?;
    let kept = trimmed(actions);
    if kept.len() > ACTIONS {
        return Err(AppError::validation(
            "actions",
            format!("a review carries exactly {ACTIONS} actions"),
        ));
    }
    replace_actions(conn, cmd, id, &kept)?;
    get(conn, id)
}

/// Complete the review with exactly three non-empty actions, in this transaction: the steps and
/// the surplus are recomputed and stored, the actions replace the draft, and a `plan` snapshot
/// is taken. Two or four actions leave nothing changed.
pub fn complete(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    actions: &[String],
    notes: &str,
    today: CivilDate,
) -> AppResult<Review> {
    let before = in_progress(conn, id)?;
    let kept = trimmed(actions);
    if kept.len() != ACTIONS {
        return Err(AppError::validation(
            "actions",
            format!(
                "a review completes with exactly {ACTIONS} actions; {} given",
                kept.len()
            ),
        ));
    }
    let previous = last_completed(conn)?;
    let steps = steps(conn, today, &before.period_start, previous.as_ref())?;
    let surplus = surplus(conn, today)?;
    store_figures(conn, id, &steps, &surplus)?;
    replace_actions(conn, cmd, id, &kept)?;
    conn.execute(
        "UPDATE review SET status = 'completed', completed_at = ?2, period_end = ?3, notes = ?4 WHERE id = ?1",
        params![id, now_rfc3339(), format_civil(today), notes.trim()],
    )?;
    let baseline = forecast::run(conn, today, &forecast::Scenario::default())?;
    snapshot::take(
        conn,
        cmd,
        today,
        "plan",
        snapshot::Detail {
            accounts: Vec::new(),
            days: baseline
                .days
                .iter()
                .map(|d| forecast::Point {
                    date: d.date.clone(),
                    cents: d.closing_cents,
                })
                .collect(),
            review_id: Some(id),
        },
    )?;
    let after = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "review",
        id,
        AuditAction::Update,
        Some(&review_json(&before)),
        Some(&review_json(&after)),
    )?;
    Ok(after)
}

pub fn abandon(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<Review> {
    let before = in_progress(conn, id)?;
    conn.execute(
        "UPDATE review SET status = 'abandoned', completed_at = ?2 WHERE id = ?1",
        params![id, now_rfc3339()],
    )?;
    let after = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "review",
        id,
        AuditAction::Update,
        Some(&review_json(&before)),
        Some(&review_json(&after)),
    )?;
    Ok(after)
}

/// Tick (or untick) one committed action.
pub fn set_action_done(
    conn: &Connection,
    cmd: &CommandRecord,
    action_id: i64,
    done: bool,
) -> AppResult<ReviewAction> {
    let before: Option<ReviewAction> = conn
        .query_row(
            "SELECT id, review_id, position, text, done, done_at FROM review_action WHERE id = ?1",
            [action_id],
            |r| {
                Ok(ReviewAction {
                    id: r.get(0)?,
                    review_id: r.get(1)?,
                    position: r.get(2)?,
                    text: r.get(3)?,
                    done: r.get::<_, i64>(4)? == 1,
                    done_at: r.get(5)?,
                })
            },
        )
        .optional()?;
    let Some(before) = before else {
        return Err(AppError::NotFound {
            entity: "review_action",
            id: action_id,
        });
    };
    if before.done == done {
        return Ok(before);
    }
    conn.execute(
        "UPDATE review_action SET done = ?2, done_at = ?3 WHERE id = ?1",
        params![action_id, i64::from(done), done.then(now_rfc3339)],
    )?;
    let after = ReviewAction {
        done,
        done_at: if done { Some(now_rfc3339()) } else { None },
        ..before.clone()
    };
    audit::record(
        conn,
        cmd,
        "review_action",
        action_id,
        AuditAction::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(after)
}
