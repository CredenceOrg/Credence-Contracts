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
// is the only mode where this deny fires (tests
// stay free to use format!/write! for diagnostics).
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use credence_errors::ContractError;
use ethnum::U256;
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

/// Rounding behavior for [`mul_div_i128`] and [`sat_mul_div_i128`].
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

/// Floor a Unix timestamp (seconds since epoch) to the start of its UTC day.
///
/// Equivalent to `ts / SECS_PER_DAY * SECS_PER_DAY`, where
/// `SECS_PER_DAY = 86_400`.  The result is the Unix timestamp of the most
/// recent midnight (00:00:00 UTC) that is ≤ `ts`.
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
/// // Epoch zero is already a midnight boundary.
/// assert_eq!(floor_to_day(0), 0);
///
/// // Mid-day: 2024-01-01 12:00:00 UTC  →  2024-01-01 00:00:00 UTC
/// assert_eq!(floor_to_day(1_704_067_200 + 43_200), 1_704_067_200);
///
/// // Last second of the day floors back to the same midnight.
/// assert_eq!(floor_to_day(86_399), 0);
/// ```
#[inline]
#[must_use]
pub fn floor_to_day(ts: u64) -> u64 {
    (ts / SECONDS_PER_DAY) * SECONDS_PER_DAY
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

/// Checked `i128` addition returning a typed error instead of panicking.
///
/// Returns `Ok(sum)` on success, or [`ContractError::Overflow`] when the
/// addition would exceed `i128::MIN` / `i128::MAX`.
///
/// This is the typed counterpart to [`add_i128`]; prefer it on paths where
/// overflow is a reachable runtime state so callers receive a wire-stable
/// error code rather than a free-form panic string.
///
/// # Examples
///
/// ```
/// use credence_math::checked_add_or_error;
/// use credence_errors::ContractError;
///
/// assert_eq!(checked_add_or_error(1, 2), Ok(3));
/// assert_eq!(checked_add_or_error(i128::MAX, 1), Err(ContractError::Overflow));
/// assert_eq!(checked_add_or_error(i128::MIN, -1), Err(ContractError::Overflow));
/// ```
#[inline]
pub fn checked_add_or_error(a: i128, b: i128) -> Result<i128, ContractError> {
    a.checked_add(b).ok_or(ContractError::Overflow)
}

/// Compute `a * b / denom` over a 256-bit intermediate, **panicking** on overflow
/// or `denom == 0`.
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

/// Compute `a * b / denom` over a 256-bit intermediate with **saturating**
/// semantics.
///
/// Unlike [`mul_div_i128`] — which panics on overflow or `denom == 0` — this
/// helper silently **clamps** the result to `i128::MIN` / `i128::MAX` and
/// **returns `0`** when `denom == 0`. Use it on UX/aggregation paths that must
/// never revert the transaction.
///
/// # Examples
///
/// ```
/// use credence_math::{sat_mul_div_i128, Rounding};
///
/// // Saturation at upper bound: never panics, clamps to i128::MAX.
/// assert_eq!(sat_mul_div_i128(i128::MAX, 2, 1, Rounding::Down), i128::MAX);
/// // Saturation at lower bound: clamps to i128::MIN.
/// assert_eq!(sat_mul_div_i128(i128::MIN, 2, 1, Rounding::Down), i128::MIN);
/// // Zero denominator is treated as zero (no panic).
/// assert_eq!(sat_mul_div_i128(10, 3, 0, Rounding::Down), 0);
/// // Basic rounding semantics:
/// assert_eq!(sat_mul_div_i128(10, 3, 4, Rounding::Down), 7);
/// assert_eq!(sat_mul_div_i128(10, 3, 4, Rounding::Up), 8);
/// ```
#[inline]
#[must_use]
pub fn sat_mul_div_i128(a: i128, b: i128, denom: i128, mode: Rounding) -> i128 {
    if denom == 0 {
        return 0;
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

/// Calculate a basis-point percentage of an `i128` amount: `amount * bps / BPS_DENOMINATOR`.
///
/// # Panics
/// Panics with `mul_msg` on i128 multiplication overflow, with `div_msg` on
/// division by zero. Panics never fire on hot paths because the i128 multiply
/// step is the only widening boundary on this call.
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
///
/// # Panics
/// Panics with `mul_msg` on u64 multiplication overflow.
#[inline]
#[must_use]
pub fn bps_u64(amount: u64, bps: u32, mul_msg: &'static str) -> u64 {
    mul_u64(amount, bps as u64, mul_msg) / BPS_DENOMINATOR as u64
}

/// Split an amount into `(fee, net)` using basis-point math.
///
/// # Panics
/// Panics with `mul_msg` on i128 multiplication overflow, with `div_msg` on
/// division by zero, with `sub_msg` if `fee > amount`.
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

/// Alias for [`sat_mul_bps`].
#[inline]
#[must_use]
pub fn sat_bps(amount: i128, bps_value: u32) -> i128 {
    sat_mul_bps(amount, bps_value)
}

/// Saturating basis-point multiply for `u64` amounts.
///
/// Clamps to `u64::MAX` on overflow. Returns `0` when the effective
/// denominator path would divide by zero (unreachable for `BPS_DENOMINATOR`).
#[inline]
#[must_use]
pub fn sat_mul_bps_u64(amount: u64, bps_value: u32) -> u64 {
    let widened = sat_mul_div_i128(
        amount as i128,
        bps_value as i128,
        BPS_DENOMINATOR,
        Rounding::Down,
    );
    if widened <= 0 {
        0
    } else if widened as u128 > u64::MAX as u128 {
        u64::MAX
    } else {
        widened as u64
    }
}

/// Alias for [`sat_mul_bps_u64`].
#[inline]
#[must_use]
pub fn sat_bps_u64(amount: u64, bps_value: u32) -> u64 {
    sat_mul_bps_u64(amount, bps_value)
}

/// Saturating basis-point multiply that rounds away from zero.
#[inline]
#[must_use]
pub fn sat_bps_round_up(amount: i128, bps_value: u32) -> i128 {
    sat_mul_div_i128(
        amount,
        bps_value as i128,
        BPS_DENOMINATOR,
        Rounding::Up,
    )
}

/// Saturating split into `(fee, net)` using basis points.
///
/// When saturation would make `fee > amount` for a positive amount, `net`
/// saturates to `0` (and symmetrically for negatives).
#[inline]
#[must_use]
pub fn sat_split_bps(amount: i128, bps_value: u32) -> (i128, i128) {
    let fee = sat_mul_bps(amount, bps_value);
    let net = amount.saturating_sub(fee);
    (fee, net)
}

/// Percentage of an amount: `amount * percentage / 100`.
///
/// Uses a 256-bit intermediate via [`mul_div_i128`]. The trailing `_div_msg`
/// is retained as a no-op forward-compat placeholder.
///
/// # Panics
/// Panics with `mul_msg` if the final result does not fit in `i128`.
#[inline]
#[must_use]
pub fn percent(
    amount: i128,
    percentage: u32,
    mul_msg: &'static str,
    _div_msg: &'static str,
) -> i128 {
    mul_div_i128(
        amount,
        percentage as i128,
        PERCENT_DENOMINATOR,
        Rounding::Down,
        mul_msg,
    )
}

/// Percentage of a `u64` amount: `amount * percentage / 100`.
///
/// # Panics
/// Panics with `mul_msg` on `u64` multiplication overflow.
#[inline]
#[must_use]
pub fn percent_u64(amount: u64, percentage: u32, mul_msg: &'static str) -> u64 {
    mul_u64(amount, percentage as u64, mul_msg) / PERCENT_DENOMINATOR as u64
}

/// Percentage of an amount, rounding away from zero.
#[inline]
#[must_use]
pub fn percent_round_up(amount: i128, percentage: u32, msg: &'static str) -> i128 {
    mul_div_i128(
        amount,
        percentage as i128,
        PERCENT_DENOMINATOR,
        Rounding::Up,
        msg,
    )
}

/// Split an amount into `(fee, net)` using percentage math.
///
/// `div_msg` is retained as a no-op forward-compat placeholder.
#[inline]
#[must_use]
pub fn split_percent(
    amount: i128,
    percentage: u32,
    mul_msg: &'static str,
    div_msg: &'static str,
    sub_msg: &'static str,
) -> (i128, i128) {
    let fee = percent(amount, percentage, mul_msg, div_msg);
    let net = sub_i128(amount, fee, sub_msg);
    (fee, net)
}

/// Saturating percentage multiply: `amount * percentage / 100`.
#[inline]
#[must_use]
pub fn sat_percent(amount: i128, percentage: u32) -> i128 {
    sat_mul_div_i128(
        amount,
        percentage as i128,
        PERCENT_DENOMINATOR,
        Rounding::Down,
    )
}

/// Alias for [`sat_percent`].
#[inline]
#[must_use]
pub fn sat_mul_percent(amount: i128, percentage: u32) -> i128 {
    sat_percent(amount, percentage)
}

/// Saturating percentage multiply for `u64` amounts.
#[inline]
#[must_use]
pub fn sat_percent_u64(amount: u64, percentage: u32) -> u64 {
    let widened = sat_mul_div_i128(
        amount as i128,
        percentage as i128,
        PERCENT_DENOMINATOR,
        Rounding::Down,
    );
    if widened <= 0 {
        0
    } else if widened as u128 > u64::MAX as u128 {
        u64::MAX
    } else {
        widened as u64
    }
}

/// Alias for [`sat_percent_u64`].
#[inline]
#[must_use]
pub fn sat_mul_percent_u64(amount: u64, percentage: u32) -> u64 {
    sat_percent_u64(amount, percentage)
}

/// Saturating percentage multiply that rounds away from zero.
#[inline]
#[must_use]
pub fn sat_percent_round_up(amount: i128, percentage: u32) -> i128 {
    sat_mul_div_i128(
        amount,
        percentage as i128,
        PERCENT_DENOMINATOR,
        Rounding::Up,
    )
}

/// Saturating split into `(fee, net)` using percentages.
#[inline]
#[must_use]
pub fn sat_split_percent(amount: i128, percentage: u32) -> (i128, i128) {
    let fee = sat_percent(amount, percentage);
    let net = amount.saturating_sub(fee);
    (fee, net)
}

/// Check that the absolute difference between `requested` and `actual` does not
/// exceed `max_slippage_bps` basis points.
///
/// Returns `Ok(())` when the actual amount is within the slippage tolerance of
/// the requested amount. Returns [`ContractError::SlippageExceeded`] when the
/// slippage exceeds the bound.
///
/// # Arguments
///
/// * `requested` - The expected/requested amount.
/// * `actual` - The realized amount.
/// * `max_slippage_bps` - Maximum allowed slippage in basis points.
///
/// # Examples
///
/// ```
/// use credence_math::slippage_bps_check;
/// use credence_errors::ContractError;
///
/// assert_eq!(slippage_bps_check(1000, 1000, 100), Ok(()));
/// assert_eq!(slippage_bps_check(1000, 990, 100), Ok(()));
/// assert_eq!(slippage_bps_check(1000, 900, 100), Err(ContractError::SlippageExceeded));
/// ```
#[inline]
pub fn slippage_bps_check(
    requested: i128,
    actual: i128,
    max_slippage_bps: u32,
) -> Result<(), ContractError> {
    if requested == actual {
        return Ok(());
    }
    if max_slippage_bps == 0 || requested == 0 {
        return Err(ContractError::SlippageExceeded);
    }
    let diff = requested.abs_diff(actual);
    let requested_abs = requested.unsigned_abs();
    if U256::new(diff) * U256::new(BPS_DENOMINATOR as u128)
        <= U256::new(requested_abs) * U256::new(max_slippage_bps as u128)
    {
        Ok(())
    } else {
        Err(ContractError::SlippageExceeded)
    }
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

/// Validate that an array of percentage splits sums to exactly 10,000 bps.
///
/// Returns `ContractError::InvalidPercentSplit` if the splits sum is not exactly `BPS_DENOMINATOR`.
/// Returns `ContractError::Overflow` if the sum exceeds `u32::MAX`.
#[inline]
pub fn require_valid_percent_split(splits: &soroban_sdk::Vec<u32>) -> Result<(), ContractError> {
    let mut sum: u32 = 0;
    for i in 0..splits.len() {
        let split = splits.get(i).unwrap();
        sum = sum.checked_add(split).ok_or(ContractError::Overflow)?;
    }
    if sum != BPS_DENOMINATOR as u32 {
        return Err(ContractError::InvalidPercentSplit);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{
        bps, bps_round_up, bps_u64, ceil_div_i128, div_i128, floor_to_day, mul_div_i128,
        percent, percent_round_up, percent_u64, sat_bps, sat_mul_bps, sat_percent,
        sat_split_bps, sat_split_percent, slippage_bps_check, split_bps, split_percent,
        BPS_DENOMINATOR, Rounding,
    };
    use credence_errors::ContractError;

    // ── floor_to_day ─────────────────────────────────────────────────────────

    /// Epoch zero is already a midnight: flooring it must return 0.
    #[test]
    fn floor_to_day_at_epoch_zero() {
        assert_eq!(floor_to_day(0), 0);
    }

    /// A timestamp that falls exactly on a midnight boundary is unchanged.
    ///
    /// 2024-01-01 00:00:00 UTC = 1_704_067_200
    #[test]
    fn floor_to_day_at_midnight() {
        let midnight: u64 = 1_704_067_200;
        assert_eq!(floor_to_day(midnight), midnight);
    }

    /// A mid-day timestamp floors back to the start of the same UTC day.
    ///
    /// 2024-01-01 12:00:00 UTC = midnight + 43_200  →  midnight
    #[test]
    fn floor_to_day_mid_day() {
        let midnight: u64 = 1_704_067_200;
        let midday = midnight + 43_200; // 12 hours into the day
        assert_eq!(floor_to_day(midday), midnight);
    }

    /// The last second of a day (23:59:59) floors to the same day's midnight.
    ///
    /// 1970-01-01 23:59:59 UTC = 86_399  →  0 (epoch midnight)
    #[test]
    fn floor_to_day_end_of_day() {
        assert_eq!(floor_to_day(86_399), 0);
    }

    /// The last second of an arbitrary day floors to that day's midnight.
    ///
    /// 2024-01-01 23:59:59 UTC  →  2024-01-01 00:00:00 UTC
    #[test]
    fn floor_to_day_last_second_of_arbitrary_day() {
        let midnight: u64 = 1_704_067_200;
        let last_second = midnight + 86_399; // 23:59:59 on the same day
        assert_eq!(floor_to_day(last_second), midnight);
    }

    /// The first second of the next day is the next midnight, not the previous.
    ///
    /// 1970-01-02 00:00:00 UTC = 86_400  →  86_400 (already a boundary)
    #[test]
    fn floor_to_day_first_second_of_next_day() {
        let day2_midnight: u64 = 86_400;
        assert_eq!(floor_to_day(day2_midnight), day2_midnight);
    }

    /// floor_to_day is idempotent: applying it twice gives the same result.
    #[test]
    fn floor_to_day_is_idempotent() {
        let cases: &[u64] = &[0, 1, 43_200, 86_399, 86_400, 1_704_067_200, u64::MAX];
        for &ts in cases {
            assert_eq!(
                floor_to_day(floor_to_day(ts)),
                floor_to_day(ts),
                "idempotent check failed for ts={ts}"
            );
        }
    }

    /// floor_to_day result is always a multiple of 86_400 (seconds-per-day).
    #[test]
    fn floor_to_day_result_is_multiple_of_86400() {
        let cases: &[u64] = &[0, 1, 43_200, 86_399, 86_400, 1_704_067_200, u64::MAX];
        for &ts in cases {
            let result = floor_to_day(ts);
            assert_eq!(
                result % 86_400,
                0,
                "result {result} is not a multiple of 86_400 (input ts={ts})"
            );
        }
    }

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
    fn test_checked_add_or_error() {
        assert_eq!(super::checked_add_or_error(1, 2), Ok(3));
        assert_eq!(
            super::checked_add_or_error(i128::MAX, 1),
            Err(ContractError::Overflow)
        );
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
        // Keep the product inside u64 for the panicking helper.
        assert_eq!(bps_u64(20_000, BPS_DENOMINATOR as u32, "mul"), 20_000);
        // Saturating sibling handles the extreme bound without panicking.
        assert_eq!(super::sat_bps_u64(u64::MAX, BPS_DENOMINATOR as u32), u64::MAX);
    }

    #[test]
    fn test_require_valid_percent_split_valid() {
        let env = soroban_sdk::Env::default();
        let mut splits = soroban_sdk::Vec::new(&env);
        splits.push_back(5000);
        splits.push_back(5000);
        assert_eq!(crate::require_valid_percent_split(&splits), Ok(()));

        let mut splits2 = soroban_sdk::Vec::new(&env);
        splits2.push_back(10000);
        assert_eq!(crate::require_valid_percent_split(&splits2), Ok(()));

        let mut splits3 = soroban_sdk::Vec::new(&env);
        splits3.push_back(3333);
        splits3.push_back(3333);
        splits3.push_back(3334);
        assert_eq!(crate::require_valid_percent_split(&splits3), Ok(()));
    }

    #[test]
    fn test_require_valid_percent_split_less_than() {
        let env = soroban_sdk::Env::default();
        let mut splits = soroban_sdk::Vec::new(&env);
        splits.push_back(5000);
        splits.push_back(4999);
        assert_eq!(
            crate::require_valid_percent_split(&splits),
            Err(ContractError::InvalidPercentSplit)
        );

        let splits_empty = soroban_sdk::Vec::new(&env); // empty sums to 0
        assert_eq!(
            crate::require_valid_percent_split(&splits_empty),
            Err(ContractError::InvalidPercentSplit)
        );
    }

    #[test]
    fn test_require_valid_percent_split_greater_than() {
        let env = soroban_sdk::Env::default();
        let mut splits = soroban_sdk::Vec::new(&env);
        splits.push_back(5000);
        splits.push_back(5001);
        assert_eq!(
            crate::require_valid_percent_split(&splits),
            Err(ContractError::InvalidPercentSplit)
        );

        let mut splits2 = soroban_sdk::Vec::new(&env);
        splits2.push_back(10001);
        assert_eq!(
            crate::require_valid_percent_split(&splits2),
            Err(ContractError::InvalidPercentSplit)
        );
    }

    #[test]
    fn test_require_valid_percent_split_overflow() {
        let env = soroban_sdk::Env::default();
        let mut splits = soroban_sdk::Vec::new(&env);
        splits.push_back(u32::MAX);
        splits.push_back(1);
        assert_eq!(
            crate::require_valid_percent_split(&splits),
            Err(ContractError::Overflow)
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

    #[test]
    fn slippage_bps_check_exact_match() {
        assert_eq!(slippage_bps_check(1000, 1000, 100), Ok(()));
        assert_eq!(slippage_bps_check(0, 0, 100), Ok(()));
        assert_eq!(slippage_bps_check(-1000, -1000, 100), Ok(()));
    }

    #[test]
    fn slippage_bps_check_zero_slippage_tolerance() {
        assert_eq!(slippage_bps_check(1000, 1000, 0), Ok(()));
        assert_eq!(
            slippage_bps_check(1000, 1001, 0),
            Err(ContractError::SlippageExceeded)
        );
    }

    #[test]
    fn slippage_bps_check_within_tolerance() {
        assert_eq!(slippage_bps_check(1000, 995, 100), Ok(()));
        assert_eq!(slippage_bps_check(1000, 1005, 100), Ok(()));
        assert_eq!(slippage_bps_check(1000, 990, 100), Ok(()));
        assert_eq!(slippage_bps_check(1000, 1010, 100), Ok(()));
    }

    #[test]
    fn slippage_bps_check_at_boundary() {
        assert_eq!(slippage_bps_check(10000, 9900, 100), Ok(()));
        assert_eq!(slippage_bps_check(10000, 10100, 100), Ok(()));
        assert_eq!(
            slippage_bps_check(10000, 9899, 100),
            Err(ContractError::SlippageExceeded)
        );
        assert_eq!(
            slippage_bps_check(10000, 10101, 100),
            Err(ContractError::SlippageExceeded)
        );
    }

    #[test]
    fn slippage_bps_check_beyond_tolerance() {
        assert_eq!(
            slippage_bps_check(1000, 900, 100),
            Err(ContractError::SlippageExceeded)
        );
        assert_eq!(
            slippage_bps_check(1000, 1100, 100),
            Err(ContractError::SlippageExceeded)
        );
    }

    #[test]
    fn slippage_bps_check_zero_requested() {
        assert_eq!(
            slippage_bps_check(0, 1, 100),
            Err(ContractError::SlippageExceeded)
        );
        assert_eq!(
            slippage_bps_check(0, 100, 100),
            Err(ContractError::SlippageExceeded)
        );
    }

    #[test]
    fn slippage_bps_check_negative_values() {
        assert_eq!(slippage_bps_check(-1000, -1000, 100), Ok(()));
        assert_eq!(slippage_bps_check(-1000, -990, 100), Ok(()));
        assert_eq!(
            slippage_bps_check(-1000, -900, 100),
            Err(ContractError::SlippageExceeded)
        );
    }

    #[test]
    fn slippage_bps_check_large_values() {
        assert_eq!(slippage_bps_check(i128::MAX, i128::MAX, 100), Ok(()));
        assert_eq!(
            slippage_bps_check(i128::MAX, i128::MAX - 1, 0),
            Err(ContractError::SlippageExceeded)
        );
        let tiny_diff = i128::MAX / 10000;
        assert_eq!(
            slippage_bps_check(i128::MAX, i128::MAX - tiny_diff, 100),
            Ok(())
        );
    }

    /// Percentage + saturating helper regression vectors.
    #[test]
    fn percent_and_sat_regression_vectors() {
        assert_eq!(percent(10_000, 50, "mul", "div"), 5_000);
        assert_eq!(percent_u64(10_000, 25, "mul"), 2_500);
        assert_eq!(percent_round_up(101, 50, "mul"), 51); // 50.5 → 51
        assert_eq!(split_percent(1_000, 10, "mul", "div", "sub"), (100, 900));

        assert_eq!(sat_bps(i128::MAX, 10_000), i128::MAX);
        assert_eq!(sat_mul_bps(1_000, 500), 50);
        assert_eq!(sat_percent(1_000, 10), 100);
        assert_eq!(sat_split_bps(1_000, 1_000), (100, 900));
        assert_eq!(sat_split_percent(1_000, 10), (100, 900));

        // Wide intermediate: amount that would overflow naive amount*bps.
        let huge = i128::MAX / 2;
        assert_eq!(sat_bps(huge, 10_000), huge);
        assert_eq!(percent(huge, 100, "mul", "div"), huge);
    }
}

#[cfg(test)]
mod timestamp_floor_tests {
    use super::*;

    #[test]
    fn test_timestamp_floor_to_day() {
        assert_eq!(Timestamp::floor_to_day(0), 0);
        assert_eq!(Timestamp::floor_to_day(86_399), 0);
        assert_eq!(Timestamp::floor_to_day(86_400), 86_400);
        assert_eq!(Timestamp::floor_to_day(86_401), 86_400);
        assert_eq!(Timestamp::floor_to_day(172_800), 172_800);
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

    proptest! {
        #[test]
        fn mul_wad_identity(amount in -1_000_000_000_000_000_000i128..1_000_000_000_000_000_000i128) {
            prop_assert_eq!(mul_wad(amount, WAD, "prop"), amount);
        }
    }
}

#[cfg(test)]
mod proptest_extended {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Rounding direction monotonicity: Up >= Down for all positive inputs.
        #[test]
        fn mul_div_up_ge_down(
            a in -1_000_000_000i128..1_000_000_000i128,
            b in -1_000_000_000i128..1_000_000_000i128,
            d in 1i128..10_001i128
        ) {
            let down = mul_div_i128(a, b, d, Rounding::Down, "prop");
            let up = mul_div_i128(a, b, d, Rounding::Up, "prop");
            prop_assert!(up >= down, "Up {up} < Down {down} for {a}*{b}/{d}");
        }
    }

    proptest! {
        /// Nearest rounding is either floor or ceil (never outside).
        #[test]
        fn nearest_between_down_and_up(
            a in -1_000_000i128..1_000_000i128,
            b in 1i128..1_000i128,
            d in 1i128..1_001i128
        ) {
            let nearest = mul_div_i128(a, b, d, Rounding::Nearest, "prop");
            let down = mul_div_i128(a, b, d, Rounding::Down, "prop");
            let up = mul_div_i128(a, b, d, Rounding::Up, "prop");
            prop_assert!(nearest >= down && nearest <= up,
                "Nearest {nearest} not between Down {down} and Up {up}");
        }
    }

    proptest! {
        /// For positive integers, mul_wad(a, b) / WAD == (a*b)/WAD (exact).
        #[test]
        fn mul_wad_overflow_safe(
            a in 0i128..10_000_000_000i128,
            b in 0i128..10_000_000_000i128
        ) {
            let result = mul_wad(a, b, "prop");
            let expected = (a as i128 * b as i128) / WAD;
            prop_assert_eq!(result, expected);
        }
    }

    proptest! {
        /// sat_mul_wad never panics and returns a value >= 0 for non-negative inputs.
        #[test]
        fn sat_mul_wad_non_negative(
            a in 0i128..i128::MAX,
            b in 0i128..1_000_000_000_000_000_000i128
        ) {
            let result = sat_mul_wad(a, b);
            prop_assert!(result >= 0);
        }
    }

    proptest! {
        /// bps(N, BPS_DENOMINATOR) == N (identity for 100%).
        #[test]
        fn bps_identity(
            amount in -1_000_000i128..1_000_000i128
        ) {
            let result = bps(amount, BPS_DENOMINATOR as u32, "mul", "div");
            prop_assert_eq!(result, amount);
        }
    }

    proptest! {
        /// split_bps partitions amount into fee + net without loss.
        #[test]
        fn split_bps_conserves_value(
            amount in 0i128..1_000_000i128,
            bps_val in 0u32..=BPS_DENOMINATOR as u32
        ) {
            let (fee, net) = split_bps(amount, bps_val, "mul", "div", "sub");
            if bps_val <= BPS_DENOMINATOR as u32 {
                prop_assert_eq!(fee.checked_add(net), Some(amount),
                    "fee {fee} + net {net} != amount {amount}");
            }
        }
    }

    proptest! {
        /// ceil_div_i128(a, b) >= div_i128(a, b) for all positive b.
        #[test]
        fn ceil_div_ge_floor(
            a in -10_000i128..10_000i128,
            b in 1i128..10_000i128
        ) {
            let ceil = ceil_div_i128(a, b, "prop");
            let floor = div_i128(a, b, "prop");
            prop_assert!(ceil >= floor,
                "ceil {ceil} < floor {floor} for {a}/{b}");
        }
    }

    proptest! {
        /// sat_bps never exceeds the amount (for bps <= 10_000).
        #[test]
        fn sat_bps_bounded(
            amount in 0i128..i128::MAX,
            bps_val in 0u32..=BPS_DENOMINATOR as u32
        ) {
            let result = sat_bps(amount, bps_val);
            prop_assert!(result <= amount,
                "sat_bps {result} > amount {amount} at {bps_val} bps");
        }
    }

    proptest! {
        /// floor_to_day is idempotent.
        #[test]
        fn floor_to_day_idempotent(ts in 0..=u64::MAX / 86400 * 86400) {
            let first = floor_to_day(ts);
            let second = floor_to_day(first);
            prop_assert_eq!(first, second);
        }
    }

    proptest! {
        /// floor_to_day is monotonic with respect to its input.
        #[test]
        fn floor_to_day_monotonic(
            a in 0u64..1_000_000_000u64,
            b in 0u64..1_000_000_000u64
        ) {
            let fa = floor_to_day(a);
            let fb = floor_to_day(b);
            if a <= b {
                prop_assert!(fa <= fb,
                    "floor({a})={fa} > floor({b})={fb}");
            }
        }
    }
}
