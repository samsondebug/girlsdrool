//! Row identity and duplicate detection (ADR-0017, ADR-0037).

use sha2::{Digest, Sha256};

use crate::dates::{format_civil, CivilDate};
use crate::import::csv::RowStatus;

/// Stable identity of an observed statement row within an account.
pub fn source_row_hash(
    account_id: i64,
    posted_date: CivilDate,
    amount_cents: i64,
    payee_raw: &str,
    memo: &str,
    external_id: Option<&str>,
) -> String {
    let mut h = Sha256::new();
    h.update(account_id.to_string().as_bytes());
    h.update(b"\x1f");
    h.update(format_civil(posted_date).as_bytes());
    h.update(b"\x1f");
    h.update(amount_cents.to_string().as_bytes());
    h.update(b"\x1f");
    h.update(payee_raw.trim().as_bytes());
    h.update(b"\x1f");
    h.update(memo.trim().as_bytes());
    h.update(b"\x1f");
    h.update(external_id.unwrap_or("").as_bytes());
    hex::encode(h.finalize())
}

/// Jaro–Winkler similarity of two normalised payees, in basis points (0..=10000).
pub fn similarity_bps(a: &str, b: &str) -> i64 {
    if a.is_empty() && b.is_empty() {
        return 10_000;
    }
    let jw = strsim::jaro_winkler(a, b);
    // strsim returns an f64 by API; it is a similarity score, not money, and is reduced to an
    // integer here and never used in a monetary computation.
    (jw * 10_000.0).round() as i64
}

/// Days between two civil dates, absolute.
pub fn days_apart(a: CivilDate, b: CivilDate) -> i64 {
    (a - b).num_days().abs()
}

pub const FUZZY_WINDOW_DAYS: i64 = 3;

/// A ledger row that could be the same economic event as an incoming row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub txn_id: i64,
    pub status: RowStatus,
    pub payee_norm: String,
    pub external_id: Option<String>,
    pub posted_date: CivilDate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Already in the ledger (hash or external id): nothing to do.
    Skip {
        txn_id: i64,
        matched_by: &'static str,
    },
    /// The incoming row is a better observation of this ledger row.
    Update {
        txn_id: i64,
        similarity_bps: i64,
    },
    /// The incoming row is an older (pending) observation of a row already posted.
    SkipOlder {
        txn_id: i64,
        similarity_bps: i64,
    },
    /// Looks like a duplicate but neither side is clearly better: hold for review.
    Quarantine {
        txn_id: i64,
        similarity_bps: i64,
    },
    Insert,
}

/// Decide what to do with an incoming row given its exact match (if any) and the fuzzy
/// candidates within the ±3-day window with the same amount on the same account.
pub fn decide(
    exact: Option<(i64, &'static str)>,
    incoming_status: RowStatus,
    incoming_payee_norm: &str,
    incoming_external_id: Option<&str>,
    candidates: &[Candidate],
    threshold_bps: i64,
) -> Decision {
    if let Some((txn_id, matched_by)) = exact {
        return Decision::Skip { txn_id, matched_by };
    }
    let best = candidates
        .iter()
        .map(|c| (c, similarity_bps(&c.payee_norm, incoming_payee_norm)))
        .filter(|(_, bps)| *bps >= threshold_bps)
        .max_by_key(|(_, bps)| *bps);
    let Some((candidate, bps)) = best else {
        return Decision::Insert;
    };
    let posts_a_pending =
        candidate.status == RowStatus::Pending && incoming_status == RowStatus::Posted;
    let gains_external_id = candidate.external_id.is_none() && incoming_external_id.is_some();
    if posts_a_pending || gains_external_id {
        Decision::Update {
            txn_id: candidate.txn_id,
            similarity_bps: bps,
        }
    } else if candidate.status == RowStatus::Posted && incoming_status == RowStatus::Pending {
        Decision::SkipOlder {
            txn_id: candidate.txn_id,
            similarity_bps: bps,
        }
    } else {
        Decision::Quarantine {
            txn_id: candidate.txn_id,
            similarity_bps: bps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(s: &str) -> CivilDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// Hand values from fixtures/EXPECTED.md ("Dedup outcomes").
    #[test]
    fn similarity_matches_the_fixture_hand_values() {
        assert_eq!(similarity_bps("jewel osco", "jewel osco chicago"), 9111);
        assert_eq!(similarity_bps("amazon com", "amazon com"), 10_000);
        assert_eq!(similarity_bps("", ""), 10_000);
        assert!(similarity_bps("comed electric", "peoples gas") < 8500);
    }

    #[test]
    fn hash_is_stable_and_sensitive_to_every_identity_field() {
        let base = source_row_hash(1, d("2026-07-01"), -240_000, " RENT ", "", None);
        assert_eq!(
            base,
            source_row_hash(1, d("2026-07-01"), -240_000, "RENT", "", None)
        );
        assert_ne!(
            base,
            source_row_hash(2, d("2026-07-01"), -240_000, "RENT", "", None)
        );
        assert_ne!(
            base,
            source_row_hash(1, d("2026-07-02"), -240_000, "RENT", "", None)
        );
        assert_ne!(
            base,
            source_row_hash(1, d("2026-07-01"), -240_001, "RENT", "", None)
        );
        assert_ne!(
            base,
            source_row_hash(1, d("2026-07-01"), -240_000, "RENT", "memo", None)
        );
        assert_ne!(
            base,
            source_row_hash(1, d("2026-07-01"), -240_000, "RENT", "", Some("x"))
        );
        assert_eq!(base.len(), 64);
    }

    #[test]
    fn decisions() {
        let pending = Candidate {
            txn_id: 7,
            status: RowStatus::Pending,
            payee_norm: "amazon com".into(),
            external_id: None,
            posted_date: d("2026-08-30"),
        };
        let posted = Candidate {
            status: RowStatus::Posted,
            ..pending.clone()
        };
        assert_eq!(
            decide(Some((3, "hash")), RowStatus::Posted, "x", None, &[], 8500),
            Decision::Skip {
                txn_id: 3,
                matched_by: "hash"
            }
        );
        assert_eq!(
            decide(
                None,
                RowStatus::Posted,
                "amazon com",
                None,
                std::slice::from_ref(&pending),
                8500
            ),
            Decision::Update {
                txn_id: 7,
                similarity_bps: 10_000
            }
        );
        assert_eq!(
            decide(
                None,
                RowStatus::Pending,
                "amazon com",
                None,
                std::slice::from_ref(&posted),
                8500
            ),
            Decision::SkipOlder {
                txn_id: 7,
                similarity_bps: 10_000
            }
        );
        assert_eq!(
            decide(
                None,
                RowStatus::Posted,
                "amazon com",
                None,
                std::slice::from_ref(&posted),
                8500
            ),
            Decision::Quarantine {
                txn_id: 7,
                similarity_bps: 10_000
            }
        );
        assert_eq!(
            decide(
                None,
                RowStatus::Posted,
                "amazon com",
                Some("ext"),
                std::slice::from_ref(&posted),
                8500
            ),
            Decision::Update {
                txn_id: 7,
                similarity_bps: 10_000
            }
        );
        assert_eq!(
            decide(
                None,
                RowStatus::Posted,
                "peoples gas",
                None,
                &[posted],
                8500
            ),
            Decision::Insert
        );
    }
}
