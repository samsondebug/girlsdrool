//! M6 acceptance: balances owed, minimum obligations, informal loans found in the ledger, and the
//! avalanche / snowball / custom schedules equal `fixtures/EXPECTED.md` ("Debts and informal
//! loans (M6)"; `fixtures/debts.json` is the twin) to the cent in every period.

mod common;

use common::debts::{install_debts, row_id, DebtsFile, PeriodSpec};
use common::forecast::ForecastFile;
use common::plan::{import_everything, install_plan, PlanFile};
use common::*;
use kept::debt::{self, informal, strategy};
use kept::forecast::{self, Scenario};
use kept::import::csv::{FLAG_BORROWING, FLAG_NEEDS_REVIEW};
use kept::plan::obligation;
use rusqlite::Connection;
use std::collections::HashMap;

struct Setup {
    conn: Connection,
    accounts: Vec<(&'static str, kept::db::repo::account::Account)>,
    plan: PlanFile,
    file: DebtsFile,
    ids: HashMap<String, i64>,
}

fn setup() -> Setup {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    let plan: PlanFile = load_json("plan.json");
    import_everything(&mut conn, &accounts, &plan.as_of);
    install_plan(&mut conn, &accounts, &plan);
    reconcile_fixture_periods(&conn, &accounts);
    let file: DebtsFile = load_json("debts.json");
    assert_eq!(file.as_of, plan.as_of);
    let ids = install_debts(&mut conn, &accounts, &file, date(&file.as_of));
    Setup {
        conn,
        accounts,
        plan,
        file,
        ids,
    }
}

#[test]
fn balances_minimums_and_informal_loans_match_the_fixture() {
    let s = setup();
    let today = date(&s.file.as_of);
    let views = debt::views(&s.conn, today).unwrap();
    for spec in &s.file.debts {
        let v = views
            .iter()
            .find(|v| v.debt.id == s.ids[&spec.key])
            .unwrap_or_else(|| panic!("view for {}", spec.key));
        assert_eq!(v.owed_cents, spec.owed_cents, "{}: owed", spec.key);
        assert_eq!(v.debt.kind, spec.kind);
    }
    assert_eq!(
        debt::total_owed(&s.conn, today).unwrap(),
        s.file.total_debt_cents
    );
    assert_eq!(
        informal::total_remaining(&s.conn, today).unwrap(),
        s.file.informal_remaining_cents
    );

    let loans = informal::list(&s.conn, today).unwrap();
    for spec in &s.file.informal {
        let loan = loans
            .iter()
            .find(|l| l.debt_id == s.ids[&spec.key])
            .unwrap_or_else(|| panic!("loan {}", spec.key));
        assert_eq!(
            loan.remaining_cents, spec.remaining_cents,
            "{}: remaining",
            spec.key
        );
        assert_eq!(loan.original_cents, spec.original_cents);
        assert_eq!(loan.schedule.len(), spec.schedule.len());
        let got: Vec<(String, i64)> = loan
            .repayments
            .iter()
            .map(|p| (p.paid_date.clone(), p.amount_cents))
            .collect();
        let want: Vec<(String, i64)> = spec
            .repayments
            .iter()
            .map(|r| (r.posted.clone(), r.amount_cents))
            .collect();
        assert_eq!(got, want, "{}: repayments", spec.key);
        for (p, r) in loan.repayments.iter().zip(&spec.repayments) {
            let id = row_id(&s.conn, &s.accounts, &r.account, &r.posted, &r.description);
            assert_eq!(p.txn_id, Some(id), "{}: repayment row", spec.key);
            // a repayment is a transfer to a liability, never an expense
            let root: String = s
                .conn
                .query_row(
                    "SELECT c.root_kind FROM txn t JOIN category c ON c.id = t.category_id WHERE t.id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(root, "transfer");
        }
        assert!(loan.schedule.iter().all(|r| r.unpaid_cents == 0) || spec.remaining_cents > 0);
        if let Some(p) = &spec.proceeds {
            let id = row_id(&s.conn, &s.accounts, &p.account, &p.posted, &p.description);
            assert_eq!(loan.proceeds_txn_id, Some(id));
            let (flags, code): (i64, String) = s
                .conn
                .query_row(
                    "SELECT t.flags, c.system_code FROM txn t JOIN category c ON c.id = t.category_id WHERE t.id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_ne!(
                flags & i64::from(FLAG_BORROWING),
                0,
                "proceeds flagged borrowing"
            );
            assert_eq!(
                flags & i64::from(FLAG_NEEDS_REVIEW),
                0,
                "proceeds leave the review queue"
            );
            assert_eq!(code, "transfer.borrowing_proceeds");
        }
    }

    // one confirmed debt_minimum obligation per debt that owes something and has a rule
    let obligations = obligation::list(&s.conn).unwrap();
    let minimums: Vec<&obligation::Obligation> = obligations
        .iter()
        .filter(|o| o.kind == "debt_minimum")
        .collect();
    assert_eq!(minimums.len(), s.file.minimum_obligations.len());
    for spec in &s.file.minimum_obligations {
        let ob = minimums
            .iter()
            .find(|o| o.debt_id == Some(s.ids[&spec.debt]))
            .unwrap_or_else(|| panic!("minimum obligation for {}", spec.debt));
        assert_eq!(ob.name, spec.name);
        assert_eq!(ob.status, "confirmed");
        assert_eq!(ob.due_rule, "monthly_day");
        assert_eq!(ob.due_day, Some(spec.due_day));
        assert_eq!(
            ob.expected_cents, spec.expected_cents,
            "{}: expected",
            spec.debt
        );
        assert_eq!(
            ob.source_account_id,
            account_id(&s.accounts, &spec.source_account)
        );
        assert_eq!(
            ob.match_payee_contains.as_deref(),
            Some(spec.match_payee_contains.as_str())
        );
        assert_eq!(
            ob.anchor_date.as_deref(),
            Some(s.file.as_of.as_str()),
            "starts today"
        );
    }
    assert!(
        minimums.iter().all(|o| o.debt_id != Some(s.ids["visa"])),
        "a credit balance owes nothing"
    );

    // the refresh is idempotent: running it again changes nothing
    let before = count(&s.conn, "SELECT COUNT(*) FROM audit_event");
    let cmd = audit_cmd(&s.conn);
    debt::refresh(&s.conn, &cmd, today).unwrap();
    assert_eq!(count(&s.conn, "SELECT COUNT(*) FROM audit_event"), before);
}

fn audit_cmd(conn: &Connection) -> kept::db::audit::CommandRecord {
    kept::db::audit::begin(conn, "test.refresh", kept::db::audit::Actor::User).unwrap()
}

#[test]
fn every_strategy_schedule_matches_the_fixture_to_the_cent() {
    let s = setup();
    let today = date(&s.file.as_of);
    assert_eq!(s.file.periods_max, strategy::PERIODS_MAX);
    let comparison = strategy::compare(&s.conn, today, Some(s.file.extra_cents)).unwrap();
    assert_eq!(comparison.extra_source, "user");
    assert!(comparison.informal_first);
    assert_eq!(comparison.strategies.len(), s.file.strategies.len());
    for run in &comparison.strategies {
        let spec = &s.file.strategies[&run.strategy];
        assert_eq!(
            run.budget_cents, spec.budget_cents,
            "{}: budget",
            run.strategy
        );
        assert_eq!(
            run.total_interest_cents, spec.total_interest_cents,
            "{}: interest",
            run.strategy
        );
        assert_eq!(
            run.payoff_date, spec.payoff_date,
            "{}: payoff",
            run.strategy
        );
        assert!(!run.unfinished);
        assert_eq!(run.debts.len(), spec.debts.len(), "{}: debts", run.strategy);
        for d in &spec.debts {
            let got = run
                .debts
                .iter()
                .find(|g| g.debt_id == s.ids[&d.key])
                .unwrap_or_else(|| panic!("{}: schedule for {}", run.strategy, d.key));
            assert_eq!(got.informal, d.informal);
            assert_eq!(got.owed_cents, d.owed_cents);
            assert_eq!(
                got.total_interest_cents, d.total_interest_cents,
                "{}/{}: interest",
                run.strategy, d.key
            );
            assert_eq!(
                got.payoff_date, d.payoff_date,
                "{}/{}: payoff",
                run.strategy, d.key
            );
            let rows: Vec<PeriodSpec> = got
                .periods
                .iter()
                .map(|r| PeriodSpec {
                    period: r.period,
                    start: r.start.clone(),
                    end: r.end.clone(),
                    opening_cents: r.opening_cents,
                    interest_cents: r.interest_cents,
                    minimum_cents: r.minimum_cents,
                    payment_cents: r.payment_cents,
                    closing_cents: r.closing_cents,
                })
                .collect();
            assert_eq!(
                rows.len(),
                d.periods.len(),
                "{}/{}: period count",
                run.strategy,
                d.key
            );
            for (got, want) in rows.iter().zip(&d.periods) {
                assert!(
                    (got.interest_cents - want.interest_cents).abs() <= 1
                        && (got.closing_cents - want.closing_cents).abs() <= 1,
                    "{}/{} period {}: within one cent",
                    run.strategy,
                    d.key,
                    want.period
                );
                assert_eq!(
                    got, want,
                    "{}/{} period {}",
                    run.strategy, d.key, want.period
                );
                assert_eq!(
                    got.closing_cents,
                    got.opening_cents + got.interest_cents - got.payment_cents,
                    "period identity"
                );
            }
        }
    }
    let avalanche = &comparison.strategies[0];
    let snowball = &comparison.strategies[1];
    assert!(avalanche.total_interest_cents < snowball.total_interest_cents);
    assert!(strategy::run(&s.conn, today, "random", 0).is_err());
    assert!(strategy::run(&s.conn, today, "avalanche", -1).is_err());
}

#[test]
fn the_informal_scenario_states_the_gap_and_the_date() {
    let s = setup();
    let today = date(&s.file.as_of);
    for spec in &s.file.informal_scenarios {
        let got = strategy::informal_scenario(&s.conn, today, spec.extra_cents).unwrap();
        assert_eq!(got.periods, spec.periods);
        assert_eq!(got.remaining_cents, spec.remaining_cents);
        assert_eq!(
            got.achievable, spec.achievable,
            "extra {}: achievable",
            spec.extra_cents
        );
        assert_eq!(
            got.gap_cents, spec.gap_cents,
            "extra {}: gap",
            spec.extra_cents
        );
        assert_eq!(
            got.payoff_date, spec.payoff_date,
            "extra {}: date",
            spec.extra_cents
        );
    }
    let none = strategy::compare(&s.conn, today, None).unwrap();
    assert_eq!((none.extra_cents, none.extra_source.as_str()), (0, "none"));
}

#[test]
fn minimums_enter_the_forecast_but_not_todays_hero() {
    let s = setup();
    let today = date(&s.file.as_of);
    let hero = kept::cash::safe::safe_to_spend(&s.conn, today).unwrap();
    assert_eq!(
        hero.safe_cents, s.plan.hero.safe_cents,
        "no minimum falls due before the next pay"
    );
    let fixture: ForecastFile = load_json("forecast.json");
    let baseline = forecast::run(&s.conn, today, &Scenario::default()).unwrap();
    // every minimum obligation lands three times in the 91 days, from today on, never before
    let mut minimums = 0i64;
    for spec in &s.file.minimum_obligations {
        minimums += 3 * spec.expected_cents;
    }
    assert_eq!(
        baseline.outflows_cents,
        fixture.scenarios["baseline"].outflows_cents + minimums
    );
    assert_eq!(
        baseline.inflows_cents,
        fixture.scenarios["baseline"].inflows_cents
    );
    assert!(baseline.days[0]
        .events
        .iter()
        .all(|e| e.kind != "obligation" || !e.name.ends_with("minimum")));
    // an informal schedule row still unpaid would be an outflow too; the fixture's are repaid
    assert!(baseline
        .days
        .iter()
        .flat_map(|d| d.events.iter())
        .all(|e| e.kind != "informal"));
}

#[test]
fn a_standalone_balance_moves_with_recorded_payments_and_deleted_rows_detach() {
    let mut s = setup();
    let today = date(&s.file.as_of);
    let auto = s.ids["auto"];
    let before = debt::owed(&s.conn, &debt::get(&s.conn, auto).unwrap(), today).unwrap();
    let cmd = audit_cmd(&s.conn);
    let p = debt::record_payment(
        &s.conn,
        &cmd,
        auto,
        &debt::PaymentInput {
            paid_date: s.file.as_of.clone(),
            amount_cents: 10_000,
            txn_id: None,
            note: "by hand".into(),
        },
    )
    .unwrap();
    assert_eq!(
        debt::owed(&s.conn, &debt::get(&s.conn, auto).unwrap(), today).unwrap(),
        before - 10_000
    );
    debt::remove_payment(&s.conn, &cmd, p.id).unwrap();
    assert_eq!(
        debt::owed(&s.conn, &debt::get(&s.conn, auto).unwrap(), today).unwrap(),
        before
    );
    // a linked debt refuses hand payments; a payment needs a positive amount
    assert!(debt::record_payment(
        &s.conn,
        &cmd,
        s.ids["amex"],
        &debt::PaymentInput {
            paid_date: s.file.as_of.clone(),
            amount_cents: 100,
            txn_id: None,
            note: String::new(),
        }
    )
    .is_err());
    // deleting a repayment row detaches its payment and the loan owes it again
    let chris = s.ids["chris"];
    let loan = informal::get(&s.conn, chris, today).unwrap();
    let txn_id = loan.repayments[0].txn_id.unwrap();
    let tx = s.conn.transaction().unwrap();
    let cmd = kept::db::audit::begin(&tx, "test.delete", kept::db::audit::Actor::User).unwrap();
    kept::db::repo::txn::delete_row(&tx, &cmd, txn_id).unwrap();
    tx.commit().unwrap();
    let loan = informal::get(&s.conn, chris, today).unwrap();
    assert_eq!(loan.repayments.len(), 1);
    assert_eq!(loan.remaining_cents, 30_000);
}
