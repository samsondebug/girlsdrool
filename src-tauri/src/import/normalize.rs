//! Payee normalisation (ADR-0018). Pure and versioned: `payee_raw` is always kept, and the
//! fixture's `EXPECTED.md` lists the expected `payee_norm` for the descriptors the tests check.

/// Bump when the transform changes so a maintenance pass can re-normalise stored rows.
pub const VERSION: u32 = 1;

const PREFIXES: &[&str] = &[
    "pos debit ",
    "checkcard ",
    "debit card purchase ",
    "sq *",
    "tst* ",
    "tst*",
    "paypal *",
    "pp*",
];

/// Lowercase, strip processor prefixes, cut at `*` when a merchant name precedes it, map
/// punctuation to spaces, drop pure-number tokens and short tokens containing digits (store
/// numbers, card last-4, dates, reference codes), collapse whitespace.
pub fn payee_norm(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();
    for prefix in PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }
    if let Some((before, _)) = s.split_once('*') {
        if before.trim().chars().count() >= 3 {
            s = before.to_string();
        }
    }
    let spaced: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    spaced
        .split_whitespace()
        .filter(|t| !t.chars().all(|c| c.is_ascii_digit()))
        .filter(|t| !(t.chars().count() <= 5 && t.chars().any(|c| c.is_ascii_digit())))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table in fixtures/EXPECTED.md ("payee_norm for descriptors the tests check").
    #[test]
    fn matches_the_fixture_table() {
        let table = [
            ("AMAZON.COM*2K4 AMZN.COM/BILL", "amazon com"),
            ("AMAZON.COM", "amazon com"),
            ("JEWEL-OSCO #3421", "jewel osco"),
            ("JEWEL-OSCO #3421 CHICAGO", "jewel osco chicago"),
            ("TRADER JOE S #702", "trader joe s"),
            ("SHELL OIL 57442", "shell oil"),
            ("ONLINE TRANSFER TO SAV ...5678", "online transfer to sav"),
            ("PAYMENT - THANK YOU", "payment thank you"),
            ("WALGREENS #5821", "walgreens"),
            (
                "ZELLE PAYMENT FROM MORGAN AVERY",
                "zelle payment from morgan avery",
            ),
        ];
        for (raw, expected) in table {
            assert_eq!(payee_norm(raw), expected, "{raw}");
        }
    }

    #[test]
    fn processor_prefixes_and_short_codes() {
        assert_eq!(payee_norm("SQ *COFFEE SHOP"), "coffee shop");
        assert_eq!(payee_norm("TST* BURGER BAR 12/03"), "burger bar");
        assert_eq!(payee_norm("POS DEBIT WALMART #1234 05/06"), "walmart");
        assert_eq!(payee_norm("PAYPAL *STEAMGAMES"), "steamgames");
        assert_eq!(payee_norm("  "), "");
        assert_eq!(payee_norm("Café Río"), "café río");
    }
}
