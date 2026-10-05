//! Named property tests (spec: Invariants). Each test is named exactly as the spec names it and
//! grows with the engines. Database-backed properties run on an in-memory SQLite with the real
//! migrations (no key derivation), so hundreds of cases stay fast.

mod common;

use common::*;
use kept::db::audit::{self, Actor};
use kept::db::repo::txn;
use kept::import::{self, ImportInput};
use kept::money::{allocate, mul_div_round, parse_decimal_cents, to_decimal_string};
use proptest::prelude::*;

const THRESHOLD: i64 = 8500;

/// A synthetic statement row: day of 2026-07, cents, payee from a small alphabet, memo.
#[derive(Debug, Clone)]
struct GenRow {
    day: u32,
    cents: i64,
    payee: String,
    memo: String,
}

fn gen_row() -> impl Strategy<Value = GenRow> {
    (
        1u32..=31,
        prop_oneof![-500_000i64..=-1, 1i64..=500_000],
        prop::sample::select(vec![
            "JEWEL-OSCO #3421",
            "SHELL OIL 57442",
            "MERIDIAN CAP ACH PAYROLL",
            "NETFLIX.COM",
            "ZELLE PAYMENT FROM MORGAN AVERY",
            "ONLINE TRANSFER TO SAV ...5678",
            "AMAZON.COM*HG6 AMZN.COM/BILL",
            "COMED ELECTRIC",
        ]),
        prop::sample::select(vec!["", "memo a", "memo b"]),
    )
        .prop_map(|(day, cents, payee, memo)| GenRow {
            day,
            cents,
            payee: payee.to_string(),
            memo: memo.to_string(),
        })
}

/// Generic CSV text for the rows (Date, Description, Amount, Memo).
fn csv_text(rows: &[GenRow]) -> String {
    let mut s = String::from("Date,Description,Amount,Memo\n");
    for r in rows {
        s.push_str(&format!(
            "2026-07-{:02},{},{},{}\n",
            r.day,
            r.payee,
            to_decimal_string(r.cents),
            r.memo
        ));
    }
    s
}

/// A profile for that layout, created once per database.
fn generic_with_memo(conn: &rusqlite::Connection) -> i64 {
    conn.execute(
        "INSERT INTO import_profile (name, institution, format, spec_json, is_system, created_at, updated_at)
         VALUES ('prop_csv', '', 'csv', ?1, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [r#"{"header_signature":["Date","Description","Amount","Memo"],"date":{"column":"Date","format":"%Y-%m-%d"},"amount":{"kind":"single_signed","column":"Amount"},"payee":{"column":"Description"},"memo":{"column":"Memo"}}"#],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn import_text(
    conn: &mut rusqlite::Connection,
    account_id: i64,
    profile_id: i64,
    name: &str,
    text: &str,
) -> kept::import::report::ImportReport {
    import::commit(
        conn,
        &ImportInput {
            account_id,
            profile_id: Some(profile_id),
            file_name: name.to_string(),
            bytes: text.as_bytes().to_vec(),
        },
        date("2026-10-05"),
        THRESHOLD,
    )
    .expect("import")
}

fn user_fields(
    conn: &rusqlite::Connection,
) -> Vec<(i64, String, String, Option<i64>, i64, String)> {
    conn.prepare("SELECT t.id, t.payee_norm, t.memo, t.category_id, t.user_edited, COALESCE((SELECT group_concat(g.name) FROM txn_tag x JOIN tag g ON g.id = x.tag_id WHERE x.txn_id = t.id), '') FROM txn t ORDER BY t.id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// Splitting a total into parts and recombining them never loses or invents a cent, and
    /// no part differs from another by more than one cent.
    #[test]
    fn money_sum_conserves(total in any::<i64>(), parts in 1usize..=64) {
        let shares = allocate(total, parts).unwrap();
        prop_assert_eq!(shares.len(), parts);
        let sum = shares.iter().try_fold(0i64, |acc, &x| acc.checked_add(x)).unwrap();
        prop_assert_eq!(sum, total);
        let min = *shares.iter().min().unwrap();
        let max = *shares.iter().max().unwrap();
        prop_assert!(max - min <= 1);
    }

    /// `mul_div_round` is the exact quotient rounded half away from zero: the result times the
    /// divisor is within half a divisor of the true product, and ties move away from zero.
    #[test]
    fn mul_div_round_is_within_half_a_unit(
        a in -1_000_000_000_000i64..=1_000_000_000_000,
        b in -1_000_000i64..=1_000_000,
        d in prop_oneof![1i64..=1_000_000, -1_000_000i64..=-1],
    ) {
        let r = i128::from(mul_div_round(i128::from(a), i128::from(b), i128::from(d)).unwrap());
        let n = i128::from(a) * i128::from(b);
        let diff = r * i128::from(d) - n;
        let half = i128::from(d).abs();
        prop_assert!(diff.abs() * 2 <= half, "r={r} n={n} d={d}");
        if diff.abs() * 2 == half {
            prop_assert!(r.abs() * half >= n.abs(), "tie must round away from zero");
        }
    }

    /// Export formatting and statement parsing are inverses on every i64.
    #[test]
    fn decimal_string_round_trips(cents in any::<i64>()) {
        prop_assert_eq!(parse_decimal_cents(&to_decimal_string(cents)).unwrap(), cents);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Splitting ledger rows and recombining them never loses a cent: every split's children sum
    /// to the parent, and the leaf total of the account is unchanged by splitting and unsplitting.
    #[test]
    fn money_sum_conserves_through_splits(
        rows in prop::collection::vec(gen_row(), 1..=12),
        parts in 2usize..=6,
    ) {
        let mut conn = memory_db();
        let accounts = fixture_accounts(&conn);
        let nbc = account_id(&accounts, "nbc");
        let profile = generic_with_memo(&conn);
        import_text(&mut conn, nbc, profile, "rows.csv", &csv_text(&rows));
        let leaf_total = |c: &rusqlite::Connection| -> i64 {
            c.query_row("SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf WHERE account_id = ?1", [nbc], |r| r.get(0)).unwrap()
        };
        let before = leaf_total(&conn);
        let ids: Vec<i64> = conn.prepare("SELECT id FROM txn WHERE account_id = ?1 ORDER BY id").unwrap()
            .query_map([nbc], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        let cmd = audit::begin(&conn, "test.split", Actor::User).unwrap();
        for id in &ids {
            let rec = txn::get(&conn, *id).unwrap();
            let shares = allocate(rec.amount_cents, parts).unwrap();
            let split_parts: Vec<txn::SplitPart> = shares.iter().map(|c| txn::SplitPart { amount_cents: *c, category_id: None, memo: String::new() }).collect();
            let children = txn::split(&conn, &cmd, *id, &split_parts).unwrap();
            let child_sum: i64 = children.iter().map(|c| c.amount_cents).sum();
            prop_assert_eq!(child_sum, rec.amount_cents);
        }
        prop_assert_eq!(leaf_total(&conn), before, "splitting changed the leaf total");
        for id in &ids {
            txn::unsplit(&conn, &cmd, *id).unwrap();
        }
        prop_assert_eq!(leaf_total(&conn), before, "unsplitting changed the leaf total");
        let children_left: i64 = conn.query_row("SELECT count(*) FROM txn WHERE parent_id IS NOT NULL", [], |r| r.get(0)).unwrap();
        prop_assert_eq!(children_left, 0);
    }

    /// Same file twice: zero new rows, zero changed user fields — as the identical file (a
    /// recorded no-op) and as the same rows in a differently named, differently hashed file.
    #[test]
    fn import_idempotent(rows in prop::collection::vec(gen_row(), 1..=20)) {
        let mut conn = memory_db();
        let accounts = fixture_accounts(&conn);
        let nbc = account_id(&accounts, "nbc");
        let profile = generic_with_memo(&conn);
        let text = csv_text(&rows);
        let first = import_text(&mut conn, nbc, profile, "a.csv", &text);
        let rows_after_first = count(&conn, "SELECT count(*) FROM txn");
        prop_assert_eq!(first.inserted.len() + first.quarantined.len() + first.skipped.len(), rows.len());
        let fields = user_fields(&conn);

        let same = import_text(&mut conn, nbc, profile, "a-again.csv", &text);
        prop_assert!(same.reason.is_some());
        prop_assert_eq!(same.inserted.len(), 0);
        prop_assert_eq!(same.updated.len(), 0);

        let mut variant = text.clone();
        variant.push_str(",,,\n");
        let again = import_text(&mut conn, nbc, profile, "b.csv", &variant);
        prop_assert!(again.reason.is_none());
        prop_assert_eq!(again.inserted.len(), 0, "{}", again.summary_text());
        prop_assert_eq!(again.updated.len(), 0);
        prop_assert_eq!(count(&conn, "SELECT count(*) FROM txn"), rows_after_first);
        prop_assert_eq!(user_fields(&conn), fields);
    }

    /// Same amount and payee on two accounts are two events: importing the same rows into two
    /// accounts doubles the ledger and quarantines nothing.
    #[test]
    fn dedup_no_cross_account_collapse(rows in prop::collection::vec(gen_row(), 1..=20)) {
        let mut conn = memory_db();
        let accounts = fixture_accounts(&conn);
        let nbc = account_id(&accounts, "nbc");
        let rvc = account_id(&accounts, "rvc");
        let profile = generic_with_memo(&conn);
        let text = csv_text(&rows);
        let a = import_text(&mut conn, nbc, profile, "a.csv", &text);
        let b = import_text(&mut conn, rvc, profile, "a.csv", &text);
        prop_assert!(b.reason.is_none(), "a file for another account is never a duplicate file");
        prop_assert_eq!(a.inserted.len(), b.inserted.len());
        prop_assert_eq!(a.quarantined.len(), b.quarantined.len());
        prop_assert_eq!(a.skipped.len(), b.skipped.len());
        let per_account: Vec<i64> = conn.prepare("SELECT count(*) FROM txn GROUP BY account_id ORDER BY account_id").unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        prop_assert_eq!(per_account.len(), 2);
        prop_assert_eq!(per_account[0], per_account[1]);
    }

    /// A user's edits (payee, memo, category, tags) survive a re-import of the same rows.
    #[test]
    fn user_edit_survives_reimport(
        rows in prop::collection::vec(gen_row(), 1..=12),
        pick in 0usize..12,
        new_payee in "[a-z]{3,12}",
        new_memo in "[a-z ]{0,20}",
    ) {
        let mut conn = memory_db();
        let accounts = fixture_accounts(&conn);
        let nbc = account_id(&accounts, "nbc");
        let profile = generic_with_memo(&conn);
        let text = csv_text(&rows);
        let first = import_text(&mut conn, nbc, profile, "a.csv", &text);
        prop_assume!(!first.inserted.is_empty());
        let id = first.inserted[pick % first.inserted.len()];
        let groceries: i64 = conn.query_row("SELECT id FROM category WHERE system_code = 'variable.cash'", [], |r| r.get(0)).unwrap();
        let cmd = audit::begin(&conn, "test.edit", Actor::User).unwrap();
        let edited = txn::apply_user_patch(&conn, &cmd, id, &txn::TxnPatch {
            payee_norm: Some(new_payee.clone()),
            memo: Some(new_memo.clone()),
            category_id: Some(Some(groceries)),
            tags: Some(vec!["kept".into()]),
            ..Default::default()
        }, date("2026-10-05")).unwrap();
        prop_assert_eq!(edited.payee_norm.as_str(), new_payee.as_str());

        let mut variant = text.clone();
        variant.push_str(",,,\n");
        let again = import_text(&mut conn, nbc, profile, "b.csv", &variant);
        prop_assert_eq!(again.updated.len(), 0);
        let after = txn::get(&conn, id).unwrap();
        prop_assert_eq!(after.payee_norm, new_payee);
        prop_assert_eq!(after.memo, new_memo.trim().to_string());
        prop_assert_eq!(after.category_id, Some(groceries));
        prop_assert_eq!(after.tags, vec!["kept".to_string()]);
        prop_assert_eq!(after.user_edited, txn::UE_PAYEE_NORM | txn::UE_MEMO | txn::UE_CATEGORY | txn::UE_TAGS);
    }
}

/// Build a generic 4-column CSV with the given rows plus one transfer leg.
fn csv_with_leg(rows: &[GenRow], leg_day: u32, leg_cents: i64, leg_payee: &str) -> String {
    let mut text = csv_text(rows);
    text.push_str(&format!(
        "2026-07-{:02},{},{},\n",
        leg_day,
        leg_payee,
        to_decimal_string(leg_cents)
    ));
    text
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// A linked transfer pair nets to zero in the spending view and lands with the right cash
    /// timing in the cash view: between two cash accounts it vanishes; between a cash account and
    /// a card, the cash leg counts on its own date.
    #[test]
    fn transfer_not_spending(
        rows in prop::collection::vec(gen_row(), 0..=8),
        amount in 1i64..=500_000,
        out_day in 1u32..=28,
        lag in 0i64..=3,
        to_card in any::<bool>(),
    ) {
        let mut conn = memory_db();
        let accounts = fixture_accounts(&conn);
        let nbc = account_id(&accounts, "nbc");
        let other = account_id(&accounts, if to_card { "sv" } else { "nbs" });
        let profile = generic_with_memo(&conn);
        // the filler rows never collide with the pair: amounts in gen_row are never ±amount here
        let filler: Vec<GenRow> = rows.into_iter().filter(|r| r.cents.abs() != amount).collect();
        let in_day = u32::try_from(i64::from(out_day) + lag).unwrap();
        let out_text = csv_with_leg(&filler, out_day, -amount, "TRANSFER OUT TO OTHER");
        let in_text = format!("Date,Description,Amount,Memo\n2026-07-{:02},TRANSFER IN FROM NBC,{},\n", in_day, to_decimal_string(amount));
        import_text(&mut conn, nbc, profile, "out.csv", &out_text);
        import_text(&mut conn, other, profile, "in.csv", &in_text);

        let out_id: i64 = conn.query_row("SELECT id FROM txn WHERE account_id = ?1 AND payee_raw = 'TRANSFER OUT TO OTHER'", [nbc], |r| r.get(0)).unwrap();
        let in_id: i64 = conn.query_row("SELECT id FROM txn WHERE account_id = ?1 AND payee_raw = 'TRANSFER IN FROM NBC'", [other], |r| r.get(0)).unwrap();
        let out_row = txn::get(&conn, out_id).unwrap();
        let in_row = txn::get(&conn, in_id).unwrap();
        prop_assert!(out_row.transfer_link_id.is_some(), "out leg not linked");
        prop_assert_eq!(out_row.transfer_link_id, in_row.transfer_link_id);

        // spending view: the pair contributes nothing; every filler outflow counts because no rule
        // is installed here and no heuristic gives these payees an income or transfer category
        let s = kept::cash::views::spending_view(&conn, "2026-07-01", "2026-07-31").unwrap();
        let filler_out: i64 = filler.iter().filter(|r| r.cents < 0).map(|r| -r.cents).sum();
        prop_assert_eq!(s.gross_outflows_cents, filler_out);

        // cash view: cash↔cash vanishes, cash↔card counts the cash leg on its date
        let c = kept::cash::views::cash_view(&conn, "2026-07-01", "2026-07-31").unwrap();
        let filler_cash_out: i64 = filler.iter().filter(|r| r.cents < 0).map(|r| -r.cents).sum();
        let filler_cash_in: i64 = filler.iter().filter(|r| r.cents > 0).map(|r| r.cents).sum();
        if to_card {
            prop_assert_eq!(c.outflows_cents, filler_cash_out + amount);
            prop_assert_eq!(c.inflows_cents, filler_cash_in);
        } else {
            prop_assert_eq!(c.outflows_cents, filler_cash_out);
            prop_assert_eq!(c.inflows_cents, filler_cash_in);
        }
        prop_assert_eq!(c.net_cents, c.inflows_cents - c.outflows_cents);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// The reconciliation identity: for any rows and any period end, `computed = opening + Σ posted
    /// rows through the end`; the statement equal to that balances, and the next period rolls
    /// forward from it and is off by exactly the gap in its statement.
    #[test]
    fn recon_identity(
        rows in prop::collection::vec(gen_row(), 0..=20),
        end_day in 1u32..=30,
        gap in -500_000i64..=500_000,
    ) {
        let mut conn = memory_db();
        let accounts = fixture_accounts(&conn);
        let nbc = account_id(&accounts, "nbc");
        let opening = accounts.iter().find(|(k, _)| *k == "nbc").map(|(_, a)| a.opening_balance_cents).unwrap_or(0);
        let profile = generic_with_memo(&conn);
        import_text(&mut conn, nbc, profile, "a.csv", &csv_text(&rows));
        let end = format!("2026-07-{end_day:02}");
        let sum_to_end: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf WHERE account_id = ?1 AND status = 'posted' AND posted_date <= ?2",
                rusqlite::params![nbc, end],
                |r| r.get(0),
            )
            .unwrap();
        let sum_rest: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf WHERE account_id = ?1 AND status = 'posted' AND posted_date > ?2",
                rusqlite::params![nbc, end],
                |r| r.get(0),
            )
            .unwrap();
        let cmd = audit::begin(&conn, "test.recon", Actor::User).unwrap();
        let first = kept::cash::recon::reconcile(&conn, &cmd, &kept::cash::recon::ReconInput {
            account_id: nbc,
            period_end: end.clone(),
            statement_closing_cents: opening + sum_to_end,
            statement_source: "user".into(),
        }).unwrap();
        prop_assert_eq!(first.period_start.as_str(), "2026-07-01");
        prop_assert_eq!(first.opening_cents, opening);
        prop_assert_eq!(first.computed_closing_cents, opening + sum_to_end);
        prop_assert_eq!(first.difference_cents, 0);
        prop_assert_eq!(first.status.as_str(), "balanced");

        let second = kept::cash::recon::reconcile(&conn, &cmd, &kept::cash::recon::ReconInput {
            account_id: nbc,
            period_end: "2026-07-31".into(),
            statement_closing_cents: opening + sum_to_end + sum_rest + gap,
            statement_source: "user".into(),
        }).unwrap();
        prop_assert_eq!(second.period_start, format!("2026-07-{:02}", end_day + 1));
        prop_assert_eq!(second.opening_cents, first.statement_closing_cents);
        prop_assert_eq!(second.computed_closing_cents, opening + sum_to_end + sum_rest);
        prop_assert_eq!(second.difference_cents, -gap);
        prop_assert_eq!(second.status.as_str(), if gap == 0 { "balanced" } else { "off" });
        prop_assert_eq!(second.balanced_at.is_some(), gap == 0);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// The hero is its terms: `safe = available − earmarks − obligations − buffer`, and every term
    /// is the sum of the items it returns.
    #[test]
    fn safe_terms_sum(
        rows in prop::collection::vec(gen_row(), 0..=12),
        buffer in 0i64..=100_000,
        entries in prop::collection::vec(-50_000i64..=50_000, 0..=3),
    ) {
        let mut conn = memory_db();
        let accounts = fixture_accounts(&conn);
        let nbc = account_id(&accounts, "nbc");
        let nbs = account_id(&accounts, "nbs");
        let profile = generic_with_memo(&conn);
        import_text(&mut conn, nbc, profile, "a.csv", &csv_text(&rows));
        kept::db::settings::update(&mut conn, "timing_buffer_cents", &serde_json::json!(buffer)).unwrap();
        let cmd = audit::begin(&conn, "test.plan", Actor::User).unwrap();
        let em = kept::plan::earmark::create(&conn, &cmd, &kept::plan::earmark::EarmarkInput {
            name: "Reserve".into(),
            kind: "emergency_reserve".into(),
            funding_account_id: nbs,
            obligation_id: None,
            target_cents: 1_000_000,
            target_date: None,
            schedule: "none".into(),
            schedule_amount_cents: None,
            schedule_day: None,
            schedule_income_stream_id: None,
            active: true,
        }).unwrap();
        for (i, cents) in entries.iter().enumerate() {
            if *cents == 0 {
                continue;
            }
            kept::plan::earmark::add_entry(&conn, &cmd, em.id, &kept::plan::earmark::EntryInput {
                entry_date: format!("2026-07-{:02}", i + 1),
                kind: "adjust".into(),
                amount_cents: *cents,
                txn_id: None,
                note: String::new(),
            }).unwrap();
        }
        let hero = kept::cash::safe::safe_to_spend(&conn, date("2026-07-31")).unwrap();
        let t = &hero.terms;
        prop_assert_eq!(hero.safe_cents, t.available.cents - t.earmarks.cents - t.obligations.cents - t.buffer.cents);
        prop_assert_eq!(t.buffer.cents, buffer);
        prop_assert_eq!(t.available.cents, t.available.accounts.iter().map(|a| a.posted_cents + a.pending_in_cents - a.pending_out_cents).sum::<i64>());
        prop_assert_eq!(t.earmarks.cents, t.earmarks.items.iter().map(|e| e.counted_cents).sum::<i64>());
        prop_assert_eq!(t.earmarks.cents, entries.iter().sum::<i64>().max(0));
        prop_assert_eq!(t.obligations.cents, t.obligations.items.iter().map(|o| o.counted_cents).sum::<i64>());
        prop_assert!(t.earmarks.items.iter().all(|e| e.counted_cents >= 0));
    }

    /// A firewalled account never contributes to `available`, whatever its balance; it is listed
    /// among the excluded with its balance and the reason.
    #[test]
    fn firewall_excluded(rows in prop::collection::vec(gen_row(), 0..=10), firewalled in any::<bool>()) {
        let mut conn = memory_db();
        fixture_accounts(&conn);
        let cmd = audit::begin(&conn, "test.plan", Actor::User).unwrap();
        let vault = kept::db::repo::account::create(&conn, &cmd, &kept::db::repo::account::NewAccount {
            name: "Vault".into(),
            institution: "Northbank".into(),
            kind: "checking".into(),
            opening_balance_cents: 100_000,
            opening_date: "2026-07-01".into(),
            venture_id: None,
            firewalled,
        }).unwrap();
        let profile = generic_with_memo(&conn);
        import_text(&mut conn, vault.id, profile, "v.csv", &csv_text(&rows));
        let posted: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf WHERE account_id = ?1 AND status = 'posted'",
                [vault.id],
                |r| r.get(0),
            )
            .unwrap();
        let hero = kept::cash::safe::safe_to_spend(&conn, date("2026-07-31")).unwrap();
        let in_available = hero.terms.available.accounts.iter().find(|a| a.account_id == vault.id);
        let in_excluded = hero.excluded.firewalled_accounts.iter().find(|a| a.account_id == vault.id);
        if firewalled {
            prop_assert!(in_available.is_none());
            prop_assert_eq!(in_excluded.map(|a| a.posted_cents), Some(100_000 + posted));
            prop_assert!(in_excluded.is_some_and(|a| a.reason.contains("firewall")));
        } else {
            prop_assert_eq!(in_available.map(|a| a.posted_cents), Some(100_000 + posted));
            prop_assert!(in_excluded.is_none());
        }
    }

    /// A pending inflow flagged borrowing or securities-sale is not cash: it is left out of
    /// `available` and listed as excluded. Posted, it sits inside the balance and is still listed,
    /// never as income.
    #[test]
    fn borrowing_not_income(
        rows in prop::collection::vec(gen_row(), 0..=8),
        cents in 1i64..=500_000,
        flag in prop::sample::select(vec![8i64, 16, 24]),
        pending in any::<bool>(),
    ) {
        let mut conn = memory_db();
        let accounts = fixture_accounts(&conn);
        let nbc = account_id(&accounts, "nbc");
        let profile = generic_with_memo(&conn);
        let mut text = csv_text(&rows);
        text.push_str(&format!("2026-07-15,LOAN PROCEEDS,{},\n", to_decimal_string(cents)));
        import_text(&mut conn, nbc, profile, "a.csv", &text);
        let id: i64 = conn
            .query_row("SELECT id FROM txn WHERE account_id = ?1 AND payee_raw = 'LOAN PROCEEDS'", [nbc], |r| r.get(0))
            .unwrap();
        conn.execute(
            "UPDATE txn SET status = ?2, flags = ?3 WHERE id = ?1",
            rusqlite::params![id, if pending { "pending" } else { "posted" }, flag],
        )
        .unwrap();
        let base: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf WHERE account_id = ?1 AND status = 'posted' AND id <> ?2",
                rusqlite::params![nbc, id],
                |r| r.get(0),
            )
            .unwrap();
        let hero = kept::cash::safe::safe_to_spend(&conn, date("2026-07-31")).unwrap();
        let acct = hero.terms.available.accounts.iter().find(|a| a.account_id == nbc).unwrap();
        if pending {
            prop_assert_eq!(acct.pending_in_cents, 0);
            prop_assert_eq!(acct.posted_cents, 321_455 + base);
            prop_assert!(hero.excluded.pending_flagged_inflows.iter().any(|f| f.txn_id == id && f.cents == cents));
            prop_assert!(!acct.pending_row_ids.contains(&id));
        } else {
            prop_assert_eq!(acct.posted_cents, 321_455 + base + cents);
            prop_assert!(hero.excluded.posted_flagged_inflows.iter().any(|f| f.txn_id == id && f.cents == cents));
            prop_assert!(hero.excluded.pending_flagged_inflows.is_empty());
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// For every scenario, every day closes at `opening + inflows − outflows` and the horizon
    /// ties out the same way; the lowest point, the shortfall and the buffer breach are read off
    /// that same series. A scenario changes inputs, never arithmetic.
    #[test]
    fn forecast_ties(
        rows in prop::collection::vec(gen_row(), 0..=12),
        downside in any::<bool>(),
        bill in prop::option::of((0i64..=90, 1i64..=500_000)),
        buffer in 0i64..=50_000,
        pending_day in prop::option::of(1u32..=28),
    ) {
        let mut conn = memory_db();
        let accounts = fixture_accounts(&conn);
        let nbc = account_id(&accounts, "nbc");
        let profile = generic_with_memo(&conn);
        import_text(&mut conn, nbc, profile, "a.csv", &csv_text(&rows));
        kept::db::settings::update(&mut conn, "timing_buffer_cents", &serde_json::json!(buffer)).unwrap();
        let today = date("2026-07-31");
        if let (Some(day), false) = (pending_day, rows.is_empty()) {
            // one imported row still pending, posting inside the window
            conn.execute(
                "UPDATE txn SET status = 'pending', effective_date = ?1 WHERE id = (SELECT MIN(id) FROM txn)",
                [format!("2026-08-{day:02}")],
            ).unwrap();
        }
        let cmd = audit::begin(&conn, "test.plan", Actor::User).unwrap();
        kept::plan::income::create(&conn, &cmd, &kept::plan::income::IncomeInput {
            name: "Pay".into(),
            kind: "base".into(),
            cycle: "biweekly".into(),
            anchor_date: "2026-07-10".into(),
            semimonthly_day_1: None,
            semimonthly_day_2: None,
            expected_net_cents: 300_000,
            variability_cents: 0,
            confidence: "confirmed".into(),
            weekend_rule: "previous_business_day".into(),
            deposit_account_id: Some(nbc),
            match_payee_contains: Some("meridian cap".into()),
            active: true,
        }).unwrap();
        kept::plan::obligation::create(&conn, &cmd, &kept::plan::obligation::ObligationInput {
            name: "Rent".into(),
            kind: "bill".into(),
            status: "confirmed".into(),
            due_rule: "monthly_day".into(),
            due_day: Some(1),
            due_month: None,
            due_weekday: None,
            due_nth: None,
            anchor_date: None,
            expected_cents: 240_000,
            variability_cents: 0,
            source_account_id: nbc,
            autopay: false,
            category_id: None,
            debt_id: None,
            match_payee_contains: Some("lakeshore".into()),
        }).unwrap();
        let scenario = kept::forecast::Scenario {
            downside,
            surprise_bill: bill.map(|(offset, cents)| kept::forecast::SurpriseBill {
                date: kept::dates::format_civil(today + chrono::Duration::days(offset)),
                cents,
            }),
        };
        let f = kept::forecast::run(&conn, today, &scenario).unwrap();
        prop_assert_eq!(f.days.len(), 91);
        prop_assert_eq!(f.weeks.len(), 13);
        let mut bal = f.opening_cents;
        let (mut inflows, mut outflows) = (0i64, 0i64);
        for (i, d) in f.days.iter().enumerate() {
            prop_assert_eq!(d.day as usize, i);
            prop_assert_eq!(d.opening_cents, bal);
            prop_assert_eq!(d.inflows_cents, d.events.iter().filter(|e| e.cents > 0).map(|e| e.cents).sum::<i64>());
            prop_assert_eq!(d.outflows_cents, -d.events.iter().filter(|e| e.cents < 0).map(|e| e.cents).sum::<i64>());
            prop_assert_eq!(d.closing_cents, d.opening_cents + d.inflows_cents - d.outflows_cents);
            prop_assert_eq!(d.headroom_cents, d.closing_cents - d.committed_cents);
            prop_assert!(d.committed_cents >= buffer);
            bal = d.closing_cents;
            inflows += d.inflows_cents;
            outflows += d.outflows_cents;
        }
        prop_assert_eq!((f.inflows_cents, f.outflows_cents), (inflows, outflows));
        prop_assert_eq!(f.closing_cents, f.opening_cents + f.inflows_cents - f.outflows_cents);
        prop_assert_eq!(f.closing_cents, bal);
        let min = f.days.iter().map(|d| d.closing_cents).min().unwrap();
        prop_assert_eq!(f.lowest.cents, min);
        prop_assert_eq!(&f.lowest.date, &f.days.iter().find(|d| d.closing_cents == min).unwrap().date);
        prop_assert_eq!(f.first_shortfall.as_ref().map(|p| p.date.clone()), f.days.iter().find(|d| d.closing_cents < 0).map(|d| d.date.clone()));
        prop_assert_eq!(f.first_buffer_breach.as_ref().map(|p| p.date.clone()), f.days.iter().find(|d| d.headroom_cents < 0).map(|d| d.date.clone()));
        prop_assert_eq!(f.weeks.iter().map(|w| w.inflows_cents).sum::<i64>(), f.inflows_cents);
        prop_assert_eq!(f.weeks.iter().map(|w| w.outflows_cents).sum::<i64>(), f.outflows_cents);
        prop_assert_eq!(f.weeks.last().unwrap().closing_cents, f.closing_cents);
        // the scenario never invents pay: income is the confirmed stream's occurrences, shifted or not
        let pay: i64 = f.days.iter().flat_map(|d| d.events.iter()).filter(|e| e.kind == "income").map(|e| e.cents).sum();
        prop_assert_eq!(pay, 300_000 * f.pay_dates.len() as i64);
        prop_assert_eq!(f.pay_dates.iter().filter(|p| p.shifted).count(), usize::from(downside && !f.pay_dates.is_empty()));
        if let Some((_, cents)) = bill {
            let surprise: i64 = f.days.iter().flat_map(|d| d.events.iter()).filter(|e| e.kind == "surprise").map(|e| e.cents).sum();
            prop_assert_eq!(surprise, -cents);
        }
    }
}
