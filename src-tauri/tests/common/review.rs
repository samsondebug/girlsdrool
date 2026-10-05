//! The M8 review exactly as fixtures/review.json states it, for the acceptance tests and the seed.

use std::collections::HashMap;

use kept::db::audit::{self, Actor};
use kept::review;
use rusqlite::Connection;

use super::date;

#[derive(serde::Deserialize)]
pub struct ReviewFile {
    pub as_of: String,
    pub income_window_days: i64,
    pub horizon_days: i64,
    pub period_start: String,
    pub surplus: SurplusSpec,
    pub steps: StepsSpec,
    pub actions: Vec<String>,
    pub snapshot: SnapshotSpec,
}

#[derive(serde::Deserialize)]
pub struct TermSpec {
    pub name: String,
    pub monthly_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct SurplusSpec {
    pub income_90_cents: i64,
    pub income_receipts: usize,
    pub income_cents: i64,
    pub fixed_cents: i64,
    pub fixed_items: Vec<TermSpec>,
    pub debt_service_cents: i64,
    pub debt_items: Vec<TermSpec>,
    pub informal_schedule_12m_cents: i64,
    pub irregular_cents: i64,
    pub irregular_items: Vec<TermSpec>,
    pub variable_cents: i64,
    pub surplus_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct RowSpec {
    pub account: String,
    pub posted: String,
    pub description: String,
    pub amount_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct BalanceSpec {
    pub account: String,
    pub balance_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct BalancesSpec {
    pub accounts: Vec<BalanceSpec>,
    pub available_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct UnreviewedSpec {
    pub count: usize,
    pub total_abs_cents: i64,
    pub rows: Vec<RowSpec>,
}

#[derive(serde::Deserialize)]
pub struct DueSpec {
    pub obligation: String,
    pub due_date: String,
    pub expected_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct ObligationsSpec {
    pub count: usize,
    pub expected_cents: i64,
    pub items: Vec<DueSpec>,
}

#[derive(serde::Deserialize)]
pub struct DebtsSpec {
    pub total_debt_cents: i64,
    pub informal_remaining_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct VenturesSpec {
    pub cap_used_cents: i64,
    pub cap_cents: i64,
    pub utilization_bps: i64,
}

#[derive(serde::Deserialize)]
pub struct FlagsSpec {
    pub borrowing: Vec<RowSpec>,
    pub securities_sale: Vec<RowSpec>,
    pub firewall_unacknowledged: Vec<RowSpec>,
}

#[derive(serde::Deserialize)]
pub struct StepsSpec {
    pub balances: BalancesSpec,
    pub unreviewed: UnreviewedSpec,
    pub obligations_14: ObligationsSpec,
    pub debts: DebtsSpec,
    pub ventures: VenturesSpec,
    pub flags: FlagsSpec,
}

#[derive(serde::Deserialize)]
pub struct SnapshotSpec {
    pub safe_cents: i64,
    pub available_cents: i64,
    pub earmarks_cents: i64,
    pub obligations_cents: i64,
    pub buffer_cents: i64,
    pub trusted: bool,
    pub total_debt_cents: i64,
    pub informal_remaining_cents: i64,
    pub venture_cap_used_cents: i64,
}

/// Start and complete the fixture's review as of its date with its three actions.
pub fn complete_fixture_review(conn: &Connection, file: &ReviewFile) -> review::Review {
    let cmd = audit::begin(conn, "test.review", Actor::User).unwrap();
    let today = date(&file.as_of);
    let started = review::start(conn, &cmd, today).unwrap();
    review::complete(conn, &cmd, started.id, &file.actions, "", today).unwrap()
}

/// Fixture terms by name, for comparisons that do not depend on order.
pub fn by_name(items: &[TermSpec]) -> HashMap<String, i64> {
    items
        .iter()
        .map(|t| (t.name.clone(), t.monthly_cents))
        .collect()
}
