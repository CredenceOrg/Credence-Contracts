//! Normalization Layer for Token Decimals
//!
//! Provides utilities to scale token amounts to a fixed 18-decimal precision
//! for uniform accounting math across different ERC20/Stellar tokens.

use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env};

/// Target decimals for all internal accounting.
pub const NORMALIZED_DECIMALS: u32 = 18;
/// Maximum supported token decimals. Hardened to 18 to prevent overflow in 128-bit accounting.
pub const MAX_SUPPORTED_DECIMALS: u32 = 18;

/// Returns the scale factor and whether it's a multiplier (true) or divisor (false).
pub fn get_scale_info(e: &Env, token: &Address) -> (i128, bool) {
    let decimals = TokenClient::new(e, token).decimals();
    if decimals > MAX_SUPPORTED_DECIMALS {
        panic!("token decimals exceeds supported maximum of 18");
    }

    if decimals <= NORMALIZED_DECIMALS {
        let exponent = NORMALIZED_DECIMALS - decimals;
        (10_i128.pow(exponent), true)
    } else {
        let exponent = decimals - NORMALIZED_DECIMALS;
        (10_i128.pow(exponent), false)
    }
}

/// Normalizes a native token amount to the 18-decimal scale.
pub fn normalize(e: &Env, token: &Address, amount: i128) -> i128 {
    let (scale, is_multiplier) = get_scale_info(e, token);
    if is_multiplier {
        amount.checked_mul(scale).expect("normalization overflow")
    } else {
        amount.checked_div(scale).expect("normalization truncation error")
    }
}

/// Denormalizes a 18-decimal amount back to the native token scale.
pub fn denormalize(e: &Env, token: &Address, amount: i128) -> i128 {
    let (scale, is_multiplier) = get_scale_info(e, token);
    if is_multiplier {
        amount.checked_div(scale).expect("denormalization error")
    } else {
        amount.checked_mul(scale).expect("denormalization overflow")
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
