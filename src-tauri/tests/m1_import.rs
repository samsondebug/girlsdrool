//! M1 acceptance against fixtures/EXPECTED.md: every fixture file imports, closings match, the
//! duplicate/overlap/pending/skip/EUR scenarios resolve exactly as written down, and a second
//! import of everything is a no-op.

mod common;

use common::*;
use kept::db::repo::{batch, txn};
use kept::error::AppError;
use kept::import::csv::{
    FLAG_INTEREST, FLAG_NEEDS_REVIEW, FLAG_PAYMENT_APP_UNKNOWN, FLAG_SECURITIES_SALE,
};
use kept::import::{self, ImportInput, QuarantineAction};
use rusqlite::Connection;

const TODAY: &str = "2026-10-05";
const THRESHOLD: i64 = 8500;

fn import(conn: &mut Connection, account_id: i64, rel: &str) -> kept::import::report::ImportReport {
    let input = ImportInput {
        account_id,
        profile_id: None,
        file_name: rel.rsplit('/').next().unwrap_or(rel).to_string(),
        bytes: fixture_bytes(rel),
    };
    import::commit(conn, &input, date(TODAY), THRESHOLD)
        .unwrap_or_else(|e| panic!("import {rel}: {e:?}"))
}

/// Every file once, in the order EXPECTED.md describes, returning the reports by file name.
fn import_all(
    conn: &mut Connection,
    accounts: &[(&str, kept::db::repo::account::Account)],
) -> Vec<(String, kept::import::report::ImportReport)> {
    let plan: [(&str, &str); 15] = [
        ("nbc", "northbank/northbank_checking_2026-07.csv"),
        ("nbc", "northbank/northbank_checking_2026-08.csv"),
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
    ];
    let mut reports = Vec::new();
    for (key, rel) in plan {
        reports.push((
            rel.to_string(),
            import(conn, account_id(accounts, key), rel),
        ));
    }
    reports.push((
        "harbor/harbor_brokerage_2026-Q3.csv".into(),
        import(
            conn,
            account_id(accounts, "hb"),
            "harbor/harbor_brokerage_2026-Q3.csv",
        ),
    ));
    reports.push((
        "venmo/venmo_2026-Q3.csv".into(),
        import(conn, account_id(accounts, "vm"), "venmo/venmo_2026-Q3.csv"),
    ));
    reports
}

#[test]
fn fixture_files_import_and_closings_match_expected() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    let reports = import_all(&mut conn, &accounts);

    for (file, r) in &reports {
        assert!(r.reason.is_none(), "{file} should not be a duplicate file");
        assert!(
            r.quarantined.is_empty(),
            "{file} should quarantine nothing: {:?}",
            r.quarantined
        );
    }
    assert_eq!(
        count(&conn, "SELECT count(*) FROM txn"),
        104,
        "ledger rows after importing every file once"
    );

    // Closing per statement month, exactly as EXPECTED.md lists them (cents).
    let expected: [(&str, [i64; 3]); 7] = [
        ("nbc", [564_748, 945_342, 1_362_647]),
        ("nbs", [1_251_023, 1_302_090, 1_353_199]),
        ("rvc", [135_613, 142_266, 186_476]),
        ("sv", [-28_886, 51_727, 142_133]),
        ("sa", [-11_600, -11_600, -11_600]),
        ("hb", [41_842, 41_842, 41_842]),
        ("vm", [0, 0, 4_200]),
    ];
    for (key, months) in expected {
        let acct = &accounts.iter().find(|(k, _)| *k == key).unwrap().1;
        for (i, end) in ["2026-07-31", "2026-08-31", "2026-09-30"]
            .iter()
            .enumerate()
        {
            assert_eq!(
                closing(&conn, acct, end),
                months[i],
                "{key} closing through {end}"
            );
        }
    }

    // The Visa pending row was inserted by August and updated to posted by September.
    let sept = &reports
        .iter()
        .find(|(f, _)| f.ends_with("summit_visa_2026-09.csv"))
        .unwrap()
        .1;
    assert_eq!(sept.inserted.len(), 8, "{}", sept.summary_text());
    assert_eq!(sept.updated.len(), 1);
    let up = &sept.updated[0];
    assert_eq!(up.similarity_bps, 10_000);
    let amazon = txn::get(&conn, up.txn_id).unwrap();
    assert_eq!(amazon.status, "posted");
    assert_eq!(amazon.posted_date, "2026-09-01");
    assert_eq!(amazon.effective_date, "2026-08-30");
    assert_eq!(amazon.payee_raw, "AMAZON.COM*2K4 AMZN.COM/BILL");
    assert_eq!(amazon.payee_norm, "amazon com");
    assert_eq!(amazon.amount_cents, -6_250);
    assert_eq!(
        count(&conn, "SELECT count(*) FROM txn WHERE status = 'pending'"),
        0
    );

    // Card interest rows carry the interest flag from the profile's type map (the savings
    // interest rows get theirs from the M2 heuristic, which m2_rules_links checks row by row).
    let sv = account_id(&accounts, "sv");
    assert_eq!(
        count(
            &conn,
            &format!("SELECT count(*) FROM txn WHERE account_id = {sv} AND (flags & {FLAG_INTEREST}) <> 0")
        ),
        3
    );
    // The brokerage sale is flagged securities_sale; nothing else is.
    assert_eq!(count(&conn, &format!("SELECT count(*) FROM txn WHERE (flags & {FLAG_SECURITIES_SALE}) <> 0 AND amount_cents = 250000")), 1);
    assert_eq!(
        count(
            &conn,
            &format!("SELECT count(*) FROM txn WHERE (flags & {FLAG_SECURITIES_SALE}) <> 0")
        ),
        1
    );

    // Venmo: 4 data rows, 1 skipped by the funding-source rule, 3 inserted and flagged. The
    // transfer to the bank is linked by automation and loses its review flags, leaving 2.
    let venmo = &reports
        .iter()
        .find(|(f, _)| f.ends_with("venmo_2026-Q3.csv"))
        .unwrap()
        .1;
    assert_eq!(venmo.rows_read, 4);
    assert_eq!(venmo.inserted.len(), 3);
    assert_eq!(venmo.skipped.len(), 1);
    assert_eq!(venmo.skipped[0].by, "profile_rule");
    let vm = account_id(&accounts, "vm");
    assert_eq!(
        count(
            &conn,
            &format!(
                "SELECT count(*) FROM txn WHERE account_id = {vm} AND (flags & {}) = {}",
                FLAG_PAYMENT_APP_UNKNOWN | FLAG_NEEDS_REVIEW,
                FLAG_PAYMENT_APP_UNKNOWN | FLAG_NEEDS_REVIEW
            )
        ),
        2
    );
    assert_eq!(
        count(
            &conn,
            &format!(
                "SELECT count(*) FROM txn WHERE account_id = {vm} AND external_id = '4211000003'"
            )
        ),
        0
    );
    assert_eq!(count(&conn, &format!("SELECT count(*) FROM txn WHERE account_id = {vm} AND external_id = '4211000001' AND memo = 'til payday'")), 1);

    // Riverside keeps the transaction date as effective_date and the posted date as posted_date.
    let rvc = account_id(&accounts, "rvc");
    assert_eq!(count(&conn, &format!("SELECT count(*) FROM txn WHERE account_id = {rvc} AND posted_date = '2026-08-15' AND effective_date = '2026-08-14'")), 3);

    // Every inserted row has an insert audit row under an import command; the pending→posted
    // observation is the one update whose before-image is a pending row.
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM audit_event WHERE entity = 'txn' AND action = 'insert'"
        ),
        104
    );
    assert_eq!(
        count(&conn, "SELECT count(*) FROM audit_event WHERE entity = 'txn' AND action = 'update' AND before_json LIKE '%\"status\":\"pending\"%' AND after_json LIKE '%\"status\":\"posted\"%'"),
        1
    );
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM command WHERE name = 'import.commit' AND actor = 'import'"
        ),
        17
    );
}

#[test]
fn duplicate_overlap_and_eur_files_behave_as_written_down() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    import_all(&mut conn, &accounts);
    let nbc = account_id(&accounts, "nbc");

    // byte-identical duplicate: file-level no-op, recorded
    let dup = import(
        &mut conn,
        nbc,
        "northbank/northbank_checking_2026-07_copy.csv",
    );
    assert!(
        dup.reason
            .as_deref()
            .unwrap_or("")
            .starts_with("duplicate_file_of_batch_"),
        "{:?}",
        dup.reason
    );
    assert_eq!(dup.rows_read, 14);
    assert_eq!(dup.inserted.len(), 0);
    assert_eq!(dup.skipped.len(), 14);
    assert_eq!(
        dup.summary_text(),
        "Read 14 rows. Nothing to do: duplicate file of batch 1."
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), 104);

    // overlapping export: 31 skipped by hash, 1 quarantined at Jaro–Winkler 0.9111
    let overlap = import(
        &mut conn,
        nbc,
        "northbank/northbank_checking_2026-08_09_overlap.csv",
    );
    assert!(overlap.reason.is_none());
    assert_eq!(overlap.rows_read, 32);
    assert_eq!(overlap.inserted.len(), 0);
    assert_eq!(overlap.updated.len(), 0);
    assert_eq!(overlap.skipped.len(), 31);
    assert!(overlap.skipped.iter().all(|s| s.by == "hash"));
    assert_eq!(overlap.quarantined.len(), 1);
    let q = &overlap.quarantined[0];
    assert_eq!(q.similarity_bps, 9111);
    let suspected = txn::get(&conn, q.suspected_txn_id).unwrap();
    assert_eq!(suspected.payee_raw, "JEWEL-OSCO #3421");
    assert_eq!(suspected.posted_date, "2026-08-13");
    assert_eq!(suspected.amount_cents, -11_206);
    assert_eq!(
        overlap.summary_text(),
        "Read 32 rows. Inserted 0. Skipped 31 already-imported rows. Held 1 suspected duplicate for review (similarity ≥ 85.00%)."
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), 104);
    let pending = batch::quarantine_pending(&conn).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, q.quarantine_id);

    // EUR: rejected before any write, no batch, no rows
    let batches_before = count(&conn, "SELECT count(*) FROM import_batch");
    let input = ImportInput {
        account_id: account_id(&accounts, "rvc"),
        profile_id: None,
        file_name: "riverside_checking_2026-07_eur.csv".into(),
        bytes: fixture_bytes("riverside/riverside_checking_2026-07_eur.csv"),
    };
    let err =
        import::commit(&mut conn, &input, date(TODAY), THRESHOLD).expect_err("must reject EUR");
    assert!(matches!(err, AppError::Unsupported(_)), "{err:?}");
    assert_eq!(
        count(&conn, "SELECT count(*) FROM import_batch"),
        batches_before
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), 104);

    // resolving the quarantine as a real row inserts it; discarding leaves the ledger alone
    let inserted = import::resolve_quarantine(
        &mut conn,
        q.quarantine_id,
        QuarantineAction::Insert,
        date(TODAY),
    )
    .unwrap()
    .unwrap();
    assert_eq!(inserted.payee_raw, "JEWEL-OSCO #3421 CHICAGO");
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), 105);
    assert!(batch::quarantine_pending(&conn).unwrap().is_empty());
    assert!(matches!(
        import::resolve_quarantine(
            &mut conn,
            q.quarantine_id,
            QuarantineAction::Discard,
            date(TODAY)
        ),
        Err(AppError::Conflict(_))
    ));
}

#[test]
fn second_import_of_everything_is_a_no_op_and_user_edits_survive() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    import_all(&mut conn, &accounts);

    // a user edit on a row the Visa September file would otherwise re-observe
    let amazon_id: i64 = conn
        .query_row(
            "SELECT id FROM txn WHERE payee_raw = 'AMAZON.COM*2K4 AMZN.COM/BILL'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let groceries: i64 = conn
        .query_row(
            "SELECT id FROM category WHERE system_code = 'variable.cash'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    {
        let cmd = kept::db::audit::begin(&conn, "test.edit", kept::db::audit::Actor::User).unwrap();
        txn::apply_user_patch(
            &conn,
            &cmd,
            amazon_id,
            &txn::TxnPatch {
                payee_norm: Some("amazon household".into()),
                memo: Some("paper towels".into()),
                category_id: Some(Some(groceries)),
                tags: Some(vec!["household".into()]),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let edited = txn::get(&conn, amazon_id).unwrap();
    assert_eq!(
        edited.user_edited,
        txn::UE_PAYEE_NORM | txn::UE_MEMO | txn::UE_CATEGORY | txn::UE_TAGS
    );

    let rows_before = count(&conn, "SELECT count(*) FROM txn");
    let user_fields_before: Vec<(i64, String, String, Option<i64>, i64)> = conn
        .prepare("SELECT id, payee_norm, memo, category_id, user_edited FROM txn ORDER BY id")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();

    let again = import_all(&mut conn, &accounts);
    for (file, r) in &again {
        assert!(
            r.reason.is_some(),
            "{file}: a second import of the same file is a recorded no-op"
        );
        assert_eq!(r.inserted.len(), 0, "{file}");
        assert_eq!(r.updated.len(), 0, "{file}");
    }
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), rows_before);

    // the same rows from a *different* file (a blank record changes the hash of the file, not of
    // any row): row-level dedup skips everything, including the user-edited row
    let mut bytes = fixture_bytes("summit/summit_visa_2026-09.csv");
    bytes.extend_from_slice(b",,,,,\n");
    let input = ImportInput {
        account_id: account_id(&accounts, "sv"),
        profile_id: None,
        file_name: "summit_visa_2026-09_reexport.csv".into(),
        bytes,
    };
    let r = import::commit(&mut conn, &input, date(TODAY), THRESHOLD).unwrap();
    assert!(r.reason.is_none());
    assert_eq!(r.inserted.len(), 0);
    assert_eq!(r.updated.len(), 0);
    assert_eq!(r.skipped.len(), 9);
    assert_eq!(r.blank_rows, 1);

    // and the August file again (its pending row now exists posted): skipped as an older observation
    let r = import::commit(
        &mut conn,
        &ImportInput {
            account_id: account_id(&accounts, "sv"),
            profile_id: None,
            file_name: "summit_visa_2026-08_again.csv".into(),
            bytes: {
                let mut b = fixture_bytes("summit/summit_visa_2026-08.csv");
                b.extend_from_slice(b"\n");
                b
            },
        },
        date(TODAY),
        THRESHOLD,
    )
    .unwrap();
    assert_eq!(r.inserted.len(), 0);
    assert_eq!(r.updated.len(), 0);
    assert_eq!(r.quarantined.len(), 0);
    assert_eq!(
        r.skipped
            .iter()
            .filter(|s| s.by == "older_observation")
            .count(),
        1
    );
    assert_eq!(r.skipped.iter().filter(|s| s.by == "hash").count(), 9);

    let user_fields_after: Vec<(i64, String, String, Option<i64>, i64)> = conn
        .prepare("SELECT id, payee_norm, memo, category_id, user_edited FROM txn ORDER BY id")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        user_fields_before, user_fields_after,
        "no user field changed"
    );
    let still = txn::get(&conn, amazon_id).unwrap();
    assert_eq!(still.payee_norm, "amazon household");
    assert_eq!(still.memo, "paper towels");
    assert_eq!(still.tags, vec!["household".to_string()]);
}

#[test]
fn undo_reverses_a_batch_and_refuses_after_later_changes() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    let sv = account_id(&accounts, "sv");
    let july = import(&mut conn, sv, "summit/summit_visa_2026-07.csv");
    let august = import(&mut conn, sv, "summit/summit_visa_2026-08.csv");
    let september = import(&mut conn, sv, "summit/summit_visa_2026-09.csv");
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), 28);

    // August inserted the pending row that September updated: August cannot be undone first
    let err = import::undo(&mut conn, august.batch_id, date("2026-09-30")).expect_err("conflict");
    assert!(matches!(err, AppError::Conflict(_)), "{err:?}");

    // September undone: its 8 rows go, the Amazon row returns to pending with its old descriptor
    let undone = import::undo(&mut conn, september.batch_id, date("2026-09-30")).unwrap();
    assert_eq!(undone.deleted, 8);
    assert_eq!(undone.restored, 1);
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), 20);
    let amazon = txn::get(&conn, september.updated[0].txn_id).unwrap();
    assert_eq!(amazon.status, "pending");
    assert_eq!(amazon.payee_raw, "AMAZON.COM");
    assert_eq!(amazon.posted_date, "2026-08-30");
    assert!(batch::get(&conn, september.batch_id)
        .unwrap()
        .undone_at
        .is_some());
    assert!(matches!(
        import::undo(&mut conn, september.batch_id, date("2026-09-30")),
        Err(AppError::Conflict(_))
    ));

    // now August can go, then July
    assert_eq!(
        import::undo(&mut conn, august.batch_id, date("2026-09-30"))
            .unwrap()
            .deleted,
        10
    );
    assert_eq!(
        import::undo(&mut conn, july.batch_id, date("2026-09-30"))
            .unwrap()
            .deleted,
        10
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), 0);
    assert_eq!(
        count(&conn, "SELECT count(*) FROM command WHERE actor = 'undo'"),
        3
    );
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM audit_event WHERE entity = 'txn' AND action = 'delete'"
        ),
        28
    );

    // re-importing after an undo is a fresh import, not a duplicate file
    let again = import(&mut conn, sv, "summit/summit_visa_2026-07.csv");
    assert!(again.reason.is_none());
    assert_eq!(again.inserted.len(), 10);
}

#[test]
fn preview_detects_profiles_and_reports_problems_without_writing() {
    let conn = memory_db();
    let accounts = fixture_accounts(&conn);
    let input = ImportInput {
        account_id: account_id(&accounts, "vm"),
        profile_id: None,
        file_name: "venmo_2026-Q3.csv".into(),
        bytes: fixture_bytes("venmo/venmo_2026-Q3.csv"),
    };
    let p = import::preview(&conn, &input).unwrap();
    assert_eq!(
        p.profile.as_ref().map(|p| p.name.as_str()),
        Some("venmo_csv")
    );
    assert_eq!(p.total_rows, 4);
    assert_eq!(p.rows.len(), 4);
    assert_eq!(p.rows[0].payee_raw, "Chris Park");
    assert_eq!(p.rows[0].flags, vec!["needs_review", "payment_app_unknown"]);
    assert!(p.rows[2].skipped.is_some());
    assert!(p.problem.is_none());
    assert_eq!(p.already_imported_batch, None);

    let bad = ImportInput {
        account_id: account_id(&accounts, "nbc"),
        profile_id: None,
        file_name: "bad.csv".into(),
        bytes: b"Date,Description,Amount\n2026-07-01,RENT,-2400\nnot a date,X,1\n".to_vec(),
    };
    let p = import::preview(&conn, &bad).unwrap();
    assert_eq!(
        p.profile.as_ref().map(|p| p.name.as_str()),
        Some("generic_csv")
    );
    let problem = p.problem.expect("problem reported");
    assert_eq!(problem.row, 2);
    assert_eq!(problem.column, "Date");

    let unknown = ImportInput {
        account_id: account_id(&accounts, "nbc"),
        profile_id: None,
        file_name: "mystery.csv".into(),
        bytes: b"When,What,HowMuch\n2026-07-01,RENT,-2400\n".to_vec(),
    };
    let p = import::preview(&conn, &unknown).unwrap();
    assert!(p.profile.is_none());
    assert!(p.candidates.is_empty());
    assert_eq!(p.header, vec!["When", "What", "HowMuch"]);
    assert_eq!(count(&conn, "SELECT count(*) FROM import_batch"), 0);
}

#[test]
fn rows_outside_the_account_window_are_rejected_before_any_write() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    let nbc = account_id(&accounts, "nbc");
    let future = ImportInput {
        account_id: nbc,
        profile_id: None,
        file_name: "future.csv".into(),
        bytes: b"Date,Description,Amount\n2026-12-25,FUTURE,-1.00\n".to_vec(),
    };
    let err = import::commit(&mut conn, &future, date(TODAY), THRESHOLD)
        .err()
        .unwrap();
    assert!(
        matches!(err, AppError::Validation { ref field, .. } if field == "posted_date"),
        "{err:?}"
    );
    let early = ImportInput {
        account_id: nbc,
        profile_id: None,
        file_name: "early.csv".into(),
        bytes: b"Date,Description,Amount\n2026-06-30,EARLY,-1.00\n".to_vec(),
    };
    let err = import::commit(&mut conn, &early, date(TODAY), THRESHOLD)
        .err()
        .unwrap();
    assert!(
        matches!(err, AppError::Validation { ref field, .. } if field == "posted_date"),
        "{err:?}"
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), 0);
    assert_eq!(count(&conn, "SELECT count(*) FROM import_batch"), 0);
}

/// Development aid, never run by `just check`: build a real encrypted data folder with the
/// fixture rules installed, every fixture imported (plus the overlap file so one quarantine
/// row exists) and every monthly period reconciled, for screenshots and manual exploration. `KEPT_SEED_DIR=/path cargo test
/// --no-default-features --test m1_import seed_fixture_data_folder -- --ignored`. Passphrase:
/// `correct horse battery staple`.
#[test]
#[ignore]
fn seed_fixture_data_folder() {
    let Some(dir) = std::env::var_os("KEPT_SEED_DIR") else {
        return;
    };
    let paths = kept::config::DataPaths::new(std::path::PathBuf::from(dir));
    let mut db = kept::db::Db::open(
        &paths,
        "correct horse battery staple",
        kept::db::OpenMode::CreateNew,
    )
    .expect("create seed database");
    let accounts = fixture_accounts(db.conn());
    install_rules(db.conn());
    import_all(db.conn_mut(), &accounts);
    import(
        db.conn_mut(),
        account_id(&accounts, "nbc"),
        "northbank/northbank_checking_2026-08_09_overlap.csv",
    );
    assert_eq!(count(db.conn(), "SELECT count(*) FROM txn"), 104);
    // every monthly period balanced (fixtures/recon.json), so trust has something to show
    let recon: serde_json::Value = load_json("recon.json");
    let cmd = kept::db::audit::begin(db.conn(), "seed.reconcile", kept::db::audit::Actor::User)
        .expect("command");
    for p in recon["periods"].as_array().expect("periods") {
        kept::cash::recon::reconcile(
            db.conn(),
            &cmd,
            &kept::cash::recon::ReconInput {
                account_id: account_id(&accounts, p["account"].as_str().expect("account")),
                period_end: p["period_end"].as_str().expect("period_end").to_string(),
                statement_closing_cents: p["statement_cents"].as_i64().expect("statement"),
                statement_source: "user".to_string(),
            },
        )
        .expect("reconcile");
    }
    assert_eq!(
        count(
            db.conn(),
            "SELECT count(*) FROM reconciliation WHERE status = 'balanced'"
        ),
        21
    );
    // the plan (fixtures/plan.json), so the hero has terms
    let plan: common::plan::PlanFile = load_json("plan.json");
    common::plan::install_plan(db.conn_mut(), &accounts, &plan);
    let debts: common::debts::DebtsFile = load_json("debts.json");
    common::debts::install_debts(db.conn_mut(), &accounts, &debts, date(&debts.as_of));
}
