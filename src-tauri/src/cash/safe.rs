//! Safe-to-spend (ARCHITECTURE §5.4, ADR-0022, ADR-0041): the spec's formula, verbatim, with
//! every term's rows. The total is computed from the returned terms, so `safe_terms_sum` holds
//! by construction.

use chrono::Duration;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::cash::recon::{self, TrustReport};
use crate::dates::{format_civil, parse_civil, CivilDate};
use crate::db::repo::account::{self, Account, CASH_KINDS};
use crate::db::settings;
use crate::error::AppResult;
use crate::import::csv::{flag_names, FLAG_BORROWING, FLAG_SECURITIES_SALE};
use crate::money::Cents;
use crate::plan::{earmark, income, obligation, OVERDUE_LOOKBACK_DAYS};

/// The window when no confirmed income stream exists.
pub const NO_INCOME_WINDOW_DAYS: i64 = 30;
/// How far ahead the next confirmed income is looked for.
const NEXT_INCOME_HORIZON_DAYS: i64 = 400;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AvailableAccount {
    pub account_id: i64,
    pub account_name: String,
    pub posted_cents: i64,
    pub pending_in_cents: i64,
    pub pending_out_cents: i64,
    pub pending_row_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AvailableTerm {
    pub cents: i64,
    pub accounts: Vec<AvailableAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EarmarkItem {
    pub earmark_id: i64,
    pub name: String,
    pub kind: String,
    pub funding_account_id: i64,
    pub remaining_cents: i64,
    /// `max(0, remaining)`: a released-past-zero earmark reserves nothing.
    pub counted_cents: i64,
    pub entry_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EarmarkTerm {
    pub cents: i64,
    pub items: Vec<EarmarkItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NextIncome {
    pub date: String,
    pub stream_id: i64,
    pub stream_name: String,
    pub expected_net_cents: i64,
    pub days_away: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObligationItem {
    pub obligation_id: i64,
    pub name: String,
    pub due_date: String,
    pub expected_cents: i64,
    pub earmark_covered_cents: i64,
    pub counted_cents: i64,
    pub overdue: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObligationTerm {
    pub cents: i64,
    pub next_income: Option<NextIncome>,
    /// Occurrences due on or before this date count.
    pub window_end: String,
    /// `no_confirmed_income` when the window is the 30-day fallback (a warning, not untrusted).
    pub window_reason: Option<String>,
    pub items: Vec<ObligationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BufferTerm {
    pub cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Terms {
    pub available: AvailableTerm,
    pub earmarks: EarmarkTerm,
    pub obligations: ObligationTerm,
    pub buffer: BufferTerm,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExcludedAccount {
    pub account_id: i64,
    pub account_name: String,
    pub kind: String,
    pub posted_cents: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlaggedInflow {
    pub txn_id: i64,
    pub account_id: i64,
    pub account_name: String,
    pub posted_date: String,
    pub cents: i64,
    pub flags: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Excluded {
    pub firewalled_accounts: Vec<ExcludedAccount>,
    pub venture_accounts: Vec<ExcludedAccount>,
    /// Pending borrowing / securities-sale inflows: not cash until posted, never counted here.
    pub pending_flagged_inflows: Vec<FlaggedInflow>,
    /// Posted ones are inside `available` (they are cash in the bank) and listed, never income.
    pub posted_flagged_inflows: Vec<FlaggedInflow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafeToSpend {
    pub as_of: String,
    pub safe_cents: i64,
    pub terms: Terms,
    pub excluded: Excluded,
    pub trust: TrustReport,
}

/// Account set A: cash kinds, personal, not firewalled, not archived.
pub fn contributes(acct: &Account) -> bool {
    CASH_KINDS.contains(&acct.kind.as_str())
        && acct.owner == "personal"
        && !acct.firewalled
        && !acct.archived
}

/// `opening + Σ posted leaf rows with posted_date ≤ as_of`.
pub fn posted_balance_as_of(conn: &Connection, acct: &Account, as_of: CivilDate) -> AppResult<i64> {
    let sum: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf WHERE account_id = ?1 AND status = 'posted' AND posted_date <= ?2",
        params![acct.id, format_civil(as_of)],
        |r| r.get(0),
    )?;
    Ok(Cents(acct.opening_balance_cents).checked_add(Cents(sum))?.0)
}

struct Pending {
    inflow: i64,
    outflow: i64,
    ids: Vec<i64>,
    flagged: Vec<FlaggedInflow>,
}

fn pending_rows(conn: &Connection, acct: &Account) -> AppResult<Pending> {
    let mut stmt = conn.prepare(
        "SELECT id, posted_date, amount_cents, flags FROM txn_leaf WHERE account_id = ?1 AND status = 'pending' ORDER BY posted_date, id",
    )?;
    let rows = stmt
        .query_map([acct.id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let excluded_bits = i64::from(FLAG_BORROWING) | i64::from(FLAG_SECURITIES_SALE);
    let mut out = Pending {
        inflow: 0,
        outflow: 0,
        ids: Vec::new(),
        flagged: Vec::new(),
    };
    let mut inflow = Cents::ZERO;
    let mut outflow = Cents::ZERO;
    for (id, posted_date, amount, flags) in rows {
        if amount > 0 && flags & excluded_bits != 0 {
            out.flagged.push(FlaggedInflow {
                txn_id: id,
                account_id: acct.id,
                account_name: acct.name.clone(),
                posted_date,
                cents: amount,
                flags: flag_names(u32::try_from(flags).unwrap_or(0))
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                status: "pending".into(),
            });
            continue;
        }
        if amount > 0 {
            inflow = inflow.checked_add(Cents(amount))?;
        } else {
            outflow = outflow.checked_add(Cents(amount).checked_neg()?)?;
        }
        out.ids.push(id);
    }
    out.inflow = inflow.0;
    out.outflow = outflow.0;
    Ok(out)
}

fn posted_flagged_inflows(
    conn: &Connection,
    acct: &Account,
    as_of: CivilDate,
) -> AppResult<Vec<FlaggedInflow>> {
    let excluded_bits = i64::from(FLAG_BORROWING) | i64::from(FLAG_SECURITIES_SALE);
    let mut stmt = conn.prepare(
        "SELECT id, posted_date, amount_cents, flags FROM txn_leaf
         WHERE account_id = ?1 AND status = 'posted' AND flags <> 0 AND amount_cents > 0 AND (flags & ?2) <> 0 AND posted_date <= ?3
         ORDER BY posted_date, id",
    )?;
    let rows = stmt
        .query_map(params![acct.id, excluded_bits, format_civil(as_of)], |r| {
            Ok(FlaggedInflow {
                txn_id: r.get(0)?,
                account_id: acct.id,
                account_name: acct.name.clone(),
                posted_date: r.get(1)?,
                cents: r.get(2)?,
                flags: flag_names(u32::try_from(r.get::<_, i64>(3)?).unwrap_or(0))
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                status: "posted".into(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The earliest unreceived occurrence of an active confirmed stream on or after `today`.
pub fn next_income(conn: &Connection, today: CivilDate) -> AppResult<Option<NextIncome>> {
    let mut best: Option<NextIncome> = None;
    for stream in income::list(conn)?
        .into_iter()
        .filter(|s| s.active && s.confidence == "confirmed")
    {
        for due in income::occurrences(
            &stream,
            today,
            today + Duration::days(NEXT_INCOME_HORIZON_DAYS),
        )? {
            if income::receipt_for(conn, stream.id, &format_civil(due))?.is_some() {
                continue;
            }
            let candidate = NextIncome {
                date: format_civil(due),
                stream_id: stream.id,
                stream_name: stream.name.clone(),
                expected_net_cents: stream.expected_net_cents,
                days_away: (due - today).num_days(),
            };
            if best.as_ref().is_none_or(|b| candidate.date < b.date) {
                best = Some(candidate);
            }
            break;
        }
    }
    Ok(best)
}

/// Unpaid confirmed occurrences due in `[from, to]` (never before the source account's opening
/// date), each reduced by what a counted earmark linked to it still holds.
pub(crate) fn unpaid_occurrences(
    conn: &Connection,
    today: CivilDate,
    from: CivilDate,
    to: CivilDate,
    cover: &mut std::collections::BTreeMap<i64, i64>,
) -> AppResult<Vec<ObligationItem>> {
    let mut items = Vec::new();
    for ob in obligation::list(conn)?
        .into_iter()
        .filter(|o| o.status == "confirmed")
    {
        let acct = account::get(conn, ob.source_account_id)?;
        let opened = parse_civil(&acct.opening_date)?;
        let start = if opened > from { opened } else { from };
        if start > to {
            continue;
        }
        for due in obligation::occurrences(&ob, start, to)? {
            let due_s = format_civil(due);
            if obligation::payment_for(conn, ob.id, &due_s)?.is_some() {
                continue;
            }
            let left = cover.get(&ob.id).copied().unwrap_or(0);
            let covered = ob.expected_cents.min(left.max(0));
            if covered > 0 {
                cover.insert(ob.id, left - covered);
            }
            items.push(ObligationItem {
                obligation_id: ob.id,
                name: ob.name.clone(),
                due_date: due_s,
                expected_cents: ob.expected_cents,
                earmark_covered_cents: covered,
                counted_cents: Cents(ob.expected_cents).checked_sub(Cents(covered))?.0,
                overdue: due < today,
            });
        }
    }
    items.sort_by(|a, b| a.due_date.cmp(&b.due_date).then(a.name.cmp(&b.name)));
    Ok(items)
}

/// Counted earmarks (active, funded from an A account) with their remaining as of `today`, and
/// the coverage map an obligation draws on.
fn earmark_term(
    conn: &Connection,
    today: CivilDate,
    contributing: &[Account],
) -> AppResult<(EarmarkTerm, std::collections::BTreeMap<i64, i64>)> {
    let mut items = Vec::new();
    let mut cover = std::collections::BTreeMap::new();
    let mut total = Cents::ZERO;
    for em in earmark::list(conn)?.into_iter().filter(|e| e.active) {
        if !contributing.iter().any(|a| a.id == em.funding_account_id) {
            continue;
        }
        let remaining = earmark::remaining(conn, em.id, today)?;
        let counted = remaining.max(0);
        total = total.checked_add(Cents(counted))?;
        if let Some(ob) = em.obligation_id {
            cover.insert(ob, counted);
        }
        items.push(EarmarkItem {
            earmark_id: em.id,
            name: em.name.clone(),
            kind: em.kind.clone(),
            funding_account_id: em.funding_account_id,
            remaining_cents: remaining,
            counted_cents: counted,
            entry_ids: earmark::entry_ids(conn, em.id, today)?,
        });
    }
    Ok((
        EarmarkTerm {
            cents: total.0,
            items,
        },
        cover,
    ))
}

pub fn safe_to_spend(conn: &Connection, today: CivilDate) -> AppResult<SafeToSpend> {
    let cfg = settings::load(conn)?;
    let all = account::list(conn)?;
    let contributing: Vec<Account> = all.iter().filter(|a| contributes(a)).cloned().collect();

    let mut accounts = Vec::new();
    let mut available = Cents::ZERO;
    let mut pending_flagged = Vec::new();
    let mut posted_flagged = Vec::new();
    for acct in &contributing {
        let posted = posted_balance_as_of(conn, acct, today)?;
        let pending = pending_rows(conn, acct)?;
        available = available
            .checked_add(Cents(posted))?
            .checked_add(Cents(pending.inflow))?
            .checked_sub(Cents(pending.outflow))?;
        pending_flagged.extend(pending.flagged);
        posted_flagged.extend(posted_flagged_inflows(conn, acct, today)?);
        accounts.push(AvailableAccount {
            account_id: acct.id,
            account_name: acct.name.clone(),
            posted_cents: posted,
            pending_in_cents: pending.inflow,
            pending_out_cents: pending.outflow,
            pending_row_ids: pending.ids,
        });
    }

    let (earmarks, mut cover) = earmark_term(conn, today, &contributing)?;

    let next = next_income(conn, today)?;
    let (window_end, window_reason) = match &next {
        Some(n) => (parse_civil(&n.date)?, None),
        None => (
            today + Duration::days(NO_INCOME_WINDOW_DAYS),
            Some("no_confirmed_income".to_string()),
        ),
    };
    let items = unpaid_occurrences(
        conn,
        today,
        today - Duration::days(OVERDUE_LOOKBACK_DAYS),
        window_end,
        &mut cover,
    )?;
    let mut obligations_total = Cents::ZERO;
    for item in &items {
        obligations_total = obligations_total.checked_add(Cents(item.counted_cents))?;
    }
    let obligations = ObligationTerm {
        cents: obligations_total.0,
        next_income: next,
        window_end: format_civil(window_end),
        window_reason,
        items,
    };
    let buffer = BufferTerm {
        cents: cfg.timing_buffer_cents,
    };

    let safe = available
        .checked_sub(Cents(earmarks.cents))?
        .checked_sub(Cents(obligations.cents))?
        .checked_sub(Cents(buffer.cents))?;

    let mut firewalled_accounts = Vec::new();
    let mut venture_accounts = Vec::new();
    for acct in all
        .iter()
        .filter(|a| !a.archived && CASH_KINDS.contains(&a.kind.as_str()) || a.firewalled)
    {
        if acct.archived {
            continue;
        }
        let posted = posted_balance_as_of(conn, acct, today)?;
        if acct.firewalled {
            firewalled_accounts.push(ExcludedAccount {
                account_id: acct.id,
                account_name: acct.name.clone(),
                kind: acct.kind.clone(),
                posted_cents: posted,
                reason: "firewalled: not available cash (policy firewall_exclusion)".into(),
            });
        } else if acct.owner == "venture" {
            venture_accounts.push(ExcludedAccount {
                account_id: acct.id,
                account_name: acct.name.clone(),
                kind: acct.kind.clone(),
                posted_cents: posted,
                reason: "venture-owned: capped venture money, not personal cash".into(),
            });
        }
    }

    Ok(SafeToSpend {
        as_of: format_civil(today),
        safe_cents: safe.0,
        terms: Terms {
            available: AvailableTerm {
                cents: available.0,
                accounts,
            },
            earmarks,
            obligations,
            buffer,
        },
        excluded: Excluded {
            firewalled_accounts,
            venture_accounts,
            pending_flagged_inflows: pending_flagged,
            posted_flagged_inflows: posted_flagged,
        },
        trust: recon::trust(conn, today, cfg.recon_stale_after_days)?,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpcomingObligation {
    pub obligation_id: i64,
    pub name: String,
    pub due_date: String,
    pub days_away: i64,
    pub expected_cents: i64,
    pub variability_cents: i64,
    pub earmark_covered_cents: i64,
    pub autopay: bool,
    /// Due before today and still unpaid (within the overdue lookback): listed first, never hidden.
    pub overdue: bool,
    pub source_account_id: i64,
    pub source_account_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Upcoming {
    pub as_of: String,
    pub horizon_days: i64,
    pub next_income: Option<NextIncome>,
    pub obligations: Vec<UpcomingObligation>,
}

/// The next confirmed income and the unpaid confirmed occurrences due within `horizon_days`,
/// overdue ones (unpaid, due within the lookback) first: the panel never hides a bill that is past due.
pub fn upcoming(conn: &Connection, today: CivilDate, horizon_days: i64) -> AppResult<Upcoming> {
    let all = account::list(conn)?;
    let contributing: Vec<Account> = all.iter().filter(|a| contributes(a)).cloned().collect();
    let (_, mut cover) = earmark_term(conn, today, &contributing)?;
    let items = unpaid_occurrences(
        conn,
        today,
        today - Duration::days(OVERDUE_LOOKBACK_DAYS),
        today + Duration::days(horizon_days),
        &mut cover,
    )?;
    let mut obligations = Vec::with_capacity(items.len());
    for item in items {
        let ob = obligation::get(conn, item.obligation_id)?;
        let acct = account::get(conn, ob.source_account_id)?;
        obligations.push(UpcomingObligation {
            obligation_id: ob.id,
            name: ob.name,
            days_away: (parse_civil(&item.due_date)? - today).num_days(),
            due_date: item.due_date,
            expected_cents: ob.expected_cents,
            variability_cents: ob.variability_cents,
            earmark_covered_cents: item.earmark_covered_cents,
            autopay: ob.autopay,
            overdue: item.overdue,
            source_account_id: acct.id,
            source_account_name: acct.name,
        });
    }
    Ok(Upcoming {
        as_of: format_civil(today),
        horizon_days,
        next_income: next_income(conn, today)?,
        obligations,
    })
}
