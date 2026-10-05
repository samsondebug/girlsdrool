//! Informal loans (ADR-0024, ADR-0043): a debt of kind `informal` with a counterparty, the
//! promised terms, schedule rows, the repayments found in the ledger (transfers to a liability,
//! never expenses), and a note draft that never leaves the machine.

use chrono::Duration;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{Debt, DebtInput, Payment, PaymentInput};
use crate::dates::{format_civil, parse_civil, CivilDate};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::{category, txn};
use crate::error::{AppError, AppResult};
use crate::import::csv::{FLAG_BORROWING, FLAG_NEEDS_REVIEW, FLAG_PAYMENT_APP_UNKNOWN};
use crate::money::Cents;
use crate::plan::{MATCH_AFTER_DAYS, MATCH_BEFORE_DAYS, MATCH_LOOKBACK_DAYS};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleRow {
    pub id: i64,
    pub debt_id: i64,
    pub due_date: String,
    pub amount_cents: i64,
    /// What is still owed against this row once earlier rows absorbed the repayments.
    pub unpaid_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InformalLoan {
    pub debt_id: i64,
    pub name: String,
    pub counterparty: String,
    pub original_cents: i64,
    pub borrowed_date: String,
    pub promised_terms: String,
    pub promised_date: Option<String>,
    pub proceeds_txn_id: Option<i64>,
    pub note_draft: String,
    pub strategy_participation: bool,
    pub active: bool,
    pub payment_account_id: Option<i64>,
    pub match_payee_contains: Option<String>,
    pub schedule: Vec<ScheduleRow>,
    pub repayments: Vec<Payment>,
    pub repaid_cents: i64,
    pub remaining_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InformalInput {
    pub counterparty: String,
    pub original_cents: i64,
    pub borrowed_date: String,
    #[serde(default)]
    pub promised_terms: String,
    #[serde(default)]
    pub promised_date: Option<String>,
    #[serde(default)]
    pub proceeds_txn_id: Option<i64>,
    #[serde(default)]
    pub payment_account_id: Option<i64>,
    #[serde(default)]
    pub match_payee_contains: Option<String>,
    #[serde(default = "default_true")]
    pub strategy_participation: bool,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

struct Stored {
    counterparty: String,
    original_cents: i64,
    borrowed_date: String,
    promised_terms: String,
    promised_date: Option<String>,
    proceeds_txn_id: Option<i64>,
    note_draft: String,
}

fn stored(conn: &Connection, debt_id: i64) -> AppResult<Stored> {
    conn.query_row(
        "SELECT counterparty, original_cents, borrowed_date, promised_terms, promised_date, proceeds_txn_id, note_draft
         FROM informal_loan WHERE debt_id = ?1",
        [debt_id],
        |r| {
            Ok(Stored {
                counterparty: r.get(0)?,
                original_cents: r.get(1)?,
                borrowed_date: r.get(2)?,
                promised_terms: r.get(3)?,
                promised_date: r.get(4)?,
                proceeds_txn_id: r.get(5)?,
                note_draft: r.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "informal_loan",
        id: debt_id,
    })
}

fn stored_json(s: &Stored, debt_id: i64) -> serde_json::Value {
    serde_json::json!({
        "debt_id": debt_id, "counterparty": s.counterparty, "original_cents": s.original_cents,
        "borrowed_date": s.borrowed_date, "promised_terms": s.promised_terms, "promised_date": s.promised_date,
        "proceeds_txn_id": s.proceeds_txn_id, "note_draft": s.note_draft,
    })
}

/// The loan's schedule rows in due order, each with what is still unpaid against it: repayments
/// are applied to rows in due order.
pub fn schedule(conn: &Connection, debt_id: i64, today: CivilDate) -> AppResult<Vec<ScheduleRow>> {
    let mut paid = super::paid(conn, debt_id, today)?;
    let mut stmt = conn.prepare(
        "SELECT id, debt_id, due_date, amount_cents FROM informal_loan_schedule WHERE debt_id = ?1 ORDER BY due_date, id",
    )?;
    let rows = stmt
        .query_map([debt_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, debt_id, due_date, amount_cents) in rows {
        let absorbed = paid.min(amount_cents).max(0);
        paid = Cents(paid).checked_sub(Cents(absorbed))?.0;
        out.push(ScheduleRow {
            id,
            debt_id,
            due_date,
            amount_cents,
            unpaid_cents: Cents(amount_cents).checked_sub(Cents(absorbed))?.0,
        });
    }
    Ok(out)
}

/// Σ unpaid schedule amounts due in `[start, end]` (the period minimum of rule `none`).
pub(crate) fn scheduled_between(
    conn: &Connection,
    debt_id: i64,
    today: CivilDate,
    start: CivilDate,
    end: CivilDate,
) -> AppResult<i64> {
    let (s, e) = (format_civil(start), format_civil(end));
    let mut total = Cents::ZERO;
    for row in schedule(conn, debt_id, today)? {
        if row.due_date >= s && row.due_date <= e {
            total = total.checked_add(Cents(row.unpaid_cents))?;
        }
    }
    Ok(total.0)
}

fn assemble(conn: &Connection, debt: &Debt, today: CivilDate) -> AppResult<InformalLoan> {
    let s = stored(conn, debt.id)?;
    let repayments = super::payments(conn, debt.id)?;
    let repaid = super::paid(conn, debt.id, today)?;
    Ok(InformalLoan {
        debt_id: debt.id,
        name: debt.name.clone(),
        counterparty: s.counterparty,
        original_cents: s.original_cents,
        borrowed_date: s.borrowed_date,
        promised_terms: s.promised_terms,
        promised_date: s.promised_date,
        proceeds_txn_id: s.proceeds_txn_id,
        note_draft: s.note_draft,
        strategy_participation: debt.strategy_participation,
        active: debt.active,
        payment_account_id: debt.payment_account_id,
        match_payee_contains: debt.match_payee_contains.clone(),
        schedule: schedule(conn, debt.id, today)?,
        repayments,
        repaid_cents: repaid,
        remaining_cents: Cents(s.original_cents).checked_sub(Cents(repaid))?.0.max(0),
    })
}

pub fn list(conn: &Connection, today: CivilDate) -> AppResult<Vec<InformalLoan>> {
    let mut out = Vec::new();
    for debt in super::list(conn)?
        .into_iter()
        .filter(|d| d.kind == "informal")
    {
        out.push(assemble(conn, &debt, today)?);
    }
    Ok(out)
}

pub fn get(conn: &Connection, debt_id: i64, today: CivilDate) -> AppResult<InformalLoan> {
    let debt = super::get(conn, debt_id)?;
    if debt.kind != "informal" {
        return Err(AppError::NotFound {
            entity: "informal_loan",
            id: debt_id,
        });
    }
    assemble(conn, &debt, today)
}

fn validate(conn: &Connection, input: &InformalInput, debt_id: Option<i64>) -> AppResult<()> {
    if input.counterparty.trim().is_empty() {
        return Err(AppError::validation("counterparty", "who lent the money"));
    }
    if input.original_cents <= 0 {
        return Err(AppError::validation(
            "original_cents",
            "a loan is a positive amount",
        ));
    }
    parse_civil(&input.borrowed_date)?;
    if let Some(d) = &input.promised_date {
        parse_civil(d)?;
    }
    if let Some(id) = input.proceeds_txn_id {
        let row = txn::get(conn, id)?;
        if row.amount_cents <= 0 {
            return Err(AppError::validation(
                "proceeds_txn_id",
                "the proceeds row is an inflow",
            ));
        }
        let other: Option<i64> = conn
            .query_row(
                "SELECT debt_id FROM informal_loan WHERE proceeds_txn_id = ?1 AND debt_id <> ?2",
                params![id, debt_id.unwrap_or(0)],
                |r| r.get(0),
            )
            .optional()?;
        if other.is_some() {
            return Err(AppError::Conflict(format!(
                "row {id} is already the proceeds of another loan"
            )));
        }
    }
    Ok(())
}

fn debt_input(input: &InformalInput) -> DebtInput {
    DebtInput {
        name: format!("Loan from {}", input.counterparty.trim()),
        kind: "informal".into(),
        account_id: None,
        apr_bps: 0,
        promo_apr_bps: None,
        promo_end: None,
        interest_method: "monthly_nominal".into(),
        minimum_rule: "none".into(),
        minimum_fixed_cents: 0,
        minimum_bps: 0,
        minimum_floor_cents: 0,
        due_day: None,
        strategy_participation: input.strategy_participation,
        custom_order: None,
        standalone_opening_cents: Some(input.original_cents),
        standalone_opening_date: Some(input.borrowed_date.clone()),
        active: input.active,
        payment_account_id: input.payment_account_id,
        match_payee_contains: input.match_payee_contains.clone(),
    }
}

/// Mark the proceeds row: borrowing, never income, out of the review queue.
fn mark_proceeds(
    conn: &Connection,
    cmd: &CommandRecord,
    txn_id: i64,
    today: CivilDate,
) -> AppResult<()> {
    let row = txn::get(conn, txn_id)?;
    let cleared = i64::from(FLAG_NEEDS_REVIEW) | i64::from(FLAG_PAYMENT_APP_UNKNOWN);
    let flags = (row.flags & !cleared) | i64::from(FLAG_BORROWING);
    let cat = category::by_code(conn, "transfer.borrowing_proceeds")?;
    if row.flags == flags && row.category_id == Some(cat.id) {
        return Ok(());
    }
    txn::apply_user_patch(
        conn,
        cmd,
        txn_id,
        &txn::TxnPatch {
            payee_norm: None,
            memo: None,
            category_id: Some(Some(cat.id)),
            tags: None,
            venture_id: None,
            flags: Some(flags),
            effective_date: None,
            status: None,
        },
        today,
    )?;
    Ok(())
}

pub fn create(
    conn: &Connection,
    cmd: &CommandRecord,
    input: &InformalInput,
    today: CivilDate,
) -> AppResult<InformalLoan> {
    validate(conn, input, None)?;
    let debt = super::create(conn, cmd, &debt_input(input))?;
    conn.execute(
        "INSERT INTO informal_loan (debt_id, counterparty, original_cents, borrowed_date, promised_terms, promised_date, proceeds_txn_id, note_draft)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '')",
        params![
            debt.id,
            input.counterparty.trim(),
            input.original_cents,
            input.borrowed_date,
            input.promised_terms.trim(),
            input.promised_date,
            input.proceeds_txn_id
        ],
    )?;
    let s = stored(conn, debt.id)?;
    audit::record(
        conn,
        cmd,
        "informal_loan",
        debt.id,
        Action::Insert,
        None,
        Some(&stored_json(&s, debt.id)),
    )?;
    if let Some(id) = input.proceeds_txn_id {
        mark_proceeds(conn, cmd, id, today)?;
    }
    assemble(conn, &debt, today)
}

pub fn update(
    conn: &Connection,
    cmd: &CommandRecord,
    debt_id: i64,
    input: &InformalInput,
    today: CivilDate,
) -> AppResult<InformalLoan> {
    let before = stored(conn, debt_id)?;
    validate(conn, input, Some(debt_id))?;
    let debt = super::update(conn, cmd, debt_id, &debt_input(input))?;
    conn.execute(
        "UPDATE informal_loan SET counterparty = ?2, original_cents = ?3, borrowed_date = ?4, promised_terms = ?5, promised_date = ?6,
                                  proceeds_txn_id = ?7 WHERE debt_id = ?1",
        params![
            debt_id,
            input.counterparty.trim(),
            input.original_cents,
            input.borrowed_date,
            input.promised_terms.trim(),
            input.promised_date,
            input.proceeds_txn_id
        ],
    )?;
    let after = stored(conn, debt_id)?;
    audit::record(
        conn,
        cmd,
        "informal_loan",
        debt_id,
        Action::Update,
        Some(&stored_json(&before, debt_id)),
        Some(&stored_json(&after, debt_id)),
    )?;
    if let Some(id) = input.proceeds_txn_id {
        mark_proceeds(conn, cmd, id, today)?;
    }
    assemble(conn, &debt, today)
}

/// The repayment note the person drafts here and sends however they like; it never leaves.
pub fn set_note_draft(
    conn: &Connection,
    cmd: &CommandRecord,
    debt_id: i64,
    note: &str,
) -> AppResult<()> {
    let before = stored(conn, debt_id)?;
    if before.note_draft == note {
        return Ok(());
    }
    conn.execute(
        "UPDATE informal_loan SET note_draft = ?2 WHERE debt_id = ?1",
        params![debt_id, note],
    )?;
    let after = stored(conn, debt_id)?;
    audit::record(
        conn,
        cmd,
        "informal_loan",
        debt_id,
        Action::Update,
        Some(&stored_json(&before, debt_id)),
        Some(&stored_json(&after, debt_id)),
    )
}

pub fn add_schedule_row(
    conn: &Connection,
    cmd: &CommandRecord,
    debt_id: i64,
    due_date: &str,
    amount_cents: i64,
) -> AppResult<i64> {
    stored(conn, debt_id)?;
    parse_civil(due_date)?;
    if amount_cents <= 0 {
        return Err(AppError::validation(
            "amount_cents",
            "a scheduled repayment is a positive amount",
        ));
    }
    conn.execute(
        "INSERT INTO informal_loan_schedule (debt_id, due_date, amount_cents) VALUES (?1, ?2, ?3)",
        params![debt_id, due_date, amount_cents],
    )?;
    let id = conn.last_insert_rowid();
    audit::record(
        conn,
        cmd,
        "informal_loan_schedule",
        id,
        Action::Insert,
        None,
        Some(
            &serde_json::json!({ "id": id, "debt_id": debt_id, "due_date": due_date, "amount_cents": amount_cents }),
        ),
    )?;
    Ok(id)
}

pub fn delete_schedule_row(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let row: Option<(i64, String, i64)> = conn
        .query_row(
            "SELECT debt_id, due_date, amount_cents FROM informal_loan_schedule WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((debt_id, due_date, amount_cents)) = row else {
        return Err(AppError::NotFound {
            entity: "informal_loan_schedule",
            id,
        });
    };
    conn.execute("DELETE FROM informal_loan_schedule WHERE id = ?1", [id])?;
    audit::record(
        conn,
        cmd,
        "informal_loan_schedule",
        id,
        Action::Delete,
        Some(
            &serde_json::json!({ "id": id, "debt_id": debt_id, "due_date": due_date, "amount_cents": amount_cents }),
        ),
        None,
    )
}

/// Find each unpaid schedule row's repayment in the ledger: the closest outflow on the loan's
/// payment account for exactly the unpaid amount, payee containing the match text, posted within
/// `[due − 10, due + 5]`, not yet a payment of any debt.
pub fn match_repayments(
    conn: &Connection,
    cmd: &CommandRecord,
    today: CivilDate,
) -> AppResult<usize> {
    let mut matched = 0;
    let from = today - Duration::days(MATCH_LOOKBACK_DAYS);
    let to = today + Duration::days(MATCH_BEFORE_DAYS);
    for debt in super::list(conn)?
        .into_iter()
        .filter(|d| d.kind == "informal" && d.active)
    {
        let Some(account_id) = debt.payment_account_id else {
            continue;
        };
        let needle = debt.match_payee_contains.clone().unwrap_or_default();
        for row in schedule(conn, debt.id, today)? {
            if row.unpaid_cents <= 0 {
                continue;
            }
            let due = parse_civil(&row.due_date)?;
            if due < from || due > to {
                continue;
            }
            let found: Option<(i64, String)> = conn
                .query_row(
                    "SELECT t.id, t.posted_date FROM txn_leaf t
                     WHERE t.account_id = ?1 AND t.amount_cents = ?2
                       AND t.status = 'posted'
                       AND t.posted_date BETWEEN ?3 AND ?4
                       AND (?5 = '' OR instr(t.payee_norm, ?5) > 0)
                       AND NOT EXISTS (SELECT 1 FROM debt_payment d WHERE d.txn_id = t.id)
                     ORDER BY abs(julianday(t.posted_date) - julianday(?6)), t.posted_date, t.id
                     LIMIT 1",
                    params![
                        account_id,
                        Cents(row.unpaid_cents).checked_neg()?.0,
                        format_civil(due - Duration::days(MATCH_BEFORE_DAYS)),
                        format_civil(due + Duration::days(MATCH_AFTER_DAYS)),
                        needle,
                        row.due_date,
                    ],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((txn_id, posted)) = found {
                super::record_payment(
                    conn,
                    cmd,
                    debt.id,
                    &PaymentInput {
                        paid_date: posted,
                        amount_cents: row.unpaid_cents,
                        txn_id: Some(txn_id),
                        note: String::new(),
                    },
                )?;
                matched += 1;
            }
        }
    }
    Ok(matched)
}

/// Σ remaining over active informal loans: the dashboard's figure.
pub fn total_remaining(conn: &Connection, today: CivilDate) -> AppResult<i64> {
    let mut total = Cents::ZERO;
    for loan in list(conn, today)?.into_iter().filter(|l| l.active) {
        total = total.checked_add(Cents(loan.remaining_cents))?;
    }
    Ok(total.0)
}

/// Unpaid schedule rows of active loans due in `[from, to]`: the forecast's informal outflows.
pub fn unpaid_due_between(
    conn: &Connection,
    today: CivilDate,
    from: CivilDate,
    to: CivilDate,
) -> AppResult<Vec<(i64, String, String, i64)>> {
    let (f, t) = (format_civil(from), format_civil(to));
    let mut out = Vec::new();
    for debt in super::list(conn)?
        .into_iter()
        .filter(|d| d.kind == "informal" && d.active)
    {
        for row in schedule(conn, debt.id, today)? {
            if row.unpaid_cents > 0 && row.due_date >= f && row.due_date <= t {
                out.push((debt.id, debt.name.clone(), row.due_date, row.unpaid_cents));
            }
        }
    }
    Ok(out)
}
