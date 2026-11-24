#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]
#[allow(
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
// Must come AFTER `#[allow(clippy::restriction, ...)]` above: the
// `clippy::disallowed_macros` lint belongs to the `restriction` group, so
// a later allow would re-silence it. cargo build --release / WASM build
// is the only mode where this deny fires (tests
// stay free to use format!/write! for diagnostics).
use credence_errors::ContractError;
use ethnum::U256;
use soroban_sdk::{panic_with_error, Env};

pub mod fixed_point;
pub mod rate;
pub mod time;
pub mod timestamp;

#[allow(deprecated)]
pub use fixed_point::{
    checked_div_wad, checked_div_wad_up, checked_mul_wad, checked_mul_wad_up, div_wad, div_wad_up,
    mul_wad, mul_wad_up, sat_div_wad, sat_mul_wad, WAD,
};
pub use rate::{Rate, MAX_COMPOUND_PERIODS};
pub use time::{
    SECONDS_PER_DAY, SECONDS_PER_HOUR, SECONDS_PER_MINUTE, SECONDS_PER_WEEK, SECONDS_PER_YEAR,
};
pub use timestamp::Timestamp;

/// Fixed-point denominator for basis-point calculations.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Fixed-point denominator for percentage calculations.
pub const PERCENT_DENOMINATOR: i128 = 100;

/// Multiply a value by basis points and divide by BPS_DENOMINATOR.
/// Returns `(value * bps) / BPS_DENOMINATOR`.
/// The operation names are for panic messages only (kept for backward compatibility).
#[inline]
pub fn bps(value: i128, bps: u32, _mul_op: &str, _div_op: &str) -> i128 {
    value.saturating_mul(bps as i128) / BPS_DENOMINATOR
}

/// Convert basis points to a u64 numerator.
#[inline]
pub fn bps_u64(bps: u32) -> u64 {
    bps as u64
}

/// Saturating multiply by basis points: `(value * bps) / BPS_DENOMINATOR`.
/// Saturates on overflow instead of panicking.
#[inline]
pub fn sat_mul_bps(value: i128, bps: u32) -> i128 {
    let num = value.saturating_mul(bps as i128);
    num.saturating_div(BPS_DENOMINATOR)
}

/// Split a value into fee and net portions based on basis points.
/// Returns (fee, net) where fee = (value * bps) / BPS_DENOMINATOR and net = value - fee.
/// The operation names (mul_op, div_op, sub_op) are for panic messages only.
#[inline]
pub fn split_bps(
    value: i128,
    bps: u32,
    _mul_op: &str,
    _div_op: &str,
    _sub_op: &str,
) -> (i128, i128) {
    let fee = value.saturating_mul(bps as i128) / BPS_DENOMINATOR;
    let net = value.saturating_sub(fee);
    (fee, net)
}

/// Rounding behavior for [`mul_div_i128`] and [`sat_mul_div_i128`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rounding {
    /// Truncate the fractional remainder toward zero.
    Down,
    /// Round away from zero when the division leaves a remainder.
    Up,
    /// Round to the nearest integer, with exact half-way cases rounded away from zero.
    Nearest,
}

/// Unwrap a `Result<T, ContractError>` by aborting with a typed Soroban
/// `ContractError` via `panic_with_error!`.
///
/// Preferred over legacy message-taking wrappers whenever an `&Env` is in scope.
#[inline]
pub fn unwrap_or_panic_error<T>(env: &Env, result: Result<T, ContractError>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => panic_with_error!(env, err),
    }
}

/// Internal compatibility unwrapper for legacy helpers that do not accept an
/// `&Env`. Uses fixed static messages and never formats a runtime string.
#[inline]
fn unwrap_legacy<T>(result: Result<T, ContractError>) -> T {
    match result {
        Ok(value) => value,
        Err(ContractError::DivisionByZero) => {
            Option::<T>::None.expect("credence_math: division by zero")
        }
        Err(ContractError::Underflow) => {
            Option::<T>::None.expect("credence_math: arithmetic underflow")
        }
        Err(_) => Option::<T>::None.expect("credence_math: arithmetic overflow"),
    }
}

/// Checked `u64` multiplication returning [`ContractError::Overflow`] on overflow.
#[inline]
pub fn checked_mul_u64(a: u64, b: u64) -> Result<u64, ContractError> {
    a.checked_mul(b).ok_or(ContractError::Overflow)
}

/// Checked `u64` multiplication (legacy compatibility wrapper).
///
/// Prefer [`checked_mul_u64`] or [`unwrap_or_panic_error`]. The `_message`
/// parameter is ignored; overflow aborts with a fixed static message.
#[inline]
#[must_use]
#[deprecated(note = "use checked_mul_u64 instead")]
pub fn mul_u64(a: u64, b: u64, _message: &str) -> u64 {
    unwrap_legacy(checked_mul_u64(a, b))
}

/// Floor a Unix timestamp (seconds since epoch) to the start of its UTC day.
///
/// Equivalent to `ts / SECS_PER_DAY * SECS_PER_DAY`, where
/// `SECS_PER_DAY = 86_400`. The result is the Unix timestamp of the most
/// recent midnight (00:00:00 UTC) that is `<= ts`.
///
/// # Properties
///
/// * **Idempotent**: `floor_to_day(floor_to_day(ts)) == floor_to_day(ts)`.
/// * **Monotone**: `a <= b` implies `floor_to_day(a) <= floor_to_day(b)`.
/// * **Epoch zero**: `floor_to_day(0) == 0` (epoch is already a midnight).
/// * **Range**: the result is always a multiple of `86_400`.
///
/// # Examples
///
/// ```
/// use credence_math::floor_to_day;
///
/// assert_eq!(floor_to_day(0), 0);
/// assert_eq!(floor_to_day(1_704_067_200 + 43_200), 1_704_067_200);
/// assert_eq!(floor_to_day(86_399), 0);
/// assert_eq!(floor_to_day(86_400), 86_400);
/// ```
#[inline]
#[must_use]
pub fn floor_to_day(ts: u64) -> u64 {
    (ts / SECONDS_PER_DAY) * SECONDS_PER_DAY
}

/// Checked `i128` addition returning [`ContractError::Overflow`] on overflow.
#[inline]
pub fn checked_add_i128(a: i128, b: i128) -> Result<i128, ContractError> {
    a.checked_add(b).ok_or(ContractError::Overflow)
}

/// Checked `i128` addition (legacy compatibility wrapper).
///
/// Prefer [`checked_add_i128`]. The `_message` parameter is ignored.
#[inline]
#[must_use]
#[deprecated(note = "use checked_add_i128 instead")]
pub fn add_i128(a: i128, b: i128, _message: &str) -> i128 {
    unwrap_legacy(checked_add_i128(a, b))
}

/// Checked `i128` subtraction returning [`ContractError::Underflow`] or
/// [`ContractError::Overflow`] when the result does not fit in `i128`.
#[inline]
pub fn checked_sub_i128(a: i128, b: i128) -> Result<i128, ContractError> {
    a.checked_sub(b).ok_or(if b < 0 {
        ContractError::Overflow
    } else {
        ContractError::Underflow
    })
}

/// Checked `i128` subtraction (legacy compatibility wrapper).
///
/// Prefer [`checked_sub_i128`]. The `_message` parameter is ignored.
#[inline]
#[must_use]
#[deprecated(note = "use checked_sub_i128 instead")]
pub fn sub_i128(a: i128, b: i128, _message: &str) -> i128 {
    unwrap_legacy(checked_sub_i128(a, b))
}

/// Checked `i128` multiplication returning [`ContractError::Overflow`] on overflow.
#[inline]
pub fn checked_mul_i128(a: i128, b: i128) -> Result<i128, ContractError> {
    a.checked_mul(b).ok_or(ContractError::Overflow)
}

/// Checked `i128` multiplication (legacy compatibility wrapper).
///
/// Prefer [`checked_mul_i128`]. The `_message` parameter is ignored.
#[inline]
#[must_use]
#[deprecated(note = "use checked_mul_i128 instead")]
pub fn mul_i128(a: i128, b: i128, _message: &str) -> i128 {
    unwrap_legacy(checked_mul_i128(a, b))
}

/// Checked `i128` division returning [`ContractError::DivisionByZero`] when
/// `b == 0` and [`ContractError::Overflow`] on `i128::MIN / -1`.
#[inline]
pub fn checked_div_i128(a: i128, b: i128) -> Result<i128, ContractError> {
    if b == 0 {
        return Err(ContractError::DivisionByZero);
    }
    a.checked_div(b).ok_or(ContractError::Overflow)
}

/// Checked `i128` division (legacy compatibility wrapper).
///
/// Prefer [`checked_div_i128`]. The `_message` parameter is ignored.
#[inline]
#[must_use]
#[deprecated(note = "use checked_div_i128 instead")]
pub fn div_i128(a: i128, b: i128, _message: &str) -> i128 {
    unwrap_legacy(checked_div_i128(a, b))
}

/// Checked `i128` ceiling division for positive denominator `b`.
///
/// Returns [`ContractError::DivisionByZero`] when `b == 0` and
/// [`ContractError::Overflow`] when `b < 0` or `a + (b - 1)` overflows `i128`.
#[inline]
pub fn checked_ceil_div_i128(a: i128, b: i128) -> Result<i128, ContractError> {
    if b == 0 {
        return Err(ContractError::DivisionByZero);
    }
    if b < 0 {
        return Err(ContractError::Overflow);
    }
    let adjusted = a.checked_add(b - 1).ok_or(ContractError::Overflow)?;
    adjusted.checked_div(b).ok_or(ContractError::Overflow)
}

/// Checked `i128` ceiling division (legacy compatibility wrapper).
///
/// Prefer [`checked_ceil_div_i128`]. The `_message` parameter is ignored.
#[inline]
#[must_use]
#[deprecated(note = "use checked_ceil_div_i128 instead")]
pub fn ceil_div_i128(a: i128, b: i128, _message: &str) -> i128 {
    unwrap_legacy(checked_ceil_div_i128(a, b))
}

/// Checked wide-intermediate multiply-divide: `(a * b) / denom` using a 256-bit
/// intermediate so `a * b` can exceed `i128::MAX` as long as the final rounded
/// quotient fits in `i128`.
///
/// Returns [`ContractError::DivisionByZero`] when `denom == 0`, and
/// [`ContractError::Overflow`] when the final rounded result does not fit in `i128`.
#[inline]
pub fn checked_mul_div_i128(
    a: i128,
    b: i128,
    denom: i128,
    rounding: Rounding,
) -> Result<i128, ContractError> {
    if denom == 0 {
        return Err(ContractError::DivisionByZero);
    }

    let a_abs = U256::new(a.unsigned_abs());
    let b_abs = U256::new(b.unsigned_abs());
    let denom_abs = U256::new(denom.unsigned_abs());

    let product = a_abs * b_abs;
    let mut quotient = product / denom_abs;
    let rem = product % denom_abs;

    let round_up = match rounding {
        Rounding::Down => false,
        Rounding::Up => rem != U256::ZERO,
        Rounding::Nearest => {
            // Round half away from zero: 2 * rem >= denom_abs
            rem * U256::new(2) >= denom_abs
        }
    };

    if round_up {
        quotient = quotient
            .checked_add(U256::ONE)
            .ok_or(ContractError::Overflow)?;
    }

    let negative = (a < 0) ^ (b < 0) ^ (denom < 0);
    if negative {
        let min_mag = U256::new(i128::MIN.unsigned_abs());
        if quotient > min_mag {
            return Err(ContractError::Overflow);
        }
        if quotient == min_mag {
            Ok(i128::MIN)
        } else {
            Ok(-(quotient.as_u128() as i128))
        }
    } else {
        let max_mag = U256::new(i128::MAX as u128);
        if quotient > max_mag {
            return Err(ContractError::Overflow);
        }
        Ok(quotient.as_u128() as i128)
    }
}

/// Wide-intermediate multiply-divide (legacy compatibility wrapper).
///
/// Prefer [`checked_mul_div_i128`]. The `_msg` parameter is ignored.
#[inline]
#[must_use]
#[deprecated(note = "use checked_mul_div_i128 instead")]
pub fn mul_div_i128(a: i128, b: i128, denom: i128, rounding: Rounding, _msg: &str) -> i128 {
    unwrap_legacy(checked_mul_div_i128(a, b, denom, rounding))
}

/// Saturating multiply-divide: `(a * b) / denom` with saturation on overflow.
/// Rounds according to `rounding`. Returns `0` when `denom == 0` and clamps to
/// `i128::MIN` / `i128::MAX` on overflow.
#[inline]
#[must_use]
pub fn sat_mul_div_i128(a: i128, b: i128, denom: i128, rounding: Rounding) -> i128 {
    if denom == 0 {
        return 0;
    }
    match checked_mul_div_i128(a, b, denom, rounding) {
        Ok(v) => v,
        Err(_) => {
            if (a < 0) ^ (b < 0) ^ (denom < 0) {
                i128::MIN
            } else {
                i128::MAX
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_mul_u64_works_and_reports_overflow() {
        assert_eq!(checked_mul_u64(6, 7), Ok(42));
        assert_eq!(checked_mul_u64(u64::MAX, 2), Err(ContractError::Overflow));
    }

    #[test]
    fn checked_add_sub_mul_div_i128_report_typed_errors() {
        assert_eq!(checked_add_i128(10, 20), Ok(30));
        assert_eq!(checked_add_i128(i128::MAX, 1), Err(ContractError::Overflow));

        assert_eq!(checked_sub_i128(20, 10), Ok(10));
        assert_eq!(
            checked_sub_i128(i128::MIN, 1),
            Err(ContractError::Underflow)
        );
        assert_eq!(
            checked_sub_i128(i128::MAX, -1),
            Err(ContractError::Overflow)
        );

        assert_eq!(checked_mul_i128(6, -7), Ok(-42));
        assert_eq!(checked_mul_i128(i128::MAX, 2), Err(ContractError::Overflow));

        assert_eq!(checked_div_i128(42, 6), Ok(7));
        assert_eq!(checked_div_i128(42, 0), Err(ContractError::DivisionByZero));
        assert_eq!(
            checked_div_i128(i128::MIN, -1),
            Err(ContractError::Overflow)
        );
    }

    #[test]
    fn checked_ceil_div_i128_works_and_reports_typed_errors() {
        assert_eq!(checked_ceil_div_i128(10, 3), Ok(4));
        assert_eq!(checked_ceil_div_i128(9, 3), Ok(3));
        assert_eq!(
            checked_ceil_div_i128(10, 0),
            Err(ContractError::DivisionByZero)
        );
        assert_eq!(checked_ceil_div_i128(10, -1), Err(ContractError::Overflow));
        assert_eq!(
            checked_ceil_div_i128(i128::MAX, 2),
            Err(ContractError::Overflow)
        );
    }

    #[test]
    fn checked_mul_div_i128_wide_intermediate_and_typed_errors() {
        assert_eq!(
            checked_mul_div_i128(i128::MAX, WAD, WAD, Rounding::Down),
            Ok(i128::MAX)
        );
        assert_eq!(
            checked_mul_div_i128(i128::MIN, WAD, WAD, Rounding::Down),
            Ok(i128::MIN)
        );
        assert_eq!(
            checked_mul_div_i128(10, 1, 0, Rounding::Down),
            Err(ContractError::DivisionByZero)
        );
        assert_eq!(
            checked_mul_div_i128(i128::MAX, 2, 1, Rounding::Down),
            Err(ContractError::Overflow)
        );
        assert_eq!(
            checked_mul_div_i128(3, 1, 2, Rounding::Nearest),
            Ok(2)
        );
        assert_eq!(
            checked_mul_div_i128(1, 1, 3, Rounding::Nearest),
            Ok(0)
        );
    }

    #[test]
    fn unwrap_or_panic_error_returns_ok_value() {
        let env = Env::default();
        assert_eq!(unwrap_or_panic_error(&env, checked_add_i128(2, 3)), 5);
    }
}
