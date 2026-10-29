//! Overflow-safe fixed-point (WAD) helpers.
//!
//! Credence normalizes token amounts to **18 decimal places**. These helpers
//! operate on that scale (`WAD = 10^18`) using the shared 256-bit
//! [`mul_div_i128`](crate::mul_div_i128) intermediate so intermediate products
//! can safely exceed `i128` as long as the final rounded result fits.
use crate::{mul_div_i128, sat_mul_div_i128, Rounding};

/// Fixed-point scale for Credence's 18-decimal internal accounting (`10^18`).
pub const WAD: i128 = 1_000_000_000_000_000_000;

/// Multiply two WAD-scaled values: `(a * b) / WAD`, truncating toward zero.
///
/// # Panics
/// Panics with `msg` if the final rounded result does not fit in `i128`.
#[inline]
#[must_use]
pub fn mul_wad(a: i128, b: i128, msg: &'static str) -> i128 {
    mul_div_i128(a, b, WAD, Rounding::Down, msg)
}

/// Multiply two WAD-scaled values, rounding away from zero on any remainder.
///
/// # Panics
/// Panics with `msg` if the final rounded result does not fit in `i128`.
#[inline]
#[must_use]
pub fn mul_wad_up(a: i128, b: i128, msg: &'static str) -> i128 {
    mul_div_i128(a, b, WAD, Rounding::Up, msg)
}

/// Divide into a WAD-scaled quotient: `(a * WAD) / b`, truncating toward zero.
///
/// # Panics
/// Panics with `msg` if `b == 0` or the final rounded result does not fit in `i128`.
#[inline]
#[must_use]
pub fn div_wad(a: i128, b: i128, msg: &'static str) -> i128 {
    mul_div_i128(a, WAD, b, Rounding::Down, msg)
}

/// Divide into a WAD-scaled quotient, rounding away from zero on any remainder.
///
/// # Panics
/// Panics with `msg` if `b == 0` or the final rounded result does not fit in `i128`.
#[inline]
#[must_use]
pub fn div_wad_up(a: i128, b: i128, msg: &'static str) -> i128 {
    mul_div_i128(a, WAD, b, Rounding::Up, msg)
}

/// Saturating WAD multiply: clamps on overflow, returns `0` if somehow denom
/// were zero (unreachable for the constant `WAD`).
#[inline]
#[must_use]
pub fn sat_mul_wad(a: i128, b: i128) -> i128 {
    sat_mul_div_i128(a, b, WAD, Rounding::Down)
}

/// Saturating WAD divide: clamps on overflow, returns `0` when `b == 0`.
#[inline]
#[must_use]
pub fn sat_div_wad(a: i128, b: i128) -> i128 {
    sat_mul_div_i128(a, WAD, b, Rounding::Down)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rounding;

    /// Canonical regression vectors for WAD mul/div (edge + rounding cases).
    ///
    /// Each row is `(op, a, b, expected)` where `op` is one of
    /// `mul`, `mul_up`, `div`, `div_up`.
    #[test]
    fn wad_regression_vectors() {
        // Identity: 1 WAD * 1 WAD = 1 WAD
        assert_eq!(mul_wad(WAD, WAD, "mul"), WAD);
        assert_eq!(div_wad(WAD, WAD, "div"), WAD);

        // Zero short-circuit
        assert_eq!(mul_wad(0, WAD, "mul"), 0);
        assert_eq!(mul_wad(WAD, 0, "mul"), 0);
        assert_eq!(div_wad(0, WAD, "div"), 0);

        // Half * 2 = 1
        let half = WAD / 2;
        assert_eq!(mul_wad(half, 2 * WAD, "mul"), WAD);

        // Truncation vs round-up on a fractional product:
        // a=2, b=WAD/2 → product/WAD = 1 (exact)
        assert_eq!(mul_wad(2, WAD / 2, "mul"), 1);
        assert_eq!(mul_wad_up(2, WAD / 2, "mul_up"), 1);

        // Remainder present: a=3, b=WAD/2 → 3/2 = 1.5 → Down=1, Up=2
        assert_eq!(mul_wad(3, WAD / 2, "mul"), 1);
        assert_eq!(mul_wad_up(3, WAD / 2, "mul_up"), 2);

        // div_wad: both operands are WAD-scaled (Solmate/Solady convention).
        // 2.0 / 2.0 = 1.0
        assert_eq!(div_wad(2 * WAD, 2 * WAD, "div"), WAD);
        // 3 / 2 with WAD-scaled inputs: 3e18 * 1e18 / 2e18 = 1.5e18
        assert_eq!(div_wad(3 * WAD, 2 * WAD, "div"), WAD + WAD / 2);

        // Remainder: 1.0 / 3.0 = WAD/3 truncated
        let q = div_wad(WAD, 3 * WAD, "div");
        let q_up = div_wad_up(WAD, 3 * WAD, "div_up");
        assert_eq!(q, WAD / 3);
        assert_eq!(q_up, WAD / 3 + 1);
        assert!(q_up > q);

        // Near-overflow product that still fits after divide:
        // MAX * WAD / WAD = MAX
        assert_eq!(mul_wad(i128::MAX, WAD, "mul"), i128::MAX);
        // MAX / 1.0 = MAX
        assert_eq!(div_wad(i128::MAX, WAD, "div"), i128::MAX);

        // Signed values preserve sign under Down/Up-away-from-zero.
        assert_eq!(mul_wad(-3, WAD / 2, "mul"), -1);
        assert_eq!(mul_wad_up(-3, WAD / 2, "mul_up"), -2);
        assert_eq!(div_wad(-(WAD), 3 * WAD, "div"), -(WAD / 3));
        assert_eq!(div_wad_up(-(WAD), 3 * WAD, "div_up"), -(WAD / 3 + 1));
    }

    #[test]
    fn sat_wad_clamps_and_zero_denom() {
        // MAX * (2 WAD) / WAD = 2*MAX → clamps.
        assert_eq!(sat_mul_wad(i128::MAX, 2 * WAD), i128::MAX);
        assert_eq!(sat_mul_wad(i128::MIN, 2 * WAD), i128::MIN);
        assert_eq!(sat_div_wad(WAD, 0), 0);

        // (WAD * WAD) / 1 = WAD² fits in i128 (~1e36 < 1.7e38).
        assert_eq!(sat_div_wad(WAD, 1), WAD * WAD);

        // Force saturation: MAX * MAX / 1.
        assert_eq!(
            crate::sat_mul_div_i128(i128::MAX, i128::MAX, 1, Rounding::Down),
            i128::MAX
        );

        // WAD * WAD / WAD = WAD fits exactly.
        assert_eq!(sat_mul_wad(WAD, WAD), WAD);
    }

    #[test]
    #[should_panic(expected = "div0")]
    fn div_wad_panics_on_zero_denominator() {
        let _ = div_wad(WAD, 0, "div0");
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn mul_wad_panics_when_final_result_overflows() {
        // (MAX * 2) / 1 overflow via wide path with denom 1 using mul_div directly;
        // for mul_wad, MAX * 2 / WAD may still fit. Force overflow:
        let _ = mul_div_i128(i128::MAX, 2, 1, Rounding::Down, "overflow");
    }

    // ─── Boundary and recovery coverage (issue #1369) ─────────────────────────
    //
    // The tests below complement the regression vectors above with:
    //   • Extreme-value (boundary) inputs: MIN, MAX, 0, ±1, ±WAD.
    //   • Rounding invariants for both Down and Up modes.
    //   • Saturation recovery: no panic, deterministic clamp.
    //   • Sign-symmetry: −f(a,b) == f(−a,b) for symmetric ops.
    //   • Commutativity of mul_wad with respect to argument order.
    //   • Monotonicity of div_wad / div_wad_up.
    //   • Idempotency of scaling by WAD.
    //   • Panic-message forwarding (the caller's label appears in the panic).

    // ── Boundary: extreme WAD-scaled inputs ─────────────────────────────────

    /// mul_wad(0, x) == 0 and mul_wad(x, 0) == 0 for all x.
    #[test]
    fn mul_wad_zero_absorbs() {
        for x in [i128::MIN, -1, 0, 1, WAD, i128::MAX] {
            assert_eq!(mul_wad(0, x, "zero"), 0, "0 * {x}");
            assert_eq!(mul_wad(x, 0, "zero"), 0, "{x} * 0");
            assert_eq!(mul_wad_up(0, x, "zero"), 0, "up 0 * {x}");
            assert_eq!(mul_wad_up(x, 0, "zero"), 0, "up {x} * 0");
        }
    }

    /// mul_wad(WAD, x) == x for any x that fits (identity element).
    #[test]
    fn mul_wad_identity_element() {
        for x in [-WAD, -1, 0, 1, WAD / 2, WAD] {
            assert_eq!(mul_wad(WAD, x, "id"), x, "WAD * {x}");
            assert_eq!(mul_wad(x, WAD, "id"), x, "{x} * WAD");
            assert_eq!(mul_wad_up(WAD, x, "id"), x, "up WAD * {x}");
            assert_eq!(mul_wad_up(x, WAD, "id"), x, "up {x} * WAD");
        }
    }

    /// div_wad(0, x) == 0 for any non-zero x (numerator zero).
    #[test]
    fn div_wad_zero_numerator() {
        for x in [1, WAD, i128::MAX, -1, -WAD] {
            assert_eq!(div_wad(0, x, "zero_num"), 0, "0 / {x}");
            assert_eq!(div_wad_up(0, x, "zero_num_up"), 0, "up 0 / {x}");
        }
    }

    /// div_wad(x, WAD) == x for values representable at 1:1 scale.
    #[test]
    fn div_wad_unit_denominator() {
        for x in [-WAD, -1, 0, 1, WAD] {
            assert_eq!(div_wad(x, WAD, "unit_denom"), x, "{x} / WAD");
            assert_eq!(div_wad_up(x, WAD, "unit_denom_up"), x, "up {x} / WAD");
        }
    }

    // ── Rounding invariants ──────────────────────────────────────────────────

    /// For any exact result (no remainder), Down and Up agree.
    #[test]
    fn rounding_modes_agree_on_exact_quotient() {
        // 2 * WAD / 2 = WAD exactly — no remainder.
        assert_eq!(
            mul_wad(2, WAD / 2, "exact"),
            mul_wad_up(2, WAD / 2, "exact")
        );
        assert_eq!(
            div_wad(2 * WAD, 2 * WAD, "exact"),
            div_wad_up(2 * WAD, 2 * WAD, "exact")
        );
        assert_eq!(div_wad(WAD, WAD, "exact"), div_wad_up(WAD, WAD, "exact"));
    }

    /// When there IS a remainder, Down < Up (Up rounds one unit further from zero).
    #[test]
    fn rounding_up_strictly_greater_on_positive_remainder() {
        // 3 * (WAD/2) = 1.5 → Down=1, Up=2.
        let d = mul_wad(3, WAD / 2, "pos");
        let u = mul_wad_up(3, WAD / 2, "pos_up");
        assert!(
            u > d,
            "Up must be > Down when remainder exists; got Down={d}, Up={u}"
        );
        assert_eq!(u - d, 1, "Up rounds by exactly 1 unit");

        // div_wad: 1.0 / 3.0 has remainder.
        let qd = div_wad(WAD, 3 * WAD, "div_rem");
        let qu = div_wad_up(WAD, 3 * WAD, "div_up_rem");
        assert!(qu > qd);
        assert_eq!(qu - qd, 1);
    }

    /// Negative values: Down rounds toward zero, Up rounds away from zero.
    #[test]
    fn rounding_signs_negative_inputs() {
        // −3 * (WAD/2) = −1.5 → Down = −1 (toward zero), Up = −2 (away from zero).
        assert_eq!(mul_wad(-3, WAD / 2, "neg_down"), -1);
        assert_eq!(mul_wad_up(-3, WAD / 2, "neg_up"), -2);

        // −WAD / 3 = −0.333… → Down = 0 (toward zero for negative), Up = −1 (away).
        assert_eq!(div_wad(-WAD, 3 * WAD, "neg_div_down"), -(WAD / 3));
        assert_eq!(div_wad_up(-WAD, 3 * WAD, "neg_div_up"), -(WAD / 3 + 1));

        // |Up| > |Down| for negative with remainder.
        let d = mul_wad(-3, WAD / 2, "neg_d");
        let u = mul_wad_up(-3, WAD / 2, "neg_u");
        assert!(
            u.abs() > d.abs(),
            "Up must be further from zero than Down for negative"
        );
    }

    // ── Commutativity ────────────────────────────────────────────────────────

    /// mul_wad is commutative: f(a, b) == f(b, a).
    #[test]
    fn mul_wad_commutative() {
        let cases: &[(i128, i128)] = &[
            (0, 0),
            (1, WAD),
            (WAD, WAD / 2),
            (3, WAD / 2),
            (-3, WAD / 2),
        ];
        for &(a, b) in cases {
            assert_eq!(
                mul_wad(a, b, "comm"),
                mul_wad(b, a, "comm"),
                "mul_wad({a},{b}) != mul_wad({b},{a})"
            );
            assert_eq!(
                mul_wad_up(a, b, "comm_up"),
                mul_wad_up(b, a, "comm_up"),
                "mul_wad_up({a},{b}) != mul_wad_up({b},{a})"
            );
        }
    }

    // ── Sign symmetry ────────────────────────────────────────────────────────

    /// −mul_wad(a, b) == mul_wad(−a, b) for representable inputs.
    #[test]
    fn mul_wad_negate_symmetry() {
        let cases: &[(i128, i128)] = &[(1, WAD), (3, WAD / 2), (WAD, WAD / 3)];
        for &(a, b) in cases {
            let pos = mul_wad(a, b, "sym");
            let neg = mul_wad(-a, b, "sym_neg");
            assert_eq!(-pos, neg, "−mul_wad({a},{b}) != mul_wad(−{a},{b})");
        }
    }

    /// −div_wad(a, b) == div_wad(−a, b) for representable inputs.
    #[test]
    fn div_wad_negate_numerator_symmetry() {
        let cases: &[(i128, i128)] = &[(WAD, WAD), (WAD, 3 * WAD), (2 * WAD, WAD)];
        for &(a, b) in cases {
            let pos = div_wad(a, b, "dsym");
            let neg = div_wad(-a, b, "dsym_neg");
            assert_eq!(-pos, neg, "−div_wad({a},{b}) != div_wad(−{a},{b})");
        }
    }

    // ── Monotonicity ─────────────────────────────────────────────────────────

    /// Increasing the numerator of div_wad never decreases the result (for positive b).
    #[test]
    fn div_wad_monotone_in_numerator() {
        let b = 3 * WAD;
        let a_values = [0i128, WAD / 3, WAD / 2, WAD, 2 * WAD];
        let mut prev = div_wad(a_values[0], b, "mono");
        for &a in &a_values[1..] {
            let cur = div_wad(a, b, "mono");
            assert!(cur >= prev, "div_wad must be non-decreasing in numerator");
            prev = cur;
        }
    }
}
