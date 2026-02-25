//! Overflow-safe arithmetic helpers for financial calculations.
//!
//! All functions use checked arithmetic and panic with a descriptive message on
//! overflow/underflow/div-by-zero.

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

/// Calculate a basis-point percentage of an amount: `amount * bps / 10_000`.
///
/// Uses checked arithmetic for intermediate multiplication.
#[inline]
#[must_use]
pub fn bps(amount: i128, bps: u32, mul_msg: &'static str, div_msg: &'static str) -> i128 {
    let numerator = mul_i128(amount, bps as i128, mul_msg);
    div_i128(numerator, 10_000, div_msg)
}
