#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]
use credence_errors::ContractError;
use ethnum::U256;

pub mod fixed_point;
pub mod rate;
pub mod time;
pub mod timestamp;

pub use fixed_point::{div_wad, div_wad_up, mul_wad, mul_wad_up, sat_div_wad, sat_mul_wad, WAD};
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
    assert!(b > 0, "{message}");
    let quotient = a / b;
    if a % b > 0 {
        quotient + 1
    } else {
        quotient
    }
}

/// Exact `(a * b) / denom` computed with a 256-bit intermediate, rounded
/// according to `rounding`. Returns `None` when `denom` is zero or the rounded
/// quotient does not fit in `i128`.
///
/// Rounding works on the magnitude, so `Up` and `Nearest` round away from zero.
fn checked_mul_div(a: i128, b: i128, denom: i128, rounding: Rounding) -> Option<i128> {
    if denom == 0 {
        return None;
    }
    let product = U256::new(a.unsigned_abs()) * U256::new(b.unsigned_abs());
    let divisor = U256::new(denom.unsigned_abs());
    let mut quotient = product / divisor;
    let remainder = product % divisor;
    match rounding {
        Rounding::Down => {}
        Rounding::Up => {
            if remainder != U256::ZERO {
                quotient += U256::ONE;
            }
        }
        Rounding::Nearest => {
            if remainder >= divisor / U256::new(2) {
                quotient += U256::ONE;
            }
        }
    }
    let negative = (a < 0) ^ (b < 0) ^ (denom < 0);
    let limit = if negative {
        1u128 << 127
    } else {
        i128::MAX as u128
    };
    if quotient > U256::new(limit) {
        return None;
    }
    let magnitude = quotient.as_u128() as i128;
    Some(if negative {
        magnitude.wrapping_neg()
    } else {
        magnitude
    })
}

/// Exact multiply-divide: `(a * b) / denom` with a 256-bit intermediate, rounded
/// according to `rounding`.
///
/// # Panics
/// Panics with `msg` when `denom` is zero or the rounded result does not fit in `i128`.
#[inline]
#[must_use]
pub fn mul_div_i128(a: i128, b: i128, denom: i128, rounding: Rounding, msg: &str) -> i128 {
    checked_mul_div(a, b, denom, rounding).expect(msg)
}

/// Saturating multiply-divide: `(a * b) / denom` rounded according to `rounding`.
///
/// Returns `0` when `denom` is zero, and clamps to `i128::MIN` or `i128::MAX`
/// when the rounded result does not fit in `i128`.
#[inline]
#[must_use]
pub fn sat_mul_div_i128(a: i128, b: i128, denom: i128, rounding: Rounding) -> i128 {
    if denom == 0 {
        return 0;
    }
    match checked_mul_div(a, b, denom, rounding) {
        Some(value) => value,
        None if (a < 0) ^ (b < 0) ^ (denom < 0) => i128::MIN,
        None => i128::MAX,
/// Wrapping multiply-divide: `(a * b) / denom` over a 256-bit intermediate.
/// Rounds according to `rounding`. Panics with `msg` on `denom == 0` or when
/// the final rounded result does not fit in `i128`.
#[inline]
#[must_use]
pub fn mul_div_i128(a: i128, b: i128, denom: i128, rounding: Rounding, msg: &str) -> i128 {
    if denom == 0 {
        panic!("{msg}");
    }

    let negative = (a < 0) ^ (b < 0) ^ (denom < 0);
    let numerator = U256::new(a.unsigned_abs()) * U256::new(b.unsigned_abs());
    let divisor = U256::new(denom.unsigned_abs());
    let quotient = numerator / divisor;
    let remainder = numerator % divisor;

    let rounded = match rounding {
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
    let negative_limit = U256::new(i128::MAX as u128 + 1);
    if negative {
        if rounded > negative_limit {
            panic!("{msg}");
        }
        if rounded == negative_limit {
            i128::MIN
        } else {
            -i128::try_from(rounded.as_u128()).unwrap_or_else(|_| panic!("{msg}"))
        }
    } else {
        if rounded > positive_limit {
            panic!("{msg}");
        }
        i128::try_from(rounded.as_u128()).unwrap_or_else(|_| panic!("{msg}"))
    }
}

/// Saturating multiply-divide: `(a * b) / denom` over a 256-bit intermediate.
/// Rounds according to `rounding`. Clamps on overflow and returns `0` when
/// `denom == 0`.
#[inline]
#[must_use]
pub fn sat_mul_div_i128(a: i128, b: i128, denom: i128, rounding: Rounding) -> i128 {
    if denom == 0 {
        return 0;
    }

    let negative = (a < 0) ^ (b < 0) ^ (denom < 0);
    let numerator = U256::new(a.unsigned_abs()) * U256::new(b.unsigned_abs());
    let divisor = U256::new(denom.unsigned_abs());
    let quotient = numerator / divisor;
    let remainder = numerator % divisor;

    let rounded = match rounding {
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
    let negative_limit = U256::new(i128::MAX as u128 + 1);
    if negative {
        if rounded >= negative_limit {
            i128::MIN
        } else {
            -(rounded.as_u128() as i128)
        }
    } else {
        if rounded >= positive_limit {
            i128::MAX
        } else {
            rounded.as_u128() as i128
        }
    }
}

/// Fixed-point multiplication with checked overflow.
#[inline]
pub fn checked_mul_wad(a: i128, b: i128) -> Result<i128, ContractError> {
    let num = a.checked_mul(b).ok_or(ContractError::Overflow)?;
    num.checked_div(WAD).ok_or(ContractError::Overflow)
}

/// Fixed-point division with checked overflow.
#[inline]
pub fn checked_div_wad(a: i128, b: i128) -> Result<i128, ContractError> {
    let num = a.checked_mul(WAD).ok_or(ContractError::Overflow)?;
    num.checked_div(b).ok_or(ContractError::Overflow)
}

#[cfg(test)]
mod wad_boundary_tests {
    use super::*;
    use credence_errors::ContractError;

    #[test]
    fn checked_mul_wad_exact_boundaries() {
        // Identity and exact products.
        assert_eq!(checked_mul_wad(WAD, WAD), Ok(WAD));
        assert_eq!(checked_mul_wad(2 * WAD, 3 * WAD), Ok(6 * WAD));
        assert_eq!(checked_mul_wad(0, i128::MAX), Ok(0));
        assert_eq!(checked_mul_wad(i128::MAX, 0), Ok(0));
        assert_eq!(checked_mul_wad(0, i128::MIN), Ok(0));
        // Largest operand whose scaled product still fits: result is
        // (MAX / WAD) * 1 / WAD.
        assert_eq!(
            checked_mul_wad(i128::MAX / WAD, 1),
            Ok(i128::MAX / WAD / WAD)
        );
        // (MIN / WAD) * 1 / WAD stays in range (truncation toward zero).
        assert_eq!(
            checked_mul_wad(i128::MIN / WAD, 1),
            Ok(i128::MIN / WAD / WAD)
        );
    }

    #[test]
    fn checked_mul_wad_overflow_boundaries() {
        // i128::MAX * WAD and i128::MIN * WAD both overflow the intermediate.
        assert_eq!(checked_mul_wad(i128::MAX, WAD), Err(ContractError::Overflow));
        assert_eq!(checked_mul_wad(i128::MIN, WAD), Err(ContractError::Overflow));
        assert_eq!(checked_mul_wad(WAD, i128::MAX), Err(ContractError::Overflow));
        assert_eq!(checked_mul_wad(WAD, i128::MIN), Err(ContractError::Overflow));
        // MIN * -1 overflows on its own.
        assert_eq!(checked_mul_wad(i128::MIN, -1), Err(ContractError::Overflow));
        // One past the largest fitting positive product.
        assert_eq!(checked_mul_wad(i128::MAX / WAD + 1, WAD), Err(ContractError::Overflow));
        // Negative operands with mixed signs that still overflow.
        assert_eq!(checked_mul_wad(i128::MAX, -WAD), Err(ContractError::Overflow));
        assert_eq!(checked_mul_wad(i128::MIN, WAD), Err(ContractError::Overflow));
    }

    #[test]
    fn checked_mul_wad_sign_and_rounding() {
        // Positive product truncates toward zero: 1.5 -> 1.
        assert_eq!(checked_mul_wad(3, WAD / 2), Ok(1));
        // Negative product truncates toward zero: -1.5 -> -1 (not -2).
        assert_eq!(checked_mul_wad(-3, WAD / 2), Ok(-1));
        // Opposite signs yield a negative result.
        assert_eq!(checked_mul_wad(-WAD, WAD), Ok(-WAD));
        assert_eq!(checked_mul_wad(WAD, -WAD), Ok(-WAD));
        // Same signs yield a positive result.
        assert_eq!(checked_mul_wad(-WAD, -WAD), Ok(WAD * WAD / WAD));
        // -MAX * 1 fits and stays negative.
        assert_eq!(
            checked_mul_wad(i128::MIN + 1, 1),
            Ok((i128::MIN + 1) / WAD)
        );
        // Exact halfway is impossible with WAD=1e18 scale on these inputs,
        // but an exact negative result must not be rounded away from zero.
        assert_eq!(checked_mul_wad(-2 * WAD, WAD / 2), Ok(-WAD));
    }

    #[test]
    fn checked_div_wad_exact_boundaries() {
        assert_eq!(checked_div_wad(WAD, WAD), Ok(WAD));
        assert_eq!(checked_div_wad(2 * WAD, WAD), Ok(2 * WAD));
        assert_eq!(checked_div_wad(WAD, 2 * WAD), Ok(WAD / 2));
        assert_eq!(checked_div_wad(0, WAD), Ok(0));
        // Largest dividend whose scaled product still fits.
        let a = i128::MAX / WAD;
        assert_eq!(checked_div_wad(a, 1), Ok(a * WAD));
        // Smallest (most negative) dividend that still fits.
        let b = i128::MIN / WAD;
        assert_eq!(checked_div_wad(b, 1), Ok(b * WAD));
        // Divisor of -1 with a large negative quotient stays in range.
        assert_eq!(checked_div_wad(-WAD, -1), Ok(WAD * WAD));
    }

    #[test]
    fn checked_div_wad_overflow_and_zero_divisor() {
        // Zero divisor always maps to Overflow, even for a = 0.
        assert_eq!(checked_div_wad(WAD, 0), Err(ContractError::Overflow));
        assert_eq!(checked_div_wad(0, 0), Err(ContractError::Overflow));
        assert_eq!(checked_div_wad(i128::MAX, 0), Err(ContractError::Overflow));
        assert_eq!(checked_div_wad(i128::MIN, 0), Err(ContractError::Overflow));
        // a * WAD overflows before the division is attempted.
        assert_eq!(checked_div_wad(i128::MAX, 1), Err(ContractError::Overflow));
        assert_eq!(checked_div_wad(i128::MIN, 1), Err(ContractError::Overflow));
        assert_eq!(checked_div_wad(i128::MAX, WAD), Err(ContractError::Overflow));
        assert_eq!(checked_div_wad(i128::MIN, WAD), Err(ContractError::Overflow));
        // One past the largest fitting dividend.
        assert_eq!(
            checked_div_wad(i128::MAX / WAD + 1, 1),
            Err(ContractError::Overflow)
        );
        assert_eq!(
            checked_div_wad(i128::MIN / WAD - 1, 1),
            Err(ContractError::Overflow)
        );
    }

    #[test]
    fn checked_div_wad_sign_and_rounding() {
        // Positive quotient truncates: WAD / 3 -> WAD/3.
        assert_eq!(checked_div_wad(WAD, 3 * WAD), Ok(WAD / 3));
        // Negative quotient truncates toward zero: -(WAD/3), not -(WAD/3)-1.
        assert_eq!(checked_div_wad(-WAD, 3 * WAD), Ok(-(WAD / 3)));
        // Negative divisor flips the sign.
        assert_eq!(checked_div_wad(WAD, -WAD), Ok(-WAD));
        assert_eq!(checked_div_wad(-WAD, -WAD), Ok(WAD));
        // Exact negative result stays exact.
        assert_eq!(checked_div_wad(-2 * WAD, WAD), Ok(-2 * WAD));
        // i128::MIN / -1 cannot be produced here (num is a multiple of WAD
        // and never equals MIN), but the nearest reachable negative extreme
        // divided by -1 stays in range.
        let b = i128::MIN / WAD;
        assert_eq!(checked_div_wad(b, -1), Ok(-(b * WAD)));
    }
}
