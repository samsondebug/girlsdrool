//! The plan: income streams, obligations and earmarks (ARCHITECTURE §5.4–§5.5). Receipts and
//! payments are matched heuristically inside every writing transaction and can be set or
//! removed by hand; nothing here moves money.

pub mod earmark;
pub mod income;
pub mod obligation;
pub mod occur;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::dates::CivilDate;
use crate::db::audit::CommandRecord;
use crate::error::AppResult;

/// A receipt or payment posts within `[due − MATCH_BEFORE_DAYS, due + MATCH_AFTER_DAYS]`.
pub const MATCH_BEFORE_DAYS: i64 = 10;
pub const MATCH_AFTER_DAYS: i64 = 5;
/// How far back an unpaid confirmed occurrence still counts as overdue.
pub const OVERDUE_LOOKBACK_DAYS: i64 = 120;
/// How far back matching looks for occurrences without a receipt or payment.
pub const MATCH_LOOKBACK_DAYS: i64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MatchReport {
    pub receipts: usize,
    pub payments: usize,
}

/// Match receipts for active streams and payments for confirmed obligations against the ledger
/// as it is now. Idempotent: a matched occurrence is never matched again.
pub fn match_all(
    conn: &Connection,
    cmd: &CommandRecord,
    today: CivilDate,
) -> AppResult<MatchReport> {
    Ok(MatchReport {
        receipts: income::match_receipts(conn, cmd, today)?,
        payments: obligation::match_payments(conn, cmd, today)?,
    })
}
