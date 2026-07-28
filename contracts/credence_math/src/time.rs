//! Shared constants for common time windows, expressed in seconds.
//!
//! All Credence contracts represent time as `u64` seconds elapsed since the
//! Unix epoch (see `docs/TIME_UNITS.md`). Contracts should import these
//! constants instead of hardcoding the equivalent numeric literals, so the
//! values stay consistent and self-documenting across the workspace.
//!
//! `SECONDS_PER_YEAR` uses a fixed 365-day year and does not account for
//! leap years; contracts that need calendar-accurate year handling should
//! not rely on it for that purpose.

/// Seconds in one minute.
pub const SECONDS_PER_MINUTE: u64 = 60;

/// Seconds in one hour.
pub const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;

/// Seconds in one standard (24-hour) day.
pub const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;

/// Seconds in one week (7 days).
pub const SECONDS_PER_WEEK: u64 = 7 * SECONDS_PER_DAY;

/// Seconds in a fixed 365-day year. Does not account for leap years.
pub const SECONDS_PER_YEAR: u64 = 365 * SECONDS_PER_DAY;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_documented_values() {
        assert_eq!(SECONDS_PER_MINUTE, 60);
        assert_eq!(SECONDS_PER_HOUR, 3_600);
        assert_eq!(SECONDS_PER_DAY, 86_400);
        assert_eq!(SECONDS_PER_WEEK, 604_800);
        assert_eq!(SECONDS_PER_YEAR, 31_536_000);
    }
}
