//! M5 acceptance: the forecast over the fixture ledger and plan equals `fixtures/EXPECTED.md`
//! (`fixtures/forecast.json` is its machine-readable twin) for every scenario, the variable-spend
//! model and its override, and a saved plan overlays the baseline.

mod common;

use common::forecast::{ForecastFile, PointSpec};
use common::plan::{command, import_everything, install_plan, PlanFile};
use common::*;
use kept::forecast::{self, variable, Forecast, Point, Scenario, SurpriseBill};
use kept::money::allocate;
use rusqlite::Connection;

/// (category code, buckets as (start, end, net outflow), median)
type ModelRow = (String, Vec<(String, String, i64)>, i64);
/// (week, start, end, inflows, outflows, closing, lowest)
type WeekRow = (i64, String, String, i64, i64, i64, i64);

fn setup() -> (Connection, ForecastFile) {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    let plan: PlanFile = load_json("plan.json");
    import_everything(&mut conn, &accounts, &plan.as_of);
    install_plan(&mut conn, &accounts, &plan);
    reconcile_fixture_periods(&conn, &accounts);
    let file: ForecastFile = load_json("forecast.json");
    assert_eq!(file.as_of, plan.as_of);
    (conn, file)
}

fn point(p: &Point) -> PointSpec {
    PointSpec {
        date: p.date.clone(),
        cents: p.cents,
    }
}

fn scenario_of(spec: &common::forecast::ScenarioSpec) -> Scenario {
    Scenario {
        downside: spec.downside,
        surprise_bill: spec.surprise.as_ref().map(|s| SurpriseBill {
            date: s.date.clone(),
            cents: s.cents,
        }),
    }
}

fn variable_sum(f: &Forecast, day: usize) -> i64 {
    f.days[day]
        .events
        .iter()
        .filter(|e| e.kind == "variable")
        .map(|e| e.cents)
        .sum()
}

#[test]
fn the_variable_model_is_the_median_of_three_buckets() {
    let (conn, file) = setup();
    let model = variable::model(&conn, date(&file.as_of)).unwrap();
    let got: Vec<ModelRow> = model
        .iter()
        .map(|m| {
            (
                m.code.clone().unwrap_or_default(),
                m.buckets
                    .iter()
                    .map(|b| (b.start.clone(), b.end.clone(), b.net_outflow_cents))
                    .collect(),
                m.median_cents,
            )
        })
        .collect();
    let want: Vec<ModelRow> = file
        .model
        .iter()
        .map(|m| {
            (
                m.category.clone(),
                m.buckets
                    .iter()
                    .map(|b| (b.start.clone(), b.end.clone(), b.net_outflow_cents))
                    .collect(),
                m.median_cents,
            )
        })
        .collect();
    assert_eq!(got, want);
    assert!(model.iter().all(|m| m.override_cents.is_none()));
    assert_eq!(
        model.iter().map(|m| m.per_30_days_cents).sum::<i64>(),
        file.model_total_cents
    );
    assert_eq!(file.bucket_days, variable::BUCKET_DAYS);
}

#[test]
fn every_scenario_ties_out_and_matches_the_fixture() {
    let (conn, file) = setup();
    let as_of = date(&file.as_of);
    assert_eq!(file.horizon_days, forecast::HORIZON_DAYS);
    assert_eq!(file.pay_shift_days, forecast::PAY_SHIFT_DAYS);
    for (name, spec) in &file.scenarios {
        let f = forecast::run(&conn, as_of, &scenario_of(spec)).unwrap();
        assert_eq!(f.opening_cents, spec.opening_cents, "{name}: opening");
        assert_eq!(f.inflows_cents, spec.inflows_cents, "{name}: inflows");
        assert_eq!(f.outflows_cents, spec.outflows_cents, "{name}: outflows");
        assert_eq!(f.closing_cents, spec.closing_cents, "{name}: closing");
        assert_eq!(
            f.closing_cents,
            f.opening_cents + f.inflows_cents - f.outflows_cents,
            "{name}: horizon identity"
        );
        assert_eq!(point(&f.lowest), spec.lowest, "{name}: lowest");
        assert_eq!(
            f.first_shortfall.as_ref().map(point),
            spec.first_shortfall,
            "{name}: shortfall"
        );
        assert_eq!(
            f.first_buffer_breach.as_ref().map(point),
            spec.first_buffer_breach,
            "{name}: buffer breach"
        );
        assert_eq!(f.days.len(), spec.days.len(), "{name}: day count");
        for (got, want) in f.days.iter().zip(&spec.days) {
            let label = format!("{name} day {} {}", want.day, want.date);
            assert_eq!(got.day, want.day, "{label}");
            assert_eq!(got.date, want.date, "{label}");
            assert_eq!(
                (
                    got.inflows_cents,
                    got.outflows_cents,
                    got.closing_cents,
                    got.committed_cents,
                    got.headroom_cents
                ),
                (
                    want.inflows_cents,
                    want.outflows_cents,
                    want.closing_cents,
                    want.committed_cents,
                    want.headroom_cents
                ),
                "{label}"
            );
            assert_eq!(
                got.closing_cents,
                got.opening_cents + got.inflows_cents - got.outflows_cents,
                "{label}: daily identity"
            );
            let mut got_named: Vec<(String, String, i64)> = got
                .events
                .iter()
                .filter(|e| e.kind != "variable")
                .map(|e| (e.kind.clone(), e.name.clone(), e.cents))
                .collect();
            let mut want_named: Vec<(String, String, i64)> = want
                .events
                .iter()
                .filter(|e| e.kind != "variable")
                .map(|e| (e.kind.clone(), e.name.clone(), e.cents))
                .collect();
            got_named.sort();
            want_named.sort();
            assert_eq!(got_named, want_named, "{label}: events");
            let want_variable: i64 = want
                .events
                .iter()
                .filter(|e| e.kind == "variable")
                .map(|e| e.cents)
                .sum();
            assert_eq!(
                variable_sum(&f, usize::try_from(want.day).unwrap()),
                want_variable,
                "{label}: variable"
            );
        }
        // the 30-day view spends the model exactly once
        let first_block: i64 = (0..30).map(|d| variable_sum(&f, d)).sum();
        assert_eq!(
            -first_block, file.model_total_cents,
            "{name}: 30-day variable"
        );
        let got_weeks: Vec<WeekRow> = f
            .weeks
            .iter()
            .map(|w| {
                (
                    w.week,
                    w.start.clone(),
                    w.end.clone(),
                    w.inflows_cents,
                    w.outflows_cents,
                    w.closing_cents,
                    w.lowest_cents,
                )
            })
            .collect();
        let want_weeks: Vec<WeekRow> = spec
            .weeks
            .iter()
            .map(|w| {
                (
                    w.week,
                    w.start.clone(),
                    w.end.clone(),
                    w.inflows_cents,
                    w.outflows_cents,
                    w.closing_cents,
                    w.lowest_cents,
                )
            })
            .collect();
        assert_eq!(got_weeks, want_weeks, "{name}: weeks");
        let pay: Vec<String> = f
            .pay_dates
            .iter()
            .filter(|p| p.stream_name == "Meridian payroll")
            .map(|p| p.date.clone())
            .collect();
        assert_eq!(
            &pay, &spec.pay_dates["Meridian payroll"],
            "{name}: pay dates"
        );
        assert_eq!(
            f.pay_dates.iter().filter(|p| p.shifted).count(),
            usize::from(spec.downside),
            "{name}: shifted occurrences"
        );
        assert!(f.plan.is_none());
        assert!(f.trust.hero.trusted, "{name}: the fixture is reconciled");
    }
}

#[test]
fn the_downside_moves_the_lowest_point_down_and_bills_break_buffer_then_balance() {
    let (conn, file) = setup();
    let as_of = date(&file.as_of);
    let baseline = forecast::run(&conn, as_of, &Scenario::default()).unwrap();
    let downside = forecast::run(
        &conn,
        as_of,
        &Scenario {
            downside: true,
            surprise_bill: None,
        },
    )
    .unwrap();
    assert!(downside.lowest.cents < baseline.lowest.cents);
    assert_eq!(
        (
            baseline.first_shortfall.is_none(),
            baseline.first_buffer_breach.is_none()
        ),
        (true, true)
    );
    let dent = forecast::run(&conn, as_of, &scenario_of(&file.scenarios["downside_bill"])).unwrap();
    assert!(dent.first_shortfall.is_none());
    assert!(dent.first_buffer_breach.is_some());
    let overdraft = forecast::run(&conn, as_of, &scenario_of(&file.scenarios["bill"])).unwrap();
    assert!(overdraft.first_shortfall.is_some());
    assert!(overdraft.first_shortfall.as_ref().unwrap().cents < 0);
    // the income is the same in every scenario: a scenario never invents or removes pay
    assert_eq!(downside.inflows_cents, baseline.inflows_cents);
    assert_eq!(overdraft.inflows_cents, baseline.inflows_cents);
    // a bill outside the window or a non-positive one is refused
    let late = Scenario {
        downside: false,
        surprise_bill: Some(SurpriseBill {
            date: "2027-01-15".into(),
            cents: 100,
        }),
    };
    assert!(forecast::run(&conn, as_of, &late).is_err());
    let zero = Scenario {
        downside: false,
        surprise_bill: Some(SurpriseBill {
            date: file.as_of.clone(),
            cents: 0,
        }),
    };
    assert!(forecast::run(&conn, as_of, &zero).is_err());
}

#[test]
fn an_override_replaces_one_category_and_only_that_category() {
    let (conn, file) = setup();
    let as_of = date(&file.as_of);
    let baseline = forecast::run(&conn, as_of, &Scenario::default()).unwrap();
    let groceries = category_id(&conn, "variable.groceries");
    let groceries_model = baseline
        .model
        .iter()
        .find(|m| m.category_id == groceries)
        .unwrap()
        .clone();
    let parts = allocate(groceries_model.median_cents, 30).unwrap();
    let over_91_days: i64 = (0..91).map(|i| parts[i % 30]).sum();

    let cmd = command(&conn);
    variable::set_override(&conn, &cmd, groceries, Some(0)).unwrap();
    let zeroed = forecast::run(&conn, as_of, &Scenario::default()).unwrap();
    assert_eq!(
        zeroed.outflows_cents,
        baseline.outflows_cents - over_91_days
    );
    assert_eq!(zeroed.inflows_cents, baseline.inflows_cents);
    let m = zeroed
        .model
        .iter()
        .find(|m| m.category_id == groceries)
        .unwrap();
    assert_eq!(
        (m.override_cents, m.per_30_days_cents, m.median_cents),
        (Some(0), 0, groceries_model.median_cents)
    );
    assert_eq!(
        zeroed.model_total_cents,
        baseline.model_total_cents - groceries_model.median_cents
    );

    variable::set_override(&conn, &cmd, groceries, None).unwrap();
    let restored = forecast::run(&conn, as_of, &Scenario::default()).unwrap();
    assert_eq!(restored.outflows_cents, baseline.outflows_cents);
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM variable_spend_override"),
        0
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM audit_event WHERE entity = 'variable_spend_override'"
        ),
        2
    );

    let rent = category_id(&conn, "fixed.rent");
    assert!(variable::set_override(&conn, &cmd, rent, Some(100)).is_err());
    assert!(variable::set_override(&conn, &cmd, groceries, Some(-1)).is_err());
}

#[test]
fn a_saved_plan_overlays_the_baseline() {
    let (conn, file) = setup();
    let as_of = date(&file.as_of);
    let cmd = command(&conn);
    let saved = forecast::save_plan(&conn, &cmd, as_of).unwrap();
    assert_eq!(saved.civil_date, file.as_of);
    assert_eq!(saved.days.len(), 91);
    let f = forecast::run(&conn, as_of, &Scenario::default()).unwrap();
    let plan = f.plan.expect("the saved plan");
    assert_eq!(plan.snapshot_id, saved.snapshot_id);
    let closings: Vec<(String, i64)> = f
        .days
        .iter()
        .map(|d| (d.date.clone(), d.closing_cents))
        .collect();
    let stored: Vec<(String, i64)> = plan
        .days
        .iter()
        .map(|p| (p.date.clone(), p.cents))
        .collect();
    assert_eq!(stored, closings);
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM snapshot WHERE kind = 'plan'"),
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM audit_event WHERE entity = 'snapshot'"
        ),
        1
    );
}
