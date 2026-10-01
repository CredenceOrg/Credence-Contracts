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
use ethnoum::U256;
use soroban_sdk;

pub mod fixed_point;
pub mod rate;
pub mod time;
pub mod timestamp;

pub use fixed_point::{div_wad, div_wad_up, mul_wad, mul_wad_up, sat_div_wad, sat_mul_wad, WAD};
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
    (value as i128).saturating_mul(bps as i128) / BPS_DENOMINATOR
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
pub fn split_bps(value: i128, bps: u32, _mul_op: &str, _div_op: &str, _sub_op: &str) -> (i128, i128) {
    let fee = (value as i128).saturating_mul(bps as i128) / BPS_DENOMINATOR;
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

/// Fixed-point multiplication with widening intermediate.
#[inline]
#[must_use]
pub fn mul_u64(a: u64, b: u64, message: &str) -> u64 {
    a.checked_mul(b).expect(message)
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

/// Checked `i128` addition with a stable panic message.
#[inline]
#[must_use]
pub fn add_i128(a: i128, b: i128, message: &str) -> i128 {
    a.checked_add(b).expect(message)
}

/// Checked `i128` subtraction with a stable panic message.
#[inline]
#[must_use]
pub fn sub_i128(a: i128, b: i128, message: &str) -> i128 {
    a.checked_sub(b).expect(message)
}

/// Checked `i128` multiplication with a stable panic message.
#[inline]
#[must_use]
pub fn mul_i128(a: i128, b: i128, message: &str) -> i128 {
    a.checked_mul(b).expect(message)
}

/// Checked `i128` division with a stable panic message.
#[inline]
#[must_use]
pub fn div_i128(a: i128, b: i128, message: &str) -> i128 {
    a.checked_div(b).expect(message)
}

/// Checked `i128` ceiling division with a stable panic message.
#[inline]
#[must_use]
pub fn ceil_div_i128(a: i128, b: i128, message: &str) -> i128 {
    assert!(b > 0, "denominator must be positive");
    (a + b - 1) / b
}

/// Wrapping multiply-divide: `(a * b) / denom` with full i128 intermediate.
/// Rounds according to `rounding`. Panics with `msg` on overflow.
#[inline]
#[must_use]
pub fn mul_div_i128(a: i128, b: i128, denom: i128, rounding: Rounding, msg: &str) -> i128 {
    assert!(denom > 0, "denominator must be positive");
    // Use u128 for intermediate to avoid i128 overflow
    let a_abs = a.unsigned_abs();
    let b_abs = b.unsigned_abs();
    let denom_abs = denom.unsigned_abs();
    
    let num = match a_abs.checked_mul(b_abs) {
        Some(v) => v,
        None => panic!("{}", msg),
    };
    
    let mut result = (num / denom_abs) as i128;
    let rem = num % denom_abs;
    
    match rounding {
        Rounding::Down => {}
        Rounding::Up => {
            if rem != 0 {
                result = result.saturating_add(1);
            }
        }
        Rounding::Nearest => {
            let half = denom_abs / 2;
            if rem >= half {
                result = result.saturating_add(1);
            }
        }
    }
    
    // Apply sign
    let sign = if (a < 0) ^ (b < 0) ^ (denom < 0) { -1 } else { 1 };
    result.saturating_mul(sign)
}

/// Saturating multiply-divide: `(a * b) / denom` with saturation on overflow.
/// Rounds according to `rounding`. Returns clamped value on overflow.
#[inline]
#[must_use]
pub fn sat_mul_div_i128(a: i128, b: i128, denom: i128, rounding: Rounding) -> i128 {
    assert!(denom > 0, "denominator must be positive");
    // Use i128 checked operations with manual overflow handling
    let a_abs = a.unsigned_abs();
    let b_abs = b.unsigned_abs();
    let denom_abs = denom.unsigned_abs();
    
    // Compute (a * b) / denom using u256 intermediate via u128::checked_mul
    let num = match a_abs.checked_mul(b_abs) {
        Some(v) => v,
        None => return if (a < 0) ^ (b < 0) { i128::MIN } else { i128::MAX },
    };
    
    let mut result = (num / denom_abs) as i128;
    let rem = num % denom_abs;
    let mut result = result as i128;
    
    match rounding {
        Rounding::Down => {}
        Rounding::Up => {
            if rem != 0 {
                result = result.saturating_add(1);
            }
        }
        Rounding::Nearest => {
            let half = denom_abs / 2;
            if rem >= half {
                result = result.saturating_add(1);
            }
        }
    }
    
    // Apply sign
    let sign = if (a < 0) ^ (b < 0) ^ (denom < 0) { -1 } else { 1 };
    result.saturating_mul(sign)
}

/// Fixed-point multiplication with checked overflow.
#[inline]
#[must_use]
pub fn checked_mul_wad(a: i128, b: i128) -> Result<i128, ContractError> {
    let num = a.checked_mul(b).ok_or(ContractError::Overflow)?;
    num.checked_div(WAD).ok_or(ContractError::Overflow)
}

/// Fixed-point division with checked overflow.
#[inline]
#[must_use]
pub fn checked_div_wad(a: i128, b: i128) -> Result<i128, ContractError> {
    let num = a.checked_mul(WAD).ok_or(ContractError::Overflow)?;
    num.checked_div(b).ok_or(ContractError::Overflow)
}
