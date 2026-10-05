//! M2 acceptance against fixtures/EXPECTED.md and its machine-readable twins (rules.json,
//! automation.json): every transfer pair links with the right kind, the refund links, every row
//! lands in the category the written-down rules and heuristics give it, the review queue is the
//! six rows in |amount| order, and the spending and cash views equal the stated totals.

mod common;

use std::collections::HashMap;

use common::*;
use kept::cash::views;
use kept::db::audit::{self, Actor};
use kept::db::repo::link::{self, Confidence, TransferKind};
use kept::db::repo::rule;
use kept::db::repo::{ledger, txn};
use kept::error::AppError;
use kept::import::{self, ImportInput};
use kept::rules;
use rusqlite::Connection;

const TODAY: &str = "2026-10-05";

#[derive(serde::Deserialize)]
struct AutomationFile {
    rows: Vec<ExpectedRow>,
    pairs: HashMap<String, PairSpec>,
}

#[derive(serde::Deserialize)]
struct ExpectedRow {
    account: String,
    posted: String,
    description: String,
    amount_cents: i64,
    category: Option<String>,
    why: String,
    flags: Vec<String>,
    pair: Option<String>,
    refund_of: Option<String>,
}

#[derive(serde::Deserialize)]
struct PairSpec {
    kind: String,
}

fn import_everything(conn: &mut Connection, accounts: &[(&str, kept::db::repo::account::Account)]) {
    let plan: [(&str, &str); 17] = [
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
        ("hb", "harbor/harbor_brokerage_2026-Q3.csv"),
        ("vm", "venmo/venmo_2026-Q3.csv"),
    ];
    for (key, rel) in plan {
        let input = ImportInput {
            account_id: account_id(accounts, key),
            profile_id: None,
            file_name: rel.to_string(),
            bytes: fixture_bytes(rel),
        };
        import::commit(conn, &input, date(TODAY), 8500).unwrap_or_else(|e| panic!("{rel}: {e:?}"));
    }
}

fn find_row(
    conn: &Connection,
    accounts: &[(&str, kept::db::repo::account::Account)],
    e: &ExpectedRow,
) -> txn::TxnRecord {
    let id: i64 = conn
        .query_row(
            "SELECT id FROM txn WHERE account_id = ?1 AND posted_date = ?2 AND payee_raw = ?3 AND amount_cents = ?4 AND parent_id IS NULL",
            rusqlite::params![account_id(accounts, &e.account), e.posted, e.description, e.amount_cents],
            |r| r.get(0),
        )
        .unwrap_or_else(|err| panic!("{} {} {} {}: {err}", e.account, e.posted, e.description, e.amount_cents));
    txn::get(conn, id).unwrap()
}

fn code_of(conn: &Connection, category_id: Option<i64>) -> Option<String> {
    category_id.map(|id| {
        conn.query_row(
            "SELECT COALESCE(system_code, name) FROM category WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    })
}

#[test]
fn rules_heuristics_and_links_match_the_written_outcomes() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    import_everything(&mut conn, &accounts);
    let expected: AutomationFile = load_json("automation.json");

    let mut mismatches = Vec::new();
    let mut pair_links: HashMap<String, Vec<(i64, Option<i64>)>> = HashMap::new();
    for e in &expected.rows {
        let row = find_row(&conn, &accounts, e);
        let actual_cat = code_of(&conn, row.category_id);
        if actual_cat != e.category {
            mismatches.push(format!(
                "{} {} {}: category {:?}, expected {:?}",
                e.account, e.posted, e.description, actual_cat, e.category
            ));
        }
        let actual_why = match row.classification.as_str() {
            "rule" => row
                .rule_id
                .map(|id| format!("rule:{}", rule::get(&conn, id).unwrap().name))
                .unwrap_or_default(),
            "heuristic" => format!(
                "heuristic:{}",
                row.heuristic_code.clone().unwrap_or_default()
            ),
            other => other.to_string(),
        };
        if actual_why != e.why {
            mismatches.push(format!(
                "{} {} {}: why {:?}, expected {:?}",
                e.account, e.posted, e.description, actual_why, e.why
            ));
        }
        let mut actual_flags: Vec<String> =
            kept::import::csv::flag_names(u32::try_from(row.flags).unwrap())
                .into_iter()
                .map(str::to_string)
                .collect();
        actual_flags.sort();
        if actual_flags != e.flags {
            mismatches.push(format!(
                "{} {} {}: flags {:?}, expected {:?}",
                e.account, e.posted, e.description, actual_flags, e.flags
            ));
        }
        if let Some(pair) = &e.pair {
            pair_links
                .entry(pair.clone())
                .or_default()
                .push((row.id, row.transfer_link_id));
        } else if row.transfer_link_id.is_some() {
            mismatches.push(format!(
                "{} {} {}: linked as a transfer but no pair is expected",
                e.account, e.posted, e.description
            ));
        }
        if e.refund_of.is_some() {
            assert!(
                row.refund_link_id.is_some(),
                "{} should be refund-linked",
                e.description
            );
            let link = link::get_refund(&conn, row.refund_link_id.unwrap()).unwrap();
            let original = txn::get(&conn, link.original_txn_id).unwrap();
            assert_eq!(original.payee_raw, e.refund_of.clone().unwrap());
            assert_eq!(original.amount_cents, -e.amount_cents);
            assert_eq!(original.posted_date, "2026-08-09");
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );

    // every expected pair is one link joining exactly its two legs, with the expected kind
    assert_eq!(pair_links.len(), expected.pairs.len());
    for (pair, legs) in &pair_links {
        assert_eq!(legs.len(), 2, "pair {pair}");
        let link_ids: Vec<Option<i64>> = legs.iter().map(|(_, l)| *l).collect();
        assert!(
            link_ids[0].is_some() && link_ids[0] == link_ids[1],
            "pair {pair} legs {legs:?} are not linked together"
        );
        let l = link::get_transfer(&conn, link_ids[0].unwrap()).unwrap();
        assert_eq!(l.kind, expected.pairs[pair].kind, "pair {pair}");
        assert_eq!(l.confidence, "heuristic");
    }
    assert_eq!(count(&conn, "SELECT count(*) FROM transfer_link"), 11);
    assert_eq!(count(&conn, "SELECT count(*) FROM refund_link"), 1);

    // the review queue, exactly as EXPECTED.md lists it
    let queue = ledger::review_queue(&conn, 100).unwrap();
    let got: Vec<(String, i64)> = queue
        .iter()
        .map(|r| (r.payee_raw.clone(), r.amount_cents))
        .collect();
    assert_eq!(
        got,
        vec![
            ("ACH TRANSFER TO NORTHBANK ...1234".to_string(), -250_000),
            ("Chris Park".to_string(), 60_000),
            ("ATM WITHDRAWAL BANCO AZTECA CDMX".to_string(), -16_342),
            ("ATM WITHDRAWAL 1120 N STATE".to_string(), -10_000),
            ("VENMO *MORGAN AVERY".to_string(), -8_500),
            ("Morgan Avery".to_string(), 4_200),
        ]
    );
    // EXPECTED.md's "without a category" rows: the ATM and payment-app rows the heuristics flagged
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM txn_leaf WHERE category_id IS NULL"
        ),
        5
    );

    // rows per category, as the EXPECTED.md table
    let per_cat: Vec<(String, i64)> = conn
        .prepare("SELECT COALESCE(c.system_code, '—'), count(*) FROM txn_leaf t LEFT JOIN category c ON c.id = t.category_id GROUP BY 1 ORDER BY 1")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let lookup: HashMap<String, i64> = per_cat.into_iter().collect();
    for (code, n) in [
        ("debt.fees", 2),
        ("debt.interest", 3),
        ("fixed.rent", 6),
        ("fixed.utilities", 6),
        ("income.salary", 6),
        ("transfer.card_payment", 12),
        ("transfer.internal", 10),
        ("transfer.loan_repayment", 2),
        ("transfer.securities_sale_proceeds", 1),
        ("venture.operating_expense", 6),
        ("variable.groceries", 11),
        ("—", 5),
    ] {
        assert_eq!(lookup.get(code).copied().unwrap_or(0), n, "{code}");
    }
    // venture rows carry the venture from the rule's action
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM txn WHERE venture_id IS NOT NULL"
        ),
        6
    );
}

#[test]
fn spending_and_cash_views_equal_the_stated_totals() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    import_everything(&mut conn, &accounts);

    let s = views::spending_view(&conn, "2026-07-01", "2026-09-30").unwrap();
    assert_eq!(s.gross_outflows_cents, 1_184_620, "gross outflows");
    assert_eq!(s.linked_refunds_cents, 8_417, "linked refunds");
    assert_eq!(s.reimbursements_cents, 360_000, "reimbursements");
    assert_eq!(s.net_spending_cents, 816_203, "net spending");
    assert_eq!(
        s.positive_review_cents, 64_200,
        "positive rows awaiting review"
    );
    let rent = s
        .by_category
        .iter()
        .find(|l| l.path.as_deref() == Some("Fixed › Rent"))
        .unwrap();
    assert_eq!(
        (
            rent.outflows_cents,
            rent.inflows_cents,
            rent.net_cents,
            rent.rows
        ),
        (720_000, 360_000, 360_000, 6)
    );
    let shopping = s
        .by_category
        .iter()
        .find(|l| l.path.as_deref() == Some("Variable › Shopping"))
        .unwrap();
    assert_eq!(
        (shopping.outflows_cents, shopping.inflows_cents),
        (4_299 + 8_417 + 6_250, 8_417)
    );

    let c = views::cash_view(&conn, "2026-07-01", "2026-09-30").unwrap();
    assert_eq!(c.outflows_cents, 1_579_994, "cash outflows");
    assert_eq!(c.inflows_cents, 2_860_061, "cash inflows");
    assert_eq!(c.net_cents, 1_280_067, "net change in cash accounts");
    let by_account_net: i64 = c.by_account.iter().map(|l| l.net_cents).sum();
    assert_eq!(by_account_net, 1_280_067);
    let cash_accounts: Vec<&str> = c
        .by_account
        .iter()
        .map(|l| l.account_name.as_str())
        .collect();
    assert_eq!(
        cash_accounts,
        vec![
            "Northbank Checking",
            "Northbank Savings",
            "Riverside Checking",
            "Venmo"
        ]
    );

    assert_eq!(
        s.gross_outflows_cents - c.outflows_cents,
        -395_374,
        "spending gross minus cash outflows"
    );
}

#[test]
fn corrections_propose_rules_and_links_can_be_undone_and_remade_by_hand() {
    let mut conn = memory_db();
    let accounts = fixture_accounts(&conn);
    install_rules(&conn);
    import_everything(&mut conn, &accounts);

    // a manual correction on a review row proposes (never creates) a rule
    let nbc = account_id(&accounts, "nbc");
    let venmo_row: i64 = conn
        .query_row(
            "SELECT id FROM txn WHERE account_id = ?1 AND payee_raw = 'VENMO *MORGAN AVERY'",
            [nbc],
            |r| r.get(0),
        )
        .unwrap();
    let dining = category_id(&conn, "variable.dining");
    let cmd = audit::begin(&conn, "test.correct", Actor::User).unwrap();
    txn::apply_user_patch(
        &conn,
        &cmd,
        venmo_row,
        &txn::TxnPatch {
            category_id: Some(Some(dining)),
            ..Default::default()
        },
    )
    .unwrap();
    let rules_before = count(&conn, "SELECT count(*) FROM rule");
    let proposal = rules::propose_rule(&conn, venmo_row).unwrap();
    // the normaliser cuts `VENMO *MORGAN AVERY` at the `*` (EXPECTED.md normalisation table)
    assert_eq!(proposal.match_payee_contains, "venmo");
    assert_eq!(proposal.action_category_id, Some(dining));
    // the only other payee containing "venmo" is the linked VENMO CASHOUT, which rules skip
    assert_eq!(proposal.would_match, 0);
    assert_eq!(
        count(&conn, "SELECT count(*) FROM rule"),
        rules_before,
        "a proposal creates nothing"
    );
    // the corrected row is out of the queue, which now has five rows
    assert_eq!(ledger::review_queue(&conn, 100).unwrap().len(), 5);

    // unlinking a transfer sends both legs back to review; relinking by hand marks them manual
    let visa_09 = txn::get(
        &conn,
        conn.query_row("SELECT id FROM txn WHERE payee_raw = 'SUMMIT CARD SERVICES PAYMENT' AND posted_date = '2026-09-20'", [], |r| r.get(0)).unwrap(),
    )
    .unwrap();
    let link_id = visa_09.transfer_link_id.expect("linked by automation");
    let l = link::get_transfer(&conn, link_id).unwrap();
    link::remove_transfer(&conn, &cmd, link_id).unwrap();
    let out = txn::get(&conn, l.out_txn_id).unwrap();
    let into = txn::get(&conn, l.in_txn_id).unwrap();
    assert!(out.transfer_link_id.is_none() && into.transfer_link_id.is_none());
    assert_eq!(out.classification, "unclassified");
    assert_eq!(ledger::review_queue(&conn, 100).unwrap().len(), 7);
    let relinked = link::create_transfer(
        &conn,
        &cmd,
        l.out_txn_id,
        l.in_txn_id,
        TransferKind::CardPayment,
        Confidence::User,
    )
    .unwrap();
    assert_eq!(relinked.confidence, "user");
    let out = txn::get(&conn, l.out_txn_id).unwrap();
    assert_eq!(out.classification, "manual");
    assert_eq!(
        code_of(&conn, out.category_id).as_deref(),
        Some("transfer.card_payment")
    );
    assert_eq!(ledger::review_queue(&conn, 100).unwrap().len(), 5);

    // a transfer cannot be built from two rows on one account or with unequal amounts
    let rent: i64 = conn
        .query_row("SELECT id FROM txn WHERE payee_raw = 'LAKESHORE PROPERTIES RENT' AND posted_date = '2026-07-01'", [], |r| r.get(0))
        .unwrap();
    let payroll: i64 = conn
        .query_row("SELECT id FROM txn WHERE payee_raw = 'MERIDIAN CAP ACH PAYROLL' AND posted_date = '2026-07-10'", [], |r| r.get(0))
        .unwrap();
    assert!(matches!(
        link::create_transfer(
            &conn,
            &cmd,
            rent,
            payroll,
            TransferKind::Internal,
            Confidence::User
        ),
        Err(AppError::Validation { .. })
    ));

    // acknowledging the firewall touch releases the only remaining classified queue row
    let hb_out: i64 = conn
        .query_row(
            "SELECT id FROM txn WHERE payee_raw = 'ACH TRANSFER TO NORTHBANK ...1234'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let acked =
        link::acknowledge_firewall(&conn, &cmd, hb_out, "moved sale proceeds to checking").unwrap();
    assert_eq!(acked.flags & 1, 0);
    assert_eq!(count(&conn, "SELECT count(*) FROM firewall_ack"), 1);
    assert!(matches!(
        link::acknowledge_firewall(&conn, &cmd, hb_out, ""),
        Err(AppError::Conflict(_))
    ));
    assert_eq!(ledger::review_queue(&conn, 100).unwrap().len(), 4);

    // re-running automation over unclassified rows is idempotent
    let before: Vec<(i64, Option<i64>, String)> = conn
        .prepare("SELECT id, category_id, classification FROM txn ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let report = rules::automate(&conn, &cmd, None).unwrap();
    assert_eq!(report.changed, 0);
    assert_eq!(report.transfers_linked, 0);
    assert_eq!(
        report.rule_hits + report.heuristic_hits + report.unclassified,
        report.considered
    );
    let after: Vec<(i64, Option<i64>, String)> = conn
        .prepare("SELECT id, category_id, classification FROM txn ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(before, after);
}
