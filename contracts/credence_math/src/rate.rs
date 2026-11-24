use credence_errors::ContractError;
use ethnum::U256;
use soroban_sdk::contracttype;

/// Fixed-point denominator: `10_000` represents `1.0x`.
const BPS_DENOMINATOR: i128 = 10_000;

/// Maximum number of periods accepted by [`Rate::try_compound`].
///
/// This is a denial-of-service guard: `u32::MAX` periods must be rejected
/// before any multiplication runs, otherwise a caller can burn the whole
/// instruction budget.
pub const MAX_COMPOUND_PERIODS: u32 = 10_000;

/// Horizons at or below this value keep the historical per-period truncation.
///
/// The legacy loop truncates after every period, and a single-rounding power
/// does not reproduce that sequence exactly. Realistic accrual horizons (500
/// periods is over a year of daily compounding) therefore stay on the exact
/// legacy path, while longer horizons use exponentiation by squaring.
const LEGACY_ITERATIVE_PERIODS: u32 = 500;

/// Compute `floor(a * b / 10_000)` for non-negative `a` and `b`.
///
/// The 256-bit intermediate keeps `a * b` exact even when it exceeds `u128`,
/// so the only failure mode is a final quotient that does not fit in `i128`.
fn mul_div_bps_down(a: i128, b: i128) -> Result<i128, ContractError> {
    let product = U256::new(a as u128) * U256::new(b as u128);
    let quotient = product / U256::new(BPS_DENOMINATOR as u128);
    if quotient > U256::new(i128::MAX as u128) {
        return Err(ContractError::Overflow);
    }
    Ok(quotient.as_u128() as i128)
}

/// Exact legacy recurrence: truncate after every period.
fn compound_iterative(rate: u32, periods: u32) -> Result<i128, ContractError> {
    let factor = BPS_DENOMINATOR + rate as i128;
    let mut multiplier = BPS_DENOMINATOR;
    for _ in 0..periods {
        multiplier = mul_div_bps_down(multiplier, factor)?;
    }
    Ok(multiplier)
}

/// Exponentiation by squaring over the compounding combine operation.
fn compound_by_squaring(rate: u32, periods: u32) -> Result<i128, ContractError> {
    let factor = BPS_DENOMINATOR + rate as i128;
    let mut multiplier = BPS_DENOMINATOR;
    let mut power = factor;
    let mut remaining = periods;

    while remaining > 0 {
        if remaining & 1 == 1 {
            multiplier = mul_div_bps_down(multiplier, power)?;
        }
        remaining >>= 1;
        if remaining > 0 {
            power = mul_div_bps_down(power, power)?;
        }
    }

    Ok(multiplier)
}

/// A fixed-point interest rate (in basis points).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rate {
    pub bps: u32,
}

impl Rate {
    /// Compound the given rate (in bps) over `periods` periods.
    ///
    /// Returns the compounded multiplier scaled by 10_000 (i.e., 10_000 =
    /// 1.0x). Rejects `periods > MAX_COMPOUND_PERIODS` with
    /// [`ContractError::InvalidPeriodCount`] before doing any arithmetic, and
    /// returns [`ContractError::Overflow`] when the multiplier does not fit in
    /// `i128`.
    pub fn try_compound(rate: u32, periods: u32) -> Result<i128, ContractError> {
        if periods > MAX_COMPOUND_PERIODS {
            return Err(ContractError::InvalidPeriodCount);
        }
        if periods <= LEGACY_ITERATIVE_PERIODS {
            compound_iterative(rate, periods)
        } else {
            compound_by_squaring(rate, periods)
        }
    }

    /// Panicking compatibility wrapper around [`Self::try_compound`].
    ///
    /// # Panics
    ///
    /// Panics with `"compound period cap exceeded"` when `periods` exceeds
    /// [`MAX_COMPOUND_PERIODS`], or `"compound overflow"` when the multiplier
    /// does not fit in `i128`.
    pub fn compound(rate: u32, periods: u32) -> i128 {
        let message = match Self::try_compound(rate, periods) {
            Ok(value) => return value,
            Err(ContractError::InvalidPeriodCount) => "compound period cap exceeded",
            Err(_) => "compound overflow",
        };
        let absent: Option<i128> = None;
        absent.expect(message)
        assert!(
            periods <= MAX_COMPOUND_PERIODS,
            "compound period cap exceeded"
        );
        Self::try_compound(rate, periods).expect("compound overflow")
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
            (
                9_999,
                113,
                103_260_831_812_728_121_609_515_865_262_270_659_652,
            ),
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
        assert_eq!(
            result,
            62_771_894_172_262_314_583_782_748_390_709_427_055_i128
        );
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
    /// (simulating a pause/resume) stays within a bounded delta of the one-shot
    /// call. The legacy recurrence truncates every period, so splitting a run
    /// into two windows can differ by one unit per checkpoint; it is not
    /// exactly associative.
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
            delta <= 2,
            "one_shot={one_shot}, resumed={resumed}, delta={delta}"
        );
    }

    // ── Period cap and typed errors ───────────────────────────────────────────

    #[test]
    fn try_compound_rejects_periods_above_cap() {
        assert_eq!(
            Rate::try_compound(0, MAX_COMPOUND_PERIODS + 1),
            Err(ContractError::InvalidPeriodCount)
        );
        assert_eq!(
            Rate::try_compound(u32::MAX, u32::MAX),
            Err(ContractError::InvalidPeriodCount),
            "the cap must be checked before any arithmetic"
        );
    }

    #[test]
    #[should_panic(expected = "compound period cap exceeded")]
    fn compound_panics_above_period_cap() {
        let _ = Rate::compound(0, MAX_COMPOUND_PERIODS + 1);
    }

    #[test]
    fn try_compound_returns_typed_overflow() {
        assert_eq!(Rate::try_compound(9_999, 114), Err(ContractError::Overflow));
        assert_eq!(
            Rate::try_compound(u32::MAX, 7),
            Err(ContractError::Overflow)
        );
    }

    // ── 10_000 bps overflow boundary ─────────────────────────────────────────
    //
    // At 10_000 bps (100% per period), the factor is 20_000 and the multiplier
    // doubles every period: 10_000 × 2^n / 10_000^(n-1).  The last period that
    // fits in i128 is 113; period 114 produces a value that exceeds i128::MAX
    // (170_141_183_460_469_231_731_687_303_715_884_105_727).
    //
    // First overflowing period for rate = 10_000 bps: **114**.

    /// `compound(10_000, 0)` is the identity regardless of rate.
    #[test]
    fn compound_10000_bps_zero_periods_is_identity() {
        assert_eq!(Rate::compound(10_000, 0), 10_000);
    }

    /// `compound(10_000, 113)` is the last call at 100% bps that fits in i128.
    #[test]
    fn compound_10000_bps_period_113_last_safe() {
        let result = Rate::compound(10_000, 113);
        assert!(result > 0);
        assert_eq!(result, 103_845_937_170_696_552_570_609_926_584_401_920_000);
    }

    /// `compound(10_000, 114)` is the first call at 100% bps that overflows i128.
    #[test]
    #[should_panic(expected = "compound overflow")]
    fn compound_10000_bps_period_114_overflows() {
        let _ = Rate::compound(10_000, 114);
    }

    /// `try_compound(10_000, 114)` returns the typed overflow instead of panicking.
    #[test]
    fn try_compound_10000_bps_period_114_returns_typed_overflow() {
        assert_eq!(
            Rate::try_compound(10_000, 114),
            Err(ContractError::Overflow)
        );
    }

    // ── Exponentiation by squaring ───────────────────────────────────────────

    #[test]
    fn try_compound_squaring_path_pins_known_values() {
        assert_eq!(Rate::try_compound(0, MAX_COMPOUND_PERIODS), Ok(10_000));
        assert_eq!(Rate::try_compound(1, MAX_COMPOUND_PERIODS), Ok(26_902));
        assert_eq!(Rate::try_compound(1, 1_000), Ok(11_040));
        assert_eq!(Rate::try_compound(10, 1_000), Ok(26_890));
        assert_eq!(Rate::try_compound(100, 501), Ok(1_454_428));
        assert_eq!(Rate::compound(1, 1_000), 11_040);
    }

    #[test]
    fn try_compound_squaring_path_returns_typed_overflow() {
        assert_eq!(Rate::try_compound(9_999, 501), Err(ContractError::Overflow));
    }

    /// Differential test: for every period count up to the legacy horizon, the
    /// implementation must match an independent U256 oracle that truncates once
    /// per period. Once the oracle exceeds `i128::MAX`, the implementation must
    /// return the typed overflow instead of panicking.
    #[test]
    fn try_compound_matches_legacy_loop_for_periods_0_to_500() {
        const RATES: &[u32] = &[
            0,
            1,
            2,
            5,
            10,
            50,
            100,
            500,
            1_000,
            2_000,
            5_000,
            9_999,
            10_000,
            u32::MAX,
        ];

        for &rate in RATES {
            let factor = U256::new(10_000u128 + rate as u128);
            let mut oracle = U256::new(10_000);

            for periods in 0..=LEGACY_ITERATIVE_PERIODS {
                if oracle > U256::new(i128::MAX as u128) {
                    assert_eq!(
                        Rate::try_compound(rate, periods),
                        Err(ContractError::Overflow),
                        "compound({rate}, {periods}) must report overflow"
                    );
                    break;
                }

                assert_eq!(
                    Rate::try_compound(rate, periods),
                    Ok(oracle.as_u128() as i128),
                    "compound({rate}, {periods}) drifted from the legacy loop"
                );

                oracle = oracle * factor / U256::new(10_000);
            }
        }
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
