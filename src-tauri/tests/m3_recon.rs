//! M3 acceptance against fixtures/EXPECTED.md and its twin recon.json: every account's monthly
//! periods balance against the written statement closings; the mutated August file shows the
//! exact difference, rolls it forward and marks the hero untrusted naming the account; trust
//! statuses as of 2026-10-05 follow the stale window and the per-account override.

mod common;

use common::*;
use kept::cash::recon::{self, ReconInput};
use kept::db::audit::{self, Actor, CommandRecord};
use kept::db::repo::account::{self, AccountPatch};
use kept::error::AppError;
use kept::import::{self, ImportInput};
use rusqlite::Connection;

const TODAY: &str = "2026-10-05";
const AUGUST_NBC: &str = "northbank/northbank_checking_2026-08.csv";

#[derive(serde::Deserialize)]
struct ReconFile {
    periods: Vec<PeriodSpec>,
    mutated: Mutated,
    trust: TrustSpec,
}

#[derive(serde::Deserialize)]
struct PeriodSpec {
    account: String,
    period_start: String,
    period_end: String,
    opening_cents: i64,
    sum_cents: i64,
    computed_cents: i64,
    statement_cents: i64,
}

#[derive(serde::Deserialize)]
struct Mutated {
    file: String,
    row: MutatedRow,
    august: PeriodFigures,
    september: PeriodFigures,
    explorer: ExplorerCounts,
}

#[derive(serde::Deserialize)]
struct MutatedRow {
    posted: String,
    description: String,
    mutated_cents: i64,
}

#[derive(serde::Deserialize)]
struct PeriodFigures {
    period_start: String,
    period_end: String,
    opening_cents: i64,
    computed_cents: i64,
    statement_cents: i64,
    difference_cents: i64,
}

#[derive(serde::Deserialize)]
struct ExplorerCounts {
    in_period: usize,
    before: usize,
    after: usize,
    pending: usize,
    quarantine: usize,
}

#[derive(serde::Deserialize)]
struct TrustSpec {
    as_of: String,
    stale_after_days: i64,
    rows: Vec<TrustRow>,
    hero_contributing: Vec<String>,
}

#[derive(serde::Deserialize)]
struct TrustRow {
    scenario: String,
    account: String,
    contributes: bool,
    latest_period_end: String,
    days: i64,
    status: String,
}

const PLAN: [(&str, &str); 16] = [
    ("nbc", "northbank/northbank_checking_2026-07.csv"),
    ("nbc", "northbank/northbank_checking_2026-09.csv"),
    ("nbs", "northbank/northbank_savings_2026-07.csv"),
    ("nbs", "northbank/northbank_savings_2026-08.csv"),
    ("nbs", "northbank/northbank_savings_2026-09.csv"),
    ("rvc", "riverside/riverside_checking_2026-07.csv"),
    ("rvc", "riverside/riverside_checking_2026-08.csv"),
    ("rvc", "riverside/riverside_checking_2026-09.csv"),
    ("sv", "summit/summit_visa_2026-07.csv"),
    ("sv", "summit/summit_visa_2026-08.csv"),
    ("sv", "summit/summit_visa_2026-09.csv"),
    ("sa", "summit/summit_amex_2026-07.csv"),
    ("sa", "summit/summit_amex_2026-08.csv"),
    ("sa", "summit/summit_amex_2026-09.csv"),
    ("hb", "harbor/harbor_brokerage_2026-Q3.csv"),
    ("vm", "venmo/venmo_2026-Q3.csv"),
];

fn import_file(conn: &mut Connection, account_id: i64, rel: &str) {
    let input = ImportInput {
        account_id,
        profile_id: None,
        file_name: rel.to_string(),
        bytes: fixture_bytes(rel),
    };
    import::commit(conn, &input, date(TODAY), 8500).unwrap_or_else(|e| panic!("{rel}: {e:?}"));
}

/// Every fixture file, with the Northbank checking August file (real or mutated) imported last
/// so that undoing it later touches no row another batch changed.
fn import_plan(
    conn: &mut Connection,
    accounts: &[(&str, kept::db::repo::account::Account)],
    august_nbc: &str,
) {
    for (key, rel) in PLAN {
        import_file(conn, account_id(accounts, key), rel);
    }
    import_file(conn, account_id(accounts, "nbc"), august_nbc);
}

fn command(conn: &Connection) -> CommandRecord {
    audit::begin(conn, "test.reconcile", Actor::User).unwrap()
}

fn enter(
    conn: &Connection,
    account_id: i64,
    period_end: &str,
    statement: i64,
) -> recon::Reconciliation {
    recon::reconcile(
        conn,
        &command(conn),
        &ReconInput {
            account_id,
            period_end: period_end.to_string(),
            statement_closing_cents: statement,
            statement_source: "user".to_string(),
        },
    )
    .unwrap_or_else(|e| panic!("reconcile {account_id} {period_end}: {e:?}"))
}

/// Enter every period of `file.periods` for the accounts in `keys`.
fn enter_all(
    conn: &Connection,
    accounts: &[(&str, kept::db::repo::account::Account)],
    file: &ReconFile,
    keys: &[&str],
) {
    for p in file
        .periods
        .iter()
        .filter(|p| keys.contains(&p.account.as_str()))
    {
        enter(
            conn,
            account_id(accounts, &p.account),
            &p.period_end,
            p.statement_cents,
        );
    }
}

#[test]
fn balanced_fixture_reconciles_every_period_and_the_hero_is_trusted() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    import_plan(&mut conn, &accounts, AUGUST_NBC);
    let file: ReconFile = load_json("recon.json");

    for p in &file.periods {
        let r = enter(
            &conn,
            account_id(&accounts, &p.account),
            &p.period_end,
            p.statement_cents,
        );
        assert_eq!(
            (
                r.period_start.as_str(),
                r.opening_cents,
                r.computed_closing_cents,
                r.difference_cents,
                r.status.as_str()
            ),
            (
                p.period_start.as_str(),
                p.opening_cents,
                p.computed_cents,
                0,
                "balanced"
            ),
            "{} {}",
            p.account,
            p.period_end
        );
        assert_eq!(
            r.computed_closing_cents - r.opening_cents,
            p.sum_cents,
            "Σ posted {} {}",
            p.account,
            p.period_end
        );
        assert!(r.balanced_at.is_some());
        assert_eq!(r.statement_source, "user");
    }
    assert_eq!(
        count(&conn, "SELECT count(*) FROM reconciliation"),
        i64::try_from(file.periods.len()).unwrap()
    );
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM reconciliation WHERE status <> 'balanced'"
        ),
        0
    );

    let trust = recon::trust(&conn, date(&file.trust.as_of), file.trust.stale_after_days).unwrap();
    assert!(trust.hero.trusted, "{:?}", trust.hero);
    assert!(trust.hero.untrusted.is_empty());
    for row in file.trust.rows.iter().filter(|r| r.scenario == "A") {
        let t = trust
            .accounts
            .iter()
            .find(|a| a.account_id == account_id(&accounts, &row.account))
            .unwrap();
        assert_eq!(
            (
                t.contributes,
                t.status.as_str(),
                t.days_since,
                t.latest_period_end.as_deref(),
                t.stale_after_days
            ),
            (
                row.contributes,
                row.status.as_str(),
                Some(row.days),
                Some(row.latest_period_end.as_str()),
                file.trust.stale_after_days
            ),
            "{}",
            row.account
        );
    }
    let contributing: Vec<i64> = trust
        .accounts
        .iter()
        .filter(|a| a.contributes)
        .map(|a| a.account_id)
        .collect();
    let expected: Vec<i64> = file
        .trust
        .hero_contributing
        .iter()
        .map(|k| account_id(&accounts, k))
        .collect();
    assert_eq!(
        contributing, expected,
        "the hero's set is the non-firewalled cash accounts"
    );

    // a balanced period is immutable: the same statement is a no-op, another one is a conflict
    let nbc = account_id(&accounts, "nbc");
    let sept = file
        .periods
        .iter()
        .find(|p| p.account == "nbc" && p.period_end == "2026-09-30")
        .unwrap();
    let again = enter(&conn, nbc, "2026-09-30", sept.statement_cents);
    assert_eq!(again.status, "balanced");
    let cmd = command(&conn);
    let other = ReconInput {
        account_id: nbc,
        period_end: "2026-09-30".into(),
        statement_closing_cents: sept.statement_cents + 1,
        statement_source: "user".into(),
    };
    assert!(matches!(
        recon::reconcile(&conn, &cmd, &other),
        Err(AppError::Conflict(_))
    ));
    // periods go in order: nothing may end before the last balanced period
    let early = ReconInput {
        account_id: nbc,
        period_end: "2026-08-15".into(),
        statement_closing_cents: 0,
        statement_source: "user".into(),
    };
    assert!(matches!(
        recon::reconcile(&conn, &cmd, &early),
        Err(AppError::Validation { .. })
    ));
    // only the latest period can go
    let periods = recon::list(&conn, nbc).unwrap();
    assert_eq!(periods.len(), 3);
    assert!(matches!(
        recon::delete(&conn, &cmd, periods[0].id),
        Err(AppError::Validation { .. })
    ));
    recon::delete(&conn, &cmd, periods[2].id).unwrap();
    assert_eq!(recon::list(&conn, nbc).unwrap().len(), 2);
    assert_eq!(count(&conn, "SELECT count(*) FROM audit_event WHERE entity = 'reconciliation' AND action = 'delete'"), 1);
}

#[test]
fn mutated_fixture_shows_the_difference_rolls_it_forward_and_marks_the_hero_untrusted() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    let file: ReconFile = load_json("recon.json");
    import_plan(&mut conn, &accounts, &file.mutated.file);
    let nbc = account_id(&accounts, "nbc");
    let row = &file.mutated.row;
    assert_eq!(
        count(
            &conn,
            &format!(
                "SELECT count(*) FROM txn WHERE account_id = {nbc} AND posted_date = '{}' AND payee_raw = '{}' AND amount_cents = {}",
                row.posted, row.description, row.mutated_cents
            )
        ),
        1,
        "the mutated row is in the ledger with the wrong amount"
    );
    // every other contributing account balances, so the hero names exactly one account
    enter_all(&conn, &accounts, &file, &["nbs", "rvc", "vm"]);

    let july = file
        .periods
        .iter()
        .find(|p| p.account == "nbc" && p.period_end == "2026-07-31")
        .unwrap();
    assert_eq!(
        enter(&conn, nbc, "2026-07-31", july.statement_cents).status,
        "balanced"
    );

    let aug = &file.mutated.august;
    let r_aug = enter(&conn, nbc, &aug.period_end, aug.statement_cents);
    assert_eq!(
        (
            r_aug.period_start.as_str(),
            r_aug.opening_cents,
            r_aug.computed_closing_cents,
            r_aug.difference_cents,
            r_aug.status.as_str()
        ),
        (
            aug.period_start.as_str(),
            aug.opening_cents,
            aug.computed_cents,
            aug.difference_cents,
            "off"
        )
    );
    assert!(r_aug.balanced_at.is_none());

    let sep = &file.mutated.september;
    let r_sep = enter(&conn, nbc, &sep.period_end, sep.statement_cents);
    assert_eq!(
        (
            r_sep.period_start.as_str(),
            r_sep.opening_cents,
            r_sep.computed_closing_cents,
            r_sep.difference_cents,
            r_sep.status.as_str()
        ),
        (
            sep.period_start.as_str(),
            sep.opening_cents,
            sep.computed_cents,
            sep.difference_cents,
            "off"
        ),
        "September rolls forward from July, the last balanced period"
    );

    // the explorer: the period's rows with a running balance, the neighbours, pending, quarantine
    let ex = recon::explorer(&conn, r_aug.id).unwrap();
    let c = &file.mutated.explorer;
    assert_eq!(
        (
            ex.in_period.len(),
            ex.before.len(),
            ex.after.len(),
            ex.pending.len(),
            ex.quarantine.len()
        ),
        (c.in_period, c.before, c.after, c.pending, c.quarantine)
    );
    assert_eq!(
        ex.in_period.last().unwrap().running_cents,
        aug.computed_cents
    );
    assert!(ex
        .in_period
        .iter()
        .any(|r| r.row.posted_date == row.posted && r.row.amount_cents == row.mutated_cents));
    assert_eq!(ex.neighbour_days, recon::NEIGHBOUR_DAYS);
    assert!(ex.before.iter().all(|r| r.posted_date < aug.period_start));
    assert!(ex.after.iter().all(|r| r.posted_date > aug.period_end));

    // the hero is untrusted and names the account and the difference
    let trust = recon::trust(&conn, date(&file.trust.as_of), file.trust.stale_after_days).unwrap();
    assert!(!trust.hero.trusted);
    let named: Vec<(&str, &str)> = trust
        .hero
        .untrusted
        .iter()
        .map(|u| (u.account_name.as_str(), u.status.as_str()))
        .collect();
    assert_eq!(named, vec![("Northbank Checking", "off")]);
    assert!(
        trust.hero.untrusted[0].reason.contains("-9.00"),
        "{}",
        trust.hero.untrusted[0].reason
    );
    let b = file.trust.rows.iter().find(|r| r.scenario == "B").unwrap();
    let t = trust.accounts.iter().find(|a| a.account_id == nbc).unwrap();
    assert_eq!(
        (
            t.status.as_str(),
            t.latest_period_end.as_deref(),
            t.difference_cents
        ),
        (
            b.status.as_str(),
            Some(b.latest_period_end.as_str()),
            Some(sep.difference_cents)
        )
    );

    // fixing the ledger fixes the periods: undo the mutated batch, import the real file, nothing re-entered
    let mutated_batch: i64 = conn
        .query_row(
            "SELECT id FROM import_batch WHERE file_name = ?1",
            [&file.mutated.file],
            |r| r.get(0),
        )
        .unwrap();
    import::undo(&mut conn, mutated_batch, date("2026-09-30")).unwrap();
    let after_undo = recon::get(&conn, r_aug.id).unwrap();
    assert_eq!(
        after_undo.status, "off",
        "without August's rows the period is further off"
    );
    import_file(&mut conn, nbc, AUGUST_NBC);
    let fixed_aug = recon::get(&conn, r_aug.id).unwrap();
    let fixed_sep = recon::get(&conn, r_sep.id).unwrap();
    assert_eq!(
        (fixed_aug.status.as_str(), fixed_aug.difference_cents),
        ("balanced", 0)
    );
    assert!(fixed_aug.balanced_at.is_some());
    assert_eq!(
        (
            fixed_sep.status.as_str(),
            fixed_sep.difference_cents,
            fixed_sep.period_start.as_str(),
            fixed_sep.opening_cents
        ),
        ("balanced", 0, "2026-09-01", aug.statement_cents),
        "September now rolls forward from the balanced August"
    );
    let trust = recon::trust(&conn, date(&file.trust.as_of), file.trust.stale_after_days).unwrap();
    assert!(trust.hero.trusted, "{:?}", trust.hero);
}

#[test]
fn stale_and_never_reconciled_statuses_follow_the_window_and_the_override() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    import_plan(&mut conn, &accounts, AUGUST_NBC);
    let file: ReconFile = load_json("recon.json");
    let nbs = account_id(&accounts, "nbs");
    let as_of = date(&file.trust.as_of);

    enter_all(&conn, &accounts, &file, &["nbc", "rvc", "vm"]);
    let july = file
        .periods
        .iter()
        .find(|p| p.account == "nbs" && p.period_end == "2026-07-31")
        .unwrap();
    enter(&conn, nbs, "2026-07-31", july.statement_cents);

    let c = file.trust.rows.iter().find(|r| r.scenario == "C").unwrap();
    let trust = recon::trust(&conn, as_of, file.trust.stale_after_days).unwrap();
    let t = trust.accounts.iter().find(|a| a.account_id == nbs).unwrap();
    assert_eq!(
        (
            t.status.as_str(),
            t.days_since,
            t.latest_period_end.as_deref(),
            t.contributes
        ),
        (
            c.status.as_str(),
            Some(c.days),
            Some(c.latest_period_end.as_str()),
            c.contributes
        )
    );
    assert!(!trust.hero.trusted);
    let named: Vec<(&str, &str)> = trust
        .hero
        .untrusted
        .iter()
        .map(|u| (u.account_name.as_str(), u.status.as_str()))
        .collect();
    assert_eq!(named, vec![("Northbank Savings", "stale")]);

    // a wider default window (the setting) makes it reconciled; so does a per-account override
    let wide = recon::trust(&conn, as_of, c.days).unwrap();
    assert!(wide.hero.trusted, "{:?}", wide.hero);
    let cmd = command(&conn);
    account::update(
        &conn,
        &cmd,
        nbs,
        &AccountPatch {
            recon_stale_after_days: Some(Some(90)),
            ..Default::default()
        },
    )
    .unwrap();
    let overridden = recon::trust(&conn, as_of, file.trust.stale_after_days).unwrap();
    let t = overridden
        .accounts
        .iter()
        .find(|a| a.account_id == nbs)
        .unwrap();
    assert_eq!((t.status.as_str(), t.stale_after_days), ("reconciled", 90));
    assert!(overridden.hero.trusted);

    // accounts outside the hero's set carry their own status without touching it
    let sa = overridden
        .accounts
        .iter()
        .find(|a| a.account_id == account_id(&accounts, "sa"))
        .unwrap();
    assert_eq!(
        (
            sa.status.as_str(),
            sa.contributes,
            sa.latest_period_end.as_deref()
        ),
        ("never_reconciled", false, None)
    );
    let hb = overridden
        .accounts
        .iter()
        .find(|a| a.account_id == account_id(&accounts, "hb"))
        .unwrap();
    assert!(
        !hb.contributes,
        "the firewalled brokerage never enters the hero's set"
    );

    // with no statement entered anywhere the hero is untrusted naming every contributing account
    let fresh = memory_db();
    let fresh_accounts = fixture_accounts(&fresh);
    let none = recon::trust(&fresh, as_of, 45).unwrap();
    assert!(!none.hero.trusted);
    let names: Vec<i64> = none.hero.untrusted.iter().map(|u| u.account_id).collect();
    let expected: Vec<i64> = file
        .trust
        .hero_contributing
        .iter()
        .map(|k| account_id(&fresh_accounts, k))
        .collect();
    assert_eq!(names, expected);
    assert!(none
        .hero
        .untrusted
        .iter()
        .all(|u| u.status == "never_reconciled"));
}
