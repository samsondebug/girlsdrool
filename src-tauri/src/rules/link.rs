//! Transfer, card-payment and refund detection (ARCHITECTURE §6.5, ADR-0019). Detection only
//! proposes heuristic links; the user can unlink or link by hand.

use chrono::Duration;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::dates::{format_civil, now_rfc3339, parse_civil};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::link::{self, Confidence, TransferKind};
use crate::db::repo::{account, txn};
use crate::error::AppResult;
use crate::import::csv::FLAG_NEEDS_REVIEW;
use crate::import::dedup::similarity_bps;

pub const TRANSFER_WINDOW_DAYS: i64 = 3;
pub const REFUND_WINDOW_DAYS: i64 = 90;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LinkReport {
    pub transfers: Vec<(i64, i64, String)>,
    pub refunds: Vec<(i64, i64)>,
    pub refund_candidates: Vec<i64>,
}

/// An unlinked leaf row that could be the other leg of a transfer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    pub txn_id: i64,
    pub account_id: i64,
    pub account_name: String,
    pub posted_date: String,
    pub amount_cents: i64,
    pub payee_norm: String,
    pub days_apart: i64,
}

fn load(conn: &Connection, id: i64) -> AppResult<txn::TxnRecord> {
    txn::get(conn, id)
}

/// Rows on other accounts with the opposite amount within the window, unlinked, unsplit.
pub fn transfer_candidates(conn: &Connection, row: &txn::TxnRecord) -> AppResult<Vec<Candidate>> {
    let posted = parse_civil(&row.posted_date)?;
    let from = format_civil(posted - Duration::days(TRANSFER_WINDOW_DAYS));
    let to = format_civil(posted + Duration::days(TRANSFER_WINDOW_DAYS));
    let opposite = row.amount_cents.checked_neg().unwrap_or(0);
    let mut stmt = conn.prepare(
        "SELECT t.id, t.account_id, a.name, t.posted_date, t.amount_cents, t.payee_norm FROM txn t
         JOIN account a ON a.id = t.account_id
         WHERE t.account_id <> ?1 AND t.amount_cents = ?2 AND t.parent_id IS NULL AND t.transfer_link_id IS NULL
           AND t.refund_link_id IS NULL AND t.posted_date BETWEEN ?3 AND ?4
           AND NOT EXISTS (SELECT 1 FROM txn c WHERE c.parent_id = t.id)
         ORDER BY t.posted_date, t.id",
    )?;
    let rows = stmt
        .query_map(params![row.account_id, opposite, from, to], |r| {
            Ok(Candidate {
                txn_id: r.get(0)?,
                account_id: r.get(1)?,
                account_name: r.get(2)?,
                posted_date: r.get(3)?,
                amount_cents: r.get(4)?,
                payee_norm: r.get(5)?,
                days_apart: 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for mut c in rows {
        c.days_apart = (parse_civil(&c.posted_date)? - posted).num_days().abs();
        out.push(c);
    }
    Ok(out)
}

/// Earlier outflows on the same account with the same magnitude within the refund window.
pub fn refund_candidates(
    conn: &Connection,
    row: &txn::TxnRecord,
    threshold_bps: i64,
) -> AppResult<Vec<(Candidate, i64)>> {
    let posted = parse_civil(&row.posted_date)?;
    let from = format_civil(posted - Duration::days(REFUND_WINDOW_DAYS));
    let opposite = row.amount_cents.checked_neg().unwrap_or(0);
    let mut stmt = conn.prepare(
        "SELECT t.id, t.account_id, a.name, t.posted_date, t.amount_cents, t.payee_norm FROM txn t
         JOIN account a ON a.id = t.account_id
         WHERE t.account_id = ?1 AND t.amount_cents = ?2 AND t.parent_id IS NULL AND t.transfer_link_id IS NULL
           AND t.posted_date BETWEEN ?3 AND ?4 AND t.id <> ?5
         ORDER BY t.posted_date DESC, t.id DESC",
    )?;
    let rows = stmt
        .query_map(
            params![row.account_id, opposite, from, row.posted_date, row.id],
            |r| {
                Ok(Candidate {
                    txn_id: r.get(0)?,
                    account_id: r.get(1)?,
                    account_name: r.get(2)?,
                    posted_date: r.get(3)?,
                    amount_cents: r.get(4)?,
                    payee_norm: r.get(5)?,
                    days_apart: 0,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::new();
    for mut c in rows {
        c.days_apart = (posted - parse_civil(&c.posted_date)?).num_days();
        let sim = similarity_bps(&c.payee_norm, &row.payee_norm);
        if sim >= threshold_bps {
            out.push((c, sim));
        }
    }
    Ok(out)
}

/// Detect links among `ids` (or every unlinked leaf row when `None`). A transfer needs exactly
/// one closest counterpart; a refund needs an exact payee match, otherwise the row is only
/// flagged as a candidate for the person to decide.
pub fn detect(
    conn: &Connection,
    cmd: &CommandRecord,
    ids: Option<&[i64]>,
) -> AppResult<LinkReport> {
    let ids: Vec<i64> = match ids {
        Some(ids) => ids.to_vec(),
        None => {
            let mut stmt = conn.prepare(
                "SELECT id FROM txn WHERE parent_id IS NULL AND transfer_link_id IS NULL AND refund_link_id IS NULL ORDER BY posted_date, id",
            )?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        }
    };
    let threshold: i64 = conn
        .query_row(
            "SELECT value_json FROM setting WHERE key = 'dedup_similarity_bps'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8500);
    let mut report = LinkReport::default();

    for id in &ids {
        let row = load(conn, *id)?;
        if row.parent_id.is_some() || row.transfer_link_id.is_some() || row.refund_link_id.is_some()
        {
            continue;
        }
        if row.user_edited & txn::UE_CATEGORY != 0 {
            continue;
        }
        if row.amount_cents < 0 {
            let candidates = transfer_candidates(conn, &row)?;
            let best_days = candidates.iter().map(|c| c.days_apart).min();
            let Some(best_days) = best_days else {
                continue;
            };
            let closest: Vec<&Candidate> = candidates
                .iter()
                .filter(|c| c.days_apart == best_days)
                .collect();
            if closest.len() != 1 {
                mark(conn, cmd, &row, "transfer_ambiguous")?;
                continue;
            }
            let other = load(conn, closest[0].txn_id)?;
            if other.user_edited & txn::UE_CATEGORY != 0 {
                continue;
            }
            let out_acct = account::get(conn, row.account_id)?;
            let in_acct = account::get(conn, other.account_id)?;
            let kind = TransferKind::infer(&out_acct, &in_acct);
            link::create_transfer(conn, cmd, row.id, other.id, kind, Confidence::Heuristic)?;
            report
                .transfers
                .push((row.id, other.id, kind.as_str().to_string()));
        } else if row.amount_cents > 0 {
            // an inflow may be the in-leg of a transfer whose out-leg was imported earlier
            let inflow_pairs = transfer_candidates(conn, &row)?;
            if !inflow_pairs.is_empty() {
                let best_days = inflow_pairs.iter().map(|c| c.days_apart).min().unwrap_or(0);
                let closest: Vec<&Candidate> = inflow_pairs
                    .iter()
                    .filter(|c| c.days_apart == best_days)
                    .collect();
                if closest.len() == 1 {
                    let other = load(conn, closest[0].txn_id)?;
                    if other.user_edited & txn::UE_CATEGORY == 0 {
                        let out_acct = account::get(conn, other.account_id)?;
                        let in_acct = account::get(conn, row.account_id)?;
                        let kind = TransferKind::infer(&out_acct, &in_acct);
                        link::create_transfer(
                            conn,
                            cmd,
                            other.id,
                            row.id,
                            kind,
                            Confidence::Heuristic,
                        )?;
                        report
                            .transfers
                            .push((other.id, row.id, kind.as_str().to_string()));
                        continue;
                    }
                } else {
                    mark(conn, cmd, &row, "transfer_ambiguous")?;
                    continue;
                }
            }
            let refunds = refund_candidates(conn, &row, threshold)?;
            if refunds.is_empty() {
                continue;
            }
            let exact: Vec<&(Candidate, i64)> = refunds
                .iter()
                .filter(|(c, _)| c.payee_norm == row.payee_norm)
                .collect();
            if exact.len() == 1 {
                let original = exact[0].0.txn_id;
                link::create_refund(conn, cmd, original, row.id, Confidence::Heuristic)?;
                report.refunds.push((original, row.id));
            } else {
                mark(conn, cmd, &row, "refund_candidate")?;
                report.refund_candidates.push(row.id);
            }
        }
    }
    Ok(report)
}

/// Flag a row for review with the reason, without touching its category.
fn mark(conn: &Connection, cmd: &CommandRecord, row: &txn::TxnRecord, code: &str) -> AppResult<()> {
    let mut after = row.clone();
    after.flags |= i64::from(FLAG_NEEDS_REVIEW);
    if after.classification == "unclassified" {
        after.heuristic_code = Some(code.to_string());
    }
    if after != *row {
        after.updated_at = now_rfc3339();
        txn::write_all_columns(conn, &after)?;
        audit::record(
            conn,
            cmd,
            "txn",
            row.id,
            Action::Update,
            Some(&serde_json::to_value(row)?),
            Some(&serde_json::to_value(&after)?),
        )?;
    }
    Ok(())
}
