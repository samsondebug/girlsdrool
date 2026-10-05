//! Transfer and refund links (ARCHITECTURE §6.5). A link joins two ledger rows; the rows carry
//! back-pointers, and the writing function keeps both sides consistent in one transaction.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::now_rfc3339;
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::{account, category, txn};
use crate::error::{AppError, AppResult};
use crate::import::csv::{FLAG_NEEDS_REVIEW, FLAG_PAYMENT_APP_UNKNOWN};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    Internal,
    CardPayment,
    LoanRepayment,
    VentureContribution,
    VentureWithdrawal,
}

impl TransferKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TransferKind::Internal => "internal",
            TransferKind::CardPayment => "card_payment",
            TransferKind::LoanRepayment => "loan_repayment",
            TransferKind::VentureContribution => "venture_contribution",
            TransferKind::VentureWithdrawal => "venture_withdrawal",
        }
    }

    pub fn parse(s: &str) -> AppResult<TransferKind> {
        Ok(match s {
            "internal" => TransferKind::Internal,
            "card_payment" => TransferKind::CardPayment,
            "loan_repayment" => TransferKind::LoanRepayment,
            "venture_contribution" => TransferKind::VentureContribution,
            "venture_withdrawal" => TransferKind::VentureWithdrawal,
            other => {
                return Err(AppError::validation(
                    "kind",
                    format!("{other:?} is not a transfer kind"),
                ))
            }
        })
    }

    /// The category system code each leg receives.
    pub fn category_code(self) -> &'static str {
        match self {
            TransferKind::Internal => "transfer.internal",
            TransferKind::CardPayment => "transfer.card_payment",
            TransferKind::LoanRepayment => "transfer.loan_repayment",
            TransferKind::VentureContribution => "venture.owner_contribution",
            TransferKind::VentureWithdrawal => "venture.withdrawal",
        }
    }

    /// The kind implied by the two accounts (out → in).
    pub fn infer(out: &account::Account, into: &account::Account) -> TransferKind {
        if into.kind == "credit" || out.kind == "credit" {
            TransferKind::CardPayment
        } else if into.kind == "loan" || out.kind == "loan" {
            TransferKind::LoanRepayment
        } else if out.owner == "personal" && into.owner == "venture" {
            TransferKind::VentureContribution
        } else if out.owner == "venture" && into.owner == "personal" {
            TransferKind::VentureWithdrawal
        } else {
            TransferKind::Internal
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    User,
    Heuristic,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::User => "user",
            Confidence::Heuristic => "heuristic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferLink {
    pub id: i64,
    pub out_txn_id: i64,
    pub in_txn_id: i64,
    pub kind: String,
    pub confidence: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefundLink {
    pub id: i64,
    pub original_txn_id: i64,
    pub refund_txn_id: i64,
    pub confidence: String,
    pub created_at: String,
}

pub fn get_transfer(conn: &Connection, id: i64) -> AppResult<TransferLink> {
    conn.query_row(
        "SELECT id, out_txn_id, in_txn_id, kind, confidence, created_at FROM transfer_link WHERE id = ?1",
        [id],
        |r| {
            Ok(TransferLink {
                id: r.get(0)?,
                out_txn_id: r.get(1)?,
                in_txn_id: r.get(2)?,
                kind: r.get(3)?,
                confidence: r.get(4)?,
                created_at: r.get(5)?,
            })
        },
    )
    .optional()?
    .ok_or(AppError::NotFound { entity: "transfer_link", id })
}

pub fn get_refund(conn: &Connection, id: i64) -> AppResult<RefundLink> {
    conn.query_row(
        "SELECT id, original_txn_id, refund_txn_id, confidence, created_at FROM refund_link WHERE id = ?1",
        [id],
        |r| {
            Ok(RefundLink {
                id: r.get(0)?,
                original_txn_id: r.get(1)?,
                refund_txn_id: r.get(2)?,
                confidence: r.get(3)?,
                created_at: r.get(4)?,
            })
        },
    )
    .optional()?
    .ok_or(AppError::NotFound { entity: "refund_link", id })
}

/// Refund links whose original purchase is `txn_id`.
pub fn refunds_of_original(conn: &Connection, txn_id: i64) -> AppResult<Vec<RefundLink>> {
    let mut stmt = conn.prepare(
        "SELECT id, original_txn_id, refund_txn_id, confidence, created_at FROM refund_link WHERE original_txn_id = ?1 ORDER BY id",
    )?;
    let rows = stmt
        .query_map([txn_id], |r| {
            Ok(RefundLink {
                id: r.get(0)?,
                original_txn_id: r.get(1)?,
                refund_txn_id: r.get(2)?,
                confidence: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Whether an outflow on a firewalled account still awaits its acknowledgment (policy 1).
pub fn awaits_firewall_ack(conn: &Connection, row: &txn::TxnRecord) -> AppResult<bool> {
    if row.amount_cents >= 0 {
        return Ok(false);
    }
    let acct = account::get(conn, row.account_id)?;
    if !acct.firewalled {
        return Ok(false);
    }
    let acked: i64 = conn.query_row(
        "SELECT count(*) FROM firewall_ack WHERE txn_id = ?1",
        [row.id],
        |r| r.get(0),
    )?;
    Ok(acked == 0)
}

fn write_leg(
    conn: &Connection,
    cmd: &CommandRecord,
    before: &txn::TxnRecord,
    mut after: txn::TxnRecord,
) -> AppResult<txn::TxnRecord> {
    if !awaits_firewall_ack(conn, &after)? {
        after.flags &= !(i64::from(FLAG_NEEDS_REVIEW) | i64::from(FLAG_PAYMENT_APP_UNKNOWN));
    }
    if after != *before {
        after.updated_at = now_rfc3339();
        txn::write_all_columns(conn, &after)?;
        audit::record(
            conn,
            cmd,
            "txn",
            after.id,
            Action::Update,
            Some(&serde_json::to_value(before)?),
            Some(&serde_json::to_value(&after)?),
        )?;
    }
    Ok(after)
}

/// Link an outflow and an inflow as one transfer. Validates the pair; sets the category of
/// both legs from the kind; clears review flags (except a pending firewall acknowledgment).
pub fn create_transfer(
    conn: &Connection,
    cmd: &CommandRecord,
    out_id: i64,
    in_id: i64,
    kind: TransferKind,
    confidence: Confidence,
) -> AppResult<TransferLink> {
    if out_id == in_id {
        return Err(AppError::validation(
            "in_txn_id",
            "a transfer needs two different rows",
        ));
    }
    let out = txn::get(conn, out_id)?;
    let into = txn::get(conn, in_id)?;
    if out.account_id == into.account_id {
        return Err(AppError::validation(
            "in_txn_id",
            "both rows are on the same account",
        ));
    }
    if out.amount_cents >= 0 || into.amount_cents <= 0 {
        return Err(AppError::validation(
            "out_txn_id",
            "the out leg must be an outflow and the in leg an inflow",
        ));
    }
    if out.amount_cents.checked_neg() != Some(into.amount_cents) {
        return Err(AppError::validation(
            "amount",
            "the two legs must have equal and opposite amounts",
        ));
    }
    if out.transfer_link_id.is_some() || into.transfer_link_id.is_some() {
        return Err(AppError::Conflict(
            "one of the rows is already part of a transfer".into(),
        ));
    }
    if out.refund_link_id.is_some() || into.refund_link_id.is_some() {
        return Err(AppError::Conflict(
            "one of the rows is part of a refund".into(),
        ));
    }
    if !txn::children(conn, out_id)?.is_empty() || !txn::children(conn, in_id)?.is_empty() {
        return Err(AppError::Conflict(
            "a split row cannot be linked; link its parts".into(),
        ));
    }
    conn.execute(
        "INSERT INTO transfer_link (out_txn_id, in_txn_id, kind, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![out_id, in_id, kind.as_str(), confidence.as_str(), now_rfc3339()],
    )?;
    let link_id = conn.last_insert_rowid();
    let cat = category::by_code(conn, kind.category_code())?;
    let classification = match confidence {
        Confidence::User => "manual",
        Confidence::Heuristic => "heuristic",
    };
    let code = format!("{}_pair", kind.as_str());
    for leg in [&out, &into] {
        let mut after = leg.clone();
        after.transfer_link_id = Some(link_id);
        after.category_id = Some(cat.id);
        after.classification = classification.into();
        after.rule_id = None;
        after.heuristic_code = Some(code.clone());
        if confidence == Confidence::User {
            after.user_edited |= txn::UE_CATEGORY;
        }
        write_leg(conn, cmd, leg, after)?;
    }
    let link = get_transfer(conn, link_id)?;
    audit::record(
        conn,
        cmd,
        "transfer_link",
        link_id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&link)?),
    )?;
    Ok(link)
}

/// Unlink: both rows return to unclassified and ask for review.
pub fn remove_transfer(conn: &Connection, cmd: &CommandRecord, link_id: i64) -> AppResult<()> {
    let link = get_transfer(conn, link_id)?;
    for id in [link.out_txn_id, link.in_txn_id] {
        let before = txn::get(conn, id)?;
        let mut after = before.clone();
        after.transfer_link_id = None;
        after.category_id = None;
        after.classification = "unclassified".into();
        after.heuristic_code = None;
        after.rule_id = None;
        after.user_edited &= !txn::UE_CATEGORY;
        after.flags |= i64::from(FLAG_NEEDS_REVIEW);
        after.updated_at = now_rfc3339();
        txn::write_all_columns(conn, &after)?;
        audit::record(
            conn,
            cmd,
            "txn",
            id,
            Action::Update,
            Some(&serde_json::to_value(&before)?),
            Some(&serde_json::to_value(&after)?),
        )?;
    }
    audit::record(
        conn,
        cmd,
        "transfer_link",
        link_id,
        Action::Delete,
        Some(&serde_json::to_value(&link)?),
        None,
    )?;
    conn.execute("DELETE FROM transfer_link WHERE id = ?1", [link_id])?;
    Ok(())
}

/// Link a refund to the purchase it reverses. A refund that nothing has categorised yet takes
/// the original's category; one a rule or the user already categorised keeps that decision and
/// its reason (ADR-0039).
pub fn create_refund(
    conn: &Connection,
    cmd: &CommandRecord,
    original_id: i64,
    refund_id: i64,
    confidence: Confidence,
) -> AppResult<RefundLink> {
    if original_id == refund_id {
        return Err(AppError::validation(
            "refund_txn_id",
            "a refund needs two different rows",
        ));
    }
    let original = txn::get(conn, original_id)?;
    let refund = txn::get(conn, refund_id)?;
    if original.amount_cents >= 0 || refund.amount_cents <= 0 {
        return Err(AppError::validation(
            "refund_txn_id",
            "the original must be an outflow and the refund an inflow",
        ));
    }
    if refund.amount_cents
        > original
            .amount_cents
            .checked_neg()
            .ok_or(AppError::Overflow)?
    {
        return Err(AppError::validation(
            "amount",
            "a refund cannot exceed the original purchase",
        ));
    }
    if refund.refund_link_id.is_some() || refund.transfer_link_id.is_some() {
        return Err(AppError::Conflict(
            "the refund row is already linked".into(),
        ));
    }
    if original.transfer_link_id.is_some() {
        return Err(AppError::Conflict("the original row is a transfer".into()));
    }
    conn.execute(
        "INSERT INTO refund_link (original_txn_id, refund_txn_id, confidence, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![original_id, refund_id, confidence.as_str(), now_rfc3339()],
    )?;
    let link_id = conn.last_insert_rowid();
    let mut after = refund.clone();
    after.refund_link_id = Some(link_id);
    if refund.category_id.is_none() {
        after.category_id = original.category_id;
        after.venture_id = original.venture_id;
        after.classification = match confidence {
            Confidence::User => "manual",
            Confidence::Heuristic => "heuristic",
        }
        .into();
        after.rule_id = None;
        after.heuristic_code = Some("refund_match".into());
        if confidence == Confidence::User {
            after.user_edited |= txn::UE_CATEGORY;
        }
    }
    write_leg(conn, cmd, &refund, after)?;
    let link = get_refund(conn, link_id)?;
    audit::record(
        conn,
        cmd,
        "refund_link",
        link_id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&link)?),
    )?;
    Ok(link)
}

pub fn remove_refund(conn: &Connection, cmd: &CommandRecord, link_id: i64) -> AppResult<()> {
    let link = get_refund(conn, link_id)?;
    let before = txn::get(conn, link.refund_txn_id)?;
    let mut after = before.clone();
    after.refund_link_id = None;
    after.category_id = None;
    after.classification = "unclassified".into();
    after.heuristic_code = None;
    after.user_edited &= !txn::UE_CATEGORY;
    after.flags |= i64::from(FLAG_NEEDS_REVIEW);
    after.updated_at = now_rfc3339();
    txn::write_all_columns(conn, &after)?;
    audit::record(
        conn,
        cmd,
        "txn",
        before.id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    audit::record(
        conn,
        cmd,
        "refund_link",
        link_id,
        Action::Delete,
        Some(&serde_json::to_value(&link)?),
        None,
    )?;
    conn.execute("DELETE FROM refund_link WHERE id = ?1", [link_id])?;
    Ok(())
}

/// Record the in-app acknowledgment of an outflow from a firewalled account (policy 1) and
/// release its review flag.
pub fn acknowledge_firewall(
    conn: &Connection,
    cmd: &CommandRecord,
    txn_id: i64,
    note: &str,
) -> AppResult<txn::TxnRecord> {
    let before = txn::get(conn, txn_id)?;
    let acct = account::get(conn, before.account_id)?;
    if !acct.firewalled {
        return Err(AppError::validation(
            "txn_id",
            format!("{} is not firewalled", acct.name),
        ));
    }
    if before.amount_cents >= 0 {
        return Err(AppError::validation(
            "txn_id",
            "only outflows need an acknowledgment",
        ));
    }
    let existing: i64 = conn.query_row(
        "SELECT count(*) FROM firewall_ack WHERE txn_id = ?1",
        [txn_id],
        |r| r.get(0),
    )?;
    if existing > 0 {
        return Err(AppError::Conflict(
            "this outflow was already acknowledged".into(),
        ));
    }
    conn.execute(
        "INSERT INTO firewall_ack (txn_id, acknowledged_at, note) VALUES (?1, ?2, ?3)",
        params![txn_id, now_rfc3339(), note.trim()],
    )?;
    let ack_id = conn.last_insert_rowid();
    audit::record(
        conn,
        cmd,
        "firewall_ack",
        ack_id,
        Action::Insert,
        None,
        Some(&serde_json::json!({ "txn_id": txn_id, "note": note.trim() })),
    )?;
    let mut after = before.clone();
    after.flags &= !i64::from(FLAG_NEEDS_REVIEW);
    if after != before {
        after.updated_at = now_rfc3339();
        txn::write_all_columns(conn, &after)?;
        audit::record(
            conn,
            cmd,
            "txn",
            txn_id,
            Action::Update,
            Some(&serde_json::to_value(&before)?),
            Some(&serde_json::to_value(&after)?),
        )?;
    }
    Ok(after)
}
