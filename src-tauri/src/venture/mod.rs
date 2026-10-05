//! Ventures (ARCHITECTURE §5.9, ADR-0025, ADR-0044): the rollup of every row tagged to a venture
//! into the five buckets, the derived operating cash flow, cap used and utilization, the
//! milestone countdown and stop-condition alert, and venture spend as a share of take-home.
//! The verdict is the person's; sunk cost is never an input.

use chrono::Datelike;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::dates::{format_civil, parse_civil, CivilDate};
use crate::db::repo::account::{self, Account};
use crate::db::repo::venture::{self, Venture};
use crate::error::AppResult;
use crate::money::{mul_div_round, Cents};
use crate::plan::occur;

pub const TRAILING_MONTHS: u32 = 12;
pub const BUCKETS: [&str; 5] = [
    "customer_revenue",
    "operating_expense",
    "owner_contribution",
    "financing",
    "withdrawal",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bucket {
    pub cents: i64,
    pub rows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VentureAccount {
    pub account_id: i64,
    pub name: String,
    pub kind: String,
    pub balance_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rollup {
    #[serde(flatten)]
    pub venture: Venture,
    pub as_of: String,
    pub window_start: String,
    pub customer_revenue: Bucket,
    pub operating_expense: Bucket,
    pub owner_contribution: Bucket,
    pub financing: Bucket,
    pub withdrawal: Bucket,
    /// Operating expense rows that sit on a personal account: counted in the cap (ADR-0025).
    pub operating_expense_from_personal_cents: i64,
    pub operating_cash_flow_cents: i64,
    pub cap_used_cents: i64,
    pub cap_remaining_cents: i64,
    pub cap_utilization_bps: i64,
    pub milestone_days: Option<i64>,
    /// `cap used` and/or `milestone date passed`; empty when nothing fires.
    pub alerts: Vec<String>,
    pub accounts: Vec<VentureAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Summary {
    pub as_of: String,
    pub window_start: String,
    pub ventures: Vec<Rollup>,
    pub total_cap_cents: i64,
    pub total_cap_used_cents: i64,
    pub total_operating_expense_cents: i64,
    /// Confirmed base-pay receipts in the window.
    pub take_home_cents: i64,
    pub spend_share_bps: i64,
}

/// The same civil day `TRAILING_MONTHS` months back (clamped to the month), exclusive.
pub fn window_start(as_of: CivilDate) -> AppResult<CivilDate> {
    let months = i64::from(as_of.month()) - i64::from(TRAILING_MONTHS);
    let (y, m) = if months <= 0 {
        (as_of.year() - 1, u32::try_from(months + 12).unwrap_or(1))
    } else {
        (as_of.year(), u32::try_from(months).unwrap_or(1))
    };
    occur::clamp_day(y, m, as_of.day())
}

fn bucket_of(code: &str) -> Option<&'static str> {
    BUCKETS
        .iter()
        .copied()
        .find(|b| code == format!("venture.{b}"))
}

/// One venture's rollup over `(window_start, as_of]`.
pub fn rollup(
    conn: &Connection,
    v: &Venture,
    as_of: CivilDate,
    accounts: &[Account],
) -> AppResult<Rollup> {
    let start = window_start(as_of)?;
    let (s, e) = (format_civil(start), format_civil(as_of));
    let mut buckets: [Bucket; 5] = Default::default();
    let mut from_personal = Cents::ZERO;
    // rows tagged to the venture, bucketed by their venture category code (transfers aside)
    let mut stmt = conn.prepare(
        "SELECT t.amount_cents, c.system_code, a.owner FROM txn_leaf t
         JOIN category c ON c.id = t.category_id
         JOIN account a ON a.id = t.account_id
         WHERE t.venture_id = ?1 AND t.status = 'posted' AND t.transfer_link_id IS NULL
           AND t.posted_date > ?2 AND t.posted_date <= ?3",
    )?;
    let rows = stmt
        .query_map(params![v.id, s, e], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (amount, code, owner) in rows {
        let Some(b) = code.as_deref().and_then(bucket_of) else {
            continue;
        };
        let i = BUCKETS.iter().position(|x| *x == b).unwrap_or(0);
        let abs = Cents(amount).checked_abs()?;
        buckets[i].cents = Cents(buckets[i].cents).checked_add(abs)?.0;
        buckets[i].rows += 1;
        if b == "operating_expense" && owner == "personal" {
            from_personal = from_personal.checked_add(abs)?;
        }
    }
    // transfers that cross the personal / venture boundary on this venture's accounts
    let mine: Vec<i64> = accounts
        .iter()
        .filter(|a| a.owner == "venture" && a.venture_id == Some(v.id))
        .map(|a| a.id)
        .collect();
    if !mine.is_empty() {
        let mut stmt = conn.prepare(
            "SELECT t.amount_cents FROM txn_leaf t
             JOIN txn o ON o.transfer_link_id = t.transfer_link_id AND o.id <> t.id AND o.parent_id IS NULL
             JOIN account oa ON oa.id = o.account_id
             WHERE t.account_id = ?1 AND t.status = 'posted' AND t.transfer_link_id IS NOT NULL
               AND oa.owner = 'personal' AND t.posted_date > ?2 AND t.posted_date <= ?3",
        )?;
        for id in &mine {
            let amounts = stmt
                .query_map(params![id, s, e], |r| r.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for amount in amounts {
                let i = if amount > 0 { 2 } else { 4 };
                let abs = Cents(amount).checked_abs()?;
                buckets[i].cents = Cents(buckets[i].cents).checked_add(abs)?.0;
                buckets[i].rows += 1;
            }
        }
    }
    let [customer_revenue, operating_expense, owner_contribution, financing, withdrawal] = buckets;
    let operating_cash_flow = Cents(customer_revenue.cents)
        .checked_sub(Cents(operating_expense.cents))?
        .0;
    let cap_used = Cents(owner_contribution.cents)
        .checked_add(from_personal)?
        .checked_sub(Cents(withdrawal.cents))?
        .0;
    let cap_remaining = Cents(v.cash_cap_cents).checked_sub(Cents(cap_used))?.0;
    let cap_utilization_bps = if v.cash_cap_cents > 0 {
        mul_div_round(i128::from(cap_used), 10_000, i128::from(v.cash_cap_cents))?
    } else {
        0
    };
    let milestone_days = match &v.milestone_date {
        Some(d) => Some((parse_civil(d)? - as_of).num_days()),
        None => None,
    };
    let mut alerts = Vec::new();
    if cap_used >= v.cash_cap_cents {
        alerts.push("cap used".to_string());
    }
    if milestone_days.is_some_and(|d| d < 0) {
        alerts.push("milestone date passed".to_string());
    }
    let mut owned = Vec::new();
    for a in accounts
        .iter()
        .filter(|a| a.owner == "venture" && a.venture_id == Some(v.id))
    {
        owned.push(VentureAccount {
            account_id: a.id,
            name: a.name.clone(),
            kind: a.kind.clone(),
            balance_cents: crate::cash::safe::posted_balance_as_of(conn, a, as_of)?,
        });
    }
    Ok(Rollup {
        venture: v.clone(),
        as_of: format_civil(as_of),
        window_start: format_civil(start),
        customer_revenue,
        operating_expense,
        owner_contribution,
        financing,
        withdrawal,
        operating_expense_from_personal_cents: from_personal.0,
        operating_cash_flow_cents: operating_cash_flow,
        cap_used_cents: cap_used,
        cap_remaining_cents: cap_remaining,
        cap_utilization_bps,
        milestone_days,
        alerts,
        accounts: owned,
    })
}

/// Σ receipts of active confirmed base-pay streams posted in the window: trailing take-home.
pub fn take_home(conn: &Connection, as_of: CivilDate) -> AppResult<i64> {
    let start = window_start(as_of)?;
    let sum: i64 = conn.query_row(
        "SELECT COALESCE(SUM(t.amount_cents), 0) FROM income_receipt r
         JOIN txn t ON t.id = r.txn_id
         JOIN income_stream s ON s.id = r.income_stream_id
         WHERE s.kind = 'base' AND s.confidence = 'confirmed' AND t.posted_date > ?1 AND t.posted_date <= ?2",
        params![format_civil(start), format_civil(as_of)],
        |r| r.get(0),
    )?;
    Ok(sum)
}

/// Every venture's rollup plus the totals and the spend share.
pub fn summary(conn: &Connection, as_of: CivilDate) -> AppResult<Summary> {
    let accounts = account::list(conn)?;
    let mut ventures = Vec::new();
    let mut total_cap = Cents::ZERO;
    let mut total_used = Cents::ZERO;
    let mut total_opex = Cents::ZERO;
    for v in venture::list(conn)?.into_iter().filter(|v| !v.archived) {
        let r = rollup(conn, &v, as_of, &accounts)?;
        total_cap = total_cap.checked_add(Cents(v.cash_cap_cents))?;
        total_used = total_used.checked_add(Cents(r.cap_used_cents))?;
        total_opex = total_opex.checked_add(Cents(r.operating_expense.cents))?;
        ventures.push(r);
    }
    let take_home = take_home(conn, as_of)?;
    let spend_share_bps = if take_home > 0 {
        mul_div_round(i128::from(total_opex.0), 10_000, i128::from(take_home))?
    } else {
        0
    };
    Ok(Summary {
        as_of: format_civil(as_of),
        window_start: format_civil(window_start(as_of)?),
        ventures,
        total_cap_cents: total_cap.0,
        total_cap_used_cents: total_used.0,
        total_operating_expense_cents: total_opex.0,
        take_home_cents: take_home,
        spend_share_bps,
    })
}
