//! Shared helpers for integration tests: a fast in-memory database with the real migrations
//! (no SQLCipher key derivation per test), fixture accounts, and file loading.

#![allow(dead_code)]

pub mod debts;
pub mod forecast;
pub mod plan;

use std::path::{Path, PathBuf};

use kept::db::audit::{self, Actor};
use kept::db::migrate;
use kept::db::repo::account::{self, Account, NewAccount};
use kept::db::repo::rule::{self, RuleInput};
use kept::db::repo::venture::{self, VentureInput};
use rusqlite::Connection;

pub fn memory_db() -> Connection {
    let mut conn = Connection::open_in_memory().expect("in-memory sqlite");
    kept::db::apply_pragmas(&conn).expect("pragmas");
    migrate::apply_pending(&mut conn).expect("migrations");
    conn
}

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures")
}

pub fn fixture_bytes(rel: &str) -> Vec<u8> {
    let path = fixtures_dir().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

pub fn date(s: &str) -> kept::dates::CivilDate {
    kept::dates::parse_civil(s).expect("civil date")
}

/// The seven fixture accounts exactly as fixtures/EXPECTED.md lists them. Returns them keyed by
/// the short key used in EXPECTED.md.
pub fn fixture_accounts(conn: &Connection) -> Vec<(&'static str, Account)> {
    let specs: [(&str, &str, &str, &str, i64, bool); 7] = [
        (
            "nbc",
            "Northbank Checking",
            "Northbank",
            "checking",
            321_455,
            false,
        ),
        (
            "nbs",
            "Northbank Savings",
            "Northbank",
            "savings",
            1_200_000,
            false,
        ),
        (
            "rvc",
            "Riverside Checking",
            "Riverside Bank",
            "checking",
            105_000,
            false,
        ),
        (
            "sv",
            "Summit Visa",
            "Summit Card Services",
            "credit",
            -182_040,
            false,
        ),
        (
            "sa",
            "Summit Amex",
            "Summit Card Services",
            "credit",
            -31_218,
            false,
        ),
        (
            "hb",
            "Harbor Brokerage",
            "Harbor Securities",
            "brokerage",
            40_000,
            true,
        ),
        ("vm", "Venmo", "Venmo", "payment_app", 0, false),
    ];
    let cmd = audit::begin(conn, "test.accounts", Actor::User).expect("command");
    specs
        .iter()
        .map(|(key, name, institution, kind, opening, firewalled)| {
            let acct = account::create(
                conn,
                &cmd,
                &NewAccount {
                    name: (*name).to_string(),
                    institution: (*institution).to_string(),
                    kind: (*kind).to_string(),
                    opening_balance_cents: *opening,
                    opening_date: "2026-07-01".to_string(),
                    venture_id: None,
                    firewalled: *firewalled,
                },
            )
            .expect("create account");
            (*key, acct)
        })
        .collect()
}

pub fn account_id(accounts: &[(&str, Account)], key: &str) -> i64 {
    accounts
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, a)| a.id)
        .expect("account key")
}

/// opening + Σ posted leaf rows dated ≤ `through` (EXPECTED.md "Closing per statement month").
pub fn closing(conn: &Connection, acct: &Account, through: &str) -> i64 {
    let sum: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf WHERE account_id = ?1 AND status = 'posted' AND posted_date <= ?2",
            rusqlite::params![acct.id, through],
            |r| r.get(0),
        )
        .expect("closing");
    acct.opening_balance_cents + sum
}

pub fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect(sql)
}

pub fn load_json<T: serde::de::DeserializeOwned>(rel: &str) -> T {
    serde_json::from_slice(&fixture_bytes(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

pub fn category_id(conn: &Connection, code: &str) -> i64 {
    conn.query_row(
        "SELECT id FROM category WHERE system_code = ?1",
        [code],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("category {code}: {e}"))
}

#[derive(serde::Deserialize)]
pub struct RulesFile {
    pub venture: VentureSpec,
    pub rules: Vec<RuleSpec>,
}

#[derive(serde::Deserialize)]
pub struct VentureSpec {
    pub name: String,
    pub status: String,
    pub cash_cap_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct RuleSpec {
    pub name: String,
    pub match_payee_contains: String,
    pub category_code: String,
    pub venture: Option<String>,
}

/// The venture and the ordered rules of fixtures/rules.json, installed before any import.
pub fn install_rules(conn: &Connection) {
    let file: RulesFile = load_json("rules.json");
    let cmd = audit::begin(conn, "test.rules", Actor::User).expect("command");
    let v = venture::create(
        conn,
        &cmd,
        &VentureInput {
            name: file.venture.name.clone(),
            status: file.venture.status.clone(),
            cash_cap_cents: file.venture.cash_cap_cents,
            time_budget_hours: None,
            milestone: String::new(),
            milestone_date: None,
            stop_condition: String::new(),
        },
    )
    .expect("create venture");
    for spec in &file.rules {
        rule::create(
            conn,
            &cmd,
            &RuleInput {
                name: spec.name.clone(),
                match_payee_contains: Some(spec.match_payee_contains.clone()),
                action_category_id: Some(category_id(conn, &spec.category_code)),
                action_venture_id: spec.venture.as_ref().map(|_| v.id),
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("rule {}: {e:?}", spec.name));
    }
}

/// Enter every balanced monthly period of fixtures/recon.json (scenario A) so the hero is trusted.
pub fn reconcile_fixture_periods(conn: &Connection, accounts: &[(&str, Account)]) {
    let recon: serde_json::Value = load_json("recon.json");
    let cmd = audit::begin(conn, "test.reconcile_all", Actor::User).expect("command");
    for p in recon["periods"].as_array().expect("periods") {
        kept::cash::recon::reconcile(
            conn,
            &cmd,
            &kept::cash::recon::ReconInput {
                account_id: account_id(accounts, p["account"].as_str().expect("account")),
                period_end: p["period_end"].as_str().expect("period_end").to_string(),
                statement_closing_cents: p["statement_cents"].as_i64().expect("statement"),
                statement_source: "user".to_string(),
            },
        )
        .expect("reconcile");
    }
}
