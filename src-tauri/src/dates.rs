//! Civil dates and instants (ARCHITECTURE §3.2). Ledger and plan dates are civil `YYYY-MM-DD`
//! in the user's zone; `*_at` columns are UTC RFC3339. Pay-cycle math is civil arithmetic only.

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use chrono_tz::Tz;

use crate::error::{AppError, AppResult};

pub type CivilDate = NaiveDate;

pub const DEFAULT_ZONE: &str = "America/Chicago";

/// Parse an IANA zone name as stored in `setting.zone`.
pub fn parse_zone(name: &str) -> AppResult<Tz> {
    name.parse::<Tz>()
        .map_err(|_| AppError::validation("zone", format!("unknown IANA time zone {name:?}")))
}

/// Today's civil date in the given zone.
pub fn today_in(zone: Tz) -> CivilDate {
    Utc::now().with_timezone(&zone).date_naive()
}

/// The civil date a given instant falls on in the given zone.
pub fn civil_date_of(instant: DateTime<Utc>, zone: Tz) -> CivilDate {
    instant.with_timezone(&zone).date_naive()
}

/// Current instant as `2026-10-05T03:13:09Z` (for `*_at` columns).
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// `YYYY-MM-DD`.
pub fn format_civil(d: CivilDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Strict `YYYY-MM-DD`.
pub fn parse_civil(s: &str) -> AppResult<CivilDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation("date", format!("{s:?} is not a YYYY-MM-DD civil date")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn zone_parsing() {
        assert!(parse_zone(DEFAULT_ZONE).is_ok());
        assert!(matches!(
            parse_zone("Mars/Olympus"),
            Err(AppError::Validation { .. })
        ));
    }

    #[test]
    fn civil_date_follows_the_zone_not_utc() {
        // 2026-03-01 03:30 UTC is still 2026-02-28 in Chicago (UTC-6 before DST).
        let instant = Utc.with_ymd_and_hms(2026, 3, 1, 3, 30, 0).unwrap();
        let chicago = parse_zone("America/Chicago").unwrap();
        assert_eq!(format_civil(civil_date_of(instant, chicago)), "2026-02-28");
        let utc = parse_zone("UTC").unwrap();
        assert_eq!(format_civil(civil_date_of(instant, utc)), "2026-03-01");
    }

    #[test]
    fn civil_round_trip_and_strictness() {
        let d = parse_civil("2026-10-05").unwrap();
        assert_eq!(format_civil(d), "2026-10-05");
        assert!(parse_civil("10/05/2026").is_err());
        assert!(parse_civil("2026-13-01").is_err());
        assert!(parse_civil("2026-02-30").is_err());
    }

    #[test]
    fn rfc3339_shape() {
        let s = now_rfc3339();
        assert_eq!(s.len(), 20);
        assert!(s.ends_with('Z'));
        assert_eq!(&s[10..11], "T");
    }
}
