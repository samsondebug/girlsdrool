//! Avalanche, snowball and custom in interest cents (ARCHITECTURE §5.8, ADR-0043): every open debt
//! gets its minimum each period; a constant monthly budget (extra + the first period's minimums)
//! pays informal loans first under policy `informal_first`, then the strategy's target. The
//! 12-month informal scenario is read off the same run.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{informal, period_bounds, period_interest, period_minimum, Debt};
use crate::dates::{format_civil, parse_civil, CivilDate};
use crate::db::repo::policy;
use crate::error::{AppError, AppResult};
use crate::money::Cents;

pub const STRATEGIES: [&str; 3] = ["avalanche", "snowball", "custom"];
/// The engine stops after this many periods even if a balance remains (a non-amortizing debt).
pub const PERIODS_MAX: u32 = 120;
pub const SCENARIO_PERIODS: u32 = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeriodRow {
    pub period: u32,
    pub start: String,
    pub end: String,
    pub opening_cents: i64,
    pub interest_cents: i64,
    pub minimum_cents: i64,
    pub payment_cents: i64,
    pub closing_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebtSchedule {
    pub debt_id: i64,
    pub name: String,
    pub kind: String,
    pub informal: bool,
    pub owed_cents: i64,
    pub total_interest_cents: i64,
    pub total_paid_cents: i64,
    pub payoff_date: Option<String>,
    pub periods: Vec<PeriodRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrategyRun {
    pub strategy: String,
    pub extra_cents: i64,
    pub budget_cents: i64,
    pub total_interest_cents: i64,
    pub payoff_date: Option<String>,
    /// Balances still open after `PERIODS_MAX` periods: the budget does not amortize them.
    pub unfinished: bool,
    pub debts: Vec<DebtSchedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InformalScenario {
    pub extra_cents: i64,
    pub periods: u32,
    pub remaining_cents: i64,
    pub achievable: bool,
    pub gap_cents: i64,
    pub payoff_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Comparison {
    pub as_of: String,
    pub extra_cents: i64,
    /// Where the extra came from: `user` or `none` (a review's dependable surplus at M8).
    pub extra_source: String,
    pub informal_first: bool,
    pub strategies: Vec<StrategyRun>,
    pub scenario: InformalScenario,
}

struct State {
    debt: Debt,
    informal: bool,
    promised_date: String,
    /// Unpaid schedule rows (date, cents) of an informal loan.
    schedule: Vec<(CivilDate, i64)>,
    owed: i64,
    balance: i64,
    opening: i64,
    interest: i64,
    minimum: i64,
    payment: i64,
    interest_total: i64,
    paid_total: i64,
    payoff: Option<String>,
    rows: Vec<PeriodRow>,
}

fn states(conn: &Connection, today: CivilDate) -> AppResult<Vec<State>> {
    let mut out = Vec::new();
    for debt in super::list(conn)?
        .into_iter()
        .filter(|d| d.active && d.strategy_participation)
    {
        let owed = super::owed(conn, &debt, today)?;
        if owed <= 0 {
            continue;
        }
        let informal = debt.kind == "informal";
        let (promised_date, schedule) = if informal {
            let loan = informal::get(conn, debt.id, today)?;
            let mut rows = Vec::new();
            for r in loan.schedule.iter().filter(|r| r.unpaid_cents > 0) {
                rows.push((parse_civil(&r.due_date)?, r.unpaid_cents));
            }
            (
                loan.promised_date.unwrap_or_else(|| "9999-12-31".into()),
                rows,
            )
        } else {
            (String::new(), Vec::new())
        };
        out.push(State {
            debt,
            informal,
            promised_date,
            schedule,
            owed,
            balance: owed,
            opening: 0,
            interest: 0,
            minimum: 0,
            payment: 0,
            interest_total: 0,
            paid_total: 0,
            payoff: None,
            rows: Vec::new(),
        });
    }
    Ok(out)
}

fn informal_first_on(conn: &Connection) -> AppResult<bool> {
    Ok(policy::list(conn)?
        .iter()
        .any(|p| p.kind == "informal_first"))
}

/// Run one strategy from today with `extra_cents` per month on top of the minimums.
pub fn run(
    conn: &Connection,
    today: CivilDate,
    strategy: &str,
    extra_cents: i64,
) -> AppResult<StrategyRun> {
    if !STRATEGIES.contains(&strategy) {
        return Err(AppError::validation(
            "strategy",
            "must be avalanche, snowball or custom",
        ));
    }
    if extra_cents < 0 {
        return Err(AppError::validation(
            "extra_cents",
            "the monthly extra cannot be negative",
        ));
    }
    let informal_first = informal_first_on(conn)?;
    let mut states = states(conn, today)?;
    let mut budget: Option<i64> = None;
    let mut unfinished = false;
    for k in 1..=PERIODS_MAX {
        let open: Vec<usize> = (0..states.len())
            .filter(|&i| states[i].balance > 0)
            .collect();
        if open.is_empty() {
            break;
        }
        if k == PERIODS_MAX {
            unfinished = true;
        }
        let (start, end) = period_bounds(today, k)?;
        let (s, e) = (format_civil(start), format_civil(end));
        for &i in &open {
            let st = &mut states[i];
            st.opening = st.balance;
            st.interest = period_interest(&st.debt, st.opening, start, end)?;
            let scheduled = {
                let mut sum = Cents::ZERO;
                for (d, cents) in &st.schedule {
                    if *d >= start && *d <= end {
                        sum = sum.checked_add(Cents(*cents))?;
                    }
                }
                sum.0
            };
            st.minimum = period_minimum(&st.debt, st.opening, st.interest, scheduled)?;
            st.payment = st.minimum;
        }
        let minimums = open.iter().try_fold(Cents::ZERO, |acc, &i| {
            acc.checked_add(Cents(states[i].minimum))
        })?;
        let budget_now = match budget {
            Some(b) => b,
            None => {
                let b = Cents(extra_cents).checked_add(minimums)?.0;
                budget = Some(b);
                b
            }
        };
        let mut pool = Cents(budget_now).checked_sub(minimums)?.0.max(0);
        let mut order: Vec<usize> = Vec::with_capacity(open.len());
        if informal_first {
            let mut loans: Vec<usize> = open
                .iter()
                .copied()
                .filter(|&i| states[i].informal)
                .collect();
            loans.sort_by(|&a, &b| {
                states[a]
                    .promised_date
                    .cmp(&states[b].promised_date)
                    .then(states[a].debt.name.cmp(&states[b].debt.name))
            });
            order.extend(loans);
        }
        let mut others: Vec<usize> = open
            .iter()
            .copied()
            .filter(|&i| !(informal_first && states[i].informal))
            .collect();
        match strategy {
            "avalanche" => others.sort_by(|&a, &b| {
                super::effective_apr(&states[b].debt, start)
                    .cmp(&super::effective_apr(&states[a].debt, start))
                    .then(states[a].opening.cmp(&states[b].opening))
                    .then(states[a].debt.name.cmp(&states[b].debt.name))
            }),
            "snowball" => others.sort_by(|&a, &b| {
                states[a]
                    .opening
                    .cmp(&states[b].opening)
                    .then(states[a].debt.name.cmp(&states[b].debt.name))
            }),
            _ => others.sort_by(|&a, &b| {
                states[a]
                    .debt
                    .custom_order
                    .unwrap_or(i64::MAX)
                    .cmp(&states[b].debt.custom_order.unwrap_or(i64::MAX))
                    .then(states[a].debt.name.cmp(&states[b].debt.name))
            }),
        }
        order.extend(others);
        for i in order {
            if pool <= 0 {
                break;
            }
            let st = &mut states[i];
            let room = Cents(st.opening)
                .checked_add(Cents(st.interest))?
                .checked_sub(Cents(st.payment))?
                .0;
            let add = pool.min(room).max(0);
            st.payment = Cents(st.payment).checked_add(Cents(add))?.0;
            pool = Cents(pool).checked_sub(Cents(add))?.0;
        }
        for &i in &open {
            let st = &mut states[i];
            st.balance = Cents(st.opening)
                .checked_add(Cents(st.interest))?
                .checked_sub(Cents(st.payment))?
                .0;
            st.interest_total = Cents(st.interest_total).checked_add(Cents(st.interest))?.0;
            st.paid_total = Cents(st.paid_total).checked_add(Cents(st.payment))?.0;
            st.rows.push(PeriodRow {
                period: k,
                start: s.clone(),
                end: e.clone(),
                opening_cents: st.opening,
                interest_cents: st.interest,
                minimum_cents: st.minimum,
                payment_cents: st.payment,
                closing_cents: st.balance,
            });
            if st.balance == 0 && st.payoff.is_none() {
                st.payoff = Some(e.clone());
            }
        }
    }
    let mut total_interest = Cents::ZERO;
    for st in &states {
        total_interest = total_interest.checked_add(Cents(st.interest_total))?;
    }
    let payoff_date = states.iter().filter_map(|s| s.payoff.clone()).max();
    let unfinished = unfinished && states.iter().any(|s| s.balance > 0);
    Ok(StrategyRun {
        strategy: strategy.to_string(),
        extra_cents,
        budget_cents: budget.unwrap_or(extra_cents),
        total_interest_cents: total_interest.0,
        payoff_date: if unfinished { None } else { payoff_date },
        unfinished,
        debts: states
            .into_iter()
            .map(|s| DebtSchedule {
                debt_id: s.debt.id,
                name: s.debt.name,
                kind: s.debt.kind,
                informal: s.informal,
                owed_cents: s.owed,
                total_interest_cents: s.interest_total,
                total_paid_cents: s.paid_total,
                payoff_date: s.payoff,
                periods: s.rows,
            })
            .collect(),
    })
}

/// "Every informal loan repaid within 12 months" is a scenario to test, never an assumption:
/// with the given extra, is it achievable, what is still owed after 12 periods, and when does the
/// budget actually get there.
pub fn informal_scenario(
    conn: &Connection,
    today: CivilDate,
    extra_cents: i64,
) -> AppResult<InformalScenario> {
    let run = run(conn, today, "avalanche", extra_cents)?;
    let loans: Vec<&DebtSchedule> = run.debts.iter().filter(|d| d.informal).collect();
    let mut remaining = Cents::ZERO;
    let mut gap = Cents::ZERO;
    for loan in &loans {
        remaining = remaining.checked_add(Cents(loan.owed_cents))?;
        let at = loan
            .periods
            .iter()
            .find(|r| r.period == SCENARIO_PERIODS)
            .map_or(0, |r| r.closing_cents);
        gap = gap.checked_add(Cents(at))?;
    }
    let achievable = !loans.is_empty()
        && loans.iter().all(|l| {
            l.payoff_date.is_some()
                && l.periods
                    .last()
                    .is_some_and(|r| r.period <= SCENARIO_PERIODS)
        });
    Ok(InformalScenario {
        extra_cents,
        periods: SCENARIO_PERIODS,
        remaining_cents: remaining.0,
        achievable,
        gap_cents: gap.0,
        payoff_date: loans.iter().filter_map(|l| l.payoff_date.clone()).max(),
    })
}

/// The three strategies side by side, plus the informal scenario, for one monthly extra.
pub fn compare(conn: &Connection, today: CivilDate, extra: Option<i64>) -> AppResult<Comparison> {
    let extra_cents = extra.unwrap_or(0);
    let mut strategies = Vec::with_capacity(STRATEGIES.len());
    for s in STRATEGIES {
        strategies.push(run(conn, today, s, extra_cents)?);
    }
    Ok(Comparison {
        as_of: format_civil(today),
        extra_cents,
        extra_source: if extra.is_some() { "user" } else { "none" }.to_string(),
        informal_first: informal_first_on(conn)?,
        strategies,
        scenario: informal_scenario(conn, today, extra_cents)?,
    })
}
