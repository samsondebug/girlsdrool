//! M7 acceptance: the fixture's SaaS charges and card payments land in the right buckets, the cap
//! gauge equals `fixtures/EXPECTED.md` ("Ventures (M7)"; `fixtures/ventures.json` is the twin),
//! the countdown and alerts fire for the right reasons, and an expense paid from a personal
//! account counts toward the cap.

mod common;

use common::plan::{import_everything, install_plan, PlanFile};
use common::ventures::{install_venture, VenturesFile};
use common::*;
use kept::db::audit::{self, Actor};
use kept::db::repo::{account, txn, venture};
use kept::venture as rollup;
use rusqlite::Connection;

fn setup() -> (
    Connection,
    Vec<(&'static str, account::Account)>,
    VenturesFile,
    i64,
) {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    let plan: PlanFile = load_json("plan.json");
    import_everything(&mut conn, &accounts, &plan.as_of);
    install_plan(&mut conn, &accounts, &plan);
    let file: VenturesFile = load_json("ventures.json");
    assert_eq!(file.as_of, plan.as_of);
    let id = install_venture(&conn, &accounts, &file);
    (conn, accounts, file, id)
}

#[test]
fn saas_charges_and_card_payments_land_in_their_buckets_and_the_gauge_matches() {
    let (conn, accounts, file, id) = setup();
    let as_of = date(&file.as_of);
    assert_eq!(file.trailing_months, rollup::TRAILING_MONTHS);
    let summary = rollup::summary(&conn, as_of).unwrap();
    assert_eq!(summary.ventures.len(), 1);
    let r = &summary.ventures[0];
    let want = &file.rollup;
    assert_eq!(r.venture.id, id);
    assert_eq!(r.venture.name, want.name);
    assert_eq!(r.window_start, want.window_start);
    let got: Vec<(&str, i64, usize)> = vec![
        (
            "customer_revenue",
            r.customer_revenue.cents,
            r.customer_revenue.rows,
        ),
        (
            "operating_expense",
            r.operating_expense.cents,
            r.operating_expense.rows,
        ),
        (
            "owner_contribution",
            r.owner_contribution.cents,
            r.owner_contribution.rows,
        ),
        ("financing", r.financing.cents, r.financing.rows),
        ("withdrawal", r.withdrawal.cents, r.withdrawal.rows),
    ];
    for (name, cents, rows) in got {
        let b = &want.buckets[name];
        assert_eq!((cents, rows), (b.cents, b.rows), "bucket {name}");
    }
    assert_eq!(
        r.operating_expense_from_personal_cents,
        want.operating_expense_from_personal_cents
    );
    assert_eq!(r.operating_cash_flow_cents, want.operating_cash_flow_cents);
    assert_eq!(r.venture.cash_cap_cents, want.cap_cents);
    assert_eq!(r.cap_used_cents, want.cap_used_cents, "cap used");
    assert_eq!(r.cap_remaining_cents, want.cap_remaining_cents);
    assert_eq!(r.cap_utilization_bps, want.cap_utilization_bps, "the gauge");
    assert_eq!(r.milestone_days, Some(want.milestone_days));
    assert_eq!(r.alerts, want.alerts);
    assert_eq!(summary.take_home_cents, want.take_home_cents);
    assert_eq!(summary.spend_share_bps, want.spend_share_bps);
    assert_eq!(summary.total_cap_used_cents, want.cap_used_cents);
    assert_eq!(summary.total_cap_cents, want.cap_cents);
    let owned: Vec<(i64, i64)> = r
        .accounts
        .iter()
        .map(|a| (a.account_id, a.balance_cents))
        .collect();
    let want_owned: Vec<(i64, i64)> = want
        .accounts
        .iter()
        .map(|a| (account_id(&accounts, &a.account), a.balance_cents))
        .collect();
    assert_eq!(owned, want_owned);
    // the venture-owned card is out of the hero's account set, as before
    let hero = kept::cash::safe::safe_to_spend(&conn, as_of).unwrap();
    assert!(hero
        .terms
        .available
        .accounts
        .iter()
        .all(|a| a.account_id != account_id(&accounts, "sa")));
}

#[test]
fn the_alert_fires_for_a_used_cap_and_a_passed_milestone_and_the_verdict_is_the_persons() {
    let (conn, _accounts, file, id) = setup();
    let as_of = date(&file.as_of);
    let cmd = audit::begin(&conn, "test.ventures", Actor::User).unwrap();
    let v = venture::get(&conn, id).unwrap();
    let mut input = venture::VentureInput {
        name: v.name.clone(),
        status: "freeze".into(),
        cash_cap_cents: 50_000,
        time_budget_hours: v.time_budget_hours,
        milestone: v.milestone.clone(),
        milestone_date: v.milestone_date.clone(),
        stop_condition: v.stop_condition.clone(),
    };
    venture::update(&conn, &cmd, id, &input).unwrap();
    let r = &rollup::summary(&conn, as_of).unwrap().ventures[0];
    assert_eq!(r.venture.status, "freeze");
    assert_eq!(r.alerts, vec!["cap used".to_string()]);
    assert_eq!(r.cap_utilization_bps, 10_884);
    assert!(r.cap_remaining_cents < 0);
    // the milestone date passes
    let later = date("2027-01-15");
    let r = &rollup::summary(&conn, later).unwrap().ventures[0];
    assert_eq!(r.milestone_days, Some(-15));
    assert_eq!(
        r.alerts,
        vec!["cap used".to_string(), "milestone date passed".to_string()]
    );
    // a comfortable cap and a future milestone: nothing fires
    input.cash_cap_cents = file.venture.cash_cap_cents;
    input.status = "fund".into();
    venture::update(&conn, &cmd, id, &input).unwrap();
    let r = &rollup::summary(&conn, as_of).unwrap().ventures[0];
    assert!(r.alerts.is_empty());
}

#[test]
fn an_operating_expense_paid_from_a_personal_account_counts_toward_the_cap() {
    let (conn, accounts, file, id) = setup();
    let as_of = date(&file.as_of);
    let before = rollup::summary(&conn, as_of).unwrap();
    let base = &before.ventures[0];
    // the person tags a personal-card row as a Ledgerline expense
    let uber: (i64, i64) = conn
        .query_row(
            "SELECT id, amount_cents FROM txn WHERE account_id = ?1 AND posted_date = '2026-09-13' AND payee_norm LIKE '%uber%'",
            [account_id(&accounts, "sv")],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let cmd = audit::begin(&conn, "test.ventures", Actor::User).unwrap();
    txn::apply_user_patch(
        &conn,
        &cmd,
        uber.0,
        &txn::TxnPatch {
            payee_norm: None,
            memo: None,
            category_id: Some(Some(category_id(&conn, "venture.operating_expense"))),
            tags: None,
            venture_id: Some(Some(id)),
            flags: None,
            effective_date: None,
            status: None,
        },
        as_of,
    )
    .unwrap();
    let after = rollup::summary(&conn, as_of).unwrap();
    let r = &after.ventures[0];
    let extra = -uber.1;
    assert_eq!(
        r.operating_expense.cents,
        base.operating_expense.cents + extra
    );
    assert_eq!(r.operating_expense.rows, base.operating_expense.rows + 1);
    assert_eq!(r.operating_expense_from_personal_cents, extra);
    assert_eq!(
        r.cap_used_cents,
        base.cap_used_cents + extra,
        "personal-account expense counts (ADR-0025)"
    );
    assert_eq!(
        r.operating_cash_flow_cents,
        base.operating_cash_flow_cents - extra
    );
    assert!(after.spend_share_bps > before.spend_share_bps);
    assert_eq!(after.take_home_cents, before.take_home_cents);
}
