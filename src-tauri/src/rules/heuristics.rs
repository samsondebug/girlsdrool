//! Heuristics (ARCHITECTURE §6.4): pure decisions for rows no rule matched. Each one stores its
//! code so the ledger can say why. Cash withdrawals and payment-app rows are never categorised.

use crate::import::csv::{
    FLAG_CASH_WITHDRAWAL, FLAG_FEE, FLAG_INTEREST, FLAG_NEEDS_REVIEW, FLAG_PAYMENT_APP_UNKNOWN,
    FLAG_SECURITIES_SALE,
};

pub const PAYMENT_APP_WORDS: &[&str] = &["venmo", "zelle", "cash app", "paypal"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heuristic {
    pub code: &'static str,
    /// Category system code, when the heuristic is confident enough to assign one.
    pub category_code: Option<&'static str>,
    pub flags_set: u32,
}

/// Evaluate in the documented order; the first applicable heuristic wins.
pub fn evaluate(
    payee_norm: &str,
    amount_cents: i64,
    flags: u32,
    account_kind: &str,
) -> Option<Heuristic> {
    if flags & FLAG_SECURITIES_SALE != 0 && amount_cents > 0 {
        return Some(Heuristic {
            code: "securities_sale",
            category_code: Some("transfer.securities_sale_proceeds"),
            flags_set: FLAG_SECURITIES_SALE,
        });
    }
    if payee_norm.contains("fee") {
        return Some(Heuristic {
            code: "fee_charge",
            category_code: Some("debt.fees"),
            flags_set: FLAG_FEE,
        });
    }
    if payee_norm.contains("interest") {
        return Some(if amount_cents < 0 {
            Heuristic {
                code: "interest_charge",
                category_code: Some("debt.interest"),
                flags_set: FLAG_INTEREST,
            }
        } else {
            Heuristic {
                code: "interest_income",
                category_code: Some("income.interest"),
                flags_set: FLAG_INTEREST,
            }
        });
    }
    if payee_norm.contains("atm") {
        return Some(Heuristic {
            code: "atm_withdrawal",
            category_code: None,
            flags_set: FLAG_CASH_WITHDRAWAL | FLAG_NEEDS_REVIEW,
        });
    }
    if account_kind == "payment_app" || PAYMENT_APP_WORDS.iter().any(|w| payee_norm.contains(w)) {
        return Some(Heuristic {
            code: "payment_app_row",
            category_code: None,
            flags_set: FLAG_PAYMENT_APP_UNKNOWN | FLAG_NEEDS_REVIEW,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order and outcomes written in fixtures/EXPECTED.md ("Heuristics, in order").
    #[test]
    fn fixture_rows_evaluate_as_written_down() {
        let h = evaluate("sell", 250_000, FLAG_SECURITIES_SALE, "brokerage").unwrap();
        assert_eq!(
            (h.code, h.category_code),
            ("securities_sale", Some("transfer.securities_sale_proceeds"))
        );
        let h = evaluate("non network atm fee", -300, 0, "checking").unwrap();
        assert_eq!(
            (h.code, h.category_code),
            ("fee_charge", Some("debt.fees")),
            "fee wins over atm"
        );
        let h = evaluate("intl transaction fee", -490, 0, "checking").unwrap();
        assert_eq!(h.code, "fee_charge");
        let h = evaluate(
            "interest charge on purchases",
            -2287,
            FLAG_INTEREST,
            "credit",
        )
        .unwrap();
        assert_eq!(
            (h.code, h.category_code),
            ("interest_charge", Some("debt.interest"))
        );
        let h = evaluate("interest payment", 1023, 0, "savings").unwrap();
        assert_eq!(
            (h.code, h.category_code),
            ("interest_income", Some("income.interest"))
        );
        let h = evaluate("atm withdrawal banco azteca cdmx", -16_342, 0, "checking").unwrap();
        assert_eq!((h.code, h.category_code), ("atm_withdrawal", None));
        assert_eq!(h.flags_set, FLAG_CASH_WITHDRAWAL | FLAG_NEEDS_REVIEW);
        let h = evaluate("venmo morgan avery", -8_500, 0, "checking").unwrap();
        assert_eq!((h.code, h.category_code), ("payment_app_row", None));
        let h = evaluate(
            "chris park",
            60_000,
            FLAG_PAYMENT_APP_UNKNOWN | FLAG_NEEDS_REVIEW,
            "payment_app",
        )
        .unwrap();
        assert_eq!(h.code, "payment_app_row");
        assert_eq!(
            evaluate("lakeshore properties rent", -240_000, 0, "checking"),
            None
        );
    }
}
