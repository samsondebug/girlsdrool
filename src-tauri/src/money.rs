//! Money is `i64` cents. This module is the only place rounding happens and the only place a
//! decimal string is created on the Rust side (exports). No `f64` anywhere.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Signed cents. Inflow positive, outflow negative, from the account's point of view.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Cents(pub i64);

impl Cents {
    pub const ZERO: Cents = Cents(0);

    pub fn checked_add(self, other: Cents) -> AppResult<Cents> {
        self.0
            .checked_add(other.0)
            .map(Cents)
            .ok_or(AppError::Overflow)
    }

    pub fn checked_sub(self, other: Cents) -> AppResult<Cents> {
        self.0
            .checked_sub(other.0)
            .map(Cents)
            .ok_or(AppError::Overflow)
    }

    pub fn checked_neg(self) -> AppResult<Cents> {
        self.0.checked_neg().map(Cents).ok_or(AppError::Overflow)
    }

    pub fn checked_abs(self) -> AppResult<Cents> {
        self.0.checked_abs().map(Cents).ok_or(AppError::Overflow)
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn is_positive(self) -> bool {
        self.0 > 0
    }
}

/// Exact sum; overflow is an error, never a wrapped number.
pub fn sum<I: IntoIterator<Item = Cents>>(items: I) -> AppResult<Cents> {
    items
        .into_iter()
        .try_fold(Cents::ZERO, |acc, c| acc.checked_add(c))
}

/// `a * b / d`, rounded half away from zero, with `i128` intermediates. The single rounding
/// primitive: interest, pro-rata, percent-of-balance and bps math all go through here.
pub fn mul_div_round(a: i128, b: i128, d: i128) -> AppResult<i64> {
    if d == 0 {
        return Err(AppError::Internal("division by zero in money math".into()));
    }
    let n = a.checked_mul(b).ok_or(AppError::Overflow)?;
    let q = n / d;
    let r = n % d;
    let rounded = if r.abs().checked_mul(2).ok_or(AppError::Overflow)? >= d.abs() {
        q + n.signum() * d.signum()
    } else {
        q
    };
    i64::try_from(rounded).map_err(|_| AppError::Overflow)
}

/// `amount × bps / 10 000`, rounded half away from zero.
pub fn bps_of(amount: i64, bps: i64) -> AppResult<i64> {
    mul_div_round(i128::from(amount), i128::from(bps), 10_000)
}

/// Largest-remainder allocation of `total` into `parts` integers that always sum to `total`.
/// Earlier parts receive the remainder, one cent each, with the sign of `total`.
pub fn allocate(total: i64, parts: usize) -> AppResult<Vec<i64>> {
    if parts == 0 {
        return Err(AppError::Internal(
            "cannot allocate cents into zero parts".into(),
        ));
    }
    let n = i64::try_from(parts).map_err(|_| AppError::Overflow)?;
    let base = total / n;
    let remainder = total - base * n;
    let extra = remainder.unsigned_abs();
    let step = remainder.signum();
    let mut out = Vec::with_capacity(parts);
    for i in 0..parts {
        let bump = if (i as u64) < extra { step } else { 0 };
        out.push(base + bump);
    }
    Ok(out)
}

/// Decimal string for exports: `-1234.56`, `0.05`, `12.00`. Never grouped, never a `$`.
pub fn to_decimal_string(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseMoneyError {
    #[error("empty amount")]
    Empty,
    #[error("unexpected character {0:?}")]
    BadChar(char),
    #[error("more than two decimal places")]
    TooManyDecimals,
    #[error("more than one decimal point")]
    MultiplePoints,
    #[error("amount does not fit in cents")]
    Overflow,
}

/// Parse a statement amount into cents with integer arithmetic only. Accepts `1234.5`,
/// `-1,234.56`, `$1,234.56`, `(1,234.56)` (negative), `+12`, leading/trailing spaces and a
/// trailing `-` as some card exports write it. Rejects anything else.
pub fn parse_decimal_cents(text: &str) -> Result<i64, ParseMoneyError> {
    let mut s = text.trim();
    if s.is_empty() {
        return Err(ParseMoneyError::Empty);
    }
    let mut negative = false;
    if s.starts_with('(') && s.ends_with(')') {
        negative = true;
        s = s[1..s.len() - 1].trim();
    }
    if let Some(rest) = s.strip_suffix('-') {
        negative = !negative;
        s = rest.trim();
    }
    let mut whole: i64 = 0;
    let mut frac: i64 = 0;
    let mut frac_digits = 0u8;
    let mut seen_point = false;
    let mut seen_digit = false;
    let mut sign_allowed = true;
    for ch in s.chars() {
        match ch {
            '$' | ',' | ' ' | '\u{a0}' => {}
            '-' if sign_allowed => {
                negative = !negative;
            }
            '+' if sign_allowed => {}
            '.' => {
                if seen_point {
                    return Err(ParseMoneyError::MultiplePoints);
                }
                seen_point = true;
                sign_allowed = false;
            }
            '0'..='9' => {
                seen_digit = true;
                sign_allowed = false;
                let digit = i64::from(ch as u8 - b'0');
                if seen_point {
                    if frac_digits >= 2 {
                        return Err(ParseMoneyError::TooManyDecimals);
                    }
                    frac = frac * 10 + digit;
                    frac_digits += 1;
                } else {
                    whole = whole
                        .checked_mul(10)
                        .and_then(|w| w.checked_add(digit))
                        .ok_or(ParseMoneyError::Overflow)?;
                }
            }
            other => return Err(ParseMoneyError::BadChar(other)),
        }
    }
    if !seen_digit {
        return Err(ParseMoneyError::Empty);
    }
    if frac_digits == 1 {
        frac *= 10;
    }
    let magnitude = whole
        .checked_mul(100)
        .and_then(|w| w.checked_add(frac))
        .ok_or(ParseMoneyError::Overflow)?;
    Ok(if negative { -magnitude } else { magnitude })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div_rounds_half_away_from_zero() {
        // 2.5 → 3, -2.5 → -3, 2.4 → 2, -2.4 → -2, 2.6 → 3
        assert_eq!(mul_div_round(25, 1, 10).unwrap(), 3);
        assert_eq!(mul_div_round(-25, 1, 10).unwrap(), -3);
        assert_eq!(mul_div_round(24, 1, 10).unwrap(), 2);
        assert_eq!(mul_div_round(-24, 1, 10).unwrap(), -2);
        assert_eq!(mul_div_round(26, 1, 10).unwrap(), 3);
        // negative divisor behaves the same as negating the numerator
        assert_eq!(mul_div_round(25, 1, -10).unwrap(), -3);
    }

    #[test]
    fn monthly_interest_example_in_cents() {
        // $1,000.00 at 19.99% APR, monthly nominal: 100000 × 1999 / 120000 = 1665.83… → 1666
        assert_eq!(mul_div_round(100_000, 1999, 120_000).unwrap(), 1666);
        // bps helper agrees: 19.99% of $1,000.00 = $199.90
        assert_eq!(bps_of(100_000, 1999).unwrap(), 19_990);
    }

    #[test]
    fn mul_div_reports_overflow_instead_of_wrapping() {
        assert!(matches!(
            mul_div_round(i128::MAX, 2, 1),
            Err(AppError::Overflow)
        ));
        assert!(matches!(
            mul_div_round(i128::from(i64::MAX), 2, 1),
            Err(AppError::Overflow)
        ));
    }

    #[test]
    fn mul_div_rejects_zero_divisor() {
        assert!(matches!(mul_div_round(1, 1, 0), Err(AppError::Internal(_))));
    }

    #[test]
    fn allocate_conserves_and_spreads_remainder() {
        assert_eq!(allocate(10, 3).unwrap(), vec![4, 3, 3]);
        assert_eq!(allocate(-10, 3).unwrap(), vec![-4, -3, -3]);
        assert_eq!(allocate(0, 4).unwrap(), vec![0, 0, 0, 0]);
        assert_eq!(allocate(7, 7).unwrap(), vec![1; 7]);
        assert_eq!(allocate(5, 7).unwrap(), vec![1, 1, 1, 1, 1, 0, 0]);
        assert!(matches!(allocate(1, 0), Err(AppError::Internal(_))));
    }

    #[test]
    fn sum_is_exact_and_checked() {
        assert_eq!(sum([Cents(1), Cents(-3), Cents(5)]).unwrap(), Cents(3));
        assert!(matches!(
            sum([Cents(i64::MAX), Cents(1)]),
            Err(AppError::Overflow)
        ));
    }

    #[test]
    fn decimal_string_for_exports() {
        assert_eq!(to_decimal_string(-123_456), "-1234.56");
        assert_eq!(to_decimal_string(5), "0.05");
        assert_eq!(to_decimal_string(1200), "12.00");
        assert_eq!(to_decimal_string(0), "0.00");
        assert_eq!(to_decimal_string(i64::MIN), "-92233720368547758.08");
    }

    #[test]
    fn parses_statement_amounts_without_floats() {
        assert_eq!(parse_decimal_cents("1234.5").unwrap(), 123_450);
        assert_eq!(parse_decimal_cents("-1,234.56").unwrap(), -123_456);
        assert_eq!(parse_decimal_cents("$1,234.56").unwrap(), 123_456);
        assert_eq!(parse_decimal_cents("(1,234.56)").unwrap(), -123_456);
        assert_eq!(parse_decimal_cents("($12.00)").unwrap(), -1200);
        assert_eq!(parse_decimal_cents("+12").unwrap(), 1200);
        assert_eq!(parse_decimal_cents("12.00-").unwrap(), -1200);
        assert_eq!(parse_decimal_cents(" 0.07 ").unwrap(), 7);
        assert_eq!(parse_decimal_cents("-0.01").unwrap(), -1);
        assert_eq!(parse_decimal_cents("-$5").unwrap(), -500);
        assert_eq!(parse_decimal_cents(".5").unwrap(), 50);
    }

    #[test]
    fn rejects_malformed_amounts() {
        assert_eq!(parse_decimal_cents(""), Err(ParseMoneyError::Empty));
        assert_eq!(parse_decimal_cents("  "), Err(ParseMoneyError::Empty));
        assert_eq!(parse_decimal_cents("$"), Err(ParseMoneyError::Empty));
        assert_eq!(
            parse_decimal_cents("1.234"),
            Err(ParseMoneyError::TooManyDecimals)
        );
        assert_eq!(
            parse_decimal_cents("1.2.3"),
            Err(ParseMoneyError::MultiplePoints)
        );
        assert_eq!(
            parse_decimal_cents("12abc"),
            Err(ParseMoneyError::BadChar('a'))
        );
        assert_eq!(
            parse_decimal_cents("1e5"),
            Err(ParseMoneyError::BadChar('e'))
        );
        assert_eq!(
            parse_decimal_cents("12-3"),
            Err(ParseMoneyError::BadChar('-'))
        );
        assert_eq!(
            parse_decimal_cents("99999999999999999999"),
            Err(ParseMoneyError::Overflow)
        );
    }
}
