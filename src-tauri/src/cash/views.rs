//! Spending view and cash view (ARCHITECTURE §5.2, ADR-0020). The fixture's `EXPECTED.md`
//! states both totals for 2026-07-01..2026-09-30; the M2 acceptance test asserts them.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::db::repo::account::CASH_KINDS;
use crate::error::AppResult;
use crate::money::Cents;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CategoryLine {
    pub category_id: Option<i64>,
    pub path: Option<String>,
    pub root_kind: Option<String>,
    pub outflows_cents: i64,
    pub inflows_cents: i64,
    pub net_cents: i64,
    pub rows: i64,
}

/// What was consumed: non-transfer, non-income, non-proceeds leaf rows, by category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpendingView {
    pub from: String,
    pub to: String,
    pub gross_outflows_cents: i64,
    pub linked_refunds_cents: i64,
    pub reimbursements_cents: i64,
    pub net_spending_cents: i64,
    /// Positive rows with no category yet: not spending, not income until classified.
    pub positive_review_cents: i64,
    pub unclassified_outflows_cents: i64,
    pub by_category: Vec<CategoryLine>,
}

const SPENDING_ROWS: &str = "FROM txn_leaf t
  LEFT JOIN category c ON c.id = t.category_id
  LEFT JOIN category cp ON cp.id = c.parent_id
  WHERE t.posted_date BETWEEN ?1 AND ?2
    AND t.status = 'posted'
    AND t.transfer_link_id IS NULL
    AND (c.id IS NULL OR c.root_kind NOT IN ('income', 'transfer'))";

pub fn spending_view(conn: &Connection, from: &str, to: &str) -> AppResult<SpendingView> {
    let (gross, refunds, reimbursements, positive_review, unclassified_out): (i64, i64, i64, i64, i64) = conn.query_row(
        &format!(
            "SELECT
               COALESCE(SUM(CASE WHEN t.amount_cents < 0 THEN -t.amount_cents ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN t.amount_cents > 0 AND t.refund_link_id IS NOT NULL THEN t.amount_cents ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN t.amount_cents > 0 AND t.refund_link_id IS NULL AND c.id IS NOT NULL THEN t.amount_cents ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN t.amount_cents > 0 AND t.refund_link_id IS NULL AND c.id IS NULL THEN t.amount_cents ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN t.amount_cents < 0 AND c.id IS NULL THEN -t.amount_cents ELSE 0 END), 0)
             {SPENDING_ROWS}"
        ),
        params![from, to],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    let net = Cents(gross)
        .checked_sub(Cents(refunds))?
        .checked_sub(Cents(reimbursements))?
        .0;

    let mut stmt = conn.prepare(&format!(
        "SELECT c.id, CASE WHEN c.id IS NULL THEN NULL ELSE COALESCE(cp.name || ' › ', '') || c.name END, c.root_kind,
                COALESCE(SUM(CASE WHEN t.amount_cents < 0 THEN -t.amount_cents ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN t.amount_cents > 0 THEN t.amount_cents ELSE 0 END), 0),
                count(*)
         {SPENDING_ROWS}
         GROUP BY c.id ORDER BY 4 DESC, 2"
    ))?;
    let lines = stmt
        .query_map(params![from, to], |r| {
            let outflows: i64 = r.get(3)?;
            let inflows: i64 = r.get(4)?;
            Ok(CategoryLine {
                category_id: r.get(0)?,
                path: r.get(1)?,
                root_kind: r.get(2)?,
                outflows_cents: outflows,
                inflows_cents: inflows,
                net_cents: 0,
                rows: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut by_category = Vec::with_capacity(lines.len());
    for mut line in lines {
        line.net_cents = Cents(line.outflows_cents)
            .checked_sub(Cents(line.inflows_cents))?
            .0;
        by_category.push(line);
    }
    Ok(SpendingView {
        from: from.to_string(),
        to: to.to_string(),
        gross_outflows_cents: gross,
        linked_refunds_cents: refunds,
        reimbursements_cents: reimbursements,
        net_spending_cents: net,
        positive_review_cents: positive_review,
        unclassified_outflows_cents: unclassified_out,
        by_category,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountLine {
    pub account_id: i64,
    pub account_name: String,
    pub outflows_cents: i64,
    pub inflows_cents: i64,
    pub net_cents: i64,
}

/// What left and entered the cash accounts, when. Transfers between two cash accounts are
/// excluded; a card payment or a deposit from a brokerage counts on its own date.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CashView {
    pub from: String,
    pub to: String,
    pub outflows_cents: i64,
    pub inflows_cents: i64,
    pub net_cents: i64,
    pub by_account: Vec<AccountLine>,
}

pub fn cash_view(conn: &Connection, from: &str, to: &str) -> AppResult<CashView> {
    let kinds = CASH_KINDS
        .iter()
        .map(|k| format!("'{k}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT a.id, a.name,
                COALESCE(SUM(CASE WHEN t.amount_cents < 0 THEN -t.amount_cents ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN t.amount_cents > 0 THEN t.amount_cents ELSE 0 END), 0)
         FROM txn_leaf t
         JOIN account a ON a.id = t.account_id
         WHERE a.kind IN ({kinds})
           AND t.posted_date BETWEEN ?1 AND ?2
           AND t.status = 'posted'
           AND NOT EXISTS (
             SELECT 1 FROM transfer_link l
             JOIN txn o ON o.id = CASE WHEN l.out_txn_id = t.id THEN l.in_txn_id ELSE l.out_txn_id END
             JOIN account oa ON oa.id = o.account_id
             WHERE (l.out_txn_id = t.id OR l.in_txn_id = t.id) AND oa.kind IN ({kinds})
           )
         GROUP BY a.id ORDER BY a.name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let lines = stmt
        .query_map(params![from, to], |r| {
            Ok(AccountLine {
                account_id: r.get(0)?,
                account_name: r.get(1)?,
                outflows_cents: r.get(2)?,
                inflows_cents: r.get(3)?,
                net_cents: 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut by_account = Vec::with_capacity(lines.len());
    let mut outflows = Cents::ZERO;
    let mut inflows = Cents::ZERO;
    for mut line in lines {
        line.net_cents = Cents(line.inflows_cents)
            .checked_sub(Cents(line.outflows_cents))?
            .0;
        outflows = outflows.checked_add(Cents(line.outflows_cents))?;
        inflows = inflows.checked_add(Cents(line.inflows_cents))?;
        by_account.push(line);
    }
    let net = inflows.checked_sub(outflows)?;
    Ok(CashView {
        from: from.to_string(),
        to: to.to_string(),
        outflows_cents: outflows.0,
        inflows_cents: inflows.0,
        net_cents: net.0,
        by_account,
    })
}
