use crate::{mul_div_i128, Rounding};
use soroban_sdk::contracttype;

/// A fixed-point interest rate (in basis points).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rate {
    pub bps: u32,
}

impl Rate {
    /// Compound the given rate (in bps) over `periods` periods.
    /// Returns the compounded multiplier scaled by 10_000 (i.e., 10_000 = 1.0x).
    pub fn compound(rate: u32, periods: u32) -> i128 {
        let mut multiplier: i128 = 10_000;
        let factor = 10_000_i128 + rate as i128;
        for _ in 0..periods {
            multiplier = mul_div_i128(
                multiplier,
                factor,
                10_000,
                Rounding::Down,
                "compound overflow",
            );
        }
        multiplier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Zero-rate boundary ───────────────────────────────────────────────────

    /// A zero rate over zero periods is the identity multiplier (1.0x = 10_000).
    #[test]
    fn compound_zero_rate_zero_periods_is_identity() {
        assert_eq!(Rate::compound(0, 0), 10_000);
    }

    /// A zero rate over any number of periods must always return 10_000 because
    /// multiplying by (10_000 + 0) / 10_000 = 1 is a no-op for every iteration.
    #[test]
    fn compound_zero_rate_many_periods_is_identity() {
        for periods in [1u32, 2, 10, 31, 100] {
            assert_eq!(
                Rate::compound(0, periods),
                10_000,
                "compound(0, {periods}) must equal 10_000"
            );
        }
    }

    // ── Zero-period boundary ─────────────────────────────────────────────────

    /// Any rate over zero periods never enters the loop, so the result is
    /// always the initial multiplier 10_000 regardless of bps.
    #[test]
    fn compound_any_rate_zero_periods_is_identity() {
        for bps in [0u32, 1, 100, 1_000, 9_999, 10_000] {
            assert_eq!(
                Rate::compound(bps, 0),
                10_000,
                "compound({bps}, 0) must equal 10_000"
            );
        }
    }

    // ── Single-period exact values ────────────────────────────────────────────

    /// One period at the minimum non-zero rate (1 bps): multiplier advances by
    /// exactly 1 bps of the base (10_000 → 10_001).
    #[test]
    fn compound_one_bps_one_period() {
        assert_eq!(Rate::compound(1, 1), 10_001);
    }

    /// 100 bps (1%) over 1 period: 10_000 * 10_100 / 10_000 = 10_100.
    #[test]
    fn compound_one_percent_one_period() {
        assert_eq!(Rate::compound(100, 1), 10_100);
    }

    /// 10_000 bps (100%) over 1 period: the multiplier doubles (10_000 → 20_000).
    #[test]
    fn compound_full_rate_one_period_doubles() {
        assert_eq!(Rate::compound(10_000, 1), 20_000);
    }

    /// 10_000 bps over 2 periods: 10_000 * 2 * 2 = 40_000 (doubles twice).
    #[test]
    fn compound_full_rate_two_periods_quadruples() {
        assert_eq!(Rate::compound(10_000, 2), 40_000);
    }

    /// 9_999 bps over 1 period: just below 100% rate.
    #[test]
    fn compound_near_full_rate_one_period() {
        assert_eq!(Rate::compound(9_999, 1), 19_999);
    }

    // ── Strict monotonicity (deterministic cases) ────────────────────────────

    /// More periods at the same non-zero rate must yield a strictly greater
    /// multiplier — interest always compounds upward.
    #[test]
    fn compound_more_periods_yields_greater_multiplier() {
        let bps = 100u32;
        let mut prev = Rate::compound(bps, 0);
        for p in 1..=10u32 {
            let curr = Rate::compound(bps, p);
            assert!(
                curr > prev,
                "compound({bps}, {p}) = {curr} should be > compound({bps}, {}) = {prev}",
                p - 1
            );
            prev = curr;
        }
    }

    /// At zero rate, adding more periods must not increase the multiplier —
    /// it remains exactly 10_000 across all period counts.
    #[test]
    fn compound_zero_rate_is_flat_across_periods() {
        let baseline = Rate::compound(0, 0);
        for p in 1..=50u32 {
            assert_eq!(
                Rate::compound(0, p),
                baseline,
                "compound(0, {p}) should equal {baseline}"
            );
        }
    }

    // ── Determinism and duplicate-input safety ───────────────────────────────

    /// Calling compound with identical arguments twice must return the same
    /// value — guards against any hidden mutable state or side-effects.
    #[test]
    fn compound_is_deterministic_for_same_inputs() {
        let cases = [(0u32, 0u32), (100, 5), (1_000, 10), (9_999, 1)];
        for (bps, periods) in cases {
            let a = Rate::compound(bps, periods);
            let b = Rate::compound(bps, periods);
            assert_eq!(a, b, "compound({bps}, {periods}) must be deterministic");
        }
    }

    // ── Known-value regression vectors ───────────────────────────────────────

    /// Regression table of (bps, periods, expected) computed analytically.
    /// Any future change to the compound formula must update these expected
    /// values with a rationale.
    #[test]
    fn compound_regression_vectors() {
        // (bps, periods, expected)
        let vectors: &[(u32, u32, i128)] = &[
            (0, 0, 10_000),
            (0, 1, 10_000),
            (0, 31, 10_000),
            (1, 1, 10_001),
            (100, 1, 10_100),
            (10_000, 1, 20_000),
            (10_000, 2, 40_000),
            (9_999, 1, 19_999),
            // 9_999 bps compounded 113 times — last period that fits in i128
            (9_999, 113, 103_260_831_812_728_121_609_515_865_262_270_659_652),
        ];

        for &(bps, periods, expected) in vectors {
            let result = Rate::compound(bps, periods);
            assert_eq!(
                result, expected,
                "compound({bps}, {periods}) = {result}, expected {expected}"
            );
        }
    }

    // ── Near-overflow boundary: largest safe inputs ──────────────────────────

    /// compound(9_999, 113) is the last call at 9_999 bps that fits in i128.
    /// This confirms the function operates correctly right up to the overflow
    /// boundary without panicking.
    #[test]
    fn compound_near_overflow_boundary_9999_bps_period_113() {
        let result = Rate::compound(9_999, 113);
        // Value is within i128 range
        assert!(result > 0, "result must be positive");
        assert_eq!(result, 103_260_831_812_728_121_609_515_865_262_270_659_652);
    }

    /// compound(5_000, 194) is the last safe period at 50% bps.
    #[test]
    fn compound_near_overflow_boundary_5000_bps_period_194() {
        let result = Rate::compound(5_000, 194);
        assert!(result > 0);
        assert_eq!(result, 145_109_688_122_541_462_685_163_811_928_486_668_505);
    }

    // ── Overflow recovery: compound must panic on overflow ───────────────────

    /// compound(9_999, 114) exceeds i128::MAX — the inner mul_div_i128 must
    /// panic with its overflow sentinel rather than silently wrapping or
    /// returning a negative/incorrect value. This verifies the contract does
    /// not produce silent data corruption on adverse inputs.
    #[test]
    #[should_panic(expected = "compound overflow")]
    fn compound_panics_on_overflow_9999_bps_period_114() {
        let _ = Rate::compound(9_999, 114);
    }

    /// compound(5_000, 195) is the first overflowing call at 50% bps.
    #[test]
    #[should_panic(expected = "compound overflow")]
    fn compound_panics_on_overflow_5000_bps_period_195() {
        let _ = Rate::compound(5_000, 195);
    }

    // ── u32::MAX rate boundary ────────────────────────────────────────────────

    /// u32::MAX bps is a valid (if extreme) input. Period 1 should not
    /// overflow because the intermediate result still fits in i128.
    #[test]
    fn compound_u32_max_bps_single_period() {
        let result = Rate::compound(u32::MAX, 1);
        assert_eq!(result, 4_294_977_295_i128);
    }

    /// u32::MAX bps over 6 periods — still fits in i128.
    #[test]
    fn compound_u32_max_bps_six_periods_fits_i128() {
        let result = Rate::compound(u32::MAX, 6);
        assert_eq!(result, 62_771_894_172_262_314_583_782_748_390_709_427_055_i128);
    }

    /// u32::MAX bps at period 7 overflows i128 — must panic.
    #[test]
    #[should_panic(expected = "compound overflow")]
    fn compound_u32_max_bps_panics_at_period_7() {
        let _ = Rate::compound(u32::MAX, 7);
    }

    // ── Additive decomposition invariant ─────────────────────────────────────

    /// compound(r, n+m) must be consistent with chaining two compounding
    /// windows: compound(r, n+m) == compound(r, n+m) applied in one shot.
    /// This verifies that the loop accumulates without losing precision
    /// compared to a split computation performed in two separate calls and
    /// manually multiplied.
    ///
    /// Specifically: compound(r, a+b) == mul_div(compound(r, a), compound(r, b), 10_000).
    #[test]
    fn compound_additive_period_decomposition() {
        use crate::{mul_div_i128, Rounding};

        let cases: &[(u32, u32, u32)] = &[
            (100, 2, 3),   // 5 periods split as 2+3
            (500, 3, 4),   // 7 periods split as 3+4
            (1_000, 1, 9), // 10 periods split as 1+9
        ];

        for &(bps, a, b) in cases {
            let combined = Rate::compound(bps, a + b);
            let split = mul_div_i128(
                Rate::compound(bps, a),
                Rate::compound(bps, b),
                10_000,
                Rounding::Down,
                "decomposition overflow",
            );
            // Allow a rounding delta of at most 1 due to integer truncation
            // at each intermediate step vs a single combined run.
            let delta = (combined - split).abs();
            assert!(
                delta <= 1,
                "compound({bps}, {a}+{b}) = {combined}, split = {split}, delta = {delta}"
            );
        }
    }

    // ── Idempotency: restarting from a paused multiplier ─────────────────────

    /// Confirm that restarting compounding from the result of a previous call
    /// (simulating a pause/resume) equals the one-shot call over the full
    /// period count. This models recovery after an interrupted accrual.
    #[test]
    fn compound_resume_from_checkpoint_equals_full_run() {
        use crate::{mul_div_i128, Rounding};

        let bps = 200u32;
        let first_half = 5u32;
        let second_half = 5u32;

        // One-shot: 10 periods from the start
        let one_shot = Rate::compound(bps, first_half + second_half);

        // Checkpointed: 5 periods, then continue from that multiplier
        let checkpoint = Rate::compound(bps, first_half);
        let resumed = mul_div_i128(
            checkpoint,
            Rate::compound(bps, second_half),
            10_000,
            Rounding::Down,
            "resume overflow",
        );

        let delta = (one_shot - resumed).abs();
        assert!(
            delta <= 1,
            "one_shot={one_shot}, resumed={resumed}, delta={delta}"
        );
    }

    // ── Rate struct field round-trip ──────────────────────────────────────────

    /// Constructing a Rate and reading its bps field returns the original value.
    #[test]
    fn rate_struct_bps_field_round_trip() {
        for bps in [0u32, 1, 100, 9_999, 10_000, u32::MAX] {
            let rate = Rate { bps };
            assert_eq!(rate.bps, bps, "round-trip failed for bps={bps}");
        }
    }

    /// Rate implements Copy — consuming one binding must not affect a clone.
    #[test]
    fn rate_struct_copy_is_independent() {
        let a = Rate { bps: 500 };
        let b = a; // copy, not move
        assert_eq!(a.bps, b.bps);
    }

    /// Rate implements PartialEq correctly.
    #[test]
    fn rate_struct_equality() {
        assert_eq!(Rate { bps: 100 }, Rate { bps: 100 });
        assert_ne!(Rate { bps: 100 }, Rate { bps: 101 });
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_rate_compound_monotonicity(
            bps in 0..10_000u32,
            period_a in 0..32u32,
            period_b in 0..32u32
        ) {
            let (min_period, max_period) = if period_a <= period_b {
                (period_a, period_b)
            } else {
                (period_b, period_a)
            };

            let val_min = Rate::compound(bps, min_period);
            let val_max = Rate::compound(bps, max_period);

            prop_assert!(val_max >= val_min, "More periods should yield higher or equal compounded rate");
        }
    }

    proptest! {
        /// For any zero rate, compound must always return 10_000 regardless
        /// of the period count (property counterpart of the unit tests above).
        #[test]
        fn prop_zero_rate_is_always_identity(periods in 0..100u32) {
            prop_assert_eq!(Rate::compound(0, periods), 10_000);
        }
    }

    proptest! {
        /// For any period count, compound over 0 periods is 10_000.
        #[test]
        fn prop_zero_periods_is_always_identity(bps in 0..10_000u32) {
            prop_assert_eq!(Rate::compound(bps, 0), 10_000);
        }
    }

    proptest! {
        /// The result of compound is always strictly positive for any valid
        /// non-overflowing input.
        #[test]
        fn prop_compound_result_always_positive(
            bps in 0..1_000u32,
            periods in 0..30u32,
        ) {
            let result = Rate::compound(bps, periods);
            prop_assert!(result > 0, "compound({bps}, {periods}) = {result} must be > 0");
        }
    }

    proptest! {
        /// compound(r, n) >= 10_000 for all valid inputs — the multiplier can
        /// never fall below the base (no negative interest).
        #[test]
        fn prop_compound_never_below_identity(
            bps in 0..1_000u32,
            periods in 0..30u32,
        ) {
            let result = Rate::compound(bps, periods);
            prop_assert!(
                result >= 10_000,
                "compound({bps}, {periods}) = {result} should be >= 10_000"
            );
        }
    }
}
