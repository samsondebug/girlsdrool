//! The M4 plan exactly as fixtures/plan.json states it, for the acceptance tests and the seed.

use std::collections::HashMap;

use kept::db::audit::{self, Actor, CommandRecord};
use kept::db::settings;
use kept::import::{self, ImportInput};
use kept::plan::{self, earmark, income, obligation};
use rusqlite::Connection;

use super::{account_id, category_id, date, fixture_bytes};

#[derive(serde::Deserialize)]
pub struct PlanFile {
    pub as_of: String,
    pub timing_buffer_cents: i64,
    pub upcoming_days: i64,
    pub income_streams: Vec<StreamSpec>,
    pub obligations: Vec<ObligationSpec>,
    pub earmarks: Vec<EarmarkSpec>,
    pub receipts: Vec<MatchSpec>,
    pub payments: Vec<MatchSpec>,
    pub next_income: NextIncomeSpec,
    pub hero: HeroSpec,
    pub upcoming: Vec<UpcomingSpec>,
    pub candidates: Vec<CandidateSpec>,
    pub candidates_with_plan: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct StreamSpec {
    pub name: String,
    pub kind: String,
    pub cycle: String,
    pub anchor_date: String,
    pub expected_net_cents: i64,
    pub variability_cents: i64,
    pub confidence: String,
    pub weekend_rule: String,
    pub deposit_account: String,
    pub match_payee_contains: String,
}

#[derive(serde::Deserialize)]
pub struct ObligationSpec {
    pub name: String,
    pub kind: String,
    pub due_rule: String,
    pub due_day: Option<i64>,
    pub due_month: Option<i64>,
    pub expected_cents: i64,
    pub variability_cents: i64,
    pub source_account: String,
    pub autopay: bool,
    pub category: String,
    pub match_payee_contains: String,
}

#[derive(serde::Deserialize)]
pub struct EarmarkSpec {
    pub name: String,
    pub kind: String,
    pub funding_account: String,
    pub obligation: Option<String>,
    pub target_cents: i64,
    pub target_date: Option<String>,
    pub schedule: String,
    pub schedule_amount_cents: Option<i64>,
    pub schedule_day: Option<i64>,
    pub schedule_income_stream: Option<String>,
    pub entries: Vec<EntrySpec>,
}

#[derive(serde::Deserialize)]
pub struct EntrySpec {
    pub entry_date: String,
    pub kind: String,
    pub amount_cents: i64,
    pub note: String,
}

#[derive(serde::Deserialize)]
pub struct MatchSpec {
    #[serde(alias = "stream", alias = "obligation")]
    pub owner: String,
    pub due_date: String,
    pub account: String,
    pub posted: String,
    pub description: String,
    pub amount_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct NextIncomeSpec {
    pub date: String,
    pub stream: String,
    pub expected_net_cents: i64,
    pub days_away: i64,
}

#[derive(serde::Deserialize)]
pub struct HeroSpec {
    pub available: AvailableSpec,
    pub earmarks: EarmarkTermSpec,
    pub obligations: ObligationTermSpec,
    pub buffer_cents: i64,
    pub safe_cents: i64,
    pub excluded_firewalled: Vec<ExcludedSpec>,
}

#[derive(serde::Deserialize)]
pub struct AvailableSpec {
    pub total_cents: i64,
    pub accounts: Vec<AvailableAccountSpec>,
}

#[derive(serde::Deserialize)]
pub struct AvailableAccountSpec {
    pub account: String,
    pub posted_cents: i64,
    pub pending_in_cents: i64,
    pub pending_out_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct EarmarkTermSpec {
    pub total_cents: i64,
    pub items: Vec<EarmarkItemSpec>,
}

#[derive(serde::Deserialize)]
pub struct EarmarkItemSpec {
    pub earmark: String,
    pub remaining_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct ObligationTermSpec {
    pub total_cents: i64,
    pub items: Vec<ObligationItemSpec>,
}

#[derive(serde::Deserialize)]
pub struct ObligationItemSpec {
    pub obligation: String,
    pub due_date: String,
    pub expected_cents: i64,
    pub earmark_covered_cents: i64,
    pub counted_cents: i64,
    pub overdue: bool,
}

#[derive(serde::Deserialize)]
pub struct ExcludedSpec {
    pub account: String,
    pub posted_cents: i64,
}

#[derive(serde::Deserialize)]
pub struct UpcomingSpec {
    pub obligation: String,
    pub due_date: String,
    pub expected_cents: i64,
    pub variability_cents: i64,
    pub earmark_covered_cents: i64,
    pub autopay: bool,
    pub overdue: bool,
    pub days_away: i64,
}

#[derive(serde::Deserialize)]
pub struct CandidateSpec {
    pub account: String,
    pub payee_norm: String,
    pub rows: usize,
    pub due_day: i64,
    pub expected_cents: i64,
    pub variability_cents: i64,
}

pub const PLAN_FILES: [(&str, &str); 17] = [
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

pub fn import_everything(
    conn: &mut Connection,
    accounts: &[(&str, kept::db::repo::account::Account)],
    today: &str,
) {
    for (key, rel) in PLAN_FILES {
        let input = ImportInput {
            account_id: account_id(accounts, key),
            profile_id: None,
            file_name: rel.to_string(),
            bytes: fixture_bytes(rel),
        };
        import::commit(conn, &input, date(today), 8500).unwrap_or_else(|e| panic!("{rel}: {e:?}"));
    }
}

pub fn command(conn: &Connection) -> CommandRecord {
    audit::begin(conn, "test.plan", Actor::User).unwrap()
}

pub struct Installed {
    pub streams: HashMap<String, i64>,
    pub obligations: HashMap<String, i64>,
    pub earmarks: HashMap<String, i64>,
}

/// The plan exactly as plan.json states it, then one matching pass as of the as-of date.
pub fn install_plan(
    conn: &mut Connection,
    accounts: &[(&str, kept::db::repo::account::Account)],
    file: &PlanFile,
) -> Installed {
    settings::update(
        conn,
        "timing_buffer_cents",
        &serde_json::json!(file.timing_buffer_cents),
    )
    .unwrap();
    let cmd = command(conn);
    let mut streams = HashMap::new();
    for s in &file.income_streams {
        let created = income::create(
            conn,
            &cmd,
            &income::IncomeInput {
                name: s.name.clone(),
                kind: s.kind.clone(),
                cycle: s.cycle.clone(),
                anchor_date: s.anchor_date.clone(),
                semimonthly_day_1: None,
                semimonthly_day_2: None,
                expected_net_cents: s.expected_net_cents,
                variability_cents: s.variability_cents,
                confidence: s.confidence.clone(),
                weekend_rule: s.weekend_rule.clone(),
                deposit_account_id: Some(account_id(accounts, &s.deposit_account)),
                match_payee_contains: Some(s.match_payee_contains.clone()),
                active: true,
            },
        )
        .unwrap_or_else(|e| panic!("stream {}: {e:?}", s.name));
        streams.insert(s.name.clone(), created.id);
    }
    let mut obligations = HashMap::new();
    for o in &file.obligations {
        let created = obligation::create(
            conn,
            &cmd,
            &obligation::ObligationInput {
                name: o.name.clone(),
                kind: o.kind.clone(),
                status: "confirmed".into(),
                due_rule: o.due_rule.clone(),
                due_day: o.due_day,
                due_month: o.due_month,
                due_weekday: None,
                due_nth: None,
                anchor_date: None,
                expected_cents: o.expected_cents,
                variability_cents: o.variability_cents,
                source_account_id: account_id(accounts, &o.source_account),
                autopay: o.autopay,
                category_id: Some(category_id(conn, &o.category)),
                debt_id: None,
                match_payee_contains: Some(o.match_payee_contains.clone()),
            },
        )
        .unwrap_or_else(|e| panic!("obligation {}: {e:?}", o.name));
        obligations.insert(o.name.clone(), created.id);
    }
    let mut earmarks = HashMap::new();
    for e in &file.earmarks {
        let created = earmark::create(
            conn,
            &cmd,
            &earmark::EarmarkInput {
                name: e.name.clone(),
                kind: e.kind.clone(),
                funding_account_id: account_id(accounts, &e.funding_account),
                obligation_id: e.obligation.as_ref().map(|n| obligations[n]),
                target_cents: e.target_cents,
                target_date: e.target_date.clone(),
                schedule: e.schedule.clone(),
                schedule_amount_cents: e.schedule_amount_cents,
                schedule_day: e.schedule_day,
                schedule_income_stream_id: e.schedule_income_stream.as_ref().map(|n| streams[n]),
                active: true,
            },
        )
        .unwrap_or_else(|err| panic!("earmark {}: {err:?}", e.name));
        for en in &e.entries {
            earmark::add_entry(
                conn,
                &cmd,
                created.id,
                &earmark::EntryInput {
                    entry_date: en.entry_date.clone(),
                    kind: en.kind.clone(),
                    amount_cents: en.amount_cents,
                    txn_id: None,
                    note: en.note.clone(),
                },
            )
            .unwrap();
        }
        earmarks.insert(e.name.clone(), created.id);
    }
    plan::match_all(conn, &cmd, date(&file.as_of)).unwrap();
    Installed {
        streams,
        obligations,
        earmarks,
    }
}
