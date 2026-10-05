//! Debts (ARCHITECTURE §5.8, ADR-0024, ADR-0043): linked or standalone, interest in integer cents,
//! minimum rules, the obligations that carry each minimum into the hero and the forecast, and the
//! payment log that moves a standalone balance. Informal loans live in `informal`, the strategy
//! comparison in `strategy`.

pub mod informal;
pub mod strategy;

use chrono::{Datelike, Duration};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::{format_civil, now_rfc3339, parse_civil, CivilDate};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::account::{self, CASH_KINDS};
use crate::error::{AppError, AppResult};
use crate::money::{mul_div_round, Cents};
use crate::plan::{obligation, occur, MATCH_LOOKBACK_DAYS};

pub const KINDS: [&str; 3] = ["credit_card", "loan", "informal"];
pub const INTEREST_METHODS: [&str; 2] = ["monthly_nominal", "actual_365"];
pub const MINIMUM_RULES: [&str; 5] = [
    "fixed",
    "percent_of_balance",
    "interest_plus_percent",
    "full_balance",
    "none",
];
/// Account kinds a linked debt may point at.
const LIABILITY_KINDS: [&str; 2] = ["credit", "loan"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Debt {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub account_id: Option<i64>,
    pub apr_bps: i64,
    pub promo_apr_bps: Option<i64>,
    pub promo_end: Option<String>,
    pub interest_method: String,
    pub minimum_rule: String,
    pub minimum_fixed_cents: i64,
    pub minimum_bps: i64,
    pub minimum_floor_cents: i64,
    pub due_day: Option<i64>,
    pub strategy_participation: bool,
    pub custom_order: Option<i64>,
    pub standalone_opening_cents: Option<i64>,
    pub standalone_opening_date: Option<String>,
    pub active: bool,
    pub payment_account_id: Option<i64>,
    pub match_payee_contains: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebtInput {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub account_id: Option<i64>,
    #[serde(default)]
    pub apr_bps: i64,
    #[serde(default)]
    pub promo_apr_bps: Option<i64>,
    #[serde(default)]
    pub promo_end: Option<String>,
    #[serde(default = "default_method")]
    pub interest_method: String,
    pub minimum_rule: String,
    #[serde(default)]
    pub minimum_fixed_cents: i64,
    #[serde(default)]
    pub minimum_bps: i64,
    #[serde(default)]
    pub minimum_floor_cents: i64,
    #[serde(default)]
    pub due_day: Option<i64>,
    #[serde(default = "default_true")]
    pub strategy_participation: bool,
    #[serde(default)]
    pub custom_order: Option<i64>,
    #[serde(default)]
    pub standalone_opening_cents: Option<i64>,
    #[serde(default)]
    pub standalone_opening_date: Option<String>,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default)]
    pub payment_account_id: Option<i64>,
    #[serde(default)]
    pub match_payee_contains: Option<String>,
}

fn default_method() -> String {
    "monthly_nominal".to_string()
}

fn default_true() -> bool {
    true
}

/// A debt with what the engines derive from it today.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebtView {
    #[serde(flatten)]
    pub debt: Debt,
    pub account_name: Option<String>,
    pub payment_account_name: Option<String>,
    pub owed_cents: i64,
    /// Σ recorded payments (standalone debts; a linked debt's payments are its account's rows).
    pub paid_cents: i64,
    pub effective_apr_bps: i64,
    pub next_period_start: String,
    pub next_period_end: String,
    pub next_interest_cents: i64,
    pub next_minimum_cents: i64,
    pub obligation_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Payment {
    pub id: i64,
    pub debt_id: i64,
    pub paid_date: String,
    pub amount_cents: i64,
    pub txn_id: Option<i64>,
    pub note: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentInput {
    pub paid_date: String,
    pub amount_cents: i64,
    #[serde(default)]
    pub txn_id: Option<i64>,
    #[serde(default)]
    pub note: String,
}

const COLS: &str = "id, name, kind, account_id, apr_bps, promo_apr_bps, promo_end, interest_method, minimum_rule, minimum_fixed_cents, minimum_bps, minimum_floor_cents, due_day, strategy_participation, custom_order, standalone_opening_cents, standalone_opening_date, active, payment_account_id, match_payee_contains, created_at, updated_at";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Debt> {
    Ok(Debt {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        account_id: r.get(3)?,
        apr_bps: r.get(4)?,
        promo_apr_bps: r.get(5)?,
        promo_end: r.get(6)?,
        interest_method: r.get(7)?,
        minimum_rule: r.get(8)?,
        minimum_fixed_cents: r.get(9)?,
        minimum_bps: r.get(10)?,
        minimum_floor_cents: r.get(11)?,
        due_day: r.get(12)?,
        strategy_participation: r.get::<_, i64>(13)? == 1,
        custom_order: r.get(14)?,
        standalone_opening_cents: r.get(15)?,
        standalone_opening_date: r.get(16)?,
        active: r.get::<_, i64>(17)? == 1,
        payment_account_id: r.get(18)?,
        match_payee_contains: r.get(19)?,
        created_at: r.get(20)?,
        updated_at: r.get(21)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Debt>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM debt ORDER BY id"))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Debt> {
    conn.query_row(
        &format!("SELECT {COLS} FROM debt WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound { entity: "debt", id })
}

fn validate(conn: &Connection, input: &DebtInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::validation("name", "a debt needs a name"));
    }
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::validation(
            "kind",
            "must be credit_card, loan or informal",
        ));
    }
    if !INTEREST_METHODS.contains(&input.interest_method.as_str()) {
        return Err(AppError::validation(
            "interest_method",
            "must be monthly_nominal or actual_365",
        ));
    }
    if !MINIMUM_RULES.contains(&input.minimum_rule.as_str()) {
        return Err(AppError::validation(
            "minimum_rule",
            "must be fixed, percent_of_balance, interest_plus_percent, full_balance or none",
        ));
    }
    if input.apr_bps < 0
        || input.promo_apr_bps.is_some_and(|p| p < 0)
        || input.minimum_fixed_cents < 0
        || input.minimum_bps < 0
        || input.minimum_floor_cents < 0
    {
        return Err(AppError::validation(
            "apr_bps",
            "rates and minimums cannot be negative",
        ));
    }
    if input.promo_apr_bps.is_some() != input.promo_end.is_some() {
        return Err(AppError::validation(
            "promo_end",
            "a promo rate needs an end date, and an end date a rate",
        ));
    }
    if let Some(end) = &input.promo_end {
        parse_civil(end)?;
    }
    if let Some(day) = input.due_day {
        if !(1..=31).contains(&day) {
            return Err(AppError::validation("due_day", "must be 1..31"));
        }
    }
    match (input.account_id, input.standalone_opening_cents) {
        (Some(account_id), None) => {
            let acct = account::get(conn, account_id)?;
            if !LIABILITY_KINDS.contains(&acct.kind.as_str()) {
                return Err(AppError::validation(
                    "account_id",
                    "a linked debt points at a credit or loan account",
                ));
            }
            if input.standalone_opening_date.is_some() {
                return Err(AppError::validation(
                    "standalone_opening_date",
                    "a linked debt has no standalone opening",
                ));
            }
        }
        (None, Some(opening)) => {
            if opening < 0 {
                return Err(AppError::validation(
                    "standalone_opening_cents",
                    "cannot be negative",
                ));
            }
            let Some(date) = &input.standalone_opening_date else {
                return Err(AppError::validation(
                    "standalone_opening_date",
                    "a standalone debt names the date its opening balance is owed",
                ));
            };
            parse_civil(date)?;
        }
        _ => {
            return Err(AppError::validation(
                "account_id",
                "a debt is linked to an account or has a standalone opening balance, not both",
            ));
        }
    }
    if let Some(pay) = input.payment_account_id {
        let acct = account::get(conn, pay)?;
        if !CASH_KINDS.contains(&acct.kind.as_str()) {
            return Err(AppError::validation(
                "payment_account_id",
                "payments leave from a cash account",
            ));
        }
    }
    Ok(())
}

pub fn create(conn: &Connection, cmd: &CommandRecord, input: &DebtInput) -> AppResult<Debt> {
    validate(conn, input)?;
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO debt (name, kind, account_id, apr_bps, promo_apr_bps, promo_end, interest_method, minimum_rule, minimum_fixed_cents,
                           minimum_bps, minimum_floor_cents, due_day, strategy_participation, custom_order, standalone_opening_cents,
                           standalone_opening_date, active, payment_account_id, match_payee_contains, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?20)",
        params![
            input.name.trim(),
            input.kind,
            input.account_id,
            input.apr_bps,
            input.promo_apr_bps,
            input.promo_end,
            input.interest_method,
            input.minimum_rule,
            input.minimum_fixed_cents,
            input.minimum_bps,
            input.minimum_floor_cents,
            input.due_day,
            i64::from(input.strategy_participation),
            input.custom_order,
            input.standalone_opening_cents,
            input.standalone_opening_date,
            i64::from(input.active),
            input.payment_account_id,
            input
                .match_payee_contains
                .as_deref()
                .map(|s| s.trim().to_lowercase()),
            now,
        ],
    )?;
    let debt = get(conn, conn.last_insert_rowid())?;
    audit::record(
        conn,
        cmd,
        "debt",
        debt.id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&debt)?),
    )?;
    Ok(debt)
}

pub fn update(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    input: &DebtInput,
) -> AppResult<Debt> {
    let before = get(conn, id)?;
    validate(conn, input)?;
    if before.kind == "informal" && input.kind != "informal" {
        return Err(AppError::validation(
            "kind",
            "an informal loan stays an informal loan",
        ));
    }
    conn.execute(
        "UPDATE debt SET name = ?2, kind = ?3, account_id = ?4, apr_bps = ?5, promo_apr_bps = ?6, promo_end = ?7, interest_method = ?8,
                         minimum_rule = ?9, minimum_fixed_cents = ?10, minimum_bps = ?11, minimum_floor_cents = ?12, due_day = ?13,
                         strategy_participation = ?14, custom_order = ?15, standalone_opening_cents = ?16, standalone_opening_date = ?17,
                         active = ?18, payment_account_id = ?19, match_payee_contains = ?20, updated_at = ?21
         WHERE id = ?1",
        params![
            id,
            input.name.trim(),
            input.kind,
            input.account_id,
            input.apr_bps,
            input.promo_apr_bps,
            input.promo_end,
            input.interest_method,
            input.minimum_rule,
            input.minimum_fixed_cents,
            input.minimum_bps,
            input.minimum_floor_cents,
            input.due_day,
            i64::from(input.strategy_participation),
            input.custom_order,
            input.standalone_opening_cents,
            input.standalone_opening_date,
            i64::from(input.active),
            input.payment_account_id,
            input
                .match_payee_contains
                .as_deref()
                .map(|s| s.trim().to_lowercase()),
            now_rfc3339(),
        ],
    )?;
    let after = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "debt",
        id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(after)
}

// ---- balances -------------------------------------------------------------------------------

/// Σ recorded payments dated on or before `as_of`.
pub fn paid(conn: &Connection, debt_id: i64, as_of: CivilDate) -> AppResult<i64> {
    let sum: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_cents), 0) FROM debt_payment WHERE debt_id = ?1 AND paid_date <= ?2",
        params![debt_id, format_civil(as_of)],
        |r| r.get(0),
    )?;
    Ok(sum)
}

/// What is owed as of `as_of`: a linked debt owes `max(0, −posted balance)` of its account, a
/// standalone one its opening minus recorded payments, floored at zero.
pub fn owed(conn: &Connection, debt: &Debt, as_of: CivilDate) -> AppResult<i64> {
    if let Some(account_id) = debt.account_id {
        let acct = account::get(conn, account_id)?;
        let balance = crate::cash::safe::posted_balance_as_of(conn, &acct, as_of)?;
        return Ok(Cents(balance).checked_neg()?.0.max(0));
    }
    let opening = debt.standalone_opening_cents.unwrap_or(0);
    Ok(Cents(opening)
        .checked_sub(Cents(paid(conn, debt.id, as_of)?))?
        .0
        .max(0))
}

// ---- periods, interest, minimums ------------------------------------------------------------

/// Period `k ≥ 1` is the k-th calendar month after the as-of month: `(first day, last day)`.
pub fn period_bounds(as_of: CivilDate, k: u32) -> AppResult<(CivilDate, CivilDate)> {
    let (mut y, mut m) = (as_of.year(), as_of.month());
    for _ in 0..k {
        if m == 12 {
            y += 1;
            m = 1;
        } else {
            m += 1;
        }
    }
    let start = occur::clamp_day(y, m, 1)?;
    let end = occur::clamp_day(y, m, 31)?;
    Ok((start, end))
}

/// The promo rate while the period starts on or before `promo_end`, else the rate.
pub fn effective_apr(debt: &Debt, period_start: CivilDate) -> i64 {
    match (debt.promo_apr_bps, &debt.promo_end) {
        (Some(promo), Some(end)) if format_civil(period_start).as_str() <= end.as_str() => promo,
        _ => debt.apr_bps,
    }
}

/// Interest for one period on its opening balance, in cents (`mul_div_round`).
pub fn period_interest(
    debt: &Debt,
    opening: i64,
    start: CivilDate,
    end: CivilDate,
) -> AppResult<i64> {
    let apr = effective_apr(debt, start);
    if debt.interest_method == "actual_365" {
        let days = (end - start).num_days() + 1;
        return mul_div_round(
            i128::from(opening),
            i128::from(apr) * i128::from(days),
            3_650_000,
        );
    }
    mul_div_round(i128::from(opening), i128::from(apr), 120_000)
}

/// The period's minimum by rule, never more than opening + interest. `scheduled` is what an
/// informal loan's schedule rows ask for in the period (rule `none`).
pub fn period_minimum(debt: &Debt, opening: i64, interest: i64, scheduled: i64) -> AppResult<i64> {
    let pct = || mul_div_round(i128::from(opening), i128::from(debt.minimum_bps), 10_000);
    let raw = match debt.minimum_rule.as_str() {
        "fixed" => debt.minimum_fixed_cents,
        "percent_of_balance" => debt.minimum_floor_cents.max(pct()?),
        "interest_plus_percent" => {
            Cents(interest)
                .checked_add(Cents(debt.minimum_floor_cents.max(pct()?)))?
                .0
        }
        "full_balance" => Cents(opening).checked_add(Cents(interest))?.0,
        _ => scheduled,
    };
    Ok(raw.min(Cents(opening).checked_add(Cents(interest))?.0))
}

/// Every debt with its balance, next period's interest and minimum, and its minimum obligation.
pub fn views(conn: &Connection, today: CivilDate) -> AppResult<Vec<DebtView>> {
    let (start, end) = period_bounds(today, 1)?;
    let mut out = Vec::new();
    for debt in list(conn)? {
        let owed = owed(conn, &debt, today)?;
        let interest = period_interest(&debt, owed, start, end)?;
        let scheduled = if debt.kind == "informal" {
            informal::scheduled_between(conn, debt.id, today, start, end)?
        } else {
            0
        };
        let minimum = if owed > 0 {
            period_minimum(&debt, owed, interest, scheduled)?
        } else {
            0
        };
        let account_name = match debt.account_id {
            Some(id) => Some(account::get(conn, id)?.name),
            None => None,
        };
        let payment_account_name = match debt.payment_account_id {
            Some(id) => Some(account::get(conn, id)?.name),
            None => None,
        };
        out.push(DebtView {
            obligation_id: minimum_obligation_id(conn, debt.id)?,
            paid_cents: paid(conn, debt.id, today)?,
            effective_apr_bps: effective_apr(&debt, start),
            next_period_start: format_civil(start),
            next_period_end: format_civil(end),
            next_interest_cents: interest,
            next_minimum_cents: minimum,
            owed_cents: owed,
            account_name,
            payment_account_name,
            debt,
        });
    }
    Ok(out)
}

// ---- payments -------------------------------------------------------------------------------

fn payment_from_row(r: &rusqlite::Row) -> rusqlite::Result<Payment> {
    Ok(Payment {
        id: r.get(0)?,
        debt_id: r.get(1)?,
        paid_date: r.get(2)?,
        amount_cents: r.get(3)?,
        txn_id: r.get(4)?,
        note: r.get(5)?,
        created_at: r.get(6)?,
    })
}

pub fn payments(conn: &Connection, debt_id: i64) -> AppResult<Vec<Payment>> {
    let mut stmt = conn.prepare(
        "SELECT id, debt_id, paid_date, amount_cents, txn_id, note, created_at FROM debt_payment WHERE debt_id = ?1 ORDER BY paid_date, id",
    )?;
    let rows = stmt
        .query_map([debt_id], payment_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_payment(conn: &Connection, id: i64) -> AppResult<Payment> {
    conn.query_row(
        "SELECT id, debt_id, paid_date, amount_cents, txn_id, note, created_at FROM debt_payment WHERE id = ?1",
        [id],
        payment_from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "debt_payment",
        id,
    })
}

/// Record a payment against a standalone debt or an informal loan (a linked debt's payments are
/// its account's rows). A row linked here is used once.
pub fn record_payment(
    conn: &Connection,
    cmd: &CommandRecord,
    debt_id: i64,
    input: &PaymentInput,
) -> AppResult<Payment> {
    let debt = get(conn, debt_id)?;
    if debt.account_id.is_some() {
        return Err(AppError::validation(
            "debt_id",
            "a linked debt is paid through its account's rows",
        ));
    }
    if input.amount_cents <= 0 {
        return Err(AppError::validation(
            "amount_cents",
            "a payment is a positive amount",
        ));
    }
    parse_civil(&input.paid_date)?;
    if let Some(txn_id) = input.txn_id {
        let used: i64 = conn.query_row(
            "SELECT COUNT(*) FROM debt_payment WHERE txn_id = ?1",
            [txn_id],
            |r| r.get(0),
        )?;
        if used > 0 {
            return Err(AppError::Conflict(format!(
                "row {txn_id} is already a debt payment"
            )));
        }
    }
    conn.execute(
        "INSERT INTO debt_payment (debt_id, paid_date, amount_cents, txn_id, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            debt_id,
            input.paid_date,
            input.amount_cents,
            input.txn_id,
            input.note.trim(),
            now_rfc3339()
        ],
    )?;
    let payment = get_payment(conn, conn.last_insert_rowid())?;
    audit::record(
        conn,
        cmd,
        "debt_payment",
        payment.id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&payment)?),
    )?;
    Ok(payment)
}

pub fn remove_payment(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let payment = get_payment(conn, id)?;
    conn.execute("DELETE FROM debt_payment WHERE id = ?1", [id])?;
    audit::record(
        conn,
        cmd,
        "debt_payment",
        id,
        Action::Delete,
        Some(&serde_json::to_value(&payment)?),
        None,
    )
}

/// Drop the payment a deleted ledger row backed (called from the ledger).
pub(crate) fn detach_row(conn: &Connection, cmd: &CommandRecord, txn_id: i64) -> AppResult<()> {
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM debt_payment WHERE txn_id = ?1")?
        .query_map([txn_id], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids {
        remove_payment(conn, cmd, id)?;
    }
    Ok(())
}

// ---- minimum obligations and the refresh every write runs -------------------------------------

pub fn minimum_obligation_id(conn: &Connection, debt_id: i64) -> AppResult<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM obligation WHERE debt_id = ?1 AND kind = 'debt_minimum' ORDER BY id LIMIT 1",
            [debt_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Keep one confirmed `debt_minimum` obligation per active debt that owes something and has a
/// minimum rule, a due day and a payment account: expected = the next period's minimum. Retire
/// it when the debt owes nothing or is inactive.
pub fn sync_minimum_obligations(
    conn: &Connection,
    cmd: &CommandRecord,
    today: CivilDate,
) -> AppResult<()> {
    let (start, end) = period_bounds(today, 1)?;
    for debt in list(conn)? {
        let existing = minimum_obligation_id(conn, debt.id)?;
        let owed = owed(conn, &debt, today)?;
        let wants = debt.active
            && owed > 0
            && debt.minimum_rule != "none"
            && debt.due_day.is_some()
            && debt.payment_account_id.is_some();
        if !wants {
            if let Some(id) = existing {
                let ob = obligation::get(conn, id)?;
                if ob.status != "retired" {
                    obligation::set_status(conn, cmd, id, "retired")?;
                }
            }
            continue;
        }
        let interest = period_interest(&debt, owed, start, end)?;
        let expected = period_minimum(&debt, owed, interest, 0)?;
        // occurrences start the day the obligation appears, never before (ADR-0043)
        let anchor = match existing {
            Some(id) => obligation::get(conn, id)?
                .anchor_date
                .unwrap_or_else(|| format_civil(today)),
            None => format_civil(today),
        };
        let input = obligation::ObligationInput {
            name: format!("{} minimum", debt.name),
            kind: "debt_minimum".into(),
            status: "confirmed".into(),
            due_rule: "monthly_day".into(),
            due_day: debt.due_day,
            due_month: None,
            due_weekday: None,
            due_nth: None,
            anchor_date: Some(anchor),
            expected_cents: expected,
            variability_cents: 0,
            source_account_id: debt.payment_account_id.unwrap_or(0),
            autopay: false,
            category_id: None,
            debt_id: Some(debt.id),
            match_payee_contains: debt.match_payee_contains.clone(),
        };
        match existing {
            None => {
                obligation::create(conn, cmd, &input)?;
            }
            Some(id) => {
                let ob = obligation::get(conn, id)?;
                let same = ob.status == "confirmed"
                    && ob.name == input.name
                    && ob.due_day == input.due_day
                    && ob.expected_cents == expected
                    && ob.source_account_id == input.source_account_id
                    && ob.match_payee_contains == input.match_payee_contains;
                if !same {
                    obligation::update(conn, cmd, id, &input)?;
                }
            }
        }
    }
    Ok(())
}

/// A standalone debt's balance moves with the rows its minimum obligation matched: each matched
/// payment row becomes a `debt_payment` once.
fn adopt_obligation_payments(conn: &Connection, cmd: &CommandRecord, debt: &Debt) -> AppResult<()> {
    if debt.account_id.is_some() {
        return Ok(());
    }
    let Some(ob_id) = minimum_obligation_id(conn, debt.id)? else {
        return Ok(());
    };
    let rows: Vec<(i64, String, i64)> = conn
        .prepare(
            "SELECT t.id, t.posted_date, t.amount_cents FROM obligation_payment p JOIN txn t ON t.id = p.txn_id
             WHERE p.obligation_id = ?1 AND NOT EXISTS (SELECT 1 FROM debt_payment d WHERE d.txn_id = t.id)
             ORDER BY t.posted_date, t.id",
        )?
        .query_map([ob_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    for (txn_id, posted, amount) in rows {
        if amount >= 0 {
            continue;
        }
        record_payment(
            conn,
            cmd,
            debt.id,
            &PaymentInput {
                paid_date: posted,
                amount_cents: Cents(amount).checked_neg()?.0,
                txn_id: Some(txn_id),
                note: String::new(),
            },
        )?;
    }
    Ok(())
}

/// What every writing transaction runs after the plan matched: informal repayments found in the
/// ledger, standalone payments adopted from matched minimums, minimum obligations re-derived.
pub fn refresh(conn: &Connection, cmd: &CommandRecord, today: CivilDate) -> AppResult<()> {
    informal::match_repayments(conn, cmd, today)?;
    for debt in list(conn)? {
        adopt_obligation_payments(conn, cmd, &debt)?;
    }
    sync_minimum_obligations(conn, cmd, today)
}

/// The dashboard's two figures and how many informal loans are still open.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Totals {
    pub as_of: String,
    pub total_debt_cents: i64,
    pub informal_remaining_cents: i64,
    pub open_informal: usize,
    pub debts: usize,
}

pub fn totals(conn: &Connection, today: CivilDate) -> AppResult<Totals> {
    let loans = informal::list(conn, today)?;
    Ok(Totals {
        as_of: format_civil(today),
        total_debt_cents: total_owed(conn, today)?,
        informal_remaining_cents: informal::total_remaining(conn, today)?,
        open_informal: loans
            .iter()
            .filter(|l| l.active && l.remaining_cents > 0)
            .count(),
        debts: list(conn)?
            .iter()
            .filter(|d| d.active && d.kind != "informal")
            .count(),
    })
}

/// Debts that are not informal: the dashboard's total.
pub fn total_owed(conn: &Connection, today: CivilDate) -> AppResult<i64> {
    let mut total = Cents::ZERO;
    for debt in list(conn)?
        .into_iter()
        .filter(|d| d.active && d.kind != "informal")
    {
        total = total.checked_add(Cents(owed(conn, &debt, today)?))?;
    }
    Ok(total.0)
}

/// Rows a payment could be recorded against: outflows on the debt's payment account in the
/// lookback window that no payment uses yet.
pub fn candidate_rows(
    conn: &Connection,
    debt: &Debt,
    today: CivilDate,
) -> AppResult<Vec<(i64, String, String, i64)>> {
    let Some(account_id) = debt.payment_account_id else {
        return Ok(Vec::new());
    };
    let from = format_civil(today - Duration::days(MATCH_LOOKBACK_DAYS));
    let rows = conn
        .prepare(
            "SELECT t.id, t.posted_date, t.payee_raw, t.amount_cents FROM txn_leaf t
             WHERE t.account_id = ?1 AND t.amount_cents < 0 AND t.posted_date >= ?2
               AND NOT EXISTS (SELECT 1 FROM debt_payment d WHERE d.txn_id = t.id)
             ORDER BY t.posted_date DESC, t.id DESC",
        )?
        .query_map(params![account_id, from], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
