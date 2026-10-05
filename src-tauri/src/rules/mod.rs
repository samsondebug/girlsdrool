//! Categorisation (ARCHITECTURE §6.4): rules first, then heuristics, then the review queue.
//! Every automatic decision stores why. Corrections propose a rule; they never create one.

pub mod heuristics;
pub mod link;

use regex::Regex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::dates::now_rfc3339;
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::rule::{self, Rule};
use crate::db::repo::{account, category, txn};
use crate::error::AppResult;
use crate::import::csv::FLAG_NEEDS_REVIEW;

/// Outcomes over the rows considered (`rule_hits + heuristic_hits + unclassified = considered`)
/// plus `changed`, the rows whose stored state moved; a re-run over settled rows changes none.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AutomationReport {
    pub considered: usize,
    pub rule_hits: usize,
    pub heuristic_hits: usize,
    pub unclassified: usize,
    pub changed: usize,
    pub transfers_linked: usize,
    pub refunds_linked: usize,
    pub refund_candidates: usize,
}

impl AutomationReport {
    pub fn summary_text(&self) -> String {
        if self.considered == 0 {
            return String::new();
        }
        let mut parts = vec![format!(
            "Rules categorised {}, heuristics {}, {} left for review.",
            self.rule_hits, self.heuristic_hits, self.unclassified
        )];
        if self.transfers_linked > 0 || self.refunds_linked > 0 {
            parts.push(format!(
                "Linked {} transfer{} and {} refund{}.",
                self.transfers_linked,
                if self.transfers_linked == 1 { "" } else { "s" },
                self.refunds_linked,
                if self.refunds_linked == 1 { "" } else { "s" }
            ));
        }
        if self.refund_candidates > 0 {
            parts.push(format!(
                "{} possible refund{} flagged for review.",
                self.refund_candidates,
                if self.refund_candidates == 1 { "" } else { "s" }
            ));
        }
        parts.join(" ")
    }
}

struct Compiled {
    rule: Rule,
    regex: Option<Regex>,
}

fn compile(rules: Vec<Rule>) -> Vec<Compiled> {
    rules
        .into_iter()
        .filter(|r| r.enabled)
        .map(|rule| {
            let regex = rule
                .match_payee_regex
                .as_deref()
                .and_then(|p| Regex::new(p).ok());
            Compiled { rule, regex }
        })
        .collect()
}

fn matches(c: &Compiled, row: &txn::TxnRecord) -> bool {
    let r = &c.rule;
    if let Some(needle) = &r.match_payee_contains {
        if !row.payee_norm.contains(needle.as_str()) {
            return false;
        }
    }
    if r.match_payee_regex.is_some() {
        match &c.regex {
            Some(re) if re.is_match(&row.payee_norm) => {}
            _ => return false,
        }
    }
    if let Some(needle) = &r.match_memo_contains {
        if !row.memo.to_lowercase().contains(needle.as_str()) {
            return false;
        }
    }
    if let Some(min) = r.match_amount_min_cents {
        if row.amount_cents < min {
            return false;
        }
    }
    if let Some(max) = r.match_amount_max_cents {
        if row.amount_cents > max {
            return false;
        }
    }
    if let Some(aid) = r.match_account_id {
        if row.account_id != aid {
            return false;
        }
    }
    true
}

/// Rows automation may touch: leaf rows that are not linked and whose category the user has
/// not set by hand.
fn eligible(row: &txn::TxnRecord) -> bool {
    row.parent_id.is_none()
        && row.transfer_link_id.is_none()
        && row.refund_link_id.is_none()
        && row.classification != "manual"
        && row.user_edited & txn::UE_CATEGORY == 0
}

/// Apply rules then heuristics to the given rows (or, when `ids` is `None`, to every unlinked
/// leaf row the user has not categorised by hand), then detect transfer and refund links among
/// them. Re-running is idempotent: a row's rule hit is counted once.
pub fn automate(
    conn: &Connection,
    cmd: &CommandRecord,
    ids: Option<&[i64]>,
) -> AppResult<AutomationReport> {
    let ids: Vec<i64> = match ids {
        Some(ids) => ids.to_vec(),
        None => {
            let mut stmt = conn.prepare(
                "SELECT id FROM txn WHERE classification <> 'manual' AND parent_id IS NULL
                   AND transfer_link_id IS NULL AND refund_link_id IS NULL ORDER BY id",
            )?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        }
    };
    let compiled = compile(rule::list(conn)?);
    let mut report = AutomationReport::default();
    for id in &ids {
        let before = txn::get(conn, *id)?;
        if !eligible(&before) {
            continue;
        }
        report.considered += 1;
        let acct = account::get(conn, before.account_id)?;
        let mut after = before.clone();
        if let Some(c) = compiled.iter().find(|c| matches(c, &before)) {
            after.classification = "rule".into();
            after.rule_id = Some(c.rule.id);
            after.heuristic_code = None;
            if c.rule.action_category_id.is_some() {
                after.category_id = c.rule.action_category_id;
            }
            if c.rule.action_venture_id.is_some() {
                after.venture_id = c.rule.action_venture_id;
            }
            after.flags |= c.rule.action_flags_set;
            if !c.rule.action_tag_ids.is_empty() {
                let mut names: Vec<String> = after.tags.clone();
                for tid in &c.rule.action_tag_ids {
                    let name: Option<String> = conn
                        .query_row("SELECT name FROM tag WHERE id = ?1", [tid], |r| r.get(0))
                        .ok();
                    if let Some(n) = name {
                        if !names.contains(&n) {
                            names.push(n);
                        }
                    }
                }
                names.sort();
                after.tags = names;
            }
            after.flags &= !i64::from(FLAG_NEEDS_REVIEW);
            if before.rule_id != Some(c.rule.id) {
                rule::bump_hits(conn, c.rule.id)?;
            }
            report.rule_hits += 1;
        } else {
            let flags = u32::try_from(before.flags).unwrap_or(0);
            match heuristics::evaluate(&before.payee_norm, before.amount_cents, flags, &acct.kind) {
                Some(h) => {
                    after.classification = "heuristic".into();
                    after.heuristic_code = Some(h.code.into());
                    after.rule_id = None;
                    after.flags |= i64::from(h.flags_set);
                    match h.category_code {
                        Some(code) => {
                            after.category_id = Some(category::by_code(conn, code)?.id);
                            after.flags &= !i64::from(FLAG_NEEDS_REVIEW);
                        }
                        None => {
                            after.category_id = None;
                            after.flags |= i64::from(FLAG_NEEDS_REVIEW);
                        }
                    }
                    report.heuristic_hits += 1;
                }
                None => {
                    after.classification = "unclassified".into();
                    after.flags |= i64::from(FLAG_NEEDS_REVIEW);
                    report.unclassified += 1;
                }
            }
        }
        if crate::db::repo::link::awaits_firewall_ack(conn, &after)? {
            after.flags |= i64::from(FLAG_NEEDS_REVIEW);
        }
        if after != before {
            after.updated_at = now_rfc3339();
            txn::write_all_columns(conn, &after)?;
            audit::record(
                conn,
                cmd,
                "txn",
                *id,
                Action::Update,
                Some(&serde_json::to_value(&before)?),
                Some(&serde_json::to_value(&after)?),
            )?;
            report.changed += 1;
        }
    }
    let links = link::detect(conn, cmd, Some(&ids))?;
    report.transfers_linked = links.transfers.len();
    report.refunds_linked = links.refunds.len();
    report.refund_candidates = links.refund_candidates.len();
    Ok(report)
}

/// The rule a correction suggests: this payee → the category the user just chose. Returned to
/// the UI; nothing is created here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleProposal {
    pub name: String,
    pub match_payee_contains: String,
    pub action_category_id: Option<i64>,
    pub action_venture_id: Option<i64>,
    /// How many other rows the proposed rule would match today.
    pub would_match: i64,
}

pub fn propose_rule(conn: &Connection, txn_id: i64) -> AppResult<RuleProposal> {
    let row = txn::get(conn, txn_id)?;
    let needle = row.payee_norm.trim().to_string();
    let would_match: i64 = conn.query_row(
        "SELECT count(*) FROM txn WHERE id <> ?1 AND parent_id IS NULL AND transfer_link_id IS NULL AND instr(payee_norm, ?2) > 0",
        rusqlite::params![txn_id, needle],
        |r| r.get(0),
    )?;
    let name = if needle.is_empty() {
        format!("Row {txn_id}")
    } else {
        let mut chars = needle.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => needle.clone(),
        }
    };
    Ok(RuleProposal {
        name,
        match_payee_contains: needle,
        action_category_id: row.category_id,
        action_venture_id: row.venture_id,
        would_match,
    })
}
