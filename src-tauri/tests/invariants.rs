//! Named property tests (spec: Invariants). Each test is named exactly as the spec names it and
//! grows with the engines: `money_sum_conserves` covers allocation at M0 and split rows at M1.

use kept::money::{allocate, mul_div_round, parse_decimal_cents, to_decimal_string};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// Splitting a total into parts and recombining them never loses or invents a cent, and
    /// no part differs from another by more than one cent.
    #[test]
    fn money_sum_conserves(total in any::<i64>(), parts in 1usize..=64) {
        let shares = allocate(total, parts).unwrap();
        prop_assert_eq!(shares.len(), parts);
        let sum = shares.iter().try_fold(0i64, |acc, &x| acc.checked_add(x)).unwrap();
        prop_assert_eq!(sum, total);
        let min = *shares.iter().min().unwrap();
        let max = *shares.iter().max().unwrap();
        prop_assert!(max - min <= 1);
    }

    /// `mul_div_round` is the exact quotient rounded half away from zero: the result times the
    /// divisor is within half a divisor of the true product, and ties move away from zero.
    #[test]
    fn mul_div_round_is_within_half_a_unit(
        a in -1_000_000_000_000i64..=1_000_000_000_000,
        b in -1_000_000i64..=1_000_000,
        d in prop_oneof![1i64..=1_000_000, -1_000_000i64..=-1],
    ) {
        let r = i128::from(mul_div_round(i128::from(a), i128::from(b), i128::from(d)).unwrap());
        let n = i128::from(a) * i128::from(b);
        let diff = r * i128::from(d) - n;           // same sign convention as n/d
        let half = i128::from(d).abs();
        prop_assert!(diff.abs() * 2 <= half, "r={r} n={n} d={d}");
        if diff.abs() * 2 == half {
            // a tie: the rounded value is the one farther from zero
            let exact_sign = (n.signum() * i128::from(d).signum()).max(0) * 2 - 1;
            prop_assert_eq!(r.signum() == 0 || r.signum() == exact_sign, true);
            prop_assert!(r.abs() * half >= n.abs(), "tie must round away from zero");
        }
    }

    /// Export formatting and statement parsing are inverses on every i64.
    #[test]
    fn decimal_string_round_trips(cents in any::<i64>()) {
        prop_assert_eq!(parse_decimal_cents(&to_decimal_string(cents)).unwrap(), cents);
    }
}
