//! Obligations (bills and debt minimums), their payments, and candidates detected from
//! recurring rows. A candidate is not an obligation until the person confirms it.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::{format_civil, now_rfc3339, parse_civil, CivilDate};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::{account, txn};
use crate::error::{AppError, AppResult};
use crate::plan::occur::{self, DueRule};
use crate::plan::{MATCH_AFTER_DAYS, MATCH_BEFORE_DAYS, MATCH_LOOKBACK_DAYS};

pub const KINDS: [&str; 3] = ["bill", "debt_minimum", "other"];
pub const STATUSES: [&str; 3] = ["candidate", "confirmed", "retired"];
pub const DUE_RULES: [&str; 5] = ["monthly_day", "nth_weekday", "biweekly", "annual", "once"];

/// Candidate detection (ADR-0041): at least this many rows per payee…
pub const DETECT_MIN_ROWS: usize = 3;
/// …with every consecutive gap inside this range of days…
pub const DETECT_GAP_DAYS: (i64, i64) = (25, 36);
/// …and every |amount| within this many basis points of the median.
pub const DETECT_SPREAD_BPS: i64 = 2500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Obligation {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub status: String,
    pub due_rule: String,
    pub due_day: Option<i64>,
    pub due_month: Option<i64>,
    pub due_weekday: Option<i64>,
    pub due_nth: Option<i64>,
    pub anchor_date: Option<String>,
    pub expected_cents: i64,
    pub variability_cents: i64,
    pub source_account_id: i64,
    pub autopay: bool,
    pub category_id: Option<i64>,
    pub debt_id: Option<i64>,
    pub match_payee_contains: Option<String>,
    pub detected_from_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObligationInput {
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_status")]
    pub status: String,
    pub due_rule: String,
    #[serde(default)]
    pub due_day: Option<i64>,
    #[serde(default)]
    pub due_month: Option<i64>,
    #[serde(default)]
    pub due_weekday: Option<i64>,
    #[serde(default)]
    pub due_nth: Option<i64>,
    #[serde(default)]
    pub anchor_date: Option<String>,
    pub expected_cents: i64,
    #[serde(default)]
    pub variability_cents: i64,
    pub source_account_id: i64,
    #[serde(default)]
    pub autopay: bool,
    #[serde(default)]
    pub category_id: Option<i64>,
    #[serde(default)]
    pub debt_id: Option<i64>,
    #[serde(default)]
    pub match_payee_contains: Option<String>,
}

fn default_kind() -> String {
    "bill".to_string()
}

fn default_status() -> String {
    "confirmed".to_string()
}

const COLS: &str = "id, name, kind, status, due_rule, due_day, due_month, due_weekday, due_nth, anchor_date, expected_cents, variability_cents, source_account_id, autopay, category_id, debt_id, match_payee_contains, detected_from_json, created_at, updated_at";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Obligation> {
    Ok(Obligation {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        status: r.get(3)?,
        due_rule: r.get(4)?,
        due_day: r.get(5)?,
        due_month: r.get(6)?,
        due_weekday: r.get(7)?,
        due_nth: r.get(8)?,
        anchor_date: r.get(9)?,
        expected_cents: r.get(10)?,
        variability_cents: r.get(11)?,
        source_account_id: r.get(12)?,
        autopay: r.get::<_, i64>(13)? != 0,
        category_id: r.get(14)?,
        debt_id: r.get(15)?,
        match_payee_contains: r.get(16)?,
        detected_from_json: r.get(17)?,
        created_at: r.get(18)?,
        updated_at: r.get(19)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Obligation>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM obligation
         ORDER BY CASE status WHEN 'confirmed' THEN 0 WHEN 'candidate' THEN 1 ELSE 2 END, name"
    ))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Obligation> {
    conn.query_row(
        &format!("SELECT {COLS} FROM obligation WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "obligation",
        id,
    })
}

fn validate(conn: &Connection, input: &ObligationInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::validation("name", "an obligation needs a name"));
    }
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::validation(
            "kind",
            "must be bill, debt_minimum or other",
        ));
    }
    if !STATUSES.contains(&input.status.as_str()) {
        return Err(AppError::validation(
            "status",
            "must be candidate, confirmed or retired",
        ));
    }
    if !DUE_RULES.contains(&input.due_rule.as_str()) {
        return Err(AppError::validation(
            "due_rule",
            format!("must be one of {}", DUE_RULES.join(", ")),
        ));
    }
    match input.due_rule.as_str() {
        "monthly_day" => {
            if !matches!(input.due_day, Some(1..=31)) {
                return Err(AppError::validation(
                    "due_day",
                    "a monthly rule needs a day 1–31",
                ));
            }
        }
        "annual" => {
            if !matches!(input.due_day, Some(1..=31)) || !matches!(input.due_month, Some(1..=12)) {
                return Err(AppError::validation(
                    "due_month",
                    "an annual rule needs a month and a day",
                ));
            }
        }
        "nth_weekday" => {
            if !matches!(input.due_weekday, Some(0..=6)) || !matches!(input.due_nth, Some(1..=5)) {
                return Err(AppError::validation(
                    "due_nth",
                    "an nth-weekday rule needs a weekday (0 = Monday) and an nth (5 = last)",
                ));
            }
        }
        _ => match &input.anchor_date {
            Some(d) => {
                parse_civil(d)?;
            }
            None => {
                return Err(AppError::validation(
                    "anchor_date",
                    "this rule needs a date",
                ))
            }
        },
    }
    if input.expected_cents < 0 {
        return Err(AppError::validation("expected_cents", "cannot be negative"));
    }
    if input.variability_cents < 0 {
        return Err(AppError::validation(
            "variability_cents",
            "cannot be negative",
        ));
    }
    account::get(conn, input.source_account_id)?;
    Ok(())
}

fn needle(input: &ObligationInput) -> Option<String> {
    input
        .match_payee_contains
        .as_ref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
}

pub fn create(
    conn: &Connection,
    cmd: &CommandRecord,
    input: &ObligationInput,
) -> AppResult<Obligation> {
    create_with_origin(conn, cmd, input, None)
}

fn create_with_origin(
    conn: &Connection,
    cmd: &CommandRecord,
    input: &ObligationInput,
    detected_from_json: Option<&str>,
) -> AppResult<Obligation> {
    validate(conn, input)?;
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO obligation (name, kind, status, due_rule, due_day, due_month, due_weekday, due_nth, anchor_date, expected_cents,
           variability_cents, source_account_id, autopay, category_id, debt_id, match_payee_contains, detected_from_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18)",
        params![
            input.name.trim(),
            input.kind,
            input.status,
            input.due_rule,
            input.due_day,
            input.due_month,
            input.due_weekday,
            input.due_nth,
            input.anchor_date,
            input.expected_cents,
            input.variability_cents,
            input.source_account_id,
            i64::from(input.autopay),
            input.category_id,
            input.debt_id,
            needle(input),
            detected_from_json,
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    let created = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "obligation",
        id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&created)?),
    )?;
    Ok(created)
}

pub fn update(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    input: &ObligationInput,
) -> AppResult<Obligation> {
    validate(conn, input)?;
    let before = get(conn, id)?;
    conn.execute(
        "UPDATE obligation SET name = ?2, kind = ?3, status = ?4, due_rule = ?5, due_day = ?6, due_month = ?7, due_weekday = ?8,
           due_nth = ?9, anchor_date = ?10, expected_cents = ?11, variability_cents = ?12, source_account_id = ?13, autopay = ?14,
           category_id = ?15, debt_id = ?16, match_payee_contains = ?17, updated_at = ?18 WHERE id = ?1",
        params![
            id,
            input.name.trim(),
            input.kind,
            input.status,
            input.due_rule,
            input.due_day,
            input.due_month,
            input.due_weekday,
            input.due_nth,
            input.anchor_date,
            input.expected_cents,
            input.variability_cents,
            input.source_account_id,
            i64::from(input.autopay),
            input.category_id,
            input.debt_id,
            needle(input),
            now_rfc3339()
        ],
    )?;
    let after = get(conn, id)?;
    if after != before {
        audit::record(
            conn,
            cmd,
            "obligation",
            id,
            Action::Update,
            Some(&serde_json::to_value(&before)?),
            Some(&serde_json::to_value(&after)?),
        )?;
    }
    Ok(after)
}

/// Confirm a candidate, retire an obligation, or bring a retired one back as confirmed.
pub fn set_status(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    status: &str,
) -> AppResult<Obligation> {
    if !STATUSES.contains(&status) {
        return Err(AppError::validation(
            "status",
            "must be candidate, confirmed or retired",
        ));
    }
    let before = get(conn, id)?;
    conn.execute(
        "UPDATE obligation SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status, now_rfc3339()],
    )?;
    let after = get(conn, id)?;
    if after != before {
        audit::record(
            conn,
            cmd,
            "obligation",
            id,
            Action::Update,
            Some(&serde_json::to_value(&before)?),
            Some(&serde_json::to_value(&after)?),
        )?;
    }
    Ok(after)
}

/// Delete a candidate that was never confirmed; confirmed obligations are retired instead.
pub fn delete_candidate(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let ob = get(conn, id)?;
    if ob.status != "candidate" {
        return Err(AppError::validation(
            "id",
            "only a candidate can be deleted; retire a confirmed obligation",
        ));
    }
    audit::record(
        conn,
        cmd,
        "obligation",
        id,
        Action::Delete,
        Some(&serde_json::to_value(&ob)?),
        None,
    )?;
    conn.execute(
        "DELETE FROM obligation_payment WHERE obligation_id = ?1",
        [id],
    )?;
    conn.execute("DELETE FROM obligation WHERE id = ?1", [id])?;
    Ok(())
}

pub fn occurrences(ob: &Obligation, from: CivilDate, to: CivilDate) -> AppResult<Vec<CivilDate>> {
    let anchor = match &ob.anchor_date {
        Some(d) => Some(parse_civil(d)?),
        None => None,
    };
    occur::obligation_occurrences(
        &DueRule {
            rule: &ob.due_rule,
            due_day: ob.due_day,
            due_month: ob.due_month,
            due_weekday: ob.due_weekday,
            due_nth: ob.due_nth,
            anchor,
        },
        from,
        to,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Payment {
    pub obligation_id: i64,
    pub due_date: String,
    pub txn_id: i64,
    pub matched_by: String,
}

fn payment_from_row(r: &rusqlite::Row) -> rusqlite::Result<Payment> {
    Ok(Payment {
        obligation_id: r.get(0)?,
        due_date: r.get(1)?,
        txn_id: r.get(2)?,
        matched_by: r.get(3)?,
    })
}

pub fn payments(conn: &Connection, obligation_id: i64) -> AppResult<Vec<Payment>> {
    let mut stmt = conn.prepare(
        "SELECT obligation_id, due_date, txn_id, matched_by FROM obligation_payment WHERE obligation_id = ?1 ORDER BY due_date",
    )?;
    let rows = stmt
        .query_map([obligation_id], payment_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn all_payments(conn: &Connection) -> AppResult<Vec<Payment>> {
    let mut stmt = conn.prepare(
        "SELECT obligation_id, due_date, txn_id, matched_by FROM obligation_payment ORDER BY obligation_id, due_date",
    )?;
    let rows = stmt
        .query_map([], payment_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn payment_for(
    conn: &Connection,
    obligation_id: i64,
    due_date: &str,
) -> AppResult<Option<Payment>> {
    Ok(conn
        .query_row(
            "SELECT obligation_id, due_date, txn_id, matched_by FROM obligation_payment WHERE obligation_id = ?1 AND due_date = ?2",
            params![obligation_id, due_date],
            payment_from_row,
        )
        .optional()?)
}

pub fn record_payment(
    conn: &Connection,
    cmd: &CommandRecord,
    obligation_id: i64,
    due_date: &str,
    txn_id: i64,
    matched_by: &str,
) -> AppResult<Payment> {
    get(conn, obligation_id)?;
    parse_civil(due_date)?;
    let row = txn::get(conn, txn_id)?;
    if row.amount_cents >= 0 {
        return Err(AppError::validation("txn_id", "a payment is an outflow"));
    }
    if payment_for(conn, obligation_id, due_date)?.is_some() {
        return Err(AppError::Conflict(format!(
            "the occurrence due {due_date} is already paid"
        )));
    }
    let used: i64 = conn.query_row(
        "SELECT count(*) FROM obligation_payment WHERE txn_id = ?1",
        [txn_id],
        |r| r.get(0),
    )?;
    if used > 0 {
        return Err(AppError::Conflict(format!(
            "row {txn_id} already pays an obligation"
        )));
    }
    conn.execute(
        "INSERT INTO obligation_payment (obligation_id, due_date, txn_id, matched_by) VALUES (?1, ?2, ?3, ?4)",
        params![obligation_id, due_date, txn_id, matched_by],
    )?;
    let payment = Payment {
        obligation_id,
        due_date: due_date.to_string(),
        txn_id,
        matched_by: matched_by.to_string(),
    };
    audit::record(
        conn,
        cmd,
        "obligation_payment",
        txn_id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&payment)?),
    )?;
    Ok(payment)
}

pub fn remove_payment(
    conn: &Connection,
    cmd: &CommandRecord,
    obligation_id: i64,
    due_date: &str,
) -> AppResult<()> {
    let Some(payment) = payment_for(conn, obligation_id, due_date)? else {
        return Err(AppError::validation(
            "due_date",
            format!("no payment is recorded for {due_date}"),
        ));
    };
    audit::record(
        conn,
        cmd,
        "obligation_payment",
        payment.txn_id,
        Action::Delete,
        Some(&serde_json::to_value(&payment)?),
        None,
    )?;
    conn.execute(
        "DELETE FROM obligation_payment WHERE obligation_id = ?1 AND due_date = ?2",
        params![obligation_id, due_date],
    )?;
    Ok(())
}

fn candidate_payment(conn: &Connection, ob: &Obligation, due: CivilDate) -> AppResult<Option<i64>> {
    let from = format_civil(due - Duration::days(MATCH_BEFORE_DAYS));
    let to = format_civil(due + Duration::days(MATCH_AFTER_DAYS));
    let low = ob.expected_cents.saturating_sub(ob.variability_cents);
    let high = ob.expected_cents.saturating_add(ob.variability_cents);
    let needle = ob.match_payee_contains.clone().unwrap_or_default();
    let id: Option<i64> = conn
        .query_row(
            "SELECT t.id FROM txn_leaf t
             WHERE t.account_id = ?1 AND t.amount_cents < 0 AND -t.amount_cents BETWEEN ?2 AND ?3
               AND t.status = 'posted'
               AND t.posted_date BETWEEN ?4 AND ?5
               AND (?6 = '' OR instr(t.payee_norm, ?6) > 0)
               AND NOT EXISTS (SELECT 1 FROM obligation_payment p WHERE p.txn_id = t.id)
             ORDER BY abs(julianday(t.posted_date) - julianday(?7)), t.posted_date, t.id
             LIMIT 1",
            params![
                ob.source_account_id,
                low,
                high,
                from,
                to,
                needle,
                format_civil(due)
            ],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id)
}

/// Match payments for every confirmed obligation's occurrences in the lookback window.
pub fn match_payments(
    conn: &Connection,
    cmd: &CommandRecord,
    today: CivilDate,
) -> AppResult<usize> {
    let mut matched = 0;
    for ob in list(conn)?.into_iter().filter(|o| o.status == "confirmed") {
        let from = today - Duration::days(MATCH_LOOKBACK_DAYS);
        let to = today + Duration::days(MATCH_BEFORE_DAYS);
        for due in occurrences(&ob, from, to)? {
            let due_s = format_civil(due);
            if payment_for(conn, ob.id, &due_s)?.is_some() {
                continue;
            }
            if let Some(txn_id) = candidate_payment(conn, &ob, due)? {
                record_payment(conn, cmd, ob.id, &due_s, txn_id, "heuristic")?;
                matched += 1;
            }
        }
    }
    Ok(matched)
}

/// What detection saw: the rows behind a candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DetectedFrom {
    pub txn_ids: Vec<i64>,
    pub posted_dates: Vec<String>,
    pub amounts_cents: Vec<i64>,
}

struct Seen {
    id: i64,
    posted: CivilDate,
    amount: i64,
    category_id: Option<i64>,
}

/// Create a candidate obligation for every recurring payee the plan does not already cover:
/// at least `DETECT_MIN_ROWS` posted, unlinked outflows on one account with the same
/// `payee_norm`, category root not income/transfer, every consecutive gap inside
/// `DETECT_GAP_DAYS`, every |amount| within `DETECT_SPREAD_BPS` of the median. Due day is the
/// median day of month, expected the median amount, variability the largest deviation.
pub fn detect_candidates(conn: &Connection, cmd: &CommandRecord) -> AppResult<Vec<Obligation>> {
    let existing = list(conn)?;
    let mut stmt = conn.prepare(
        "SELECT t.id, t.account_id, t.payee_norm, t.posted_date, t.amount_cents, t.category_id FROM txn_leaf t
         LEFT JOIN category c ON c.id = t.category_id
         WHERE t.amount_cents < 0 AND t.status = 'posted' AND t.transfer_link_id IS NULL
           AND (c.id IS NULL OR c.root_kind NOT IN ('income', 'transfer'))
         ORDER BY t.account_id, t.payee_norm, t.posted_date, t.id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut groups: BTreeMap<(i64, String), Vec<Seen>> = BTreeMap::new();
    for (id, account_id, payee_norm, posted, amount, category_id) in rows {
        groups
            .entry((account_id, payee_norm))
            .or_default()
            .push(Seen {
                id,
                posted: parse_civil(&posted)?,
                amount: amount.checked_neg().ok_or(AppError::Overflow)?,
                category_id,
            });
    }
    let mut created = Vec::new();
    for ((account_id, payee_norm), seen) in groups {
        if seen.len() < DETECT_MIN_ROWS || payee_norm.trim().is_empty() {
            continue;
        }
        let gaps_ok = seen.windows(2).all(|w| {
            let gap = (w[1].posted - w[0].posted).num_days();
            gap >= DETECT_GAP_DAYS.0 && gap <= DETECT_GAP_DAYS.1
        });
        if !gaps_ok {
            continue;
        }
        let mut amounts: Vec<i64> = seen.iter().map(|s| s.amount).collect();
        amounts.sort_unstable();
        let median = amounts[amounts.len() / 2];
        let spread_ok = amounts.iter().all(|a| {
            (a - median).abs().saturating_mul(10_000) <= median.saturating_mul(DETECT_SPREAD_BPS)
        });
        if !spread_ok {
            continue;
        }
        let covered = existing.iter().any(|o| {
            o.source_account_id == account_id
                && (o.name.to_lowercase() == payee_norm
                    || o.match_payee_contains
                        .as_deref()
                        .is_some_and(|n| payee_norm.contains(n)))
        });
        if covered {
            continue;
        }
        let mut days: Vec<i64> = seen.iter().map(|s| i64::from(s.posted.day())).collect();
        days.sort_unstable();
        let due_day = days[days.len() / 2];
        let variability = amounts
            .iter()
            .map(|a| (a - median).abs())
            .max()
            .unwrap_or(0);
        let mut categories: BTreeMap<i64, usize> = BTreeMap::new();
        for s in &seen {
            if let Some(c) = s.category_id {
                *categories.entry(c).or_default() += 1;
            }
        }
        let category_id = categories
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map(|(c, _)| c);
        let origin = DetectedFrom {
            txn_ids: seen.iter().map(|s| s.id).collect(),
            posted_dates: seen.iter().map(|s| format_civil(s.posted)).collect(),
            amounts_cents: seen.iter().map(|s| s.amount).collect(),
        };
        let input = ObligationInput {
            name: title_case(&payee_norm),
            kind: "bill".into(),
            status: "candidate".into(),
            due_rule: "monthly_day".into(),
            due_day: Some(due_day),
            due_month: None,
            due_weekday: None,
            due_nth: None,
            anchor_date: None,
            expected_cents: median,
            variability_cents: variability,
            source_account_id: account_id,
            autopay: false,
            category_id,
            debt_id: None,
            match_payee_contains: Some(payee_norm.clone()),
        };
        created.push(create_with_origin(
            conn,
            cmd,
            &input,
            Some(&serde_json::to_string(&origin)?),
        )?);
    }
    Ok(created)
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
