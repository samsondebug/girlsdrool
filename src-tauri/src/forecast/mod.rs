//! The forecast (ARCHITECTURE §5.6, ADR-0023, ADR-0042): one daily engine over days 0..=90 from
//! today, fed by the ledger's pending rows, the plan's confirmed income and unpaid obligations and
//! the variable-spend model. Scenarios bend the inputs, never the arithmetic: every day
//! `closing = opening + inflows − outflows` and the horizon ties out the same way.

pub mod variable;

use std::collections::BTreeMap;

use chrono::{Datelike, Duration};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::cash::recon::{self, TrustReport};
use crate::cash::safe;
use crate::dates::{format_civil, now_rfc3339, parse_civil, CivilDate};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::account::{self, Account};
use crate::db::settings;
use crate::error::{AppError, AppResult};
use crate::import::csv::{FLAG_BORROWING, FLAG_SECURITIES_SALE};
use crate::money::{allocate, Cents};
use crate::plan::{earmark, income, occur, OVERDUE_LOOKBACK_DAYS};

/// The engine runs days `0..=HORIZON_DAYS`; the 30-day view is days 0..29, the 13-week view is
/// thirteen seven-day buckets.
pub const HORIZON_DAYS: i64 = 90;
/// Downside: the next confirmed base-pay occurrence lands this many civil days late.
pub const PAY_SHIFT_DAYS: i64 = 7;
pub const WEEKS: usize = 13;
const DAYS: usize = HORIZON_DAYS as usize + 1;
const BLOCK: usize = variable::BUCKET_DAYS as usize;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scenario {
    /// Next base-pay occurrence +7 days; expected and rumored streams stay out (they always do).
    #[serde(default)]
    pub downside: bool,
    #[serde(default)]
    pub surprise_bill: Option<SurpriseBill>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SurpriseBill {
    pub date: String,
    pub cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    /// `pending | income | obligation | informal | variable | surprise`.
    pub kind: String,
    pub name: String,
    /// Signed from the cash accounts' point of view.
    pub cents: i64,
    /// The row, stream, obligation or category behind the event.
    pub ref_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Day {
    pub day: i64,
    pub date: String,
    pub opening_cents: i64,
    pub inflows_cents: i64,
    pub outflows_cents: i64,
    pub closing_cents: i64,
    /// Earmark remaining projected by schedule + the timing buffer.
    pub committed_cents: i64,
    pub headroom_cents: i64,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Week {
    pub week: i64,
    pub start: String,
    pub end: String,
    pub inflows_cents: i64,
    pub outflows_cents: i64,
    pub closing_cents: i64,
    pub lowest_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Point {
    pub date: String,
    pub cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayDate {
    pub stream_id: i64,
    pub stream_name: String,
    pub date: String,
    pub shifted: bool,
}

/// A `plan` snapshot's stored baseline, drawn against the live one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanOverlay {
    pub snapshot_id: i64,
    pub taken_at: String,
    pub civil_date: String,
    pub days: Vec<Point>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Forecast {
    pub as_of: String,
    pub horizon_days: i64,
    pub scenario: Scenario,
    pub opening_cents: i64,
    pub inflows_cents: i64,
    pub outflows_cents: i64,
    pub closing_cents: i64,
    pub lowest: Point,
    /// First day `closing < 0`.
    pub first_shortfall: Option<Point>,
    /// First day `headroom < 0`.
    pub first_buffer_breach: Option<Point>,
    pub days: Vec<Day>,
    pub weeks: Vec<Week>,
    pub model: Vec<variable::CategoryModel>,
    pub model_total_cents: i64,
    pub pay_dates: Vec<PayDate>,
    pub plan: Option<PlanOverlay>,
    pub trust: TrustReport,
}

struct PendingRow {
    id: i64,
    date: CivilDate,
    cents: i64,
    payee: String,
}

/// Pending leaf rows that count (§5.1 sign rules): every outflow, inflows unless flagged
/// borrowing or securities-sale.
fn pending_rows(conn: &Connection, acct: &Account) -> AppResult<Vec<PendingRow>> {
    let excluded_bits = i64::from(FLAG_BORROWING) | i64::from(FLAG_SECURITIES_SALE);
    let mut stmt = conn.prepare(
        "SELECT id, effective_date, amount_cents, flags, payee_raw FROM txn_leaf
         WHERE account_id = ?1 AND status = 'pending' ORDER BY effective_date, id",
    )?;
    let rows = stmt
        .query_map([acct.id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    for (id, date, cents, flags, payee) in rows {
        if cents > 0 && flags & excluded_bits != 0 {
            continue;
        }
        out.push(PendingRow {
            id,
            date: parse_civil(&date)?,
            cents,
            payee,
        });
    }
    Ok(out)
}

fn neg(cents: i64) -> AppResult<i64> {
    Ok(Cents(cents).checked_neg()?.0)
}

/// Run one scenario of the daily engine as of `today`.
pub fn run(conn: &Connection, today: CivilDate, scenario: &Scenario) -> AppResult<Forecast> {
    let horizon = today + Duration::days(HORIZON_DAYS);
    if let Some(bill) = &scenario.surprise_bill {
        let d = parse_civil(&bill.date)?;
        if d < today || d > horizon {
            return Err(AppError::validation(
                "surprise_bill.date",
                "a surprise bill falls inside the 91-day window",
            ));
        }
        if bill.cents <= 0 {
            return Err(AppError::validation(
                "surprise_bill.cents",
                "a surprise bill is a positive amount",
            ));
        }
    }
    let day_index = |d: CivilDate| -> usize {
        let clamped = if d < today { today } else { d };
        usize::try_from((clamped - today).num_days()).unwrap_or(0)
    };
    let accounts: Vec<Account> = account::list(conn)?
        .into_iter()
        .filter(safe::contributes)
        .collect();
    let mut events: Vec<Vec<Event>> = vec![Vec::new(); DAYS];

    // opening balance and the pending rows that will post
    let mut opening = Cents::ZERO;
    for acct in &accounts {
        opening = opening.checked_add(Cents(safe::posted_balance_as_of(conn, acct, today)?))?;
        for p in pending_rows(conn, acct)? {
            if p.date > horizon {
                continue;
            }
            events[day_index(p.date)].push(Event {
                kind: "pending".into(),
                name: p.payee,
                cents: p.cents,
                ref_id: Some(p.id),
            });
        }
    }

    // confirmed income, unreceived occurrences; the downside delays the next base-pay one
    let mut pay_dates = Vec::new();
    let mut pay_by_stream: BTreeMap<i64, Vec<CivilDate>> = BTreeMap::new();
    for st in income::list(conn)?
        .into_iter()
        .filter(|s| s.active && s.confidence == "confirmed")
    {
        let mut occs = Vec::new();
        for d in income::occurrences(&st, today, horizon)? {
            if income::receipt_for(conn, st.id, &format_civil(d))?.is_none() {
                occs.push(d);
            }
        }
        let shift_first = scenario.downside && st.kind == "base" && !occs.is_empty();
        if shift_first {
            occs[0] += Duration::days(PAY_SHIFT_DAYS);
        }
        occs.retain(|d| *d <= horizon);
        for (i, d) in occs.iter().enumerate() {
            events[day_index(*d)].push(Event {
                kind: "income".into(),
                name: st.name.clone(),
                cents: st.expected_net_cents,
                ref_id: Some(st.id),
            });
            pay_dates.push(PayDate {
                stream_id: st.id,
                stream_name: st.name.clone(),
                date: format_civil(*d),
                shifted: shift_first && i == 0,
            });
        }
        pay_by_stream.insert(st.id, occs);
    }
    pay_dates.sort_by(|a, b| a.date.cmp(&b.date).then(a.stream_id.cmp(&b.stream_id)));

    // unpaid confirmed obligations, overdue ones on day 0; the forecast spends the full amount
    let mut no_cover = BTreeMap::new();
    let mut expected_by_obligation: BTreeMap<i64, i64> = BTreeMap::new();
    for item in safe::unpaid_occurrences(
        conn,
        today,
        today - Duration::days(OVERDUE_LOOKBACK_DAYS),
        horizon,
        &mut no_cover,
    )? {
        expected_by_obligation.insert(item.obligation_id, item.expected_cents);
        events[day_index(parse_civil(&item.due_date)?)].push(Event {
            kind: "obligation".into(),
            name: item.name,
            cents: neg(item.expected_cents)?,
            ref_id: Some(item.obligation_id),
        });
    }

    // unpaid informal repayments due in the window (overdue ones on day 0)
    for (debt_id, name, due, cents) in crate::debt::informal::unpaid_due_between(
        conn,
        today,
        today - Duration::days(OVERDUE_LOOKBACK_DAYS),
        horizon,
    )? {
        events[day_index(parse_civil(&due)?)].push(Event {
            kind: "informal".into(),
            name,
            cents: neg(cents)?,
            ref_id: Some(debt_id),
        });
    }

    // variable spend, each category spread over every 30-day block by largest remainder
    let model = variable::model(conn, today)?;
    let mut model_total = Cents::ZERO;
    for m in &model {
        model_total = model_total.checked_add(Cents(m.per_30_days_cents))?;
        if m.per_30_days_cents == 0 {
            continue;
        }
        let parts = allocate(m.per_30_days_cents, BLOCK)?;
        for (i, day) in events.iter_mut().enumerate() {
            day.push(Event {
                kind: "variable".into(),
                name: m.name.clone(),
                cents: neg(parts[i % BLOCK])?,
                ref_id: Some(m.category_id),
            });
        }
    }

    if let Some(bill) = &scenario.surprise_bill {
        events[day_index(parse_civil(&bill.date)?)].push(Event {
            kind: "surprise".into(),
            name: "Surprise bill".into(),
            cents: neg(bill.cents)?,
            ref_id: None,
        });
    }

    // committed: the timing buffer plus each counted earmark's remaining projected by schedule
    let cfg = settings::load(conn)?;
    let earmarks: Vec<earmark::Earmark> = earmark::list(conn)?
        .into_iter()
        .filter(|e| e.active && accounts.iter().any(|a| a.id == e.funding_account_id))
        .collect();
    let mut remaining = Vec::with_capacity(earmarks.len());
    let mut fundings: Vec<BTreeMap<CivilDate, i64>> = Vec::with_capacity(earmarks.len());
    for em in &earmarks {
        remaining.push(earmark::remaining(conn, em.id, today)?);
        fundings.push(funding_schedule(em, today, horizon, &pay_by_stream)?);
    }

    let mut days = Vec::with_capacity(DAYS);
    let mut bal = opening;
    let mut inflows_total = Cents::ZERO;
    let mut outflows_total = Cents::ZERO;
    let mut lowest: Option<Point> = None;
    let mut first_shortfall = None;
    let mut first_buffer_breach = None;
    for (i, evs) in events.iter().enumerate() {
        let date = today + Duration::days(i as i64);
        let mut inflow = Cents::ZERO;
        let mut outflow = Cents::ZERO;
        for e in evs {
            if e.cents > 0 {
                inflow = inflow.checked_add(Cents(e.cents))?;
            } else {
                outflow = outflow.checked_add(Cents(e.cents).checked_neg()?)?;
            }
        }
        let opening_day = bal;
        bal = bal.checked_add(inflow)?.checked_sub(outflow)?;
        inflows_total = inflows_total.checked_add(inflow)?;
        outflows_total = outflows_total.checked_add(outflow)?;

        let mut committed = Cents(cfg.timing_buffer_cents);
        for (k, em) in earmarks.iter().enumerate() {
            let mut r = remaining[k];
            if let Some(ob_id) = em.obligation_id {
                let expected = expected_by_obligation.get(&ob_id).copied().unwrap_or(0);
                for _ in evs
                    .iter()
                    .filter(|e| e.kind == "obligation" && e.ref_id == Some(ob_id))
                {
                    r = Cents(r).checked_sub(Cents(r.max(0).min(expected)))?.0;
                }
            }
            if let Some(amount) = fundings[k].get(&date) {
                let room = if em.target_cents > 0 {
                    Cents(em.target_cents)
                        .checked_sub(Cents(r.max(0)))?
                        .0
                        .max(0)
                } else {
                    *amount
                };
                r = Cents(r).checked_add(Cents((*amount).min(room)))?.0;
            }
            remaining[k] = r;
            committed = committed.checked_add(Cents(r.max(0)))?;
        }
        let headroom = bal.checked_sub(committed)?;
        let point = Point {
            date: format_civil(date),
            cents: bal.0,
        };
        if lowest.as_ref().is_none_or(|l| bal.0 < l.cents) {
            lowest = Some(point.clone());
        }
        if first_shortfall.is_none() && bal.0 < 0 {
            first_shortfall = Some(point.clone());
        }
        if first_buffer_breach.is_none() && headroom.0 < 0 {
            first_buffer_breach = Some(Point {
                date: point.date.clone(),
                cents: headroom.0,
            });
        }
        days.push(Day {
            day: i as i64,
            date: point.date,
            opening_cents: opening_day.0,
            inflows_cents: inflow.0,
            outflows_cents: outflow.0,
            closing_cents: bal.0,
            committed_cents: committed.0,
            headroom_cents: headroom.0,
            events: evs.clone(),
        });
    }

    let mut weeks = Vec::with_capacity(WEEKS);
    for (w, chunk) in days.chunks(7).enumerate().take(WEEKS) {
        let mut inflow = Cents::ZERO;
        let mut outflow = Cents::ZERO;
        for d in chunk {
            inflow = inflow.checked_add(Cents(d.inflows_cents))?;
            outflow = outflow.checked_add(Cents(d.outflows_cents))?;
        }
        let first = chunk
            .first()
            .ok_or(AppError::Internal("empty week".into()))?;
        let last = chunk
            .last()
            .ok_or(AppError::Internal("empty week".into()))?;
        weeks.push(Week {
            week: w as i64,
            start: first.date.clone(),
            end: last.date.clone(),
            inflows_cents: inflow.0,
            outflows_cents: outflow.0,
            closing_cents: last.closing_cents,
            lowest_cents: chunk
                .iter()
                .map(|d| d.closing_cents)
                .min()
                .unwrap_or(last.closing_cents),
        });
    }

    let lowest = lowest.ok_or(AppError::Internal("empty forecast".into()))?;
    Ok(Forecast {
        as_of: format_civil(today),
        horizon_days: HORIZON_DAYS,
        scenario: scenario.clone(),
        opening_cents: opening.0,
        inflows_cents: inflows_total.0,
        outflows_cents: outflows_total.0,
        closing_cents: bal.0,
        lowest,
        first_shortfall,
        first_buffer_breach,
        days,
        weeks,
        model_total_cents: model_total.0,
        model,
        pay_dates,
        plan: latest_plan(conn)?,
        trust: recon::trust(conn, today, cfg.recon_stale_after_days)?,
    })
}

/// When an earmark's schedule adds money inside the window: per paycheck on the scenario's pay
/// dates of its stream, monthly on its day, by date as one funding of the gap on the target date
/// (the target caps every funding).
fn funding_schedule(
    em: &earmark::Earmark,
    today: CivilDate,
    horizon: CivilDate,
    pay_by_stream: &BTreeMap<i64, Vec<CivilDate>>,
) -> AppResult<BTreeMap<CivilDate, i64>> {
    let mut out = BTreeMap::new();
    match em.schedule.as_str() {
        "per_paycheck" => {
            if let (Some(stream), Some(amount)) =
                (em.schedule_income_stream_id, em.schedule_amount_cents)
            {
                for d in pay_by_stream.get(&stream).into_iter().flatten() {
                    let slot = out.entry(*d).or_insert(0);
                    *slot = Cents(*slot).checked_add(Cents(amount))?.0;
                }
            }
        }
        "monthly" => {
            if let (Some(day), Some(amount)) = (em.schedule_day, em.schedule_amount_cents) {
                let day = u32::try_from(day.clamp(1, 31)).unwrap_or(1);
                let (mut y, mut m) = (today.year(), today.month());
                loop {
                    let d = occur::clamp_day(y, m, day)?;
                    if d > horizon {
                        break;
                    }
                    if d >= today {
                        out.insert(d, amount);
                    }
                    if m == 12 {
                        y += 1;
                        m = 1;
                    } else {
                        m += 1;
                    }
                }
            }
        }
        "by_date" => {
            if let Some(target_date) = &em.target_date {
                let d = parse_civil(target_date)?;
                if d >= today && d <= horizon {
                    out.insert(d, em.target_cents.max(0));
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

/// The latest `plan` snapshot, if the person saved one.
pub fn latest_plan(conn: &Connection) -> AppResult<Option<PlanOverlay>> {
    let row: Option<(i64, String, String, String)> = conn
        .query_row(
            "SELECT id, taken_at, civil_date, detail_json FROM snapshot WHERE kind = 'plan'
             ORDER BY taken_at DESC, id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((id, taken_at, civil_date, detail)) = row else {
        return Ok(None);
    };
    let stored: StoredPlan = serde_json::from_str(&detail)
        .map_err(|e| AppError::Internal(format!("plan snapshot {id} is unreadable: {e}")))?;
    Ok(Some(PlanOverlay {
        snapshot_id: id,
        taken_at,
        civil_date,
        days: stored.days,
    }))
}

#[derive(Serialize, Deserialize)]
struct StoredPlan {
    days: Vec<Point>,
}

/// Store today's baseline as the plan to draw later forecasts against (snapshot kind `plan`).
pub fn save_plan(
    conn: &Connection,
    cmd: &CommandRecord,
    today: CivilDate,
) -> AppResult<PlanOverlay> {
    let baseline = run(conn, today, &Scenario::default())?;
    let hero = safe::safe_to_spend(conn, today)?;
    let stored = StoredPlan {
        days: baseline
            .days
            .iter()
            .map(|d| Point {
                date: d.date.clone(),
                cents: d.closing_cents,
            })
            .collect(),
    };
    let detail = serde_json::to_string(&stored)
        .map_err(|e| AppError::Internal(format!("plan snapshot: {e}")))?;
    let taken_at = now_rfc3339();
    conn.execute(
        "INSERT INTO snapshot (taken_at, civil_date, kind, safe_cents, available_cents, earmarks_cents, obligations_cents,
                               buffer_cents, trusted, total_debt_cents, informal_remaining_cents, venture_cap_used_cents, detail_json)
         VALUES (?1, ?2, 'plan', ?3, ?4, ?5, ?6, ?7, ?8, 0, 0, 0, ?9)",
        params![
            taken_at,
            format_civil(today),
            hero.safe_cents,
            hero.terms.available.cents,
            hero.terms.earmarks.cents,
            hero.terms.obligations.cents,
            hero.terms.buffer.cents,
            i64::from(hero.trust.hero.trusted),
            detail,
        ],
    )?;
    let id = conn.last_insert_rowid();
    let after = serde_json::json!({
        "id": id, "kind": "plan", "civil_date": format_civil(today), "safe_cents": hero.safe_cents,
        "closing_day_90_cents": baseline.closing_cents,
    });
    audit::record(
        conn,
        cmd,
        "snapshot",
        id,
        Action::Insert,
        None,
        Some(&after),
    )?;
    Ok(PlanOverlay {
        snapshot_id: id,
        taken_at,
        civil_date: format_civil(today),
        days: stored.days,
    })
}
