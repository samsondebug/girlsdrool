//! M0 acceptance: wrong passphrase fails closed, an empty database migrates, checksums are
//! enforced, backups round-trip under a new passphrase, settings writes are audited.

use kept::config::DataPaths;
use kept::db::migrate::{self, MIGRATIONS};
use kept::db::{self, settings, Db, OpenMode};
use kept::error::AppError;
use kept::export::backup;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

const PASS: &str = "correct horse battery staple";

fn temp_paths() -> (tempfile::TempDir, DataPaths) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = DataPaths::new(dir.path().join("kept"));
    (dir, paths)
}

fn create(paths: &DataPaths) -> Db {
    Db::open(paths, PASS, OpenMode::CreateNew).expect("create database")
}

fn file_sha256(path: &std::path::Path) -> String {
    hex::encode(Sha256::digest(std::fs::read(path).expect("read db file")))
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect(sql)
}

fn table_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn empty_database_migrates_to_latest_with_seeds() {
    let (_dir, paths) = temp_paths();
    let db = create(&paths);
    let conn = db.conn();

    assert_eq!(
        migrate::current_version(conn).unwrap(),
        migrate::latest_version()
    );
    let recorded: Vec<(i64, String)> = conn
        .prepare("SELECT version, sha256 FROM schema_migration ORDER BY version")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(recorded.len(), MIGRATIONS.len());
    for (m, (version, sha)) in MIGRATIONS.iter().zip(&recorded) {
        assert_eq!(m.version, *version);
        assert_eq!(migrate::checksum(m.sql), *sha);
    }

    let expected_tables = [
        "account",
        "audit_event",
        "backup_log",
        "category",
        "command",
        "debt",
        "debt_payment",
        "earmark",
        "earmark_entry",
        "firewall_ack",
        "import_batch",
        "import_profile",
        "import_quarantine",
        "income_receipt",
        "income_stream",
        "informal_loan",
        "informal_loan_schedule",
        "obligation",
        "obligation_payment",
        "policy",
        "reconciliation",
        "refund_link",
        "review",
        "review_action",
        "rule",
        "saved_view",
        "schema_migration",
        "setting",
        "snapshot",
        "tag",
        "transfer_link",
        "txn",
        "txn_tag",
        "variable_spend_override",
        "venture",
    ];
    assert_eq!(table_names(conn), expected_tables);
    assert_eq!(
        count(
            conn,
            "SELECT count(*) FROM sqlite_master WHERE type = 'view' AND name = 'txn_leaf'"
        ),
        1
    );

    db::integrity_check(conn).unwrap();
    migrate::foreign_key_check(conn).unwrap();

    // seeds
    assert_eq!(count(conn, "SELECT count(*) FROM setting"), 6);
    assert_eq!(
        count(
            conn,
            "SELECT count(*) FROM category WHERE parent_id IS NULL"
        ),
        7
    );
    assert_eq!(
        count(conn, "SELECT count(*) FROM category WHERE is_system = 1"),
        27
    );
    assert_eq!(count(conn, "SELECT count(*) FROM category WHERE system_code = 'transfer.borrowing_proceeds' AND root_kind = 'transfer'"), 1);
    assert_eq!(
        count(conn, "SELECT count(*) FROM policy WHERE is_system = 1"),
        2
    );
    assert_eq!(
        count(
            conn,
            "SELECT count(*) FROM import_profile WHERE name = 'generic_csv'"
        ),
        1
    );
    let settings = settings::load(conn).unwrap();
    assert_eq!(settings.zone, "America/Chicago");
    assert_eq!(settings.timing_buffer_cents, 0);
    assert_eq!(settings.recon_stale_after_days, 45);
    assert_eq!(settings.dedup_similarity_bps, 8500);
    assert_eq!(settings.theme, "dark");
    assert_eq!(settings.backup_keep_daily, 14);

    // a fresh database takes no pre-migration backup
    assert_eq!(std::fs::read_dir(&paths.backups).unwrap().count(), 0);
}

#[test]
fn wrong_passphrase_fails_closed_and_leaves_the_file_untouched() {
    let (_dir, paths) = temp_paths();
    drop(create(&paths));
    let before = file_sha256(&paths.db);

    let err = Db::open(&paths, "wrong passphrase", OpenMode::Existing).expect_err("must fail");
    assert!(matches!(err, AppError::WrongPassphrase), "got {err:?}");
    assert_eq!(
        file_sha256(&paths.db),
        before,
        "a failed unlock must not modify the file"
    );

    // the raw file is not a readable SQLite database without the key
    let raw = std::fs::read(&paths.db).unwrap();
    assert!(
        !raw.starts_with(b"SQLite format 3"),
        "database header must be encrypted"
    );

    let db = Db::open(&paths, PASS, OpenMode::Existing).expect("correct passphrase opens");
    assert_eq!(count(db.conn(), "SELECT count(*) FROM setting"), 6);
}

#[test]
fn reopening_applies_nothing_and_takes_no_backup() {
    let (_dir, paths) = temp_paths();
    drop(create(&paths));
    let mut conn = db::open_keyed(&paths.db, PASS, false).unwrap();
    let report = migrate::run(&mut conn, &paths, PASS).unwrap();
    assert!(report.applied.is_empty());
    assert_eq!(report.backup, None);
    assert_eq!(report.from_version, migrate::latest_version());
    assert_eq!(std::fs::read_dir(&paths.backups).unwrap().count(), 0);
}

#[test]
fn create_refuses_existing_and_open_refuses_missing() {
    let (_dir, paths) = temp_paths();
    assert!(matches!(
        Db::open(&paths, PASS, OpenMode::Existing),
        Err(AppError::Validation { .. })
    ));
    drop(create(&paths));
    assert!(matches!(
        Db::open(&paths, PASS, OpenMode::CreateNew),
        Err(AppError::Conflict(_))
    ));
}

#[test]
fn tampered_migration_checksum_refuses_to_open() {
    let (_dir, paths) = temp_paths();
    drop(create(&paths));
    {
        let conn = db::open_keyed(&paths.db, PASS, false).unwrap();
        conn.execute(
            "UPDATE schema_migration SET sha256 = 'deadbeef' WHERE version = 1",
            [],
        )
        .unwrap();
    }
    let err = Db::open(&paths, PASS, OpenMode::Existing).expect_err("must refuse");
    assert!(matches!(err, AppError::Migration(_)), "got {err:?}");
}

#[test]
fn newer_schema_than_the_binary_refuses_to_open() {
    let (_dir, paths) = temp_paths();
    drop(create(&paths));
    {
        let conn = db::open_keyed(&paths.db, PASS, false).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
    }
    let err = Db::open(&paths, PASS, OpenMode::Existing).expect_err("must refuse");
    assert!(matches!(err, AppError::Migration(_)), "got {err:?}");
}

#[test]
fn backup_round_trips_under_a_new_passphrase() {
    let (_dir, paths) = temp_paths();
    let mut db = create(&paths);
    settings::update(db.conn_mut(), "theme", &serde_json::json!("light")).unwrap();

    let dest = paths.backups.join("manual-test.db");
    backup::export_encrypted(db.conn(), &dest, "a different passphrase").unwrap();
    assert!(dest.is_file());

    let live = backup::table_counts(db.conn()).unwrap();
    let copy = backup::inspect(&dest, "a different passphrase").unwrap();
    assert_eq!(live, copy);
    assert!(copy.iter().any(|t| t.table == "audit_event" && t.rows == 1));

    assert!(matches!(
        backup::inspect(&dest, PASS),
        Err(AppError::WrongPassphrase)
    ));
    assert!(matches!(
        backup::export_encrypted(db.conn(), &dest, "x"),
        Err(AppError::Conflict(_))
    ));
}

#[test]
fn settings_updates_are_validated_and_audited() {
    let (_dir, paths) = temp_paths();
    let mut db = create(&paths);

    let err = settings::update(db.conn_mut(), "zone", &serde_json::json!("Mars/Olympus"))
        .err()
        .unwrap();
    assert!(matches!(err, AppError::Validation { ref field, .. } if field == "zone"));
    let err = settings::update(db.conn_mut(), "theme", &serde_json::json!("sepia"))
        .err()
        .unwrap();
    assert!(matches!(err, AppError::Validation { .. }));
    let err = settings::update(db.conn_mut(), "nope", &serde_json::json!(1))
        .err()
        .unwrap();
    assert!(matches!(err, AppError::Validation { .. }));
    assert_eq!(count(db.conn(), "SELECT count(*) FROM audit_event"), 0);

    let updated = settings::update(db.conn_mut(), "theme", &serde_json::json!("light")).unwrap();
    assert_eq!(updated.theme, "light");
    assert_eq!(
        count(
            db.conn(),
            "SELECT count(*) FROM command WHERE name = 'settings.update' AND actor = 'user'"
        ),
        1
    );
    let (entity, action, before, after): (String, String, String, String) = db
        .conn()
        .query_row(
            "SELECT entity, action, before_json, after_json FROM audit_event",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(entity, "setting");
    assert_eq!(action, "update");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&before).unwrap()["value"],
        "dark"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&after).unwrap()["value"],
        "light"
    );

    // an unchanged value writes nothing
    settings::update(db.conn_mut(), "theme", &serde_json::json!("light")).unwrap();
    assert_eq!(count(db.conn(), "SELECT count(*) FROM audit_event"), 1);

    let reloaded = settings::load(db.conn()).unwrap();
    assert_eq!(reloaded, updated);
}

#[test]
fn passphrase_shaped_like_a_raw_key_is_rejected_before_touching_disk() {
    let (_dir, paths) = temp_paths();
    let err = Db::open(&paths, "x'00ff'", OpenMode::CreateNew)
        .err()
        .unwrap();
    assert!(matches!(err, AppError::Validation { .. }));
    assert!(!paths.db_exists());
}
