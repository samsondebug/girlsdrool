//! M9 acceptance for OFX/QFX files and institution profiles against fixtures/m9.json and the
//! "OFX/QFX, backup and restore, audit pack" section of fixtures/EXPECTED.md.

mod common;

use common::*;
use kept::cash::recon::{self, ReconInput};
use kept::db::audit::{self, Actor};
use kept::db::repo::txn::{self, TxnPatch, UE_MEMO};
use kept::error::AppError;
use kept::import::profile::{self, AmountSpec, PayeeSpec, Profile, ProfileInput};
use kept::import::{self, ImportInput};
use rusqlite::Connection;

const THRESHOLD: i64 = 8500;

#[derive(serde::Deserialize)]
struct M9File {
    as_of: String,
    ofx: Vec<OfxAnswer>,
}

#[derive(serde::Deserialize)]
struct OfxAnswer {
    file: String,
    account: String,
    form: String,
    bank_id: String,
    acct_id: String,
    rows: usize,
    sum_cents: i64,
    fitids: Vec<String>,
    ledger_balance_cents: i64,
    ledger_balance_date: String,
    csv_file: String,
}

fn ofx_profile(conn: &Connection) -> Profile {
    profile::ofx_profile(conn)
        .unwrap()
        .expect("migration 0005 installs the OFX/QFX profile")
}

fn profile_named(conn: &Connection, name: &str) -> Profile {
    profile::list(conn)
        .unwrap()
        .into_iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("profile {name}"))
}

fn input(account_id: i64, rel: &str, profile_id: Option<i64>) -> ImportInput {
    ImportInput {
        account_id,
        profile_id,
        file_name: rel.rsplit('/').next().unwrap_or(rel).to_string(),
        bytes: fixture_bytes(rel),
    }
}

fn import_file(
    conn: &mut Connection,
    account_id: i64,
    rel: &str,
    today: &str,
) -> kept::import::report::ImportReport {
    import::commit(conn, &input(account_id, rel, None), date(today), THRESHOLD)
        .unwrap_or_else(|e| panic!("import {rel}: {e:?}"))
}

fn external_ids(conn: &Connection, account_id: i64) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT external_id FROM txn WHERE account_id = ?1 AND external_id IS NOT NULL ORDER BY posted_date, id")
        .unwrap();
    stmt.query_map([account_id], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn ofx_files_parse_to_the_rows_their_csvs_carry_in_both_forms() {
    let conn = memory_db();
    let ofx = ofx_profile(&conn);
    assert!(ofx.is_system);
    assert_eq!(ofx.format, "ofx");
    let m9: M9File = load_json("m9.json");
    assert_eq!(m9.ofx.len(), 2);
    for a in &m9.ofx {
        let bytes = fixture_bytes(&a.file);
        assert_eq!(import::detect_format(&bytes), "ofx", "{}", a.file);
        let (file, info) =
            import::parse_file(&bytes, &ofx).unwrap_or_else(|e| panic!("{}: {e:?}", a.file));
        let info = info.expect("an OFX file carries its own header");
        assert_eq!(info.form, a.form);
        assert_eq!(info.bank_id, a.bank_id);
        assert_eq!(info.acct_id, a.acct_id);
        assert_eq!(info.rows, a.rows);
        assert_eq!(info.currency, "USD");
        assert_eq!(file.rows.len(), a.rows, "{}", a.file);
        assert_eq!(
            file.rows.iter().map(|r| r.amount_cents).sum::<i64>(),
            a.sum_cents
        );
        assert_eq!(
            file.rows
                .iter()
                .map(|r| r.external_id.clone().unwrap())
                .collect::<Vec<_>>(),
            a.fitids
        );
        let closing = file.closing.expect("LEDGERBAL");
        assert_eq!(closing.cents, a.ledger_balance_cents);
        assert_eq!(
            kept::dates::format_civil(closing.date),
            a.ledger_balance_date
        );

        // the same rows as the CSV through its own profile
        let csv_profile = profile_named(
            &conn,
            match a.account.as_str() {
                "rvc" => "riverside_csv",
                "nbs" => "northbank_csv",
                other => panic!("unexpected account {other}"),
            },
        );
        let csv_bytes = fixture_bytes(&a.csv_file);
        assert_eq!(import::detect_format(&csv_bytes), "csv");
        let (csv_rows, none) = import::parse_file(&csv_bytes, &csv_profile).unwrap();
        assert!(none.is_none());
        assert_eq!(csv_rows.rows.len(), file.rows.len());
        for (o, c) in file.rows.iter().zip(csv_rows.rows.iter()) {
            assert_eq!(o.posted_date, c.posted_date);
            assert_eq!(o.effective_date, c.effective_date);
            assert_eq!(o.amount_cents, c.amount_cents);
            assert_eq!(o.payee_raw, c.payee_raw);
            assert_eq!(o.status, kept::import::csv::RowStatus::Posted);
            assert!(c.external_id.is_none());
        }
        // the ledger balance is the CSV's closing running balance
        assert_eq!(
            csv_rows.closing.map(|c| c.cents),
            Some(a.ledger_balance_cents)
        );
    }
    // a CSV profile applied to OFX bytes, and the reverse, are refused by name
    let riverside = profile_named(&conn, "riverside_csv");
    assert!(matches!(
        import::parse_file(&fixture_bytes(&m9.ofx[0].file), &riverside),
        Err(AppError::Validation { ref field, .. }) if field == "profile_id"
    ));
    assert!(matches!(
        import::parse_file(&fixture_bytes(&m9.ofx[0].csv_file), &ofx),
        Err(AppError::Validation { ref field, .. }) if field == "profile_id"
    ));
}

#[test]
fn ofx_import_inserts_with_fitids_is_idempotent_and_upgrades_csv_rows() {
    let m9: M9File = load_json("m9.json");
    let a = m9.ofx.iter().find(|a| a.account == "rvc").unwrap();

    // A. into an empty account: every row with its FITID, then nothing on a repeat
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    let rvc = account_id(&accounts, "rvc");
    let r1 = import_file(&mut conn, rvc, &a.file, &m9.as_of);
    assert_eq!(r1.profile_name, "ofx_qfx");
    assert_eq!(r1.inserted.len(), a.rows);
    assert!(r1.updated.is_empty() && r1.quarantined.is_empty() && r1.skipped.is_empty());
    assert_eq!(r1.file_closing_cents, Some(a.ledger_balance_cents));
    assert_eq!(
        r1.file_closing_date.as_deref(),
        Some(a.ledger_balance_date.as_str())
    );
    assert_eq!(external_ids(&conn, rvc), a.fitids);
    let preview = import::preview(&conn, &input(rvc, &a.file, None)).unwrap();
    assert_eq!(preview.format, "ofx");
    assert_eq!(preview.already_imported_batch, Some(r1.batch_id));
    assert_eq!(
        preview.ofx.as_ref().map(|i| i.acct_id.as_str()),
        Some(a.acct_id.as_str())
    );
    assert_eq!(
        preview.closing.map(|c| c.cents),
        Some(a.ledger_balance_cents)
    );
    let r2 = import_file(&mut conn, rvc, &a.file, &m9.as_of);
    assert_eq!(
        r2.reason.as_deref(),
        Some(format!("duplicate_file_of_batch_{}", r1.batch_id).as_str())
    );
    assert!(r2.inserted.is_empty());
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), a.rows as i64);

    // B. after the CSVs the OFX rows are better observations: FITIDs added, nothing inserted
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    let rvc = account_id(&accounts, "rvc");
    import_file(
        &mut conn,
        rvc,
        "riverside/riverside_checking_2026-07.csv",
        &m9.as_of,
    );
    let aug = import_file(&mut conn, rvc, &a.csv_file, &m9.as_of);
    assert_eq!(aug.inserted.len(), a.rows);
    // a hand edit on one of them survives the re-observation
    let cmd = audit::begin(&conn, "test.edit", Actor::User).unwrap();
    let edited_id = aug.inserted[0];
    txn::apply_user_patch(
        &conn,
        &cmd,
        edited_id,
        &TxnPatch {
            memo: Some("my own note".into()),
            ..Default::default()
        },
        date(&m9.as_of),
    )
    .unwrap();
    let before_rows = count(&conn, "SELECT count(*) FROM txn");
    let r3 = import_file(&mut conn, rvc, &a.file, &m9.as_of);
    assert!(r3.inserted.is_empty(), "{:?}", r3.inserted);
    assert!(r3.quarantined.is_empty(), "{:?}", r3.quarantined);
    assert_eq!(r3.updated.len(), a.rows);
    assert!(r3
        .updated
        .iter()
        .all(|u| u.fields.iter().any(|f| f == "external_id")));
    assert_eq!(count(&conn, "SELECT count(*) FROM txn"), before_rows);
    assert_eq!(external_ids(&conn, rvc), a.fitids);
    let edited = txn::get(&conn, edited_id).unwrap();
    assert_eq!(edited.memo, "my own note");
    assert_eq!(edited.user_edited & UE_MEMO, UE_MEMO);
    assert!(edited.external_id.is_some());

    // C. the file's ledger balance reconciles the period as statement source `file`
    let cmd = audit::begin(&conn, "test.reconcile", Actor::User).unwrap();
    let period = recon::reconcile(
        &conn,
        &cmd,
        &ReconInput {
            account_id: rvc,
            period_end: a.ledger_balance_date.clone(),
            statement_closing_cents: r3.file_closing_cents.unwrap(),
            statement_source: "file".into(),
        },
    )
    .unwrap();
    assert_eq!(period.status, "balanced");
    assert_eq!(period.difference_cents, 0);
    assert_eq!(period.statement_source, "file");
}

#[test]
fn a_csv_profile_is_drafted_from_a_sample_tested_created_edited_and_deleted() {
    let mut conn = memory_db();
    let bytes = fixture_bytes("riverside/riverside_checking_2026-08.csv");
    let draft = profile::draft_from_sample(&bytes).unwrap();
    assert_eq!(draft.skip_rows, 0);
    assert_eq!(
        draft.header,
        [
            "Transaction Date",
            "Posted Date",
            "Description",
            "Debit",
            "Credit",
            "Balance",
            "Currency"
        ]
    );
    assert_eq!(draft.spec.header_signature, draft.header);
    assert_eq!(draft.spec.date.column, "Posted Date");
    assert_eq!(draft.spec.date.format, "%Y-%m-%d");
    assert_eq!(
        draft
            .spec
            .effective_date
            .as_ref()
            .map(|d| d.column.as_str()),
        Some("Transaction Date")
    );
    assert_eq!(
        draft.spec.amount,
        AmountSpec::DebitCredit {
            debit: "Debit".into(),
            credit: "Credit".into()
        }
    );
    assert_eq!(
        draft.spec.payee,
        PayeeSpec::Column {
            column: "Description".into()
        }
    );
    assert_eq!(
        draft.spec.balance.as_ref().map(|c| c.column.as_str()),
        Some("Balance")
    );
    assert_eq!(
        draft.spec.currency.as_ref().map(|c| c.column.as_str()),
        Some("Currency")
    );

    // tested against the sample: five rows, the closing, no problem
    let test = import::test_spec(&bytes, &draft.spec).unwrap();
    assert!(test.problem.is_none(), "{:?}", test.problem);
    assert_eq!(test.total_rows, 5);
    assert_eq!(test.rows.len(), 5);
    assert_eq!(test.closing.map(|c| c.cents), Some(142_266));
    assert_eq!(test.rows[0].amount_cents, 30_000);
    // a wrong date format names the row and column, and stores nothing
    let mut wrong = draft.spec.clone();
    wrong.date.format = "%m/%d/%Y".into();
    let test = import::test_spec(&bytes, &wrong).unwrap();
    let problem = test.problem.expect("a problem");
    assert_eq!((problem.row, problem.column.as_str()), (1, "Posted Date"));
    assert!(test.rows.is_empty());
    // a signature that does not match the file is reported before parsing
    let mut other = draft.spec.clone();
    other.header_signature = vec!["Date".into(), "Description".into(), "Amount".into()];
    other.date.column = "Date".into();
    other.amount = AmountSpec::SingleSigned {
        column: "Amount".into(),
    };
    other.effective_date = None;
    other.balance = None;
    other.currency = None;
    let test = import::test_spec(&bytes, &other).unwrap();
    assert!(test
        .problem
        .unwrap()
        .message
        .contains("does not match the signature"));
    let profiles_before = count(&conn, "SELECT count(*) FROM import_profile");

    // created, listed, used, edited
    let cmd = audit::begin(&conn, "test.profile", Actor::User).unwrap();
    let created = profile::create(
        &conn,
        &cmd,
        &ProfileInput {
            name: "riverside_copy".into(),
            institution: "Riverside Bank".into(),
            spec: draft.spec.clone(),
        },
    )
    .unwrap();
    assert!(!created.is_system);
    assert_eq!(created.format, "csv");
    assert_eq!(created.csv(), Some(&draft.spec));
    assert_eq!(
        count(&conn, "SELECT count(*) FROM import_profile"),
        profiles_before + 1
    );
    assert!(matches!(
        profile::create(
            &conn,
            &cmd,
            &ProfileInput {
                name: "riverside_copy".into(),
                institution: String::new(),
                spec: draft.spec.clone()
            }
        ),
        Err(AppError::Conflict(_))
    ));
    let mut outside = draft.spec.clone();
    outside.memo = Some(kept::import::profile::TextSpec::Column {
        column: "Notes".into(),
    });
    assert!(matches!(
        profile::create(&conn, &cmd, &ProfileInput { name: "bad".into(), institution: String::new(), spec: outside }),
        Err(AppError::Validation { ref field, .. }) if field == "memo.column"
    ));
    let updated = profile::update(
        &conn,
        &cmd,
        created.id,
        &ProfileInput {
            name: "riverside_v2".into(),
            institution: "Riverside Bank".into(),
            spec: draft.spec.clone(),
        },
    )
    .unwrap();
    assert_eq!(updated.name, "riverside_v2");

    // built-in profiles are read-only; a used profile stays
    let system = profile_named(&conn, "riverside_csv");
    assert!(matches!(
        profile::update(
            &conn,
            &cmd,
            system.id,
            &ProfileInput {
                name: "x".into(),
                institution: String::new(),
                spec: draft.spec.clone()
            }
        ),
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        profile::delete(&conn, &cmd, system.id),
        Err(AppError::Conflict(_))
    ));
    let ofx = ofx_profile(&conn);
    assert!(matches!(
        profile::delete(&conn, &cmd, ofx.id),
        Err(AppError::Conflict(_))
    ));
    let accounts = fixture_accounts(&conn);
    let rvc = account_id(&accounts, "rvc");
    let report = import::commit(
        &mut conn,
        &input(
            rvc,
            "riverside/riverside_checking_2026-08.csv",
            Some(updated.id),
        ),
        date("2026-09-30"),
        THRESHOLD,
    )
    .unwrap();
    assert_eq!(report.profile_name, "riverside_v2");
    assert_eq!(report.inserted.len(), 5);
    let cmd = audit::begin(&conn, "test.profile2", Actor::User).unwrap();
    let err = profile::delete(&conn, &cmd, updated.id).err().unwrap();
    assert!(
        matches!(err, AppError::Conflict(ref m) if m.contains("1 import batch")),
        "{err:?}"
    );

    // an unused one goes, audited
    let spare = profile::create(
        &conn,
        &cmd,
        &ProfileInput {
            name: "spare".into(),
            institution: String::new(),
            spec: draft.spec.clone(),
        },
    )
    .unwrap();
    profile::delete(&conn, &cmd, spare.id).unwrap();
    assert!(matches!(
        profile::get(&conn, spare.id),
        Err(AppError::NotFound { .. })
    ));
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM audit_event WHERE entity = 'import_profile'"
        ),
        4
    );
    assert_eq!(
        count(&conn, "SELECT count(*) FROM audit_event WHERE entity = 'import_profile' AND action = 'delete'"),
        1
    );
}
