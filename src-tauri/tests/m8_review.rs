//! M8 acceptance: the review's surplus and every step equal `fixtures/EXPECTED.md` ("Weekly
//! review (M8)"; `fixtures/review.json` is the twin); a review completes only with exactly three
//! actions; history persists across unlock; snapshots are unique per civil day.

mod common;

use std::collections::HashMap;

use common::debts::{install_debts, DebtsFile};
use common::plan::{import_everything, install_plan, PlanFile};
use common::review::{by_name, complete_fixture_review, ReviewFile};
use common::ventures::{install_venture, VenturesFile};
use common::*;
use kept::config::DataPaths;
use kept::db::audit::{self, Actor};
use kept::db::{Db, OpenMode};
use kept::error::AppError;
use kept::review::{self, snapshot};
use rusqlite::Connection;

const PASS: &str = "correct horse battery staple";

/// Everything the earlier milestones install, on any connection.
fn install_everything(
    conn: &mut Connection,
) -> (
    Vec<(&'static str, kept::db::repo::account::Account)>,
    ReviewFile,
) {
    let accounts = fixture_accounts(conn);
    install_rules(conn);
    let plan: PlanFile = load_json("plan.json");
    import_everything(conn, &accounts, &plan.as_of);
    install_plan(conn, &accounts, &plan);
    reconcile_fixture_periods(conn, &accounts);
    let debts: DebtsFile = load_json("debts.json");
    install_debts(conn, &accounts, &debts, date(&debts.as_of));
    let ventures: VenturesFile = load_json("ventures.json");
    install_venture(conn, &accounts, &ventures);
    let file: ReviewFile = load_json("review.json");
    assert_eq!(file.as_of, plan.as_of);
    (accounts, file)
}

fn terms(items: &[review::Term]) -> HashMap<String, i64> {
    items
        .iter()
        .map(|t| (t.name.clone(), t.monthly_cents))
        .collect()
}

#[test]
fn the_surplus_and_every_step_equal_the_fixture() {
    let mut conn = memory_db();
    let (accounts, file) = install_everything(&mut conn);
    let today = date(&file.as_of);
    assert_eq!(file.income_window_days, review::SURPLUS_INCOME_DAYS);
    assert_eq!(file.horizon_days, review::HORIZON_DAYS);
    let cmd = audit::begin(&conn, "test.review", Actor::User).unwrap();
    let r = review::start(&conn, &cmd, today).unwrap();
    assert_eq!(r.status, "in_progress");
    assert_eq!(r.period_start, file.period_start);
    assert_eq!(r.period_end, file.as_of);
    assert!(
        review::start(&conn, &cmd, today).is_err(),
        "one review at a time"
    );

    let s = r.surplus.as_ref().expect("surplus stored at start");
    let want = &file.surplus;
    assert_eq!(s.income_window_cents, want.income_90_cents);
    assert_eq!(s.income_receipts, want.income_receipts);
    assert_eq!(s.income_cents, want.income_cents);
    assert_eq!(terms(&s.fixed_items), by_name(&want.fixed_items));
    assert_eq!(s.fixed_cents, want.fixed_cents);
    assert_eq!(terms(&s.debt_items), by_name(&want.debt_items));
    assert_eq!(
        s.informal_schedule_12m_cents,
        want.informal_schedule_12m_cents
    );
    assert_eq!(s.debt_service_cents, want.debt_service_cents);
    assert_eq!(terms(&s.irregular_items), by_name(&want.irregular_items));
    assert_eq!(s.irregular_cents, want.irregular_cents);
    assert_eq!(s.variable_cents, want.variable_cents);
    assert_eq!(s.surplus_cents, want.surplus_cents, "dependable surplus");
    assert_eq!(r.surplus_cents, Some(want.surplus_cents));
    assert_eq!(
        s.surplus_cents,
        s.income_cents
            - s.fixed_cents
            - s.debt_service_cents
            - s.irregular_cents
            - s.variable_cents
    );

    let st = &r.steps;
    let ws = &file.steps;
    for b in &ws.balances.accounts {
        let line = st
            .balances
            .accounts
            .iter()
            .find(|l| l.account_id == account_id(&accounts, &b.account))
            .unwrap_or_else(|| panic!("balance line for {}", b.account));
        assert_eq!(
            line.balance_cents, b.balance_cents,
            "{}: balance",
            b.account
        );
    }
    assert_eq!(st.balances.available_cents, ws.balances.available_cents);
    assert!(st.balances.trusted);
    assert!(st
        .balances
        .accounts
        .iter()
        .filter(|l| l.kind != "brokerage")
        .all(|l| l.trust == "reconciled"));

    assert_eq!(st.unreviewed.count, ws.unreviewed.count);
    assert_eq!(st.unreviewed.total_abs_cents, ws.unreviewed.total_abs_cents);
    let got: Vec<(String, i64)> = st
        .unreviewed
        .rows
        .iter()
        .map(|x| (x.posted_date.clone(), x.amount_cents))
        .collect();
    let want_rows: Vec<(String, i64)> = ws
        .unreviewed
        .rows
        .iter()
        .map(|x| (x.posted.clone(), x.amount_cents))
        .collect();
    assert_eq!(got, want_rows, "unreviewed rows, largest first");

    assert_eq!(st.obligations_14.count, ws.obligations_14.count);
    assert_eq!(
        st.obligations_14.expected_cents,
        ws.obligations_14.expected_cents
    );
    let got: Vec<(String, String, i64)> = st
        .obligations_14
        .items
        .iter()
        .map(|i| (i.name.clone(), i.due_date.clone(), i.expected_cents))
        .collect();
    let want_items: Vec<(String, String, i64)> = ws
        .obligations_14
        .items
        .iter()
        .map(|i| (i.obligation.clone(), i.due_date.clone(), i.expected_cents))
        .collect();
    assert_eq!(got, want_items);
    assert!(st.obligations_14.items.iter().all(|i| !i.overdue));

    assert_eq!(st.plan_variance.plan_snapshot_id, None);
    assert_eq!(st.plan_variance.variance_cents, None);
    assert_eq!(st.plan_variance.actual_cents, ws.balances.available_cents);
    assert_eq!(st.debts.total_debt_cents, ws.debts.total_debt_cents);
    assert_eq!(
        st.debts.informal_remaining_cents,
        ws.debts.informal_remaining_cents
    );
    assert_eq!(st.debts.previous_review_id, None);
    assert_eq!(st.ventures.cap_used_cents, ws.ventures.cap_used_cents);
    assert_eq!(st.ventures.cap_cents, ws.ventures.cap_cents);
    assert_eq!(
        st.ventures.ventures[0].utilization_bps,
        ws.ventures.utilization_bps
    );
    assert!(st.ventures.ventures[0].alerts.is_empty());
    let dates = |rows: &[review::RowRef]| -> Vec<(String, i64)> {
        rows.iter()
            .map(|x| (x.posted_date.clone(), x.amount_cents))
            .collect()
    };
    let spec_dates = |rows: &[common::review::RowSpec]| -> Vec<(String, i64)> {
        rows.iter()
            .map(|x| (x.posted.clone(), x.amount_cents))
            .collect()
    };
    assert_eq!(st.flags.since, file.period_start);
    assert_eq!(dates(&st.flags.borrowing), spec_dates(&ws.flags.borrowing));
    assert_eq!(
        dates(&st.flags.securities_sale),
        spec_dates(&ws.flags.securities_sale)
    );
    assert_eq!(
        dates(&st.flags.firewall_unacknowledged),
        spec_dates(&ws.flags.firewall_unacknowledged)
    );
    assert!(st.flags.firewall_acknowledged.is_empty());
}

#[test]
fn a_review_completes_only_with_exactly_three_actions_and_stores_what_it_showed() {
    let mut conn = memory_db();
    let (_accounts, file) = install_everything(&mut conn);
    let today = date(&file.as_of);
    let cmd = audit::begin(&conn, "test.review", Actor::User).unwrap();
    let r = review::start(&conn, &cmd, today).unwrap();
    let two = vec![file.actions[0].clone(), file.actions[1].clone()];
    let four = {
        let mut v = file.actions.clone();
        v.push("one more".into());
        v
    };
    let err = review::complete(&conn, &cmd, r.id, &two, "", today).expect_err("two actions");
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");
    let err = review::complete(&conn, &cmd, r.id, &four, "", today).expect_err("four actions");
    assert!(matches!(err, AppError::Validation { .. }), "{err:?}");
    let blank = vec![
        file.actions[0].clone(),
        "   ".into(),
        file.actions[2].clone(),
        "".into(),
    ];
    assert!(
        review::complete(&conn, &cmd, r.id, &blank, "", today).is_err(),
        "blank actions do not count"
    );
    assert_eq!(review::get(&conn, r.id).unwrap().status, "in_progress");
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM snapshot"),
        0,
        "nothing stored on refusal"
    );
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM review_action"), 0);

    // drafts: up to three, editable before the commit
    let draft = review::set_actions(&conn, &cmd, r.id, &two).unwrap();
    assert_eq!(draft.actions.len(), 2);
    assert!(review::set_actions(&conn, &cmd, r.id, &four).is_err());

    let done = review::complete(&conn, &cmd, r.id, &file.actions, "a good week", today).unwrap();
    assert_eq!(done.status, "completed");
    assert!(done.completed_at.is_some());
    assert_eq!(done.notes, "a good week");
    let texts: Vec<(i64, String)> = done
        .actions
        .iter()
        .map(|a| (a.position, a.text.clone()))
        .collect();
    assert_eq!(
        texts,
        file.actions
            .iter()
            .enumerate()
            .map(|(i, a)| (i as i64 + 1, a.clone()))
            .collect::<Vec<_>>()
    );
    assert!(done.actions.iter().all(|a| !a.done));
    let snap_id = done.snapshot_id.expect("a plan snapshot at completion");
    let snap = snapshot::get(&conn, snap_id).unwrap();
    let want = &file.snapshot;
    assert_eq!(snap.kind, "plan");
    assert_eq!(snap.civil_date, file.as_of);
    assert_eq!(
        (
            snap.safe_cents,
            snap.available_cents,
            snap.earmarks_cents,
            snap.obligations_cents,
            snap.buffer_cents,
            snap.trusted
        ),
        (
            want.safe_cents,
            want.available_cents,
            want.earmarks_cents,
            want.obligations_cents,
            want.buffer_cents,
            want.trusted
        )
    );
    assert_eq!(snap.total_debt_cents, want.total_debt_cents);
    assert_eq!(snap.informal_remaining_cents, want.informal_remaining_cents);
    assert_eq!(snap.venture_cap_used_cents, want.venture_cap_used_cents);
    assert_eq!(snap.detail.days.len(), 91);
    assert_eq!(snap.detail.review_id, Some(r.id));
    assert!(!snap.detail.accounts.is_empty());
    // the plan now overlays the forecast and feeds the next review's variance
    let f = kept::forecast::run(&conn, today, &kept::forecast::Scenario::default()).unwrap();
    assert_eq!(f.plan.map(|p| p.snapshot_id), Some(snap_id));
    assert!(
        review::complete(&conn, &cmd, r.id, &file.actions, "", today).is_err(),
        "completed stays completed"
    );

    // ticking an action, and the next review sees the previous totals
    let a = review::set_action_done(&conn, &cmd, done.actions[0].id, true).unwrap();
    assert!(a.done && a.done_at.is_some());
    // the next review, a day later: the plan's balance entering today is its closing for yesterday
    let tomorrow = date("2026-10-01");
    let next = review::start(&conn, &cmd, tomorrow).unwrap();
    assert_eq!(next.period_start, file.as_of);
    assert_eq!(next.period_end, "2026-10-01");
    assert_eq!(next.steps.debts.previous_review_id, Some(r.id));
    assert_eq!(
        next.steps.debts.previous_total_debt_cents,
        Some(want.total_debt_cents)
    );
    assert_eq!(next.steps.plan_variance.plan_snapshot_id, Some(snap_id));
    let available_tomorrow = kept::cash::safe::safe_to_spend(&conn, tomorrow)
        .unwrap()
        .terms
        .available
        .cents;
    let plan_yesterday = snap
        .detail
        .days
        .iter()
        .find(|d| d.date == file.as_of)
        .unwrap()
        .cents;
    assert_eq!(next.steps.plan_variance.plan_cents, Some(plan_yesterday));
    assert_eq!(next.steps.plan_variance.actual_cents, available_tomorrow);
    assert_eq!(
        next.steps.plan_variance.variance_cents,
        Some(available_tomorrow - plan_yesterday),
        "the variable spend the plan expected on the as-of day has not posted"
    );
    let abandoned = review::abandon(&conn, &cmd, next.id).unwrap();
    assert_eq!(abandoned.status, "abandoned");
    let history = review::list(&conn).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id, next.id, "newest first");
    // start, complete, start again, abandon: four review events
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM audit_event WHERE entity = 'review'"
        ),
        4
    );
}

#[test]
fn history_persists_across_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(dir.path().to_path_buf());
    let (review_id, surplus) = {
        let mut db = Db::open(&paths, PASS, OpenMode::CreateNew).unwrap();
        let (_accounts, file) = install_everything(db.conn_mut());
        let done = complete_fixture_review(db.conn(), &file);
        (done.id, done.surplus_cents)
    };
    let db = Db::open(&paths, PASS, OpenMode::Existing).unwrap();
    let history = review::list(db.conn()).unwrap();
    assert_eq!(history.len(), 1);
    let r = &history[0];
    assert_eq!(r.id, review_id);
    assert_eq!(r.status, "completed");
    assert_eq!(r.surplus_cents, surplus);
    assert_eq!(r.actions.len(), 3);
    assert!(r.surplus.is_some());
    assert_eq!(r.steps.unreviewed.count, 5);
    assert!(r.snapshot_id.is_some());
    assert_eq!(
        count(
            db.conn(),
            "SELECT COUNT(*) FROM snapshot WHERE kind = 'plan'"
        ),
        1
    );
}

#[test]
fn daily_snapshots_are_unique_per_civil_day_and_trends_read_them() {
    let mut conn = memory_db();
    let (_accounts, file) = install_everything(&mut conn);
    let today = date(&file.as_of);
    let cmd = audit::begin(&conn, "test.snapshot", Actor::User).unwrap();
    let first = snapshot::take_daily_if_missing(&conn, &cmd, today).unwrap();
    assert!(first.is_some());
    assert!(snapshot::take_daily_if_missing(&conn, &cmd, today)
        .unwrap()
        .is_none());
    assert!(matches!(
        snapshot::take(&conn, &cmd, today, "daily", snapshot::Detail::default()),
        Err(AppError::Conflict(_))
    ));
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM snapshot WHERE kind = 'daily'"),
        1
    );
    let demand =
        snapshot::take(&conn, &cmd, today, "on_demand", snapshot::Detail::default()).unwrap();
    assert_eq!(demand.safe_cents, file.snapshot.safe_cents);
    assert_eq!(demand.total_debt_cents, file.snapshot.total_debt_cents);
    assert!(snapshot::take(&conn, &cmd, today, "weekly", snapshot::Detail::default()).is_err());
    let tomorrow = date("2026-10-01");
    snapshot::take_daily_if_missing(&conn, &cmd, tomorrow)
        .unwrap()
        .expect("a new day");
    let trends = snapshot::trends(&conn).unwrap();
    assert_eq!(trends.len(), 2, "one point per civil day");
    assert_eq!(trends[0].civil_date, file.as_of);
    assert_eq!(trends[0].kind, "on_demand", "the latest of the day");
    assert_eq!(trends[1].civil_date, "2026-10-01");
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM audit_event WHERE entity = 'snapshot'"
        ),
        3
    );
}
