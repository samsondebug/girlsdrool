//! M9 acceptance for backups, the restore roundtrip, the passphrase change and the exports
//! against fixtures/m9.json: "restore into a temp data dir matches row counts and the hero".

mod common;

use std::path::Path;

use common::*;
use kept::cash::safe;
use kept::config::DataPaths;
use kept::db::audit::{self, Actor};
use kept::db::migrate;
use kept::db::repo::account::{self, NewAccount};
use kept::db::{Db, OpenMode};
use kept::error::AppError;
use kept::export::{backup, full, restore};
use kept::money::parse_decimal_cents;

const PASS: &str = "correct horse battery staple";
const NEW_PASS: &str = "a different passphrase entirely";

#[derive(serde::Deserialize)]
struct M9File {
    as_of: String,
    schema_tables: usize,
    hero_safe_cents: i64,
    audit_pack: AuditPack,
}

#[derive(serde::Deserialize)]
struct AuditPack {
    ledger_rows: usize,
    reconciliation_rows: usize,
    debt_schedule_rows: usize,
    venture_rows: usize,
}

fn temp_paths() -> (tempfile::TempDir, DataPaths) {
    let dir = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(dir.path().join("kept"));
    (dir, paths)
}

fn rows_of(counts: &[backup::TableCount], table: &str) -> i64 {
    counts
        .iter()
        .find(|t| t.table == table)
        .unwrap_or_else(|| panic!("table {table}"))
        .rows
}

#[test]
fn backup_restore_roundtrip_matches_counts_and_hero() {
    let m9: M9File = load_json("m9.json");
    let today = date(&m9.as_of);
    let (_dir, paths) = temp_paths();
    let mut db = Db::open(&paths, PASS, OpenMode::CreateNew).unwrap();
    install_everything(db.conn_mut());
    let live_counts = backup::table_counts(db.conn()).unwrap();
    assert_eq!(live_counts.len(), m9.schema_tables);
    assert_eq!(
        rows_of(&live_counts, "txn"),
        m9.audit_pack.ledger_rows as i64
    );
    assert_eq!(
        safe::safe_to_spend(db.conn(), today).unwrap().safe_cents,
        m9.hero_safe_cents
    );

    // a backup re-encrypted under a new passphrase opens only with it
    let dest = paths.backups.join("kept-manual-test.db");
    backup::export_encrypted(db.conn(), &dest, NEW_PASS).unwrap();
    assert!(matches!(
        backup::inspect(&dest, PASS),
        Err(AppError::WrongPassphrase)
    ));
    assert!(backup::verify(&dest, NEW_PASS, &live_counts).unwrap());

    // staged into a temporary data folder and compared: every table and the hero agree
    let cmp = restore::stage(&db, &dest, NEW_PASS, today).unwrap();
    assert_eq!(cmp.tables.len(), m9.schema_tables);
    assert_eq!(cmp.tables_differ, 0);
    assert!(cmp.hero_same);
    assert_eq!(cmp.hero_backup_cents, m9.hero_safe_cents);
    assert_eq!(cmp.hero_live_cents, m9.hero_safe_cents);
    assert_eq!(
        (cmp.schema_before, cmp.schema_after),
        (migrate::latest_version(), migrate::latest_version())
    );
    assert!(restore::is_staged(&paths));
    assert!(Path::new(&cmp.staged_dir).starts_with(&paths.root));
    // the wrong passphrase fails closed
    assert!(matches!(
        restore::stage(&db, &dest, "not the passphrase", today),
        Err(AppError::WrongPassphrase)
    ));
    // nothing staged → nothing to confirm
    restore::discard(&paths).unwrap();
    assert!(!restore::is_staged(&paths));
    assert!(matches!(
        restore::prepare(&db, "x"),
        Err(AppError::Conflict(_))
    ));

    // the live database moves on; the comparison shows where
    let cmd = audit::begin(db.conn(), "test.drift", Actor::User).unwrap();
    account::create(
        db.conn(),
        &cmd,
        &NewAccount {
            name: "Drift Checking".into(),
            institution: "Drift".into(),
            kind: "checking".into(),
            opening_balance_cents: 100_000,
            opening_date: "2026-09-01".into(),
            venture_id: None,
            firewalled: false,
        },
    )
    .unwrap();
    let drifted_hero = safe::safe_to_spend(db.conn(), today).unwrap().safe_cents;
    assert_eq!(drifted_hero, m9.hero_safe_cents + 100_000);
    let cmp = restore::stage(&db, &dest, NEW_PASS, today).unwrap();
    assert!(cmp.tables_differ >= 3, "{:?}", cmp.tables);
    let acct = cmp.tables.iter().find(|t| t.table == "account").unwrap();
    assert_eq!((acct.live_rows, acct.backup_rows), (8, 7));
    assert!(!cmp.hero_same);
    assert_eq!(cmp.hero_live_cents, drifted_hero);
    assert_eq!(cmp.hero_backup_cents, m9.hero_safe_cents);

    // confirm: a verified pre-restore copy, then the swap, then the restored database reopens
    let pre = restore::prepare(&db, "test").unwrap();
    assert!(pre.is_file());
    let drifted_counts = backup::table_counts(db.conn()).unwrap();
    assert_eq!(backup::inspect(&pre, PASS).unwrap(), drifted_counts);
    let db = restore::swap(db, &pre).unwrap();
    assert!(!restore::is_staged(&paths));
    let after = backup::table_counts(db.conn()).unwrap();
    let copy = backup::inspect(&dest, NEW_PASS).unwrap();
    for t in &copy {
        let got = rows_of(&after, &t.table);
        if t.table == "backup_log" {
            assert_eq!(
                got,
                t.rows + 1,
                "the restored database logs the pre-restore copy"
            );
        } else {
            assert_eq!(got, t.rows, "{}", t.table);
        }
    }
    assert_eq!(rows_of(&after, "account"), 7);
    assert_eq!(
        safe::safe_to_spend(db.conn(), today).unwrap().safe_cents,
        m9.hero_safe_cents
    );
    let log = backup::list_log(db.conn()).unwrap();
    assert_eq!(log[0].kind, "pre_restore");
    assert!(log[0].verified && log[0].exists);
    assert_eq!(Path::new(&log[0].path), pre);
    assert!(db.passphrase() == PASS);
    kept::db::integrity_check(db.conn()).unwrap();
    // it survives a close and a reopen with the live passphrase
    drop(db);
    let db = Db::open(&paths, PASS, OpenMode::Existing).unwrap();
    assert_eq!(
        rows_of(&backup::table_counts(db.conn()).unwrap(), "account"),
        7
    );
}

#[test]
fn daily_backups_rotate_and_are_logged() {
    let (_dir, paths) = temp_paths();
    let db = Db::open(&paths, PASS, OpenMode::CreateNew).unwrap();
    let d3 = backup::daily(db.conn(), &paths, PASS, date("2026-10-03"), 2)
        .unwrap()
        .unwrap();
    assert_eq!(d3, paths.backups.join("kept-2026-10-03.db"));
    assert!(
        backup::daily(db.conn(), &paths, PASS, date("2026-10-03"), 2)
            .unwrap()
            .is_none()
    );
    let manual = backup::manual(db.conn(), &paths, PASS, "20261003T120000Z").unwrap();
    assert_eq!(manual.kind, "manual");
    assert!(manual.verified && manual.exists);
    let d4 = backup::daily(db.conn(), &paths, PASS, date("2026-10-04"), 2)
        .unwrap()
        .unwrap();
    let d5 = backup::daily(db.conn(), &paths, PASS, date("2026-10-05"), 2)
        .unwrap()
        .unwrap();
    assert!(!d3.exists(), "the oldest daily copy rotates out");
    assert!(d4.exists() && d5.exists());
    assert!(
        Path::new(&manual.path).exists(),
        "manual copies never rotate"
    );
    let log = backup::list_log(db.conn()).unwrap();
    assert_eq!(log.len(), 4);
    assert_eq!(log[0].path, d5.display().to_string());
    assert!(log.iter().all(|e| e.verified));
    let rotated = log
        .iter()
        .find(|e| e.path == d3.display().to_string())
        .unwrap();
    assert_eq!(rotated.kind, "daily");
    assert!(!rotated.exists);
    assert_eq!(log.iter().filter(|e| e.kind == "daily").count(), 3);
    // a copy holds the live rows as of its export: everything but the log row written after it
    let live = backup::table_counts(db.conn()).unwrap();
    let copy = backup::inspect(&d5, PASS).unwrap();
    for t in &copy {
        let expected = if t.table == "backup_log" {
            rows_of(&live, "backup_log") - 1
        } else {
            rows_of(&live, &t.table)
        };
        assert_eq!(t.rows, expected, "{}", t.table);
    }
}

#[test]
fn rekey_takes_a_fresh_backup_and_the_old_passphrase_stops_working() {
    let (_dir, paths) = temp_paths();
    let mut db = Db::open(&paths, PASS, OpenMode::CreateNew).unwrap();
    kept::db::settings::update(db.conn_mut(), "theme", &serde_json::json!("light")).unwrap();
    let before = backup::manual(db.conn(), &paths, PASS, "before-rekey").unwrap();
    db.rekey(NEW_PASS).unwrap();
    assert_eq!(db.passphrase(), NEW_PASS);
    assert!(db.rekey("x'00ff'").is_err());
    assert_eq!(count(db.conn(), "SELECT count(*) FROM backup_log"), 1);
    drop(db);
    assert!(matches!(
        Db::open(&paths, PASS, OpenMode::Existing),
        Err(AppError::WrongPassphrase)
    ));
    let db = Db::open(&paths, NEW_PASS, OpenMode::Existing).unwrap();
    assert_eq!(kept::db::settings::load(db.conn()).unwrap().theme, "light");
    kept::db::integrity_check(db.conn()).unwrap();
    // the copy taken before the change opens with the old passphrase only
    assert!(backup::inspect(Path::new(&before.path), PASS).is_ok());
    assert!(matches!(
        backup::inspect(Path::new(&before.path), NEW_PASS),
        Err(AppError::WrongPassphrase)
    ));
}

#[test]
fn full_export_and_audit_pack_write_the_listed_files() {
    let m9: M9File = load_json("m9.json");
    let today = date(&m9.as_of);
    let mut conn = memory_db();
    install_everything(&mut conn);
    let dir = tempfile::tempdir().unwrap();

    let full_dir = dir.path().join("full");
    let report = full::full(&conn, &full_dir, "0.1.0").unwrap();
    assert_eq!(report.kind, "full");
    assert_eq!(report.files.len(), m9.schema_tables + 1);
    let counts = backup::table_counts(&conn).unwrap();
    for t in &counts {
        let f = report
            .files
            .iter()
            .find(|f| f.name == format!("{}.csv", t.table))
            .unwrap();
        assert_eq!(f.rows as i64, t.rows, "{}", t.table);
        assert!(full_dir.join(&f.name).is_file());
    }
    let doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(full_dir.join("kept.json")).unwrap()).unwrap();
    assert_eq!(doc["tables"].as_object().unwrap().len(), m9.schema_tables);
    assert_eq!(doc["schema_version"], migrate::latest_version());
    assert_eq!(
        doc["tables"]["txn"].as_array().unwrap().len(),
        m9.audit_pack.ledger_rows
    );
    let first = &doc["tables"]["txn"][0];
    assert!(
        first["amount"].is_string(),
        "money is a decimal string: {first}"
    );
    assert!(first.get("amount_cents").is_none());
    assert!(matches!(
        full::full(&conn, &full_dir, "0.1.0"),
        Err(AppError::Conflict(_))
    ));

    let pack_dir = dir.path().join("pack");
    let pack = full::audit_pack(&conn, &pack_dir, today, "0.1.0").unwrap();
    assert_eq!(pack.kind, "audit_pack");
    let names: Vec<&str> = pack.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "ledger.csv",
            "reconciliation.csv",
            "safe_to_spend.json",
            "forecast.json",
            "debt_schedule.csv",
            "venture_rollup.csv",
            "README.md"
        ]
    );
    let rows = |name: &str| pack.files.iter().find(|f| f.name == name).unwrap().rows;
    assert_eq!(rows("ledger.csv"), m9.audit_pack.ledger_rows);
    assert_eq!(
        rows("reconciliation.csv"),
        m9.audit_pack.reconciliation_rows
    );
    assert_eq!(rows("debt_schedule.csv"), m9.audit_pack.debt_schedule_rows);
    assert_eq!(rows("venture_rollup.csv"), m9.audit_pack.venture_rows);
    assert_eq!(rows("forecast.json"), 91);

    // ledger.csv sums to the ledger, cent for cent
    let mut reader = csv::Reader::from_path(pack_dir.join("ledger.csv")).unwrap();
    let amount_col = reader
        .headers()
        .unwrap()
        .iter()
        .position(|h| h == "amount")
        .unwrap();
    let mut sum = 0i64;
    let mut n = 0usize;
    for record in reader.records() {
        let record = record.unwrap();
        sum += parse_decimal_cents(&record[amount_col]).unwrap();
        n += 1;
    }
    assert_eq!(n, m9.audit_pack.ledger_rows);
    assert_eq!(sum, count(&conn, "SELECT SUM(amount_cents) FROM txn_leaf"));
    let hero: serde_json::Value =
        serde_json::from_slice(&std::fs::read(pack_dir.join("safe_to_spend.json")).unwrap())
            .unwrap();
    assert_eq!(hero["safe_cents"], m9.hero_safe_cents);
    let readme = std::fs::read_to_string(pack_dir.join("README.md")).unwrap();
    for name in &names[..6] {
        assert!(readme.contains(name), "README names {name}");
    }
    assert!(readme.contains("Sign is the account's point of view"));
}
