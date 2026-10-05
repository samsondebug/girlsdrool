//! M4 acceptance against fixtures/EXPECTED.md and its twin plan.json: the plan's receipts and
//! payments match the written rows, the hero as of 2026-09-30 equals the stated terms and total,
//! the next fourteen days list the stated occurrences, and recurring rows become exactly the
//! stated candidates, which never enter the hero until confirmed.

mod common;

use std::collections::BTreeSet;

use common::plan::*;
use common::*;
use kept::cash::safe;
use kept::db::repo::txn;
use kept::error::AppError;
use kept::plan::{self, earmark, income, obligation};

fn key_of(accounts: &[(&str, kept::db::repo::account::Account)], id: i64) -> String {
    accounts
        .iter()
        .find(|(_, a)| a.id == id)
        .map(|(k, _)| (*k).to_string())
        .unwrap_or_else(|| panic!("account {id}"))
}

#[test]
fn plan_matches_the_written_receipts_and_payments_and_the_hero_equals_expected() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    let file: PlanFile = load_json("plan.json");
    import_everything(&mut conn, &accounts, &file.as_of);
    let installed = install_plan(&mut conn, &accounts, &file);
    let as_of = date(&file.as_of);

    // receipts: exactly the written rows, matched by the heuristic
    let receipts = income::all_receipts(&conn).unwrap();
    assert_eq!(receipts.len(), file.receipts.len(), "receipts");
    for spec in &file.receipts {
        let r = receipts
            .iter()
            .find(|r| {
                r.income_stream_id == installed.streams[&spec.owner] && r.due_date == spec.due_date
            })
            .unwrap_or_else(|| panic!("no receipt for {} due {}", spec.owner, spec.due_date));
        let row = txn::get(&conn, r.txn_id).unwrap();
        assert_eq!(
            (
                key_of(&accounts, row.account_id),
                row.posted_date.as_str(),
                row.payee_raw.as_str(),
                row.amount_cents,
                r.matched_by.as_str()
            ),
            (
                spec.account.clone(),
                spec.posted.as_str(),
                spec.description.as_str(),
                spec.amount_cents,
                "heuristic"
            )
        );
    }
    // payments: the same, per obligation
    let payments = obligation::all_payments(&conn).unwrap();
    assert_eq!(payments.len(), file.payments.len(), "payments");
    for spec in &file.payments {
        let p = payments
            .iter()
            .find(|p| {
                p.obligation_id == installed.obligations[&spec.owner] && p.due_date == spec.due_date
            })
            .unwrap_or_else(|| panic!("no payment for {} due {}", spec.owner, spec.due_date));
        let row = txn::get(&conn, p.txn_id).unwrap();
        assert_eq!(
            (
                key_of(&accounts, row.account_id),
                row.posted_date.as_str(),
                row.payee_raw.as_str(),
                row.amount_cents
            ),
            (
                spec.account.clone(),
                spec.posted.as_str(),
                spec.description.as_str(),
                spec.amount_cents
            ),
            "{} due {}",
            spec.owner,
            spec.due_date
        );
    }
    // a second pass changes nothing
    let again = plan::match_all(&conn, &command(&conn), as_of).unwrap();
    assert_eq!(
        again,
        plan::MatchReport {
            receipts: 0,
            payments: 0
        }
    );

    // the hero, term by term
    let hero = safe::safe_to_spend(&conn, as_of).unwrap();
    assert_eq!(hero.as_of, file.as_of);
    let want = &file.hero;
    let got_accounts: Vec<(String, i64, i64, i64)> = hero
        .terms
        .available
        .accounts
        .iter()
        .map(|a| {
            (
                key_of(&accounts, a.account_id),
                a.posted_cents,
                a.pending_in_cents,
                a.pending_out_cents,
            )
        })
        .collect();
    let want_accounts: Vec<(String, i64, i64, i64)> = want
        .available
        .accounts
        .iter()
        .map(|a| {
            (
                a.account.clone(),
                a.posted_cents,
                a.pending_in_cents,
                a.pending_out_cents,
            )
        })
        .collect();
    assert_eq!(got_accounts, want_accounts, "available per account");
    assert_eq!(
        hero.terms.available.cents, want.available.total_cents,
        "available"
    );
    assert!(hero
        .terms
        .available
        .accounts
        .iter()
        .all(|a| a.pending_row_ids.is_empty()));

    let mut got_earmarks: Vec<(String, i64)> = hero
        .terms
        .earmarks
        .items
        .iter()
        .map(|e| (e.name.clone(), e.remaining_cents))
        .collect();
    let mut want_earmarks: Vec<(String, i64)> = want
        .earmarks
        .items
        .iter()
        .map(|e| (e.earmark.clone(), e.remaining_cents))
        .collect();
    got_earmarks.sort();
    want_earmarks.sort();
    assert_eq!(got_earmarks, want_earmarks, "earmarks");
    assert_eq!(hero.terms.earmarks.cents, want.earmarks.total_cents);
    assert!(hero
        .terms
        .earmarks
        .items
        .iter()
        .all(|e| e.entry_ids.len() == 1));

    let mut got_obligations: Vec<(String, String, i64, i64, i64, bool)> = hero
        .terms
        .obligations
        .items
        .iter()
        .map(|o| {
            (
                o.name.clone(),
                o.due_date.clone(),
                o.expected_cents,
                o.earmark_covered_cents,
                o.counted_cents,
                o.overdue,
            )
        })
        .collect();
    let mut want_obligations: Vec<(String, String, i64, i64, i64, bool)> = want
        .obligations
        .items
        .iter()
        .map(|o| {
            (
                o.obligation.clone(),
                o.due_date.clone(),
                o.expected_cents,
                o.earmark_covered_cents,
                o.counted_cents,
                o.overdue,
            )
        })
        .collect();
    got_obligations.sort();
    want_obligations.sort();
    assert_eq!(
        got_obligations, want_obligations,
        "obligations before next income"
    );
    assert_eq!(hero.terms.obligations.cents, want.obligations.total_cents);
    let next = hero
        .terms
        .obligations
        .next_income
        .clone()
        .expect("a confirmed stream exists");
    assert_eq!(
        (
            next.date.as_str(),
            next.stream_name.as_str(),
            next.expected_net_cents,
            next.days_away
        ),
        (
            file.next_income.date.as_str(),
            file.next_income.stream.as_str(),
            file.next_income.expected_net_cents,
            file.next_income.days_away
        )
    );
    assert_eq!(hero.terms.obligations.window_end, file.next_income.date);
    assert_eq!(hero.terms.obligations.window_reason, None);
    assert_eq!(hero.terms.buffer.cents, want.buffer_cents);
    assert_eq!(hero.safe_cents, want.safe_cents, "safe");
    assert_eq!(
        hero.safe_cents,
        hero.terms.available.cents
            - hero.terms.earmarks.cents
            - hero.terms.obligations.cents
            - hero.terms.buffer.cents,
        "the total is the terms"
    );
    let excluded: Vec<(String, i64)> = hero
        .excluded
        .firewalled_accounts
        .iter()
        .map(|a| (key_of(&accounts, a.account_id), a.posted_cents))
        .collect();
    let want_excluded: Vec<(String, i64)> = want
        .excluded_firewalled
        .iter()
        .map(|a| (a.account.clone(), a.posted_cents))
        .collect();
    assert_eq!(excluded, want_excluded);
    assert!(hero.excluded.venture_accounts.is_empty());
    assert!(hero.excluded.pending_flagged_inflows.is_empty());
    assert!(hero.excluded.posted_flagged_inflows.is_empty());
    assert!(!hero.trust.hero.trusted, "nothing is reconciled yet");

    // reconciling every cash account makes the same number trusted
    reconcile_fixture_periods(&conn, &accounts);
    let trusted = safe::safe_to_spend(&conn, as_of).unwrap();
    assert!(trusted.trust.hero.trusted, "{:?}", trusted.trust.hero);
    assert_eq!(trusted.safe_cents, want.safe_cents);

    // a receipt set by hand is kept: the heuristic never re-matches a settled occurrence
    let stream = installed.streams["Meridian payroll"];
    let last = file.receipts.last().unwrap();
    let cmd = command(&conn);
    let before = income::receipt_for(&conn, stream, &last.due_date)
        .unwrap()
        .unwrap();
    income::remove_receipt(&conn, &cmd, stream, &last.due_date).unwrap();
    income::record_receipt(&conn, &cmd, stream, &last.due_date, before.txn_id, "user").unwrap();
    assert!(matches!(
        income::record_receipt(&conn, &cmd, stream, &last.due_date, before.txn_id, "user"),
        Err(AppError::Conflict(_))
    ));
    let rerun = plan::match_all(&conn, &cmd, as_of).unwrap();
    assert_eq!(rerun.receipts, 0);
    assert_eq!(
        income::receipt_for(&conn, stream, &last.due_date)
            .unwrap()
            .unwrap()
            .matched_by,
        "user"
    );
    // the earmark's remaining is the entries, nothing else
    let rent = installed.earmarks["Rent"];
    assert_eq!(earmark::remaining(&conn, rent, as_of).unwrap(), 120_000);
    assert_eq!(
        earmark::remaining(&conn, rent, date("2026-09-17")).unwrap(),
        0,
        "before its first entry"
    );
}

/// (name, due, expected, variability, earmark covered, autopay, overdue, days away)
type UpcomingRow = (String, String, i64, i64, i64, bool, bool, i64);

#[test]
fn a_receipt_is_never_a_pending_linked_or_borrowed_inflow() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    let file: PlanFile = load_json("plan.json");
    import_everything(&mut conn, &accounts, &file.as_of);
    install_plan(&mut conn, &accounts, &file);
    let as_of = date(&file.as_of);
    let cmd = kept::db::audit::begin(&conn, "test.receipts", kept::db::audit::Actor::User).unwrap();

    let receipt = income::all_receipts(&conn).unwrap()[0].clone();
    let is_receipt = |conn: &rusqlite::Connection| {
        income::all_receipts(conn)
            .unwrap()
            .iter()
            .any(|r| r.txn_id == receipt.txn_id)
    };
    let forget = |conn: &rusqlite::Connection| {
        conn.execute(
            "DELETE FROM income_receipt WHERE txn_id = ?1",
            [receipt.txn_id],
        )
        .unwrap();
    };

    // flagged as borrowing: never income
    forget(&conn);
    conn.execute(
        "UPDATE txn SET flags = flags | ?1 WHERE id = ?2",
        rusqlite::params![i64::from(kept::import::csv::FLAG_BORROWING), receipt.txn_id],
    )
    .unwrap();
    plan::match_all(&conn, &cmd, as_of).unwrap();
    assert!(!is_receipt(&conn), "a borrowing-flagged inflow was matched");

    // pending: not yet the posted row
    forget(&conn);
    conn.execute(
        "UPDATE txn SET flags = flags & ~?1, status = 'pending' WHERE id = ?2",
        rusqlite::params![i64::from(kept::import::csv::FLAG_BORROWING), receipt.txn_id],
    )
    .unwrap();
    plan::match_all(&conn, &cmd, as_of).unwrap();
    assert!(!is_receipt(&conn), "a pending inflow was matched");

    // posted and clean again: matched as before
    forget(&conn);
    conn.execute(
        "UPDATE txn SET status = 'posted' WHERE id = ?1",
        [receipt.txn_id],
    )
    .unwrap();
    plan::match_all(&conn, &cmd, as_of).unwrap();
    assert!(is_receipt(&conn), "the posted row is the receipt again");
}

#[test]
fn upcoming_lists_the_next_income_and_the_next_fourteen_days() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    let file: PlanFile = load_json("plan.json");
    import_everything(&mut conn, &accounts, &file.as_of);
    install_plan(&mut conn, &accounts, &file);

    let up = safe::upcoming(&conn, date(&file.as_of), file.upcoming_days).unwrap();
    let next = up.next_income.expect("next income");
    assert_eq!(
        (next.date.as_str(), next.days_away),
        (file.next_income.date.as_str(), file.next_income.days_away)
    );
    let got: Vec<UpcomingRow> = up
        .obligations
        .iter()
        .map(|o| {
            (
                o.name.clone(),
                o.due_date.clone(),
                o.expected_cents,
                o.variability_cents,
                o.earmark_covered_cents,
                o.autopay,
                o.overdue,
                o.days_away,
            )
        })
        .collect();
    let want: Vec<UpcomingRow> = file
        .upcoming
        .iter()
        .map(|o| {
            (
                o.obligation.clone(),
                o.due_date.clone(),
                o.expected_cents,
                o.variability_cents,
                o.earmark_covered_cents,
                o.autopay,
                o.overdue,
                o.days_away,
            )
        })
        .collect();
    assert_eq!(got, want);
    assert!(up
        .obligations
        .iter()
        .all(|o| o.source_account_name == "Northbank Checking"));
}

#[test]
fn recurring_rows_become_candidates_that_never_enter_the_hero() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    let file: PlanFile = load_json("plan.json");
    import_everything(&mut conn, &accounts, &file.as_of);
    let as_of = date(&file.as_of);

    // with no plan at all, every recurring payee is proposed
    let cmd = command(&conn);
    let created = obligation::detect_candidates(&conn, &cmd).unwrap();
    let got: BTreeSet<(String, String, usize, i64, i64, i64)> = created
        .iter()
        .map(|o| {
            let origin: obligation::DetectedFrom =
                serde_json::from_str(o.detected_from_json.as_deref().unwrap()).unwrap();
            (
                key_of(&accounts, o.source_account_id),
                o.match_payee_contains.clone().unwrap(),
                origin.txn_ids.len(),
                o.due_day.unwrap(),
                o.expected_cents,
                o.variability_cents,
            )
        })
        .collect();
    let want: BTreeSet<(String, String, usize, i64, i64, i64)> = file
        .candidates
        .iter()
        .map(|c| {
            (
                c.account.clone(),
                c.payee_norm.clone(),
                c.rows,
                c.due_day,
                c.expected_cents,
                c.variability_cents,
            )
        })
        .collect();
    assert_eq!(got, want);
    assert!(created
        .iter()
        .all(|o| o.status == "candidate" && o.due_rule == "monthly_day"));
    // detection is idempotent and candidates are not in the hero
    assert!(obligation::detect_candidates(&conn, &cmd)
        .unwrap()
        .is_empty());
    let hero = safe::safe_to_spend(&conn, as_of).unwrap();
    assert!(hero.terms.obligations.items.is_empty());
    assert_eq!(
        hero.terms.obligations.window_reason.as_deref(),
        Some("no_confirmed_income")
    );
    assert_eq!(hero.terms.obligations.window_end, "2026-10-30");
    assert_eq!(
        hero.safe_cents, hero.terms.available.cents,
        "no earmark, no obligation, buffer 0"
    );

    // confirming a candidate makes it an obligation: its past occurrences match, its next is beyond the window
    let netflix = created
        .iter()
        .find(|o| o.match_payee_contains.as_deref() == Some("netflix com"))
        .unwrap();
    obligation::set_status(&conn, &cmd, netflix.id, "confirmed").unwrap();
    let report = plan::match_all(&conn, &cmd, as_of).unwrap();
    assert_eq!(report.payments, 3);
    let paid: Vec<String> = obligation::payments(&conn, netflix.id)
        .unwrap()
        .into_iter()
        .map(|p| p.due_date)
        .collect();
    assert_eq!(paid, vec!["2026-07-26", "2026-08-26", "2026-09-26"]);
    assert!(matches!(
        obligation::delete_candidate(&conn, &cmd, netflix.id),
        Err(AppError::Validation { .. })
    ));
    let shell = created
        .iter()
        .find(|o| o.match_payee_contains.as_deref() == Some("shell oil"))
        .unwrap();
    obligation::delete_candidate(&conn, &cmd, shell.id).unwrap();
    assert!(matches!(
        obligation::get(&conn, shell.id),
        Err(AppError::NotFound { .. })
    ));

    // with the plan's obligations present, only the uncovered payees are proposed
    let mut conn2 = memory_db();
    let accounts2 = fixture_accounts(&conn2);
    install_rules(&conn2);
    import_everything(&mut conn2, &accounts2, &file.as_of);
    install_plan(&mut conn2, &accounts2, &file);
    let cmd2 = command(&conn2);
    let created2 = obligation::detect_candidates(&conn2, &cmd2).unwrap();
    let got2: BTreeSet<String> = created2
        .iter()
        .map(|o| o.match_payee_contains.clone().unwrap())
        .collect();
    let want2: BTreeSet<String> = file.candidates_with_plan.iter().cloned().collect();
    assert_eq!(got2, want2);
    let hero2 = safe::safe_to_spend(&conn2, as_of).unwrap();
    assert_eq!(
        hero2.safe_cents, file.hero.safe_cents,
        "candidates leave the hero alone"
    );
}
