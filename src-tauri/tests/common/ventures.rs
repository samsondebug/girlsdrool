//! The M7 venture exactly as fixtures/ventures.json states it, for the acceptance test and the
//! seed: the venture's details and the account the person marks as venture-owned.

use std::collections::HashMap;

use kept::db::audit::{self, Actor};
use kept::db::repo::{account, venture};
use rusqlite::Connection;

use super::account_id;

#[derive(serde::Deserialize)]
pub struct VenturesFile {
    pub as_of: String,
    pub trailing_months: u32,
    pub venture: VentureSpec,
    pub venture_account: String,
    pub rollup: RollupSpec,
}

#[derive(serde::Deserialize)]
pub struct VentureSpec {
    pub name: String,
    pub status: String,
    pub cash_cap_cents: i64,
    pub time_budget_hours: Option<i64>,
    pub milestone: String,
    pub milestone_date: Option<String>,
    pub stop_condition: String,
}

#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
pub struct BucketSpec {
    pub cents: i64,
    pub rows: usize,
}

#[derive(serde::Deserialize)]
pub struct AccountBalanceSpec {
    pub account: String,
    pub balance_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct RollupSpec {
    pub name: String,
    pub window_start: String,
    pub buckets: HashMap<String, BucketSpec>,
    pub operating_expense_from_personal_cents: i64,
    pub operating_cash_flow_cents: i64,
    pub cap_cents: i64,
    pub cap_used_cents: i64,
    pub cap_remaining_cents: i64,
    pub cap_utilization_bps: i64,
    pub milestone_days: i64,
    pub alerts: Vec<String>,
    pub take_home_cents: i64,
    pub spend_share_bps: i64,
    pub accounts: Vec<AccountBalanceSpec>,
}

/// Give the venture (created with the rules) its details and mark the account as its own.
/// Returns the venture id.
pub fn install_venture(
    conn: &Connection,
    accounts: &[(&str, account::Account)],
    file: &VenturesFile,
) -> i64 {
    let cmd = audit::begin(conn, "test.ventures", Actor::User).unwrap();
    let v = venture::list(conn)
        .unwrap()
        .into_iter()
        .find(|v| v.name == file.venture.name)
        .unwrap_or_else(|| panic!("venture {} from rules.json", file.venture.name));
    let s = &file.venture;
    venture::update(
        conn,
        &cmd,
        v.id,
        &venture::VentureInput {
            name: s.name.clone(),
            status: s.status.clone(),
            cash_cap_cents: s.cash_cap_cents,
            time_budget_hours: s.time_budget_hours,
            milestone: s.milestone.clone(),
            milestone_date: s.milestone_date.clone(),
            stop_condition: s.stop_condition.clone(),
        },
    )
    .unwrap();
    account::update(
        conn,
        &cmd,
        account_id(accounts, &file.venture_account),
        &account::AccountPatch {
            venture_id: Some(Some(v.id)),
            ..Default::default()
        },
    )
    .unwrap();
    v.id
}
