//! Pay-cycle and due-rule occurrences (ARCHITECTURE §5.5): civil-date arithmetic only. A
//! stream or obligation describes a rule; the engines ask for the occurrences inside a window.

use chrono::{Datelike, Duration, Weekday};

use crate::dates::CivilDate;
use crate::error::{AppError, AppResult};

/// Upper bound on generated occurrences per call, so a bad anchor cannot spin.
const MAX_OCCURRENCES: usize = 2000;

fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = CivilDate::from_ymd_opt(ny, nm, 1);
    let first = CivilDate::from_ymd_opt(year, month, 1);
    match (first_next, first) {
        (Some(a), Some(b)) => u32::try_from((a - b).num_days()).unwrap_or(28),
        _ => 28,
    }
}

/// `day` of the month, clamped to the month's length (31 means the last day).
pub fn clamp_day(year: i32, month: u32, day: u32) -> AppResult<CivilDate> {
    let d = day.clamp(1, days_in_month(year, month));
    CivilDate::from_ymd_opt(year, month, d).ok_or_else(|| {
        AppError::validation("date", format!("{year}-{month:02}-{d:02} is not a date"))
    })
}

fn add_months(year: i32, month: u32, n: u32) -> (i32, u32) {
    let zero = year * 12 + i32::try_from(month - 1).unwrap_or(0) + i32::try_from(n).unwrap_or(0);
    (
        zero.div_euclid(12),
        u32::try_from(zero.rem_euclid(12)).unwrap_or(0) + 1,
    )
}

/// The `nth` (1–4, or 5 = last) `weekday` of the month.
pub fn nth_weekday(year: i32, month: u32, weekday: Weekday, nth: u32) -> AppResult<CivilDate> {
    let first = clamp_day(year, month, 1)?;
    let offset = (weekday.num_days_from_monday() + 7 - first.weekday().num_days_from_monday()) % 7;
    let first_match = first + Duration::days(i64::from(offset));
    let last_day = days_in_month(year, month);
    if nth >= 5 {
        let mut d = first_match;
        while (d + Duration::days(7)).day() > d.day() && (d + Duration::days(7)).month() == month {
            d += Duration::days(7);
        }
        return Ok(d);
    }
    let d = first_match + Duration::days(7 * i64::from(nth.saturating_sub(1)));
    if d.day() > last_day || d.month() != month {
        return Err(AppError::validation(
            "due_nth",
            format!("{year}-{month:02} has no {nth}th {weekday:?}"),
        ));
    }
    Ok(d)
}

/// Move a weekend date per the stream's rule; weekdays are returned unchanged.
pub fn apply_weekend_rule(d: CivilDate, rule: &str) -> CivilDate {
    let shift = |mut x: CivilDate, step: i64| {
        while matches!(x.weekday(), Weekday::Sat | Weekday::Sun) {
            x += Duration::days(step);
        }
        x
    };
    match rule {
        "previous_business_day" => shift(d, -1),
        "next_business_day" => shift(d, 1),
        _ => d,
    }
}

pub struct IncomeRule<'a> {
    pub cycle: &'a str,
    pub anchor: CivilDate,
    pub semimonthly_day_1: Option<i64>,
    pub semimonthly_day_2: Option<i64>,
    pub weekend_rule: &'a str,
}

/// Civil dates an income stream is expected on, inside `[from, to]`, after the weekend rule.
pub fn income_occurrences(
    rule: &IncomeRule<'_>,
    from: CivilDate,
    to: CivilDate,
) -> AppResult<Vec<CivilDate>> {
    let mut raw: Vec<CivilDate> = Vec::new();
    match rule.cycle {
        "weekly" | "biweekly" => {
            let step = if rule.cycle == "weekly" { 7 } else { 14 };
            let mut d = rule.anchor;
            while d <= to && raw.len() < MAX_OCCURRENCES {
                raw.push(d);
                d += Duration::days(step);
            }
        }
        "semimonthly" => {
            let d1 = u32::try_from(rule.semimonthly_day_1.unwrap_or(1)).unwrap_or(1);
            let d2 = u32::try_from(rule.semimonthly_day_2.unwrap_or(15)).unwrap_or(15);
            let (mut y, mut m) = (rule.anchor.year(), rule.anchor.month());
            while raw.len() < MAX_OCCURRENCES {
                let a = clamp_day(y, m, d1)?;
                let b = clamp_day(y, m, d2)?;
                if a > to && b > to {
                    break;
                }
                for d in [a.min(b), a.max(b)] {
                    if d >= rule.anchor {
                        raw.push(d);
                    }
                }
                let next = add_months(y, m, 1);
                y = next.0;
                m = next.1;
            }
        }
        "monthly" => {
            let (mut y, mut m) = (rule.anchor.year(), rule.anchor.month());
            while raw.len() < MAX_OCCURRENCES {
                let d = clamp_day(y, m, rule.anchor.day())?;
                if d > to {
                    break;
                }
                raw.push(d);
                let next = add_months(y, m, 1);
                y = next.0;
                m = next.1;
            }
        }
        "once" => raw.push(rule.anchor),
        other => {
            return Err(AppError::validation(
                "cycle",
                format!("{other:?} is not a pay cycle"),
            ));
        }
    }
    let mut out: Vec<CivilDate> = raw
        .into_iter()
        .map(|d| apply_weekend_rule(d, rule.weekend_rule))
        .filter(|d| *d >= from && *d <= to)
        .collect();
    out.dedup();
    Ok(out)
}

pub struct DueRule<'a> {
    pub rule: &'a str,
    pub due_day: Option<i64>,
    pub due_month: Option<i64>,
    pub due_weekday: Option<i64>,
    pub due_nth: Option<i64>,
    pub anchor: Option<CivilDate>,
}

fn weekday_from(n: i64) -> Weekday {
    match n.rem_euclid(7) {
        0 => Weekday::Mon,
        1 => Weekday::Tue,
        2 => Weekday::Wed,
        3 => Weekday::Thu,
        4 => Weekday::Fri,
        5 => Weekday::Sat,
        _ => Weekday::Sun,
    }
}

/// Civil due dates of an obligation inside `[from, to]`. No weekend shift: early is conservative.
pub fn obligation_occurrences(
    rule: &DueRule<'_>,
    from: CivilDate,
    to: CivilDate,
) -> AppResult<Vec<CivilDate>> {
    let mut out: Vec<CivilDate> = Vec::new();
    let day = u32::try_from(rule.due_day.unwrap_or(1)).unwrap_or(1);
    // for the calendar rules the anchor is the first day the obligation exists: nothing falls
    // due before it (a biweekly anchor is its first occurrence, a once rule's its only one)
    let from = match rule.anchor {
        Some(a) if a > from && !matches!(rule.rule, "biweekly" | "once") => a,
        _ => from,
    };
    match rule.rule {
        "monthly_day" => {
            let (mut y, mut m) = (from.year(), from.month());
            while out.len() < MAX_OCCURRENCES {
                let d = clamp_day(y, m, day)?;
                if d > to {
                    break;
                }
                if d >= from {
                    out.push(d);
                }
                let next = add_months(y, m, 1);
                y = next.0;
                m = next.1;
            }
        }
        "nth_weekday" => {
            let weekday = weekday_from(rule.due_weekday.unwrap_or(0));
            let nth = u32::try_from(rule.due_nth.unwrap_or(1)).unwrap_or(1);
            let (mut y, mut m) = (from.year(), from.month());
            while out.len() < MAX_OCCURRENCES {
                let first = clamp_day(y, m, 1)?;
                if first > to {
                    break;
                }
                if let Ok(d) = nth_weekday(y, m, weekday, nth) {
                    if d >= from && d <= to {
                        out.push(d);
                    }
                }
                let next = add_months(y, m, 1);
                y = next.0;
                m = next.1;
            }
        }
        "biweekly" => {
            let anchor = rule.anchor.ok_or_else(|| {
                AppError::validation("anchor_date", "a biweekly rule needs an anchor")
            })?;
            let mut d = anchor;
            while d <= to && out.len() < MAX_OCCURRENCES {
                if d >= from {
                    out.push(d);
                }
                d += Duration::days(14);
            }
        }
        "annual" => {
            let month = u32::try_from(rule.due_month.unwrap_or(1))
                .unwrap_or(1)
                .clamp(1, 12);
            for y in from.year()..=to.year() {
                let d = clamp_day(y, month, day)?;
                if d >= from && d <= to {
                    out.push(d);
                }
            }
        }
        "once" => {
            let anchor = rule.anchor.ok_or_else(|| {
                AppError::validation("anchor_date", "a one-off rule needs its date")
            })?;
            if anchor >= from && anchor <= to {
                out.push(anchor);
            }
        }
        other => {
            return Err(AppError::validation(
                "due_rule",
                format!("{other:?} is not a due rule"),
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::parse_civil;

    fn d(s: &str) -> CivilDate {
        parse_civil(s).unwrap()
    }

    #[test]
    fn biweekly_from_the_fixture_anchor_lands_on_fridays_and_the_next_pay_is_10_02() {
        let rule = IncomeRule {
            cycle: "biweekly",
            anchor: d("2026-07-10"),
            semimonthly_day_1: None,
            semimonthly_day_2: None,
            weekend_rule: "previous_business_day",
        };
        let got = income_occurrences(&rule, d("2026-07-01"), d("2026-10-31")).unwrap();
        let want: Vec<CivilDate> = [
            "2026-07-10",
            "2026-07-24",
            "2026-08-07",
            "2026-08-21",
            "2026-09-04",
            "2026-09-18",
            "2026-10-02",
            "2026-10-16",
            "2026-10-30",
        ]
        .iter()
        .map(|s| d(s))
        .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn weekend_rule_moves_saturday_pay_to_friday_and_monthly_clamps_to_month_end() {
        assert_eq!(
            apply_weekend_rule(d("2026-10-03"), "previous_business_day"),
            d("2026-10-02")
        );
        assert_eq!(
            apply_weekend_rule(d("2026-10-04"), "next_business_day"),
            d("2026-10-05")
        );
        assert_eq!(apply_weekend_rule(d("2026-10-04"), "none"), d("2026-10-04"));
        let rule = IncomeRule {
            cycle: "monthly",
            anchor: d("2026-01-31"),
            semimonthly_day_1: None,
            semimonthly_day_2: None,
            weekend_rule: "none",
        };
        let got = income_occurrences(&rule, d("2026-01-01"), d("2026-04-30")).unwrap();
        assert_eq!(
            got,
            vec![
                d("2026-01-31"),
                d("2026-02-28"),
                d("2026-03-31"),
                d("2026-04-30")
            ]
        );
    }

    #[test]
    fn semimonthly_uses_both_days_with_31_meaning_the_last_day() {
        let rule = IncomeRule {
            cycle: "semimonthly",
            anchor: d("2026-02-01"),
            semimonthly_day_1: Some(15),
            semimonthly_day_2: Some(31),
            weekend_rule: "none",
        };
        let got = income_occurrences(&rule, d("2026-02-01"), d("2026-03-31")).unwrap();
        assert_eq!(
            got,
            vec![
                d("2026-02-15"),
                d("2026-02-28"),
                d("2026-03-15"),
                d("2026-03-31")
            ]
        );
    }

    #[test]
    fn due_rules_monthly_annual_nth_weekday() {
        let monthly = DueRule {
            rule: "monthly_day",
            due_day: Some(31),
            due_month: None,
            due_weekday: None,
            due_nth: None,
            anchor: None,
        };
        assert_eq!(
            obligation_occurrences(&monthly, d("2026-09-15"), d("2026-11-30")).unwrap(),
            vec![d("2026-09-30"), d("2026-10-31"), d("2026-11-30")]
        );
        let annual = DueRule {
            rule: "annual",
            due_day: Some(22),
            due_month: Some(9),
            due_weekday: None,
            due_nth: None,
            anchor: None,
        };
        assert_eq!(
            obligation_occurrences(&annual, d("2026-01-01"), d("2027-12-31")).unwrap(),
            vec![d("2026-09-22"), d("2027-09-22")]
        );
        // the last Friday of October 2026 is the 30th; the second Monday is the 12th
        let last_friday = DueRule {
            rule: "nth_weekday",
            due_day: None,
            due_month: None,
            due_weekday: Some(4),
            due_nth: Some(5),
            anchor: None,
        };
        assert_eq!(
            obligation_occurrences(&last_friday, d("2026-10-01"), d("2026-10-31")).unwrap(),
            vec![d("2026-10-30")]
        );
        let second_monday = DueRule {
            rule: "nth_weekday",
            due_day: None,
            due_month: None,
            due_weekday: Some(0),
            due_nth: Some(2),
            anchor: None,
        };
        assert_eq!(
            obligation_occurrences(&second_monday, d("2026-10-01"), d("2026-10-31")).unwrap(),
            vec![d("2026-10-12")]
        );
    }
}
