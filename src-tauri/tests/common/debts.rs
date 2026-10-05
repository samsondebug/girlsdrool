//! The M6 debts and informal loans exactly as fixtures/debts.json states them, for the
//! acceptance tests and the seed.

use std::collections::HashMap;

use kept::db::audit::{self, Actor};
use kept::debt::{self, informal};
use rusqlite::Connection;

use super::account_id;

#[derive(serde::Deserialize)]
pub struct DebtsFile {
    pub as_of: String,
    pub extra_cents: i64,
    pub periods_max: u32,
    pub scenario_periods: u32,
    pub debts: Vec<DebtSpec>,
    pub informal: Vec<InformalSpec>,
    pub total_debt_cents: i64,
    pub informal_remaining_cents: i64,
    pub strategies: HashMap<String, RunSpec>,
    pub informal_scenarios: Vec<ScenarioSpec>,
    pub minimum_obligations: Vec<MinimumSpec>,
}

#[derive(serde::Deserialize)]
pub struct DebtSpec {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub account: Option<String>,
    pub standalone_opening: Option<i64>,
    pub apr_bps: i64,
    pub promo_apr_bps: Option<i64>,
    pub promo_end: Option<String>,
    pub interest_method: String,
    pub minimum_rule: String,
    pub minimum_fixed_cents: i64,
    pub minimum_bps: i64,
    pub minimum_floor_cents: i64,
    pub due_day: Option<i64>,
    pub participation: bool,
    pub custom_order: Option<i64>,
    pub payment_account: Option<String>,
    pub match_payee_contains: Option<String>,
    pub owed_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct ProceedsSpec {
    pub account: String,
    pub posted: String,
    pub description: String,
}

#[derive(serde::Deserialize)]
pub struct ScheduleSpec {
    pub due_date: String,
    pub amount_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct RepaymentSpec {
    pub due_date: String,
    pub account: String,
    pub posted: String,
    pub description: String,
    pub amount_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct InformalSpec {
    pub key: String,
    pub counterparty: String,
    pub original_cents: i64,
    pub borrowed_date: String,
    pub promised_terms: String,
    pub promised_date: Option<String>,
    pub proceeds: Option<ProceedsSpec>,
    pub repayment_account: String,
    pub repayment_needle: String,
    pub schedule: Vec<ScheduleSpec>,
    pub participation: bool,
    pub repayments: Vec<RepaymentSpec>,
    pub remaining_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct RunSpec {
    pub strategy: String,
    pub extra_cents: i64,
    pub budget_cents: i64,
    pub total_interest_cents: i64,
    pub payoff_date: Option<String>,
    pub debts: Vec<RunDebtSpec>,
}

#[derive(serde::Deserialize)]
pub struct RunDebtSpec {
    pub key: String,
    pub name: String,
    pub informal: bool,
    pub owed_cents: i64,
    pub total_interest_cents: i64,
    pub payoff_date: Option<String>,
    pub periods: Vec<PeriodSpec>,
}

#[derive(serde::Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct PeriodSpec {
    pub period: u32,
    pub start: String,
    pub end: String,
    pub opening_cents: i64,
    pub interest_cents: i64,
    pub minimum_cents: i64,
    pub payment_cents: i64,
    pub closing_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct ScenarioSpec {
    pub extra_cents: i64,
    pub periods: u32,
    pub remaining_cents: i64,
    pub achievable: bool,
    pub gap_cents: i64,
    pub payoff_date: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct MinimumSpec {
    pub debt: String,
    pub name: String,
    pub due_day: i64,
    pub expected_cents: i64,
    pub source_account: String,
    pub match_payee_contains: String,
}

/// The ledger row a fixture spec names, by account, posted date and payee.
pub fn row_id(
    conn: &Connection,
    accounts: &[(&str, kept::db::repo::account::Account)],
    account: &str,
    posted: &str,
    description: &str,
) -> i64 {
    conn.query_row(
        "SELECT id FROM txn WHERE account_id = ?1 AND posted_date = ?2 AND payee_raw LIKE ?3 ORDER BY id LIMIT 1",
        rusqlite::params![account_id(accounts, account), posted, format!("%{description}%")],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("row {account} {posted} {description}: {e}"))
}

/// Create every debt and informal loan of the file, then run the refresh every write runs.
/// Returns fixture key → debt id.
pub fn install_debts(
    conn: &mut Connection,
    accounts: &[(&str, kept::db::repo::account::Account)],
    file: &DebtsFile,
    today: kept::dates::CivilDate,
) -> HashMap<String, i64> {
    let cmd = audit::begin(conn, "test.debts", Actor::User).unwrap();
    let mut ids = HashMap::new();
    for d in &file.debts {
        let created = debt::create(
            conn,
            &cmd,
            &debt::DebtInput {
                name: d.name.clone(),
                kind: d.kind.clone(),
                account_id: d.account.as_deref().map(|a| account_id(accounts, a)),
                apr_bps: d.apr_bps,
                promo_apr_bps: d.promo_apr_bps,
                promo_end: d.promo_end.clone(),
                interest_method: d.interest_method.clone(),
                minimum_rule: d.minimum_rule.clone(),
                minimum_fixed_cents: d.minimum_fixed_cents,
                minimum_bps: d.minimum_bps,
                minimum_floor_cents: d.minimum_floor_cents,
                due_day: d.due_day,
                strategy_participation: d.participation,
                custom_order: d.custom_order,
                standalone_opening_cents: d.standalone_opening,
                standalone_opening_date: d.standalone_opening.map(|_| file.as_of.clone()),
                active: true,
                payment_account_id: d
                    .payment_account
                    .as_deref()
                    .map(|a| account_id(accounts, a)),
                match_payee_contains: d.match_payee_contains.clone(),
            },
        )
        .unwrap_or_else(|e| panic!("debt {}: {e:?}", d.name));
        ids.insert(d.key.clone(), created.id);
    }
    for l in &file.informal {
        let proceeds = l
            .proceeds
            .as_ref()
            .map(|p| row_id(conn, accounts, &p.account, &p.posted, &p.description));
        let loan = informal::create(
            conn,
            &cmd,
            &informal::InformalInput {
                counterparty: l.counterparty.clone(),
                original_cents: l.original_cents,
                borrowed_date: l.borrowed_date.clone(),
                promised_terms: l.promised_terms.clone(),
                promised_date: l.promised_date.clone(),
                proceeds_txn_id: proceeds,
                payment_account_id: Some(account_id(accounts, &l.repayment_account)),
                match_payee_contains: Some(l.repayment_needle.clone()),
                strategy_participation: l.participation,
                active: true,
            },
            today,
        )
        .unwrap_or_else(|e| panic!("loan {}: {e:?}", l.counterparty));
        for s in &l.schedule {
            informal::add_schedule_row(conn, &cmd, loan.debt_id, &s.due_date, s.amount_cents)
                .unwrap();
        }
        ids.insert(l.key.clone(), loan.debt_id);
    }
    debt::refresh(conn, &cmd, today).unwrap();
    ids
}
