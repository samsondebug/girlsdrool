//! The 200k-row performance fixture (M10, ADR-0047). Every number is derived from one seed
//! with integer arithmetic, never typed in: dates spread over ten years per account, amounts
//! stepping through a cycle that keeps any seven-day window free of equal amounts (so the
//! dedup pass finds no fuzzy candidates and every row inserts), payees from the fixture's own
//! list. The rows go through the real import pipeline into a real SQLCipher file, and the
//! timings `docs/performance.md` records are printed here.
//!
//! Ignored by default (it takes minutes): `KEPT_PERF=1 cargo test --no-default-features
//! --test perf_200k -- --ignored --nocapture`.

mod common;

use std::fmt::Write as _;
use std::time::Instant;

use chrono::{Datelike, Duration, NaiveDate};
use common::*;
use kept::cash::safe;
use kept::config::DataPaths;
use kept::db::audit::{self, Actor};
use kept::db::repo::account::{self, Account, NewAccount};
use kept::db::repo::ledger::{self, Cursor, LedgerFilter};
use kept::db::{Db, OpenMode};
use kept::import::{self, ImportInput};

const ROWS: usize = 200_000;
const YEARS: [i32; 10] = [2017, 2018, 2019, 2020, 2021, 2022, 2023, 2024, 2025, 2026];
const TODAY: &str = "2026-09-30";
const PASS: &str = "correct horse battery staple";
const PAYEES: [&str; 12] = [
    "JEWEL-OSCO #3421",
    "SHELL OIL 57442",
    "MERIDIAN CAP ACH PAYROLL",
    "NETFLIX.COM",
    "COMED ELECTRIC",
    "AMAZON.COM*HG6 AMZN.COM/BILL",
    "TRADER JOE S #702",
    "CHIPOTLE 1121",
    "T-MOBILE *AUTOPAY",
    "LAKESHORE PROPERTIES RENT",
    "ONLINE TRANSFER TO SAV ...5678",
    "ZELLE PAYMENT FROM MORGAN AVERY",
];

/// The seven fixture accounts, opened on the first day of the ten years so every row is after
/// its account's opening date.
fn perf_accounts(conn: &rusqlite::Connection) -> Vec<(&'static str, Account)> {
    let specs: [(&str, &str, &str, &str, i64); 7] = [
        (
            "nbc",
            "Northbank Checking",
            "Northbank",
            "checking",
            321_455,
        ),
        (
            "nbs",
            "Northbank Savings",
            "Northbank",
            "savings",
            1_200_000,
        ),
        (
            "rvc",
            "Riverside Checking",
            "Riverside Bank",
            "checking",
            105_000,
        ),
        (
            "sv",
            "Summit Visa",
            "Summit Card Services",
            "credit",
            -182_040,
        ),
        (
            "sa",
            "Summit Amex",
            "Summit Card Services",
            "credit",
            -31_218,
        ),
        (
            "hb",
            "Harbor Brokerage",
            "Harbor Securities",
            "brokerage",
            40_000,
        ),
        ("vm", "Venmo", "Venmo", "payment_app", 0),
    ];
    let cmd = audit::begin(conn, "perf.accounts", Actor::User).expect("command");
    specs
        .iter()
        .map(|(key, name, institution, kind, opening)| {
            let acct = account::create(
                conn,
                &cmd,
                &NewAccount {
                    name: (*name).to_string(),
                    institution: (*institution).to_string(),
                    kind: (*kind).to_string(),
                    opening_balance_cents: *opening,
                    opening_date: format!("{}-01-01", YEARS[0]),
                    venture_id: None,
                    firewalled: *key == "hb",
                },
            )
            .expect("create account");
            (*key, acct)
        })
        .collect()
}

/// One account's share of the rows, as (date, description, cents): the i-th row of an account
/// lands on day `i * days / n` of its ten years, owes `-(100 + (i * 37) % 20000)` cents, and
/// every 25th row is an inflow of `150000 + (i * 53) % 100000` cents.
fn rows_for(account_index: usize, n: usize) -> Vec<(NaiveDate, String, i64)> {
    let start = NaiveDate::from_ymd_opt(YEARS[0], 1, 1).expect("date");
    let end = NaiveDate::from_ymd_opt(2026, 9, 30).expect("date");
    let days = (end - start).num_days() as usize;
    (0..n)
        .map(|i| {
            let day = start + Duration::days((i * days / n) as i64);
            let k = i + account_index * 7919;
            let cents = if i % 25 == 24 {
                150_000 + ((k * 53) % 100_000) as i64
            } else {
                -(100 + ((k * 37) % 20_000) as i64)
            };
            let payee = PAYEES[k % PAYEES.len()];
            (day, format!("{payee} {}", 1000 + k % 9000), cents)
        })
        .collect()
}

fn csv_for(rows: &[(NaiveDate, String, i64)]) -> Vec<u8> {
    let mut out = String::from("Date,Description,Amount\n");
    for (day, desc, cents) in rows {
        let _ = writeln!(
            out,
            "{},{},{}",
            day.format("%Y-%m-%d"),
            desc,
            kept::money::to_decimal_string(*cents)
        );
    }
    out.into_bytes()
}

#[test]
#[ignore]
fn two_hundred_thousand_rows_import_page_and_recompute_within_budget() {
    if std::env::var_os("KEPT_PERF").is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(dir.path().join("kept"));
    let mut db = Db::open(&paths, PASS, OpenMode::CreateNew).unwrap();
    let accounts = perf_accounts(db.conn());
    install_rules(db.conn());
    let per_account = ROWS / accounts.len();
    let remainder = ROWS - per_account * accounts.len();
    let today = date(TODAY);

    let mut expected_sum: i64 = 0;
    let mut batches = 0usize;
    let mut slowest_ms: u128 = 0;
    let import_started = Instant::now();
    for (ai, (key, acct)) in accounts.iter().enumerate() {
        let n = per_account + if ai == 0 { remainder } else { 0 };
        let rows = rows_for(ai, n);
        expected_sum += rows.iter().map(|r| r.2).sum::<i64>();
        for year in YEARS {
            let slice: Vec<_> = rows
                .iter()
                .filter(|r| r.0.year() == year)
                .cloned()
                .collect();
            if slice.is_empty() {
                continue;
            }
            let started = Instant::now();
            let report = import::commit(
                db.conn_mut(),
                &ImportInput {
                    account_id: acct.id,
                    profile_id: None,
                    file_name: format!("{key}-{year}.csv"),
                    bytes: csv_for(&slice),
                },
                today,
                8500,
            )
            .unwrap_or_else(|e| panic!("{key} {year}: {e:?}"));
            assert_eq!(
                report.inserted.len(),
                slice.len(),
                "{key} {year} inserted every row"
            );
            assert!(
                report.quarantined.is_empty(),
                "{key} {year} quarantined nothing"
            );
            slowest_ms = slowest_ms.max(started.elapsed().as_millis());
            batches += 1;
        }
    }
    let import_ms = import_started.elapsed().as_millis();

    let count = common::count(db.conn(), "SELECT count(*) FROM txn_leaf");
    assert_eq!(count, ROWS as i64);
    assert_eq!(
        common::count(db.conn(), "SELECT SUM(amount_cents) FROM txn_leaf"),
        expected_sum
    );

    let started = Instant::now();
    let first = ledger::query(db.conn(), &LedgerFilter::default(), None, 200).unwrap();
    let first_page_ms = started.elapsed().as_millis();
    assert_eq!(first.rows.len(), 200);

    let started = Instant::now();
    let deep = ledger::query(
        db.conn(),
        &LedgerFilter::default(),
        Some(&Cursor {
            posted_date: "2021-12-31".into(),
            id: i64::MAX,
        }),
        200,
    )
    .unwrap();
    let deep_page_ms = started.elapsed().as_millis();
    assert_eq!(deep.rows.len(), 200);

    let started = Instant::now();
    let filtered = ledger::query(
        db.conn(),
        &LedgerFilter {
            text: Some("jewel".into()),
            ..LedgerFilter::default()
        },
        None,
        200,
    )
    .unwrap();
    let filter_ms = started.elapsed().as_millis();
    assert!(filtered.total_rows > 0);

    let started = Instant::now();
    let hero = safe::safe_to_spend(db.conn(), today).unwrap();
    let hero_cold_ms = started.elapsed().as_millis();
    assert!(hero.safe_cents != 0);
    let started = Instant::now();
    safe::safe_to_spend(db.conn(), today).unwrap();
    let hero_ms = started.elapsed().as_millis();

    // where the hero's time goes: one account's posted balance, the flagged-inflow scan, trust
    let nbc = &accounts[0].1;
    let started = Instant::now();
    safe::posted_balance_as_of(db.conn(), nbc, today).unwrap();
    let balance_ms = started.elapsed().as_millis();
    let started = Instant::now();
    let flagged: i64 = db
        .conn()
        .query_row(
            "SELECT count(*) FROM txn_leaf WHERE account_id = ?1 AND status = 'posted' AND flags <> 0 AND amount_cents > 0 AND (flags & 24) <> 0 AND posted_date <= ?2",
            rusqlite::params![nbc.id, TODAY],
            |r| r.get(0),
        )
        .unwrap();
    let flagged_ms = started.elapsed().as_millis();
    let started = Instant::now();
    kept::cash::recon::trust(db.conn(), today, 45).unwrap();
    let trust_ms = started.elapsed().as_millis();
    for (label, sql) in [
        ("balance", "EXPLAIN QUERY PLAN SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf WHERE account_id = 1 AND status = 'posted' AND posted_date <= '2026-09-30'"),
        ("flagged", "EXPLAIN QUERY PLAN SELECT id FROM txn_leaf WHERE account_id = 1 AND status = 'posted' AND flags <> 0 AND amount_cents > 0 AND (flags & 24) <> 0 AND posted_date <= '2026-09-30'"),
        ("page", "EXPLAIN QUERY PLAN SELECT t.id FROM txn_leaf t WHERE 1 AND (t.posted_date, t.id) < ('2021-12-31', 9223372036854775807) ORDER BY t.posted_date DESC, t.id DESC LIMIT 201"),
    ] {
        let mut stmt = db.conn().prepare(sql).unwrap();
        let plan: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        println!("KEPT_PLAN {label}: {}", plan.join(" | "));
    }
    println!("KEPT_PERF balance_one_account_ms={balance_ms} flagged_scan_ms={flagged_ms} trust_ms={trust_ms} flagged_rows={flagged}");

    let started = Instant::now();
    db.conn()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    let bytes = std::fs::metadata(&paths.db).unwrap().len();
    let checkpoint_ms = started.elapsed().as_millis();

    println!("KEPT_PERF rows={ROWS} batches={batches}");
    println!("KEPT_PERF import_total_ms={import_ms} slowest_batch_ms={slowest_ms}");
    println!("KEPT_PERF first_page_ms={first_page_ms} deep_page_ms={deep_page_ms} text_filter_ms={filter_ms}");
    println!(
        "KEPT_PERF hero_cold_ms={hero_cold_ms} hero_ms={hero_ms} checkpoint_ms={checkpoint_ms} db_bytes={bytes}"
    );
    // The budget (ARCHITECTURE §15) is judged on the optimized build; a debug run reports only.
    if !cfg!(debug_assertions) {
        assert!(import_ms < 60_000, "import {import_ms} ms");
        assert!(first_page_ms < 50, "first page {first_page_ms} ms");
        assert!(deep_page_ms < 50, "deep page {deep_page_ms} ms");
        assert!(hero_ms < 100, "hero {hero_ms} ms");
    }
}
