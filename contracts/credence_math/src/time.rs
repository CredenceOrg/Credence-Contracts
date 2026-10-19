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

    // ── Documented value assertions ──────────────────────────────────────────

    /// Every constant must match the value documented in the module-level doc
    /// comment and in `docs/TIME_UNITS.md`.  Changing any of these values is a
    /// breaking change for every downstream contract.
    #[test]
    fn matches_documented_values() {
        assert_eq!(SECONDS_PER_MINUTE, 60);
        assert_eq!(SECONDS_PER_HOUR, 3_600);
        assert_eq!(SECONDS_PER_DAY, 86_400);
        assert_eq!(SECONDS_PER_WEEK, 604_800);
        assert_eq!(SECONDS_PER_YEAR, 31_536_000);
    }

    // ── Arithmetic-relationship invariants ──────────────────────────────────

    /// SECONDS_PER_HOUR must be exactly 60 * SECONDS_PER_MINUTE.
    /// A change to either constant that breaks this ratio would silently corrupt
    /// any contract that derives hours from minutes.
    #[test]
    fn hour_is_sixty_minutes() {
        assert_eq!(SECONDS_PER_HOUR, 60 * SECONDS_PER_MINUTE);
    }

    /// SECONDS_PER_DAY must be exactly 24 * SECONDS_PER_HOUR.
    #[test]
    fn day_is_twenty_four_hours() {
        assert_eq!(SECONDS_PER_DAY, 24 * SECONDS_PER_HOUR);
    }

    /// SECONDS_PER_WEEK must be exactly 7 * SECONDS_PER_DAY.
    #[test]
    fn week_is_seven_days() {
        assert_eq!(SECONDS_PER_WEEK, 7 * SECONDS_PER_DAY);
    }

    /// SECONDS_PER_YEAR must be exactly 365 * SECONDS_PER_DAY (fixed, non-leap).
    /// The module doc explicitly states leap years are not accounted for; this
    /// test locks in that invariant so an accidental change to 366 is caught.
    #[test]
    fn year_is_three_hundred_sixty_five_days() {
        assert_eq!(SECONDS_PER_YEAR, 365 * SECONDS_PER_DAY);
    }

    /// Verify the full derivation chain: minute → hour → day → week.
    /// This catches any constant that is individually correct but inconsistent
    /// with the others (e.g. SECONDS_PER_DAY redefined independently of HOUR).
    #[test]
    fn derivation_chain_minute_to_week_is_consistent() {
        let derived_hour = 60 * SECONDS_PER_MINUTE;
        let derived_day = 24 * derived_hour;
        let derived_week = 7 * derived_day;
        assert_eq!(derived_hour, SECONDS_PER_HOUR);
        assert_eq!(derived_day, SECONDS_PER_DAY);
        assert_eq!(derived_week, SECONDS_PER_WEEK);
    }

    // ── Ordering invariants ──────────────────────────────────────────────────

    /// Strict monotone ordering: minute < hour < day < week < year.
    /// Any reordering would be a logical regression even if the raw values
    /// remain unchanged.
    #[test]
    fn constants_are_strictly_increasing() {
        assert!(SECONDS_PER_MINUTE < SECONDS_PER_HOUR);
        assert!(SECONDS_PER_HOUR < SECONDS_PER_DAY);
        assert!(SECONDS_PER_DAY < SECONDS_PER_WEEK);
        assert!(SECONDS_PER_WEEK < SECONDS_PER_YEAR);
    }

    /// A week must be strictly less than a year (52 weeks ≈ 364 days < 365 days).
    #[test]
    fn year_is_greater_than_fifty_two_weeks() {
        assert!(SECONDS_PER_YEAR > 52 * SECONDS_PER_WEEK);
    }

    /// A year (365 days) must be less than 53 weeks (371 days).
    #[test]
    fn year_is_less_than_fifty_three_weeks() {
        assert!(SECONDS_PER_YEAR < 53 * SECONDS_PER_WEEK);
    }

    // ── u64 boundary / overflow-safety ──────────────────────────────────────

    /// Every constant must fit comfortably in u64.  The values are const so
    /// this is a compile-time guarantee, but explicitly asserting them < u64::MAX
    /// documents the intent and prevents a future saturating-arithmetic change
    /// from silently wrapping.
    #[test]
    fn all_constants_are_less_than_u64_max() {
        assert!(SECONDS_PER_MINUTE < u64::MAX);
        assert!(SECONDS_PER_HOUR < u64::MAX);
        assert!(SECONDS_PER_DAY < u64::MAX);
        assert!(SECONDS_PER_WEEK < u64::MAX);
        assert!(SECONDS_PER_YEAR < u64::MAX);
    }

    /// Multiplying SECONDS_PER_YEAR by 1_000 must not overflow u64.
    /// Contracts that reason about multi-year windows (e.g. 200-year bond
    /// maturities) rely on this headroom.
    #[test]
    fn year_times_one_thousand_does_not_overflow_u64() {
        let result = SECONDS_PER_YEAR.checked_mul(1_000);
        assert!(result.is_some(), "1_000 * SECONDS_PER_YEAR overflowed u64");
    }

    /// At the realistic upper bound (~584 years), u64 still has room.
    /// u64::MAX / SECONDS_PER_YEAR ≈ 584,542 years; confirming 500 years fits
    /// ensures all plausible real-world timestamp arithmetic is safe.
    #[test]
    fn year_times_five_hundred_does_not_overflow_u64() {
        let result = SECONDS_PER_YEAR.checked_mul(500);
        assert!(result.is_some(), "500 * SECONDS_PER_YEAR overflowed u64");
    }

    /// Adding SECONDS_PER_WEEK to u64::MAX - SECONDS_PER_WEEK should not
    /// overflow, but adding it to u64::MAX must.  This boundary test locks in
    /// the exact overflow point for callers doing unchecked timestamp arithmetic.
    #[test]
    fn week_addition_at_u64_boundary() {
        let near_max = u64::MAX - SECONDS_PER_WEEK;
        assert!(
            near_max.checked_add(SECONDS_PER_WEEK).is_some(),
            "expected Some when exactly at boundary"
        );
        assert!(
            u64::MAX.checked_add(SECONDS_PER_WEEK).is_none(),
            "expected None when exceeding u64::MAX"
        );
    }

    /// Adding SECONDS_PER_DAY to u64::MAX - SECONDS_PER_DAY should succeed,
    /// but adding it to u64::MAX must overflow.
    #[test]
    fn day_addition_at_u64_boundary() {
        let near_max = u64::MAX - SECONDS_PER_DAY;
        assert!(near_max.checked_add(SECONDS_PER_DAY).is_some());
        assert!(u64::MAX.checked_add(SECONDS_PER_DAY).is_none());
    }

    // ── Modular / divisibility invariants ───────────────────────────────────

    /// Every larger unit must be evenly divisible by every smaller unit it
    /// is composed from.  A remainder would indicate a broken derivation.
    #[test]
    fn larger_units_are_divisible_by_smaller_units() {
        assert_eq!(SECONDS_PER_HOUR % SECONDS_PER_MINUTE, 0);
        assert_eq!(SECONDS_PER_DAY % SECONDS_PER_HOUR, 0);
        assert_eq!(SECONDS_PER_DAY % SECONDS_PER_MINUTE, 0);
        assert_eq!(SECONDS_PER_WEEK % SECONDS_PER_DAY, 0);
        assert_eq!(SECONDS_PER_WEEK % SECONDS_PER_HOUR, 0);
        assert_eq!(SECONDS_PER_WEEK % SECONDS_PER_MINUTE, 0);
        assert_eq!(SECONDS_PER_YEAR % SECONDS_PER_DAY, 0);
        assert_eq!(SECONDS_PER_YEAR % SECONDS_PER_HOUR, 0);
        assert_eq!(SECONDS_PER_YEAR % SECONDS_PER_MINUTE, 0);
    }

    /// Exact quotient checks: dividing a larger unit by a smaller must give
    /// the well-known conversion factor.
    #[test]
    fn exact_quotients_between_units() {
        assert_eq!(SECONDS_PER_HOUR / SECONDS_PER_MINUTE, 60);
        assert_eq!(SECONDS_PER_DAY / SECONDS_PER_HOUR, 24);
        assert_eq!(SECONDS_PER_DAY / SECONDS_PER_MINUTE, 1_440);
        assert_eq!(SECONDS_PER_WEEK / SECONDS_PER_DAY, 7);
        assert_eq!(SECONDS_PER_YEAR / SECONDS_PER_DAY, 365);
    }

    // ── Timestamp arithmetic patterns ────────────────────────────────────────

    /// Adding SECONDS_PER_DAY to a midnight boundary should yield the next
    /// midnight exactly.  This tests how contracts advance a settlement date by
    /// one day.
    #[test]
    fn adding_day_to_midnight_yields_next_midnight() {
        // 2024-01-01 00:00:00 UTC
        let midnight: u64 = 1_704_067_200;
        let next_midnight = midnight + SECONDS_PER_DAY;
        // Confirm the result is still a multiple of SECONDS_PER_DAY.
        assert_eq!(next_midnight % SECONDS_PER_DAY, 0);
        assert_eq!(next_midnight, midnight + 86_400);
    }

    /// A deadline expressed as `start + SECONDS_PER_WEEK` must be exactly 7
    /// days after the start, verifiable via repeated DAY addition.
    #[test]
    fn week_deadline_equals_seven_day_additions() {
        let start: u64 = 1_704_067_200; // 2024-01-01 00:00:00 UTC
        let by_week = start + SECONDS_PER_WEEK;
        let by_days = start + 7 * SECONDS_PER_DAY;
        assert_eq!(by_week, by_days);
    }

    /// A one-year lock-up expressed as `start + SECONDS_PER_YEAR` should equal
    /// the same timestamp obtained by adding 365 individual days, confirming the
    /// constant is usable as a lock-up expiry.
    #[test]
    fn year_lockup_equals_three_hundred_sixty_five_day_additions() {
        let start: u64 = 1_704_067_200;
        let by_year = start + SECONDS_PER_YEAR;
        let by_days = start + 365 * SECONDS_PER_DAY;
        assert_eq!(by_year, by_days);
    }

    /// Elapsed-time recovery: given a start and an end timestamp separated by
    /// exactly one week, the derived elapsed seconds must equal SECONDS_PER_WEEK.
    #[test]
    fn elapsed_time_recovery_one_week() {
        let start: u64 = 1_704_067_200;
        let end: u64 = start + SECONDS_PER_WEEK;
        let elapsed = end - start;
        assert_eq!(elapsed, SECONDS_PER_WEEK);
    }

    /// Elapsed-time recovery across a boundary close to u64::MAX.
    /// Using a start near the upper bound ensures subtraction is safe and
    /// produces the expected duration.
    #[test]
    fn elapsed_time_recovery_near_u64_max() {
        let end: u64 = u64::MAX;
        let start: u64 = u64::MAX - SECONDS_PER_YEAR;
        let elapsed = end - start;
        assert_eq!(elapsed, SECONDS_PER_YEAR);
    }

    /// Decomposing an arbitrary duration in seconds into weeks, days, hours, and
    /// minutes must be lossless (no remainder lost) when the duration is an
    /// exact multiple of SECONDS_PER_MINUTE.
    #[test]
    fn duration_decomposition_is_lossless_for_exact_multiples() {
        // 2 weeks + 3 days + 4 hours + 5 minutes
        let duration: u64 = 2 * SECONDS_PER_WEEK
            + 3 * SECONDS_PER_DAY
            + 4 * SECONDS_PER_HOUR
            + 5 * SECONDS_PER_MINUTE;

        let weeks = duration / SECONDS_PER_WEEK;
        let remainder = duration % SECONDS_PER_WEEK;
        let days = remainder / SECONDS_PER_DAY;
        let remainder = remainder % SECONDS_PER_DAY;
        let hours = remainder / SECONDS_PER_HOUR;
        let remainder = remainder % SECONDS_PER_HOUR;
        let minutes = remainder / SECONDS_PER_MINUTE;
        let seconds = remainder % SECONDS_PER_MINUTE;

        assert_eq!(weeks, 2);
        assert_eq!(days, 3);
        assert_eq!(hours, 4);
        assert_eq!(minutes, 5);
        assert_eq!(seconds, 0, "decomposition must be lossless");
    }

    // ── Zero / identity boundary cases ──────────────────────────────────────

    /// Adding zero seconds to a timestamp must return the same timestamp.
    /// Contracts guard expiry checks with `now >= deadline`; adding 0 must not
    /// accidentally advance the deadline.
    #[test]
    fn adding_zero_seconds_is_identity() {
        let ts: u64 = 1_704_067_200;
        assert_eq!(ts + 0, ts);
    }

    /// Subtracting a constant from itself must always yield zero — relevant for
    /// contracts that compute `deadline - now` to get remaining time.
    #[test]
    fn constant_minus_itself_is_zero() {
        assert_eq!(SECONDS_PER_MINUTE - SECONDS_PER_MINUTE, 0);
        assert_eq!(SECONDS_PER_HOUR - SECONDS_PER_HOUR, 0);
        assert_eq!(SECONDS_PER_DAY - SECONDS_PER_DAY, 0);
        assert_eq!(SECONDS_PER_WEEK - SECONDS_PER_WEEK, 0);
        assert_eq!(SECONDS_PER_YEAR - SECONDS_PER_YEAR, 0);
    }

    /// Multiplying any constant by zero must yield zero — guards against
    /// future refactors that replace the literal `0` with a constant expression.
    #[test]
    fn constant_times_zero_is_zero() {
        assert_eq!(SECONDS_PER_MINUTE * 0, 0);
        assert_eq!(SECONDS_PER_HOUR * 0, 0);
        assert_eq!(SECONDS_PER_DAY * 0, 0);
        assert_eq!(SECONDS_PER_WEEK * 0, 0);
        assert_eq!(SECONDS_PER_YEAR * 0, 0);
    }

    // ── Regression / duplicate-input safety ─────────────────────────────────

    /// Duplicate deadline detection: two separate calls that add the same
    /// constant to the same timestamp must produce identical results, ensuring
    /// there is no hidden mutable state that could cause non-determinism.
    #[test]
    fn duplicate_deadline_computation_is_deterministic() {
        let start: u64 = 1_704_067_200;
        let deadline_a = start + SECONDS_PER_WEEK;
        let deadline_b = start + SECONDS_PER_WEEK;
        assert_eq!(deadline_a, deadline_b);
    }

    /// Constants must be immutable across repeated reads — this is implicit in
    /// Rust's `const`, but asserting it makes the invariant explicit in the test
    /// suite for reviewers.
    #[test]
    fn constants_are_immutable_across_reads() {
        let a = SECONDS_PER_YEAR;
        let b = SECONDS_PER_YEAR;
        assert_eq!(a, b);
    }

    // ── Concurrent / re-entrant safety (single-threaded model) ──────────────

    /// Repeatedly computing the same deadline in a simulated loop must always
    /// yield the same value, confirming no side-effects or counter-based drift.
    #[test]
    fn repeated_deadline_computation_is_stable() {
        let start: u64 = 1_000_000;
        let expected = start + SECONDS_PER_DAY;
        for _ in 0..100 {
            assert_eq!(start + SECONDS_PER_DAY, expected);
        }
    }
}
