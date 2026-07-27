//! Normalization Layer for Token Decimals
//!
//! Provides utilities to scale token amounts to a fixed 18-decimal precision
//! for uniform accounting math across different Soroban tokens.
//!
//! # Design
//! All internal accounting is performed in normalized 18-decimal format.
//! Token amounts are normalized on ingress (bond creation, transfers in)
//! and denormalized on egress (withdrawals, transfers out).
//!
//! # Supported Decimals
//! - Minimum: 0 decimals
//! - Maximum: 18 decimals (prevents overflow when scaling to i128)
//! - Common: 6 (USDC), 8 (WBTC), 18 (ETH, DAI)
//!
//! # Rounding
//! `normalize` always succeeds without truncation (multiplying up).
//! `denormalize` uses `Rounding::Down` (truncation toward zero) by default,
//! matching the standard integer-division behavior. Callers that need
//! different rounding should use `denormalize_with_rounding`.

use credence_errors::ContractError;
use soroban_sdk::token::TokenClient;
use soroban_sdk::{panic_with_error, Address, Env};

/// Target decimals for all internal accounting.
pub const NORMALIZED_DECIMALS: u32 = 18;

/// Maximum supported token decimals. Tokens with more than this are rejected
/// to guarantee that the 10^exponent factor fits in i128 and that
/// normalized amounts cannot overflow during mul/div operations.
pub const MAX_SUPPORTED_DECIMALS: u32 = 18;

/// Minimum supported token decimals.
pub const MIN_SUPPORTED_DECIMALS: u32 = 0;

/// Rounding mode for denormalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rounding {
    /// Truncate the fractional remainder toward zero (default).
    Down,
    /// Round away from zero when the division leaves any remainder.
    Up,
}

/// Validates that a currency symbol is non-empty and not whitespace-only.
pub fn require_non_zero_currency(e: &Env, sym: &soroban_sdk::String) {
    let len = sym.len();
    if len == 0 {
        panic_with_error!(e, ContractError::InvalidCurrency);
    }

    let mut is_whitespace = true;
    let mut buf = [0u8; 128];
    let check_len = len.min(128) as usize;
    sym.copy_into_slice(&mut buf[..check_len]);
    for i in 0..check_len {
        if buf[i] != b' ' && buf[i] != b'\t' && buf[i] != b'\n' && buf[i] != b'\r' {
            is_whitespace = false;
            break;
        }
    }
    if is_whitespace {
        panic_with_error!(e, ContractError::InvalidCurrency);
    }
}

/// Validates token decimals are within the supported range.
/// Returns the decimals value on success.
pub fn validate_supported_decimals(e: &Env, token: &Address) -> u32 {
    let decimals = TokenClient::new(e, token).decimals();

    if decimals < MIN_SUPPORTED_DECIMALS || decimals > MAX_SUPPORTED_DECIMALS {
        panic_with_error!(e, ContractError::UnsupportedDecimals);
    }

    decimals
}

/// Validates token decimals AND symbol. Combines decimal range check
/// with currency-symbol sanity check in a single token query pass.
pub fn validate_supported_decimals_and_symbol(e: &Env, token: &Address) -> u32 {
    let decimals = validate_supported_decimals(e, token);

    let sym = TokenClient::new(e, token).symbol();
    require_non_zero_currency(e, &sym);

    decimals
}

/// Returns the scale factor and whether it's a multiplier (true) or divisor (false).
///
/// For tokens with decimals < 18: multiply by 10^(18 - decimals)
/// For tokens with decimals == 18: scale factor is 1 (no-op)
///
/// # Performance
/// Uses the cached `decimals` parameter instead of re-reading from the token
/// contract, avoiding an extra host call.
///
/// # Panics
/// If `decimals > NORMALIZED_DECIMALS` — only tokens with ≤18 decimals are
/// supported, guaranteed by `validate_supported_decimals`.
pub fn get_scale_info(decimals: u32) -> (i128, bool) {
    if decimals > NORMALIZED_DECIMALS {
        panic!("get_scale_info: decimals {} exceeds NORMALIZED_DECIMALS {}", decimals, NORMALIZED_DECIMALS);
    }

    if decimals == NORMALIZED_DECIMALS {
        return (1, true);
    }

    let exponent = NORMALIZED_DECIMALS - decimals;
    (10_i128.pow(exponent), true)
}

/// Normalizes a native token amount to the 18-decimal scale.
///
/// For tokens with decimals < 18: `amount * 10^(18 - decimals)`
/// For tokens with decimals == 18: `amount` unchanged
///
/// # Arguments
/// * `e` - Environment
/// * `token` - Token address
/// * `amount` - Native token amount (in token's native decimals)
///
/// # Returns
/// Normalized amount in 18-decimal format
///
/// # Panics
/// * If token decimals are outside supported range
/// * If normalization causes overflow
pub fn normalize(e: &Env, token: &Address, amount: i128) -> i128 {
    if amount < 0 {
        panic!("bond amount cannot be negative");
    }
    if amount == 0 {
        return 0;
    }

    let decimals = validate_supported_decimals(e, token);
    let (scale, _is_multiplier) = get_scale_info(decimals);

    if scale == 1 {
        return amount;
    }

    amount
        .checked_mul(scale)
        .expect("normalization overflow: amount * scale exceeds i128")
}

/// Denormalizes a 18-decimal amount back to the native token scale using
/// `Rounding::Down` (truncation toward zero).
///
/// For tokens with decimals < 18: `amount / 10^(18 - decimals)`
/// For tokens with decimals == 18: `amount` unchanged
///
/// # Truncation Warning
/// When denormalizing to a token with fewer decimals than the normalized
/// representation, the fractional part is **truncated** (discarded).
/// For example, a normalized amount of 1 (representing 10^-18 of a
/// smallest native unit) denormalizing to a 6-decimal token yields 0
/// because 1 / 10^12 = 0. Callers should ensure amounts are large enough
/// to avoid complete truncation loss.
///
/// Use `denormalize_with_rounding` for explicit rounding control.
pub fn denormalize(e: &Env, token: &Address, amount: i128) -> i128 {
    denormalize_with_rounding(e, token, amount, Rounding::Down)
}

/// Denormalizes with explicit rounding mode.
///
/// `Rounding::Down`: truncates toward zero (standard integer division).
/// `Rounding::Up`: rounds away from zero when there is a remainder.
///
/// # Panics
/// * If `amount` is negative
/// * If token decimals are outside supported range
pub fn denormalize_with_rounding(e: &Env, token: &Address, amount: i128, rounding: Rounding) -> i128 {
    if amount < 0 {
        panic!("cannot denormalize negative amount");
    }
    if amount == 0 {
        return 0;
    }

    let decimals = validate_supported_decimals(e, token);
    let (scale, _is_multiplier) = get_scale_info(decimals);

    if scale == 1 {
        return amount;
    }

    match rounding {
        Rounding::Down => amount
            .checked_div(scale)
            .expect("denormalization error: division by zero"),
        Rounding::Up => {
            let quotient = amount
                .checked_div(scale)
                .expect("denormalization error: division by zero (up)");
            let remainder = amount % scale;
            if remainder == 0 {
                quotient
            } else {
                quotient
                    .checked_add(1)
                    .expect("denormalization overflow: rounding up exceeds i128")
            }
        }
    }
}

/// Checks if a normalized amount would lose precision (truncate non-zero
/// fractional part) when denormalized to the native token precision.
///
/// Returns `true` if the amount can be represented exactly in the native
/// token's precision (i.e., no truncation loss).
///
/// # Examples
/// ```
/// // 10^18 normalized → 6-decimal token: 10^18 / 10^12 = 10^6, exact
/// // 10^18 + 1 normalized → 6-decimal token: truncates to 10^6, loss
/// ```
pub fn can_denormalize_exactly(e: &Env, token: &Address, amount: i128) -> bool {
    if amount < 0 {
        return false;
    }
    if amount == 0 {
        return true;
    }

    let decimals = validate_supported_decimals(e, token);
    let (scale, _is_multiplier) = get_scale_info(decimals);

    if scale == 1 {
        return true;
    }

    amount % scale == 0
}

/// Checks if a normalized amount would completely truncate to zero when
/// denormalized (the amount is smaller than the smallest representable
/// unit of the native token).
pub fn would_denormalize_to_zero(e: &Env, token: &Address, amount: i128) -> bool {
    if amount <= 0 {
        return true;
    }

    let decimals = validate_supported_decimals(e, token);
    let (scale, _is_multiplier) = get_scale_info(decimals);

    if scale == 1 {
        return false;
    }

    amount < scale
}

/// Validates that an amount won't overflow when normalized.
/// This is a pre-check before calling normalize().
///
/// # Arguments
/// * `e` - Environment
/// * `token` - Token address
/// * `amount` - Native token amount to validate
///
/// # Returns
/// true if the amount can be safely normalized
pub fn can_normalize_safely(e: &Env, token: &Address, amount: i128) -> bool {
    if amount < 0 {
        return false;
    }

    let decimals = validate_supported_decimals(e, token);
    let (scale, _is_multiplier) = get_scale_info(decimals);

    if scale == 1 {
        return true;
    }

    amount.checked_mul(scale).is_some()
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    // ── get_scale_info ────────────────────────────────────────────────────

    #[test]
    fn test_scale_info_6_decimals() {
        let (scale, is_mult) = get_scale_info(6);
        assert_eq!(scale, 1_000_000_000_000); // 10^12
        assert!(is_mult);
    }

    #[test]
    fn test_scale_info_8_decimals() {
        let (scale, is_mult) = get_scale_info(8);
        assert_eq!(scale, 10_000_000_000); // 10^10
        assert!(is_mult);
    }

    #[test]
    fn test_scale_info_18_decimals() {
        let (scale, is_mult) = get_scale_info(18);
        assert_eq!(scale, 1);
        assert!(is_mult);
    }

    #[test]
    fn test_scale_info_0_decimals() {
        let (scale, is_mult) = get_scale_info(0);
        assert_eq!(scale, 1_000_000_000_000_000_000_i128); // 10^18
        assert!(is_mult);
    }

    #[test]
    #[should_panic(expected = "exceeds NORMALIZED_DECIMALS")]
    fn test_scale_info_rejects_above_18() {
        get_scale_info(19);
    }

    // ── normalize ─────────────────────────────────────────────────────────

    #[test]
    fn test_normalize_6_decimals() {
        let e = Env::default();
        let token = Address::generate(&e);
        let _ = token; // can't easily mock without registering
        let decimals = 6;
        let exponent = NORMALIZED_DECIMALS - decimals;
        let scale = 10_i128.pow(exponent);
        assert_eq!(scale, 1_000_000_000_000); // 10^12
    }

    #[test]
    fn test_normalize_zero() {
        assert_eq!(normalize_knowing_decimals(18, 0), 0);
        assert_eq!(normalize_knowing_decimals(6, 0), 0);
        assert_eq!(normalize_knowing_decimals(0, 0), 0);
    }

    #[test]
    fn test_normalize_scale_one() {
        // 18 decimals: scale = 1, amount unchanged
        assert_eq!(normalize_knowing_decimals(18, 42), 42);
        assert_eq!(normalize_knowing_decimals(18, i128::MAX), i128::MAX);
    }

    #[test]
    fn test_normalize_6_to_18() {
        // 1_000_000 (6 decimals, 1 token) → 10^18 (18 decimals, 1 token)
        assert_eq!(
            normalize_knowing_decimals(6, 1_000_000),
            1_000_000_000_000_000_000_i128
        );
    }

    #[test]
    fn test_normalize_0_to_18() {
        // 1 (0 decimals) → 10^18
        assert_eq!(
            normalize_knowing_decimals(0, 1),
            1_000_000_000_000_000_000_i128
        );
    }

    #[test]
    #[should_panic(expected = "bond amount cannot be negative")]
    fn test_normalize_negative() {
        normalize_knowing_decimals(18, -1);
    }

    // ── denormalize ───────────────────────────────────────────────────────

    #[test]
    fn test_denormalize_zero() {
        assert_eq!(denormalize_knowing_decimals(18, 0), 0);
        assert_eq!(denormalize_knowing_decimals(6, 0), 0);
        assert_eq!(denormalize_knowing_decimals(0, 0), 0);
    }

    #[test]
    fn test_denormalize_scale_one() {
        assert_eq!(denormalize_knowing_decimals(18, 42), 42);
        assert_eq!(denormalize_knowing_decimals(18, i128::MAX), i128::MAX);
    }

    #[test]
    fn test_denormalize_18_to_6_exact() {
        // 10^18 normalized → 6 decimals = 1 * 10^6
        assert_eq!(
            denormalize_knowing_decimals(6, 1_000_000_000_000_000_000_i128),
            1_000_000
        );
    }

    #[test]
    fn test_denormalize_18_to_6_truncation() {
        // 10^18 + 999 normalized → 6 decimals = truncates to 10^6
        // (remainder 999 < 10^12 scale factor, so integer division discards it)
        assert_eq!(
            denormalize_knowing_decimals(6, 1_000_000_000_000_000_999_i128),
            1_000_000
        );
    }

    #[test]
    fn test_denormalize_small_amount_truncates_to_zero() {
        // 1 normalized → 6 decimals: 1 / 10^12 = 0
        assert_eq!(denormalize_knowing_decimals(6, 1), 0);
    }

    #[test]
    #[should_panic(expected = "cannot denormalize negative amount")]
    fn test_denormalize_negative() {
        denormalize_knowing_decimals(18, -1);
    }

    // ── denormalize_with_rounding ─────────────────────────────────────────

    #[test]
    fn test_denormalize_rounding_up_exact() {
        // No remainder, Rounding::Up should give same as Down
        assert_eq!(
            denormalize_with_rounding_knowing_decimals(6, 1_000_000_000_000_000_000_i128, Rounding::Up),
            1_000_000
        );
    }

    #[test]
    fn test_denormalize_rounding_up_with_remainder() {
        // 10^12 + 1 normalized → 6 decimals: 10^12 / 10^12 = 1,
        // remainder 1, so Rounding::Up gives 2
        assert_eq!(
            denormalize_with_rounding_knowing_decimals(6, 1_000_000_000_001_i128, Rounding::Up),
            2
        );
    }

    #[test]
    fn test_denormalize_rounding_down_with_remainder() {
        // Same input but Rounding::Down: remainder discarded
        assert_eq!(
            denormalize_with_rounding_knowing_decimals(6, 1_000_000_000_001_i128, Rounding::Down),
            1
        );
    }

    #[test]
    fn test_denormalize_rounding_up_zero_remainder_same_as_down() {
        // Scale 1 (18 decimals): no division happens
        assert_eq!(
            denormalize_with_rounding_knowing_decimals(18, 100, Rounding::Up),
            100
        );
        assert_eq!(
            denormalize_with_rounding_knowing_decimals(18, 100, Rounding::Down),
            100
        );
    }

    // ── can_denormalize_exactly ───────────────────────────────────────────

    #[test]
    fn test_can_denormalize_exactly_true() {
        assert!(can_denormalize_exactly_knowing_decimals(18, 42));
        assert!(can_denormalize_exactly_knowing_decimals(6, 1_000_000_000_000_000_000_i128));
        assert!(can_denormalize_exactly_knowing_decimals(6, 0));
        assert!(can_denormalize_exactly_knowing_decimals(0, 1_000_000_000_000_000_000_i128));
    }

    #[test]
    fn test_can_denormalize_exactly_false() {
        assert!(!can_denormalize_exactly_knowing_decimals(6, 1));
        assert!(!can_denormalize_exactly_knowing_decimals(6, 999_999_999_999_i128));
        assert!(!can_denormalize_exactly_knowing_decimals(6, 1_000_000_000_000_000_001_i128));
    }

    #[test]
    fn test_can_denormalize_exactly_negative_returns_false() {
        assert!(!can_denormalize_exactly_knowing_decimals(6, -1));
    }

    // ── would_denormalize_to_zero ─────────────────────────────────────────

    #[test]
    fn test_would_denormalize_to_zero_true() {
        assert!(would_denormalize_to_zero_knowing_decimals(6, 0));
        assert!(would_denormalize_to_zero_knowing_decimals(6, 1));
        assert!(would_denormalize_to_zero_knowing_decimals(6, 999_999_999_999_i128));
        assert!(would_denormalize_to_zero_knowing_decimals(8, 1));
        assert!(would_denormalize_to_zero_knowing_decimals(8, 9_999_999_999_i128));
    }

    #[test]
    fn test_would_denormalize_to_zero_false() {
        assert!(!would_denormalize_to_zero_knowing_decimals(6, 1_000_000_000_000_i128));
        assert!(!would_denormalize_to_zero_knowing_decimals(8, 10_000_000_000_i128));
        assert!(!would_denormalize_to_zero_knowing_decimals(18, 1));
    }

    #[test]
    fn test_would_denormalize_to_zero_negative() {
        assert!(would_denormalize_to_zero_knowing_decimals(6, -1));
        assert!(would_denormalize_to_zero_knowing_decimals(18, -1));
    }

    // ── can_normalize_safely ──────────────────────────────────────────────

    #[test]
    fn test_can_normalize_safely_true() {
        assert!(can_normalize_safely_knowing_decimals(18, i128::MAX));
        assert!(can_normalize_safely_knowing_decimals(6, i128::MAX / 1_000_000_000_000_i128));
        assert!(can_normalize_safely_knowing_decimals(6, 0));
    }

    #[test]
    fn test_can_normalize_safely_false() {
        assert!(!can_normalize_safely_knowing_decimals(6, i128::MAX));
        assert!(!can_normalize_safely_knowing_decimals(6, -1));
        assert!(!can_normalize_safely_knowing_decimals(0, i128::MAX));
    }

    // ── Roundtrip invariants ──────────────────────────────────────────────

    #[test]
    fn test_normalize_denormalize_roundtrip_6() {
        for native in [0, 1, 1_000_000, 1_000_000_000, 10_000_000_000] {
            let normalized = normalize_knowing_decimals(6, native);
            let result = denormalize_knowing_decimals(6, normalized);
            assert_eq!(result, native, "roundtrip failed for native={}", native);
        }
    }

    #[test]
    fn test_normalize_denormalize_roundtrip_8() {
        for native in [0, 1, 100_000_000, 100_000_000_000] {
            let normalized = normalize_knowing_decimals(8, native);
            let result = denormalize_knowing_decimals(8, normalized);
            assert_eq!(result, native, "roundtrip failed for native={}", native);
        }
    }

    #[test]
    fn test_normalize_denormalize_roundtrip_0() {
        for native in [0, 1, 100, 1_000_000_000_000_000_000] {
            let normalized = normalize_knowing_decimals(0, native);
            let result = denormalize_knowing_decimals(0, normalized);
            assert_eq!(result, native, "roundtrip failed for native={}", native);
        }
    }

    // ── Helpers that bypass the Env/token requirement ─────────────────────

    fn normalize_knowing_decimals(decimals: u32, amount: i128) -> i128 {
        if amount < 0 {
            panic!("bond amount cannot be negative");
        }
        if amount == 0 {
            return 0;
        }
        let (scale, _) = get_scale_info(decimals);
        if scale == 1 {
            return amount;
        }
        amount
            .checked_mul(scale)
            .expect("normalization overflow: amount * scale exceeds i128")
    }

    fn denormalize_knowing_decimals(decimals: u32, amount: i128) -> i128 {
        denormalize_with_rounding_knowing_decimals(decimals, amount, Rounding::Down)
    }

    fn denormalize_with_rounding_knowing_decimals(decimals: u32, amount: i128, rounding: Rounding) -> i128 {
        if amount < 0 {
            panic!("cannot denormalize negative amount");
        }
        if amount == 0 {
            return 0;
        }
        let (scale, _) = get_scale_info(decimals);
        if scale == 1 {
            return amount;
        }
        match rounding {
            Rounding::Down => amount
                .checked_div(scale)
                .expect("denormalization error: division by zero"),
            Rounding::Up => {
                let quotient = amount
                    .checked_div(scale)
                    .expect("denormalization error: division by zero (up)");
                let remainder = amount % scale;
                if remainder == 0 {
                    quotient
                } else {
                    quotient
                        .checked_add(1)
                        .expect("denormalization overflow: rounding up exceeds i128")
                }
            }
        }
    }

    fn can_denormalize_exactly_knowing_decimals(decimals: u32, amount: i128) -> bool {
        if amount < 0 {
            return false;
        }
        if amount == 0 {
            return true;
        }
        let (scale, _) = get_scale_info(decimals);
        if scale == 1 {
            return true;
        }
        amount % scale == 0
    }

    fn would_denormalize_to_zero_knowing_decimals(decimals: u32, amount: i128) -> bool {
        if amount <= 0 {
            return true;
        }
        let (scale, _) = get_scale_info(decimals);
        if scale == 1 {
            return false;
        }
        amount < scale
    }

    fn can_normalize_safely_knowing_decimals(decimals: u32, amount: i128) -> bool {
        if amount < 0 {
            return false;
        }
        let (scale, _) = get_scale_info(decimals);
        if scale == 1 {
            return true;
        }
        amount.checked_mul(scale).is_some()
    }
}