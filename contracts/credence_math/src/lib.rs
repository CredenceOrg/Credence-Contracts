#![no_std]
#![deny(clippy::float_arithmetic)]
#![allow(
    deprecated,
    unused_imports,
    unused_variables,
    dead_code,
    unused_assignments,
    unused_mut,
    mismatched_lifetime_syntaxes,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::restriction
)]
// Must come AFTER `#![allow(clippy::restriction, ...)]` above: the
// `clippy::disallowed_macros` lint belongs to the `restriction` group, so
// a later allow would re-silence it. cargo build --release / WASM build
// is the only mode where this deny fires (tests + the testutils feature
// stay free to use format!/write! for diagnostics).
#![cfg_attr(not(any(test, feature = "testutils")), deny(clippy::disallowed_macros))]

use credence_errors::ContractError;
use ethnum::U256;
use soroban_sdk;

/// Fixed-point denominator for basis-point calculations.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Rounding behavior for [`mul_div_i128`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rounding {
    /// Truncate the fractional remainder toward zero.
    Down,
    /// Round away from zero when the division leaves any remainder.
    Up,
    /// Round to the nearest integer, with exact half-way cases rounded away from zero.
    Nearest,
}

/// Checked `u64` multiplication with a stable panic message.
#[inline]
#[must_use]
pub fn mul_u64(a: u64, b: u64, msg: &'static str) -> u64 {
    a.checked_mul(b).unwrap_or_else(|| panic!("{msg}"))
}

/// Checked `i128` addition with a stable panic message.
#[inline]
#[must_use]
pub fn add_i128(a: i128, b: i128, msg: &'static str) -> i128 {
    a.checked_add(b).unwrap_or_else(|| panic!("{msg}"))
}

/// Checked `i128` subtraction with a stable panic message.
#[inline]
#[must_use]
pub fn sub_i128(a: i128, b: i128, msg: &'static str) -> i128 {
    a.checked_sub(b).unwrap_or_else(|| panic!("{msg}"))
}

/// Checked `i128` multiplication with a stable panic message.
#[inline]
#[must_use]
pub fn mul_i128(a: i128, b: i128, msg: &'static str) -> i128 {
    a.checked_mul(b).unwrap_or_else(|| panic!("{msg}"))
}

/// Checked `i128` division with a stable panic message.
#[inline]
#[must_use]
pub fn div_i128(a: i128, b: i128, msg: &'static str) -> i128 {
    a.checked_div(b).unwrap_or_else(|| panic!("{msg}"))
}

/// Checked `i128` ceiling division with a stable panic message.
/// Computes ceil(a / b) for b > 0, a >= 0.
///
/// # Panics
/// Panics with `msg` on `b == 0` (via the inner `checked_add(b - 1)` /
/// `checked_div`). Prefer [`ceil_div_checked_i128`] on hot paths where
/// `b == 0` is reachable so callers receive a typed
/// [`ContractError::DivisionByZero`] instead of a string panic.
#[inline]
#[must_use]
pub fn ceil_div_i128(a: i128, b: i128, msg: &'static str) -> i128 {
    a.checked_add(b - 1).expect(msg).checked_div(b).expect(msg)
}

/// Checked `i128` division returning a typed error instead of panicking.
///
/// Returns [`ContractError::DivisionByZero`] when `b == 0`, and
/// [`ContractError::Overflow`] for the single overflowing case
/// `i128::MIN / -1`. Otherwise returns `a / b` (truncated toward zero,
/// matching Rust integer division).
///
/// Prefer this over [`div_i128`] on paths where a zero denominator is a
/// reachable runtime state (e.g. a fully-slashed bond) so the fault maps to
/// a wire-stable Arithmetic error code rather than a free-form panic string.
///
/// # Examples
///
/// ```
/// use credence_math::div_checked_i128;
/// use credence_errors::ContractError;
///
/// assert_eq!(div_checked_i128(10, 3), Ok(3));
/// assert_eq!(div_checked_i128(7, 0), Err(ContractError::DivisionByZero));
/// ```
#[inline]
pub fn div_checked_i128(a: i128, b: i128) -> Result<i128, ContractError> {
    if b == 0 {
        return Err(ContractError::DivisionByZero);
    }
    a.checked_div(b).ok_or(ContractError::Overflow)
}

/// Checked `i128` ceiling division returning a typed error instead of panicking.
///
/// Computes `ceil(a / b)` for `b > 0`, `a >= 0`. The `b == 0` case is rejected
/// **before** the `b - 1` subtraction so a zero denominator surfaces as
/// [`ContractError::DivisionByZero`] rather than being masked as an
/// [`ContractError::Overflow`] from the subtraction. Returns
/// [`ContractError::Overflow`] if the intermediate `a + (b - 1)` overflows.
///
/// This is the typed counterpart to [`ceil_div_i128`] used on the slash-percentage
/// hot path `ceil(slashed * 10_000 / bonded)`, where `bonded == 0` is reachable
/// for a fully-slashed bond.
///
/// # Examples
///
/// ```
/// use credence_math::ceil_div_checked_i128;
/// use credence_errors::ContractError;
///
/// // bonded = 3, slashed = 2: ceil(2 * 10_000 / 3) = 6667
/// assert_eq!(ceil_div_checked_i128(2 * 10_000, 3), Ok(6667));
/// assert_eq!(ceil_div_checked_i128(10, 5), Ok(2));
/// assert_eq!(ceil_div_checked_i128(0, 5), Ok(0));
/// // b == 0 is rejected before `b - 1`, so it is DivisionByZero, not Overflow.
/// assert_eq!(ceil_div_checked_i128(5, 0), Err(ContractError::DivisionByZero));
/// ```
#[inline]
pub fn ceil_div_checked_i128(a: i128, b: i128) -> Result<i128, ContractError> {
    if b == 0 {
        return Err(ContractError::DivisionByZero);
    }
    a.checked_add(b - 1)
        .ok_or(ContractError::Overflow)?
        .checked_div(b)
        .ok_or(ContractError::Overflow)
}

/// Compute `a * b / denom` over a 256-bit intermediate.
///
/// The intermediate product is widened before division, so large products that
/// exceed `i128` can still succeed when the final rounded result fits in
/// `i128`. `Rounding::Down` matches Rust integer division by truncating toward
/// zero. `Rounding::Up` rounds away from zero on any remainder.
/// `Rounding::Nearest` rounds to the nearest integer, with half-way cases
/// rounded away from zero.
///
/// # Panics
///
/// Panics with `msg` if `denom` is zero or if the final rounded result does not
/// fit in `i128`.
///
/// # Examples
///
/// ```
/// use credence_math::{mul_div_i128, Rounding};
///
/// assert_eq!(mul_div_i128(i128::MAX, 10_000, 10_000, Rounding::Down, "overflow"), i128::MAX);
/// assert_eq!(mul_div_i128(10, 3, 4, Rounding::Down, "overflow"), 7);
/// assert_eq!(mul_div_i128(10, 3, 4, Rounding::Up, "overflow"), 8);
/// assert_eq!(mul_div_i128(10, 3, 4, Rounding::Nearest, "overflow"), 8);
/// assert_eq!(mul_div_i128(-10, 3, 4, Rounding::Up, "overflow"), -8);
/// ```
#[inline]
#[must_use]

/// Checked `i128` addition returning a typed error instead of panicking.
#[inline]
pub fn checked_add_or_error(a: i128, b: i128) -> Result<i128, ContractError> {
    a.checked_add(b).ok_or(ContractError::Overflow)
}

#[inline]
#[must_use]
pub fn mul_div_i128(a: i128, b: i128, denom: i128, mode: Rounding, msg: &'static str) -> i128 {
    if denom == 0 {
        Option::<()>::None.expect(msg);
    }

    let negative = (a < 0) ^ (b < 0) ^ (denom < 0);
    let numerator = U256::new(a.unsigned_abs()) * U256::new(b.unsigned_abs());
    let divisor = U256::new(denom.unsigned_abs());
    let quotient = numerator / divisor;
    let remainder = numerator % divisor;

    let rounded = match mode {
        Rounding::Down => quotient,
        Rounding::Up => {
            if remainder == U256::ZERO {
                quotient
            } else {
                quotient + U256::ONE
            }
        }
        Rounding::Nearest => {
            if remainder * U256::new(2) >= divisor {
                quotient + U256::ONE
            } else {
                quotient
            }
        }
    };

    let positive_limit = U256::new(i128::MAX as u128);
    let negative_limit = U256::new((i128::MAX as u128) + 1);
    if negative {
        if rounded > negative_limit {
            Option::<()>::None.expect(msg);
        }
        if rounded == negative_limit {
            i128::MIN
        } else {
            -i128::try_from(rounded.as_u128()).expect(msg)
        }
    } else {
        if rounded > positive_limit {
            Option::<()>::None.expect(msg);
        }
        i128::try_from(rounded.as_u128()).expect(msg)
    }
}

/// Calculate a basis-point percentage of an `i128` amount: `amount * bps / BPS_DENOMINATOR`.
#[inline]
#[must_use]
pub fn bps(amount: i128, bps: u32, mul_msg: &'static str, div_msg: &'static str) -> i128 {
    let numerator = mul_i128(amount, bps as i128, mul_msg);
    div_i128(numerator, BPS_DENOMINATOR, div_msg)
}

/// Saturated basis-point multiplication: `amount * bps / BPS_DENOMINATOR`.
///
/// Uses [`mul_div_i128`] so `amount * bps` cannot overflow before division.
#[inline]
#[must_use]
pub fn sat_mul_bps(amount: i128, bps_value: u32) -> i128 {
    mul_div_i128(
        amount,
        bps_value as i128,
        BPS_DENOMINATOR,
        Rounding::Down,
        "sat_mul_bps overflow",
    )
}

/// Calculate a basis-point percentage of an `i128` amount, rounded away from zero.
///
/// Uses [`mul_div_i128`] so `amount * bps` cannot overflow before division.
///
/// # Examples
///
/// ```
/// use credence_math::bps_round_up;
///
/// assert_eq!(bps_round_up(10_001, 1, "overflow"), 2);
/// assert_eq!(bps_round_up(10_000, 1, "overflow"), 1);
/// assert_eq!(bps_round_up(-10_001, 1, "overflow"), -2);
/// ```
#[inline]
#[must_use]
pub fn bps_round_up(amount: i128, bps_value: u32, msg: &'static str) -> i128 {
    mul_div_i128(
        amount,
        bps_value as i128,
        BPS_DENOMINATOR,
        Rounding::Up,
        msg,
    )
}

/// Calculate a basis-point percentage of a `u64` amount: `amount * bps / BPS_DENOMINATOR`.
#[inline]
#[must_use]
pub fn bps_u64(amount: u64, bps: u32, mul_msg: &'static str) -> u64 {
    mul_u64(amount, bps as u64, mul_msg) / BPS_DENOMINATOR as u64
}

/// Split an amount into `(fee, net)` using basis-point math.
#[inline]
#[must_use]
pub fn split_bps(
    amount: i128,
    bps_value: u32,
    mul_msg: &'static str,
    div_msg: &'static str,
    sub_msg: &'static str,
) -> (i128, i128) {
    let fee = bps(amount, bps_value, mul_msg, div_msg);
    let net = sub_i128(amount, fee, sub_msg);
    (fee, net)
}

/// Seconds in one calendar day.
pub const SECS_PER_DAY: u64 = 86_400;

/// Day-of-week from a Unix timestamp.
///
/// Returns `0 = Sunday, 1 = Monday, … 6 = Saturday`.
/// The Unix epoch (1970-01-01) was a Thursday (day 4).
#[inline]
#[must_use]
pub fn day_of_week(unix_ts: u64) -> u8 {
    ((unix_ts / SECS_PER_DAY + 4) % 7) as u8
}

/// Returns `true` when the given Unix timestamp falls on a Saturday or Sunday.
#[inline]
#[must_use]
pub fn is_weekend(unix_ts: u64) -> bool {
    matches!(day_of_week(unix_ts), 0 | 6)
}

/// Advance `start` by `business_days` weekdays (Mon–Fri), skipping weekends.
///
/// Each calendar day is treated as exactly [`SECS_PER_DAY`] seconds.  If
/// `business_days` is `0` the start timestamp is returned unchanged.
///
/// # Arguments
///
/// * `start` – Unix timestamp (seconds since epoch) of the starting day.
/// * `business_days` – number of weekdays to add.
///
/// # Panics
///
/// Panics on arithmetic overflow.
///
/// # Examples
///
/// ```
/// use credence_math::add_business_days;
///
/// // 2024-01-01 (Monday) + 1 business day → 2024-01-02 (Tuesday)
/// let mon = 1704067200;
/// assert_eq!(add_business_days(mon, 1), mon + 86_400);
/// ```
#[must_use]
pub fn add_business_days(start: u64, business_days: u32) -> u64 {
    let mut ts = start;
    let mut remaining = business_days;
    while remaining > 0 {
        ts = ts.checked_add(SECS_PER_DAY).expect("add_business_days overflow");
        if !is_weekend(ts) {
            remaining -= 1;
        }
    }
    ts
}

/// Split `items` into chunks of `chunk_size` and invoke `f` for each chunk.
///
/// The callback `f` receives a (`Vec<T>`, `chunk_index`) pair for every
/// chunk in order. The final chunk may contain fewer than `chunk_size`
/// elements when the input length is not an exact multiple.
///
/// # Boundary behaviour
///
/// | Case               | Behaviour                                     |
/// |--------------------|-----------------------------------------------|
/// | `items` is empty   | `f` is never called; returns `0`.             |
/// | exact multiple     | every chunk has exactly `chunk_size` elements |
/// | remainder          | last chunk has `len % chunk_size` elements    |
///
/// # Errors
///
/// Aborts with [`ContractError::DivisionByZero`] when `chunk_size == 0`.
///
/// # Returns
///
/// The number of chunks produced (i.e. `ceil(items.len() / chunk_size)`).
#[inline]
pub fn chunked_iter<T, F>(
    e: &soroban_sdk::Env,
    items: &soroban_sdk::Vec<T>,
    chunk_size: u32,
    mut f: F,
) -> u32
where
    T: soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>
        + soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>
        + Clone,
    F: FnMut(soroban_sdk::Vec<T>, u32),
{
    if chunk_size == 0 {
        soroban_sdk::panic_with_error!(e, ContractError::DivisionByZero);
    }

    let len = items.len();
    if len == 0 {
        return 0;
    }

    let mut chunk_index: u32 = 0;
    let mut start: u32 = 0;

    while start < len {
        let end = (start + chunk_size).min(len);
        let mut chunk: soroban_sdk::Vec<T> = soroban_sdk::Vec::new(e);
        for i in start..end {
            chunk.push_back(items.get(i).unwrap());
        }
        f(chunk, chunk_index);
        chunk_index += 1;
        start = end;
    }

    chunk_index
}

#[cfg(test)]
mod tests {
    use super::{
        bps, bps_round_up, bps_u64, ceil_div_i128, div_i128, mul_div_i128, split_bps, Rounding,
    };

    fn legacy_bps_i128(amount: i128, bps: u32) -> i128 {
        amount
            .checked_mul(bps as i128)
            .expect("legacy i128 overflow")
            / 10_000
    }

    fn legacy_bps_u64(amount: u64, bps: u32) -> u64 {
        amount.checked_mul(bps as u64).expect("legacy u64 overflow") / 10_000
    }

    fn legacy_split_bps(amount: i128, bps: u32) -> (i128, i128) {
        let fee = legacy_bps_i128(amount, bps);
        let net = amount.checked_sub(fee).expect("legacy i128 underflow");
        (fee, net)
    }

    #[test]
    
    #[test]
    fn test_checked_add_or_error() {
        assert_eq!(super::checked_add_or_error(1, 2), Ok(3));
        assert_eq!(super::checked_add_or_error(i128::MAX, 1), Err(crate::ContractError::Overflow));
    }

    #[test]
    fn bps_matches_legacy_formula() {
        let cases = [
            (0_i128, 0_u32),
            (1, 1),
            (10_000, 100),
            (999_999, 333),
            (1_000_000_000, 50),
            (i128::MAX / 20_000, 10_000),
        ];

        for (amount, bps_value) in cases {
            assert_eq!(
                bps(amount, bps_value, "mul", "div"),
                legacy_bps_i128(amount, bps_value)
            );
        }
    }

    #[test]
    fn mul_div_down_matches_legacy_bps_formula() {
        let cases = [
            (0_i128, 0_u32),
            (1, 1),
            (10_000, 100),
            (999_999, 333),
            (1_000_000_000, 50),
            (i128::MAX / 20_000, 10_000),
        ];

        for (amount, bps_value) in cases {
            assert_eq!(
                mul_div_i128(
                    amount,
                    bps_value as i128,
                    10_000,
                    Rounding::Down,
                    "overflow"
                ),
                legacy_bps_i128(amount, bps_value)
            );
        }
    }

    #[test]
    fn bps_u64_matches_legacy_formula() {
        let cases = [
            (0_u64, 0_u32),
            (1, 1),
            (10_000, 100),
            (999_999, 333),
            (u64::MAX / 20_000, 10_000),
        ];

        for (amount, bps_value) in cases {
            assert_eq!(
                bps_u64(amount, bps_value, "mul"),
                legacy_bps_u64(amount, bps_value)
            );
        }
    }

    #[test]
    fn split_bps_matches_legacy_formula() {
        let cases = [
            (0_i128, 0_u32),
            (10_000, 100),
            (10_000, 1_000),
            (123_456_789, 75),
            (i128::MAX / 20_000, 10_000),
        ];

        for (amount, bps_value) in cases {
            assert_eq!(
                split_bps(amount, bps_value, "mul", "div", "sub"),
                legacy_split_bps(amount, bps_value)
            );
        }
    }

    #[test]
    fn mul_div_down_matches_rust_division_for_signed_inputs() {
        assert_eq!(mul_div_i128(-10, 3, 4, Rounding::Down, "test"), -7);
        assert_eq!(mul_div_i128(10, -3, 4, Rounding::Down, "test"), -7);
        assert_eq!(mul_div_i128(10, 3, -4, Rounding::Down, "test"), -7);
        assert_eq!(mul_div_i128(-10, -3, -4, Rounding::Down, "test"), -7);
    }

    #[test]
    fn mul_div_uses_wide_intermediate_when_result_fits() {
        assert_eq!(
            mul_div_i128(i128::MAX, 10_000, 10_000, Rounding::Down, "test"),
            i128::MAX
        );
        assert_eq!(
            mul_div_i128(i128::MAX, 10_000, 10_000, Rounding::Up, "test"),
            i128::MAX
        );
    }

    #[test]
    fn mul_div_rounds_up_on_non_zero_remainder() {
        assert_eq!(mul_div_i128(10, 3, 4, Rounding::Down, "test"), 7);
        assert_eq!(mul_div_i128(10, 3, 4, Rounding::Up, "test"), 8);
        assert_eq!(mul_div_i128(-10, 3, 4, Rounding::Up, "test"), -8);
    }

    #[test]
    fn mul_div_nearest_rounds_half_ties_away_from_zero() {
        assert_eq!(mul_div_i128(10, 1, 4, Rounding::Nearest, "test"), 3);
        assert_eq!(mul_div_i128(9, 1, 4, Rounding::Nearest, "test"), 2);
        assert_eq!(mul_div_i128(-10, 1, 4, Rounding::Nearest, "test"), -3);
    }

    #[test]
    fn mul_div_handles_zero_numerator_and_denom_one() {
        assert_eq!(mul_div_i128(0, i128::MAX, 1, Rounding::Up, "test"), 0);
        assert_eq!(mul_div_i128(123, 456, 1, Rounding::Down, "test"), 56_088);
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn mul_div_panics_only_when_final_positive_result_overflows() {
        let _ = mul_div_i128(i128::MAX, 10_001, 10_000, Rounding::Down, "overflow");
    }

    #[test]
    #[should_panic(expected = "denom")]
    fn mul_div_panics_with_msg_on_zero_denominator() {
        let _ = mul_div_i128(1, 1, 0, Rounding::Down, "denom");
    }

    #[test]
    fn bps_round_up_uses_wide_intermediate() {
        assert_eq!(bps_round_up(10_001, 1, "test"), 2);
        assert_eq!(bps_round_up(10_000, 1, "test"), 1);
        assert_eq!(bps_round_up(i128::MAX, 10_000, "test"), i128::MAX);
    }

    #[test]
    fn ceil_div_i128_zero_numerator() {
        assert_eq!(ceil_div_i128(0, 5, "test"), 0);
    }

    #[test]
    fn ceil_div_i128_exact_division() {
        assert_eq!(ceil_div_i128(10, 5, "test"), 2);
    }

    #[test]
    fn ceil_div_i128_off_by_one_boundary() {
        assert_eq!(ceil_div_i128(11, 5, "test"), 3);
    }

    #[test]
    fn ceil_div_i128_large_values() {
        assert_eq!(ceil_div_i128(10_000 * 5_001, 10_001, "test"), 5001);
    }

    #[test]
    fn ceil_div_i128_bonded_one() {
        assert_eq!(ceil_div_i128(0, 1, "test"), 0);
        assert_eq!(ceil_div_i128(1, 1, "test"), 1);
    }

    #[test]
    fn ceil_div_i128_known_pairs() {
        // bonded=3, slashed=2: ceil(2*10_000/3) = 6667
        assert_eq!(ceil_div_i128(2 * 10_000, 3, "test"), 6667);
        // bonded=7, slashed=3: ceil(3*10_000/7) = 4286
        assert_eq!(ceil_div_i128(3 * 10_000, 7, "test"), 4286);
    }

    // -----------------------------------------------------------------------
    // Overflow boundary of the inner `a + (b - 1)` add (issue #660)
    // -----------------------------------------------------------------------

    /// `a == i128::MAX, b == 2` makes the inner `a + (b - 1)` overflow, which
    /// must hit the `checked_add` panic path with the supplied message.
    #[test]
    #[should_panic(expected = "ceil overflow")]
    fn ceil_div_i128_inner_add_overflows() {
        let _ = ceil_div_i128(i128::MAX, 2, "ceil overflow");
    }

    /// `b == 1` is the identity: `a + 0` never overflows and `a / 1 == a`.
    #[test]
    fn ceil_div_i128_divisor_one_is_identity() {
        assert_eq!(ceil_div_i128(i128::MAX, 1, "test"), i128::MAX);
        assert_eq!(ceil_div_i128(0, 1, "test"), 0);
        assert_eq!(ceil_div_i128(42, 1, "test"), 42);
    }

    /// `b == i128::MAX` with `a == i128::MAX` overflows the inner add as well
    /// (`a + (b - 1)` exceeds `i128::MAX`).
    #[test]
    #[should_panic(expected = "ceil overflow")]
    fn ceil_div_i128_large_divisor_overflows() {
        let _ = ceil_div_i128(i128::MAX, i128::MAX, "ceil overflow");
    }

    /// Just under the overflow threshold: `a == i128::MAX - (b - 1)` makes the
    /// inner add land exactly on `i128::MAX` and must still succeed.
    #[test]
    fn ceil_div_i128_just_under_overflow_succeeds() {
        // b = 2 → a + (b - 1) = (i128::MAX - 1) + 1 = i128::MAX, no overflow.
        let a = i128::MAX - 1;
        let expected = (i128::MAX) / 2; // ceil((MAX-1)/2) == MAX/2
        assert_eq!(ceil_div_i128(a, 2, "test"), expected);
    }

    /// With a remainder, ceiling division exceeds floor division by exactly one;
    /// with no remainder the two agree.
    #[test]
    fn ceil_div_i128_differs_from_floor_by_one_on_remainder() {
        // remainder present: ceil(11/5) = 3, floor(11/5) = 2
        assert_eq!(ceil_div_i128(11, 5, "test"), div_i128(11, 5, "test") + 1);
        // exact division: ceil(10/5) == floor(10/5)
        assert_eq!(ceil_div_i128(10, 5, "test"), div_i128(10, 5, "test"));
    }

    #[test]
    fn bps_round_up_zero_bps() {
        assert_eq!(bps_round_up(12345, 0, "test"), 0);
        assert_eq!(bps_round_up(i128::MAX, 0, "test"), 0);
        assert_eq!(bps_round_up(-98765, 0, "test"), 0);
    }

    #[test]
    fn bps_u64_boundaries() {
        assert_eq!(bps_u64(0, 0, "mul"), 0);
        assert_eq!(bps_u64(0, BPS_DENOMINATOR as u32, "mul"), 0);
        assert_eq!(bps_u64(10000, BPS_DENOMINATOR as u32, "mul"), 10000);
        let max_div_2 = u64::MAX / 2;
        assert_eq!(
            bps_u64(
                (u64::MAX / (BPS_DENOMINATOR as u64 * 2)) * (BPS_DENOMINATOR as u64 * 2),
                BPS_DENOMINATOR as u32,
                "mul"
            ),
            max_div_2
        );
    }

    #[test]
    fn split_bps_boundaries() {
        assert_eq!(split_bps(0, 0, "mul", "div", "sub"), (0, 0));
        assert_eq!(
            split_bps(0, BPS_DENOMINATOR as u32, "mul", "div", "sub"),
            (0, 0)
        );
        assert_eq!(split_bps(12345, 0, "mul", "div", "sub"), (0, 12345));
        assert_eq!(
            split_bps(12345, BPS_DENOMINATOR as u32, "mul", "div", "sub"),
            (12345, 0)
        );
        let amount = i128::MAX / 20000;
        assert_eq!(
            split_bps(amount, BPS_DENOMINATOR as u32, "mul", "div", "sub"),
            (amount, 0)
        );
    }

    // -----------------------------------------------------------------------
    // add_business_days tests
    // -----------------------------------------------------------------------

    const SECS: u64 = super::SECS_PER_DAY;

    /// Helper: build a Unix timestamp for a known weekday.
    /// 2024-01-01 00:00:00 UTC is a Monday (day_of_week == 1).
    const MON_2024_01_01: u64 = 1_704_067_200;

    #[test]
    fn day_of_week_monday() {
        assert_eq!(super::day_of_week(MON_2024_01_01), 1);
    }

    #[test]
    fn day_of_week_weekend() {
        // Saturday 2024-01-06
        assert_eq!(super::day_of_week(MON_2024_01_01 + 5 * SECS), 6);
        // Sunday 2024-01-07
        assert_eq!(super::day_of_week(MON_2024_01_01 + 6 * SECS), 0);
    }

    #[test]
    fn is_weekend_saturday_sunday() {
        assert!(super::is_weekend(MON_2024_01_01 + 5 * SECS)); // Sat
        assert!(super::is_weekend(MON_2024_01_01 + 6 * SECS)); // Sun
        assert!(!super::is_weekend(MON_2024_01_01));            // Mon
        assert!(!super::is_weekend(MON_2024_01_01 + 4 * SECS)); // Fri
    }

    // -- weekday inputs ---------------------------------------------------

    #[test]
    fn add_business_days_zero_returns_start() {
        assert_eq!(super::add_business_days(MON_2024_01_01, 0), MON_2024_01_01);
    }

    #[test]
    fn add_one_business_day_from_monday_gives_tuesday() {
        // Monday + 1 -> Tuesday
        assert_eq!(super::add_business_days(MON_2024_01_01, 1), MON_2024_01_01 + SECS);
    }

    #[test]
    fn add_one_business_day_from_friday_gives_next_monday() {
        // Friday 2024-01-05 + 1 -> Monday 2024-01-08
        let fri = MON_2024_01_01 + 4 * SECS;
        assert_eq!(super::add_business_days(fri, 1), fri + 3 * SECS);
    }

    #[test]
    fn add_five_business_days_from_monday_gives_next_monday() {
        // Mon + 5 -> next Mon
        assert_eq!(
            super::add_business_days(MON_2024_01_01, 5),
            MON_2024_01_01 + 7 * SECS
        );
    }

    #[test]
    fn add_two_business_days_from_wednesday_gives_friday() {
        // Wednesday 2024-01-03 + 2 -> Friday 2024-01-05
        let wed = MON_2024_01_01 + 2 * SECS;
        assert_eq!(super::add_business_days(wed, 2), wed + 2 * SECS);
    }

    #[test]
    fn add_three_business_days_from_thursday_gives_next_tuesday() {
        // Thursday 2024-01-04 + 3 -> Tue 2024-01-09
        let thu = MON_2024_01_01 + 3 * SECS;
        assert_eq!(super::add_business_days(thu, 3), thu + 5 * SECS);
    }

    // -- weekend inputs ---------------------------------------------------

    #[test]
    fn add_business_days_from_saturday_skips_to_monday_then_counts() {
        // Saturday 2024-01-06 + 1 -> Monday 2024-01-08
        let sat = MON_2024_01_01 + 5 * SECS;
        assert_eq!(super::add_business_days(sat, 1), sat + 2 * SECS);
    }

    #[test]
    fn add_business_days_from_sunday_skips_to_monday_then_counts() {
        // Sunday 2024-01-07 + 1 -> Monday 2024-01-08
        let sun = MON_2024_01_01 + 6 * SECS;
        assert_eq!(super::add_business_days(sun, 1), sun + 1 * SECS);
    }

    #[test]
    fn add_business_days_from_saturday_zero_returns_saturday() {
        let sat = MON_2024_01_01 + 5 * SECS;
        assert_eq!(super::add_business_days(sat, 0), sat);
    }

    // -- month-boundary ---------------------------------------------------

    #[test]
    fn add_business_days_crosses_month_boundary() {
        // Friday 2024-01-26 + 4 -> Wednesday 2024-01-31
        let fri_jan26 = MON_2024_01_01 + 25 * SECS;
        assert_eq!(super::day_of_week(fri_jan26), 5); // Friday
        // Fri -> Mon(+3) -> Tue(+1) -> Wed(+1) = 4 business days
        assert_eq!(super::add_business_days(fri_jan26, 4), fri_jan26 + 6 * SECS);
    }

    #[test]
    fn add_business_days_from_end_of_february() {
        // Thursday 2024-02-29 (leap year) + 3 -> Tuesday 2024-03-05
        let thu_feb29 = MON_2024_01_01 + 59 * SECS;
        assert_eq!(super::day_of_week(thu_feb29), 4); // Thursday
        // Thu -> Fri(+1) -> Mon(+3) -> Tue(+1) = 3 business days
        assert_eq!(super::add_business_days(thu_feb29, 3), thu_feb29 + 5 * SECS);
    }

    #[test]
    fn add_business_days_from_month_end_crosses_weekend() {
        // Friday 2024-02-23 + 2 -> Tuesday 2024-02-27
        let fri_feb23 = MON_2024_01_01 + (31 + 22) * SECS;
        assert_eq!(super::day_of_week(fri_feb23), 5); // Friday
        assert_eq!(super::add_business_days(fri_feb23, 2), fri_feb23 + 4 * SECS);
    }

    #[test]
    fn add_multiple_weeks() {
        // 10 business days from Monday = 2 full weeks = 14 calendar days
        assert_eq!(
            super::add_business_days(MON_2024_01_01, 10),
            MON_2024_01_01 + 14 * SECS
        );
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn sat_mul_bps_identity(amount in 0..i128::MAX) {
            prop_assert_eq!(sat_mul_bps(amount, 10_000), amount);
        }
    }
}
