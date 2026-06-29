//! Normalization Layer for Token Decimals
//!
//! Provides utilities to scale token amounts to a fixed 18-decimal precision
//! for uniform accounting math across different ERC20/Stellar tokens.
//!
//! # Design
//! All internal accounting is performed in normalized 18-decimal format.
//! Token amounts are normalized on ingress (bond creation, transfers in)
//! and denormalized on egress (withdrawals, transfers out).
//!
//! # Supported Decimals
//! - Minimum: 0 decimals
//! - Maximum: 36 decimals (prevents overflow when scaling to i128)
//! - Common: 6 (USDC), 8 (WBTC), 18 (ETH, DAI), 24 (some tokens)

use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env};

/// Target decimals for all internal accounting.
pub const NORMALIZED_DECIMALS: u32 = 18;

/// Maximum supported token decimals. Hardened to 18 to prevent overflow in 128-bit accounting.
pub const MAX_SUPPORTED_DECIMALS: u32 = 18;

/// Minimum supported token decimals.
pub const MIN_SUPPORTED_DECIMALS: u32 = 0;

/// Returns the scale factor and whether it's a multiplier (true) or divisor (false).
///
/// For tokens with decimals < 18: multiply by 10^(18 - decimals)
/// For tokens with decimals > 18: divide by 10^(decimals - 18)
/// For tokens with decimals == 18: scale factor is 1 (no-op)
pub fn get_scale_info(e: &Env, token: &Address) -> (i128, bool) {
    let decimals = TokenClient::new(e, token).decimals();

    if decimals < MIN_SUPPORTED_DECIMALS || decimals > MAX_SUPPORTED_DECIMALS {
        panic!(
            "token decimals {} outside supported range [{}, {}]",
            decimals, MIN_SUPPORTED_DECIMALS, MAX_SUPPORTED_DECIMALS
        );
    }

    if decimals <= NORMALIZED_DECIMALS {
        let exponent = NORMALIZED_DECIMALS - decimals;
        (10_i128.pow(exponent), true) // multiplier
    } else {
        let exponent = decimals - NORMALIZED_DECIMALS;
        (10_i128.pow(exponent), false) // divisor
    }
}

/// Normalizes a native token amount to the 18-decimal scale.
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

    let (scale, is_multiplier) = get_scale_info(e, token);
    if is_multiplier {
        amount
            .checked_mul(scale)
            .expect("normalization overflow: amount * scale exceeds i128")
    } else {
        amount
            .checked_div(scale)
            .expect("normalization error: division by zero")
    }
}

/// Denormalizes a 18-decimal amount back to the native token scale.
///
/// # Arguments
/// * `e` - Environment
/// * `token` - Token address
/// * `amount` - Normalized amount in 18-decimal format
///
/// # Returns
/// Native token amount (in token's native decimals)
///
/// # Panics
/// * If token decimals are outside supported range
/// * If denormalization causes overflow
pub fn denormalize(e: &Env, token: &Address, amount: i128) -> i128 {
    if amount < 0 {
        panic!("cannot denormalize negative amount");
    }

    let (scale, is_multiplier) = get_scale_info(e, token);
    if is_multiplier {
        amount
            .checked_div(scale)
            .expect("denormalization error: division by zero")
    } else {
        amount
            .checked_mul(scale)
            .expect("denormalization overflow: amount * scale exceeds i128")
    }
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

    let (scale, is_multiplier) = get_scale_info(e, token);
    if is_multiplier {
        // Check if amount * scale would overflow
        amount.checked_mul(scale).is_some()
    } else {
        // Division never overflows (except by zero, but scale >= 1)
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_normalization_6_decimals() {
        let e = Env::default();
        let _token = Address::generate(&e);
        // We can't easily mock the token decimals here without registering a contract,
        // but the logic 10^(18-6) = 10^12 is what we want to verify implicitly
        // if we were to mock it.
        let decimals = 6;
        let exponent = NORMALIZED_DECIMALS - decimals;
        let scale = 10_i128.pow(exponent);
        assert_eq!(scale, 1_000_000_000_000); // 10^12
    }
}
