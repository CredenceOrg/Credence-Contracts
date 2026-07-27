#![cfg(test)]

//! Comprehensive edge-case tests for decimal normalization.
//!
//! Tests cover the full public API of `normalization.rs` across all supported
//! decimal configurations (0, 6, 8, 18) including:
//!
//! - Zero amounts
//! - Scale=1 short-circuit (18 decimals)
//! - Truncation in denormalize
//! - Rounding mode (Down vs Up)
//! - Precision-loss detection helpers
//! - Overflow safety checks
//! - Roundtrip invariants

use crate::normalization::{
    can_denormalize_exactly, can_normalize_safely, denormalize, denormalize_with_rounding,
    get_scale_info, normalize, would_denormalize_to_zero, MAX_SUPPORTED_DECIMALS,
    MIN_SUPPORTED_DECIMALS, NORMALIZED_DECIMALS, Rounding,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

// ── Mock Token ────────────────────────────────────────────────────────────

/// Mock Soroban token with configurable `decimals()`.
#[contract]
pub struct MockDecToken;

#[contractimpl]
impl MockDecToken {
    pub fn decimals(e: Env) -> u32 {
        e.storage()
            .instance()
            .get(&Symbol::new(&e, "decimals"))
            .unwrap_or(18)
    }
    pub fn symbol(e: Env) -> soroban_sdk::String {
        e.storage()
            .instance()
            .get(&Symbol::new(&e, "symbol"))
            .unwrap_or_else(|| soroban_sdk::String::from_str(&e, "TST"))
    }
    pub fn balance(_e: Env, _id: Address) -> i128 {
        0
    }
    pub fn transfer(_e: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn transfer_from(_e: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {}
    pub fn allowance(_e: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
}

fn setup_token(e: &Env, decimals: u32) -> Address {
    let token_id = e.register(MockDecToken, ());
    e.as_contract(&token_id, || {
        e.storage()
            .instance()
            .set(&Symbol::new(e, "decimals"), &decimals);
    });
    token_id
}

// ── get_scale_info ────────────────────────────────────────────────────────

#[test]
fn test_get_scale_info_all_supported() {
    assert_eq!(get_scale_info(0), (1_000_000_000_000_000_000_i128, true));
    assert_eq!(get_scale_info(1), (100_000_000_000_000_000_i128, true));
    assert_eq!(get_scale_info(6), (1_000_000_000_000_i128, true));
    assert_eq!(get_scale_info(8), (10_000_000_000_i128, true));
    assert_eq!(get_scale_info(12), (1_000_000_i128, true));
    assert_eq!(get_scale_info(17), (10_i128, true));
    assert_eq!(get_scale_info(18), (1_i128, true));
}

#[test]
#[should_panic(expected = "exceeds NORMALIZED_DECIMALS")]
fn test_get_scale_info_rejects_above_18() {
    get_scale_info(19);
}

// ── normalize: zero amounts ───────────────────────────────────────────────

#[test]
fn test_normalize_zero_across_decimals() {
    for dec in [0u32, 1, 6, 8, 18] {
        let e = Env::default();
        let token = setup_token(&e, dec);
        assert_eq!(normalize(&e, &token, 0), 0, "zero normalize failed for {} decimals", dec);
    }
}

// ── normalize: scale=1 short-circuit (18 decimals) ────────────────────────

#[test]
fn test_normalize_18_decimals_scale_one() {
    let e = Env::default();
    let token = setup_token(&e, 18);
    assert_eq!(normalize(&e, &token, 42), 42);
    assert_eq!(normalize(&e, &token, i128::MAX), i128::MAX);
    assert_eq!(normalize(&e, &token, 1), 1);
    assert_eq!(normalize(&e, &token, 0), 0);
}

// ── normalize: basic scaling ──────────────────────────────────────────────

#[test]
fn test_normalize_6_decimals() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    // 1_000_000 (1 token in 6 decimals) → 10^18 normalized
    assert_eq!(normalize(&e, &token, 1_000_000), 1_000_000_000_000_000_000_i128);
    // 1 unit (smallest 6-decimal unit) → 10^12 normalized
    assert_eq!(normalize(&e, &token, 1), 1_000_000_000_000_i128);
}

#[test]
fn test_normalize_8_decimals() {
    let e = Env::default();
    let token = setup_token(&e, 8);
    assert_eq!(normalize(&e, &token, 100_000_000), 1_000_000_000_000_000_000_i128);
    assert_eq!(normalize(&e, &token, 1), 10_000_000_000_i128);
}

#[test]
fn test_normalize_0_decimals() {
    let e = Env::default();
    let token = setup_token(&e, 0);
    assert_eq!(normalize(&e, &token, 1), 1_000_000_000_000_000_000_i128);
    assert_eq!(normalize(&e, &token, 42), 42_000_000_000_000_000_000_i128);
}

// ── normalize: negative rejection ─────────────────────────────────────────

#[test]
#[should_panic(expected = "bond amount cannot be negative")]
fn test_normalize_negative() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    normalize(&e, &token, -1);
}

// ── normalize: overflow detection ─────────────────────────────────────────

#[test]
#[should_panic(expected = "normalization overflow")]
fn test_normalize_overflow_6_decimals() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    // i128::MAX * 10^12 would overflow
    normalize(&e, &token, i128::MAX);
}

#[test]
#[should_panic(expected = "normalization overflow")]
fn test_normalize_overflow_0_decimals() {
    let e = Env::default();
    let token = setup_token(&e, 0);
    // i128::MAX * 10^18 would overflow
    normalize(&e, &token, i128::MAX);
}

// ── denormalize: zero amounts ─────────────────────────────────────────────

#[test]
fn test_denormalize_zero_across_decimals() {
    for dec in [0u32, 1, 6, 8, 18] {
        let e = Env::default();
        let token = setup_token(&e, dec);
        assert_eq!(denormalize(&e, &token, 0), 0, "zero denormalize failed for {} decimals", dec);
    }
}

// ── denormalize: scale=1 short-circuit (18 decimals) ──────────────────────

#[test]
fn test_denormalize_18_decimals_scale_one() {
    let e = Env::default();
    let token = setup_token(&e, 18);
    assert_eq!(denormalize(&e, &token, 42), 42);
    assert_eq!(denormalize(&e, &token, i128::MAX), i128::MAX);
    assert_eq!(denormalize(&e, &token, 1), 1);
    assert_eq!(denormalize(&e, &token, 0), 0);
}

// ── denormalize: basic scaling ────────────────────────────────────────────

#[test]
fn test_denormalize_18_to_6() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    assert_eq!(denormalize(&e, &token, 1_000_000_000_000_000_000_i128), 1_000_000);
    assert_eq!(denormalize(&e, &token, 1_000_000_000_000_i128), 1);
}

#[test]
fn test_denormalize_18_to_8() {
    let e = Env::default();
    let token = setup_token(&e, 8);
    assert_eq!(denormalize(&e, &token, 1_000_000_000_000_000_000_i128), 100_000_000);
    assert_eq!(denormalize(&e, &token, 10_000_000_000_i128), 1);
}

// ── denormalize: truncation ───────────────────────────────────────────────

#[test]
fn test_denormalize_truncates_remainder() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    // 10^12 + 999 normalized → 6 decimals: truncates remainder 999
    let native = denormalize(&e, &token, 1_000_000_000_999_i128);
    assert_eq!(native, 1);
}

#[test]
fn test_denormalize_small_amount_truncates_to_zero() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    // 999 normalized → 6 decimals: 999 < 10^12, truncates to 0
    assert_eq!(denormalize(&e, &token, 999), 0);

    // 1 normalized → 6 decimals: 1 < 10^12, truncates to 0
    assert_eq!(denormalize(&e, &token, 1), 0);
}

#[test]
fn test_denormalize_0_decimals_exact_units() {
    let e = Env::default();
    let token = setup_token(&e, 0);
    // 10^18 normalized → 0 decimals: 10^18 / 10^18 = 1
    assert_eq!(denormalize(&e, &token, 1_000_000_000_000_000_000_i128), 1);
    // Amount smaller than 10^18 truncates to 0
    assert_eq!(denormalize(&e, &token, 999_999_999_999_999_999_i128), 0);
}

// ── denormalize: negative rejection ───────────────────────────────────────

#[test]
#[should_panic(expected = "cannot denormalize negative amount")]
fn test_denormalize_negative() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    denormalize(&e, &token, -1);
}

// ── denormalize_with_rounding: Down vs Up ─────────────────────────────────

#[test]
fn test_denormalize_rounding_down_matches_default() {
    let e = Env::default();
    let token = setup_token(&e, 6);

    let amounts = [0, 1, 1_000_000_000_000_i128, 1_000_000_000_001_i128, 1_000_000_000_000_000_000_i128];
    for amt in amounts {
        let default = denormalize(&e, &token, amt);
        let rounding_down = denormalize_with_rounding(&e, &token, amt, Rounding::Down);
        assert_eq!(default, rounding_down, "Rounding::Down should match default for amount={}", amt);
    }
}

#[test]
fn test_denormalize_rounding_up_exact_no_remainder() {
    let e = Env::default();
    let token = setup_token(&e, 6);

    // Exact divisions: Up and Down should agree
    assert_eq!(
        denormalize_with_rounding(&e, &token, 1_000_000_000_000_000_000_i128, Rounding::Up),
        1_000_000,
    );
    assert_eq!(
        denormalize_with_rounding(&e, &token, 1_000_000_000_000_i128, Rounding::Up),
        1,
    );
    // Zero: no remainder
    assert_eq!(
        denormalize_with_rounding(&e, &token, 0, Rounding::Up),
        0,
    );
}

#[test]
fn test_denormalize_rounding_up_with_remainder() {
    let e = Env::default();
    let token = setup_token(&e, 6);

    // 10^12 + 1 normalized: Down→1, Up→2
    let down = denormalize_with_rounding(&e, &token, 1_000_000_000_001_i128, Rounding::Down);
    let up = denormalize_with_rounding(&e, &token, 1_000_000_000_001_i128, Rounding::Up);
    assert_eq!(down, 1);
    assert_eq!(up, 2);
}

#[test]
fn test_denormalize_rounding_up_small_amount() {
    let e = Env::default();
    let token = setup_token(&e, 6);

    // 1 normalized → Down→0, Up→1 (rounds up from 0 to 1)
    assert_eq!(
        denormalize_with_rounding(&e, &token, 1, Rounding::Down),
        0,
    );
    assert_eq!(
        denormalize_with_rounding(&e, &token, 1, Rounding::Up),
        1,
    );
}

#[test]
fn test_denormalize_rounding_up_scale_one() {
    let e = Env::default();
    let token = setup_token(&e, 18);

    // Scale=1: no division, Up and Down are identical
    assert_eq!(
        denormalize_with_rounding(&e, &token, 42, Rounding::Up),
        42,
    );
    assert_eq!(
        denormalize_with_rounding(&e, &token, 42, Rounding::Down),
        42,
    );
}

// ── can_denormalize_exactly ───────────────────────────────────────────────

#[test]
fn test_can_denormalize_exactly_true_cases() {
    let e = Env::default();

    // Scale = 1: always exact
    assert!(can_denormalize_exactly(&e, &setup_token(&e, 18), 42));
    assert!(can_denormalize_exactly(&e, &setup_token(&e, 18), 0));

    // Multiple of scale factor: exact
    let t6 = setup_token(&e, 6);
    assert!(can_denormalize_exactly(&e, &t6, 1_000_000_000_000_i128));
    assert!(can_denormalize_exactly(&e, &t6, 2_000_000_000_000_i128));
    assert!(can_denormalize_exactly(&e, &t6, 0));

    // Exactly at boundary
    let t0 = setup_token(&e, 0);
    assert!(can_denormalize_exactly(&e, &t0, 1_000_000_000_000_000_000_i128));
    assert!(can_denormalize_exactly(&e, &t0, 0));
}

#[test]
fn test_can_denormalize_exactly_false_cases() {
    let e = Env::default();

    // Non-multiple of scale factor
    let t6 = setup_token(&e, 6);
    assert!(!can_denormalize_exactly(&e, &t6, 1));
    assert!(!can_denormalize_exactly(&e, &t6, 1_000_000_000_001_i128));
    assert!(!can_denormalize_exactly(&e, &t6, 999_999_999_999_i128));

    // Negative: always false
    assert!(!can_denormalize_exactly(&e, &t6, -1));
    assert!(!can_denormalize_exactly(&e, &setup_token(&e, 18), -1));
}

// ── would_denormalize_to_zero ─────────────────────────────────────────────

#[test]
fn test_would_denormalize_to_zero_true_cases() {
    let e = Env::default();

    // Amount < scale factor → truncates to zero
    let t6 = setup_token(&e, 6);
    assert!(would_denormalize_to_zero(&e, &t6, 0));
    assert!(would_denormalize_to_zero(&e, &t6, 1));
    assert!(would_denormalize_to_zero(&e, &t6, 999_999_999_999_i128));

    let t8 = setup_token(&e, 8);
    assert!(would_denormalize_to_zero(&e, &t8, 0));
    assert!(would_denormalize_to_zero(&e, &t8, 1));
    assert!(would_denormalize_to_zero(&e, &t8, 9_999_999_999_i128));

    let t0 = setup_token(&e, 0);
    assert!(would_denormalize_to_zero(&e, &t0, 0));
    assert!(would_denormalize_to_zero(&e, &t0, 999_999_999_999_999_999_i128));

    // Negative amounts
    assert!(would_denormalize_to_zero(&e, &t6, -1));
    assert!(would_denormalize_to_zero(&e, &setup_token(&e, 18), -1));
}

#[test]
fn test_would_denormalize_to_zero_false_cases() {
    let e = Env::default();

    // Amount >= scale factor → at least 1 native unit
    let t6 = setup_token(&e, 6);
    assert!(!would_denormalize_to_zero(&e, &t6, 1_000_000_000_000_i128));
    assert!(!would_denormalize_to_zero(&e, &t6, 1_000_000_000_001_i128));

    let t8 = setup_token(&e, 8);
    assert!(!would_denormalize_to_zero(&e, &t8, 10_000_000_000_i128));
    assert!(!would_denormalize_to_zero(&e, &t8, i128::MAX));

    let t18 = setup_token(&e, 18);
    assert!(!would_denormalize_to_zero(&e, &t18, 1)); // scale=1, never zero
    assert!(!would_denormalize_to_zero(&e, &t18, 0)); // zero is true though
}

// ── can_normalize_safely ──────────────────────────────────────────────────

#[test]
fn test_can_normalize_safely_true_cases() {
    let e = Env::default();

    // Scale=1 (18 decimals): always safe
    assert!(can_normalize_safely(&e, &setup_token(&e, 18), i128::MAX));
    assert!(can_normalize_safely(&e, &setup_token(&e, 18), 0));
    assert!(can_normalize_safely(&e, &setup_token(&e, 18), 1));

    // Within overflow boundary
    let t6 = setup_token(&e, 6);
    assert!(can_normalize_safely(&e, &t6, i128::MAX / 1_000_000_000_000_i128));
    assert!(can_normalize_safely(&e, &t6, 0));
    assert!(can_normalize_safely(&e, &t6, 1));
}

#[test]
fn test_can_normalize_safely_false_cases() {
    let e = Env::default();

    // Would overflow
    let t6 = setup_token(&e, 6);
    assert!(!can_normalize_safely(&e, &t6, i128::MAX));

    let t0 = setup_token(&e, 0);
    assert!(!can_normalize_safely(&e, &t0, i128::MAX));

    // Negative amounts
    assert!(!can_normalize_safely(&e, &t6, -1));
    assert!(!can_normalize_safely(&e, &setup_token(&e, 18), -1));
}

// ── Roundtrip invariants ──────────────────────────────────────────────────

#[test]
fn test_normalize_denormalize_roundtrip_6() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    let cases = [0, 1, 1_000_000, 1_000_000_000, 10_000_000_000_i128];
    for &native in &cases {
        let normalized = normalize(&e, &token, native);
        let result = denormalize(&e, &token, normalized);
        assert_eq!(result, native, "roundtrip failed for 6-dec native={}", native);
    }
}

#[test]
fn test_normalize_denormalize_roundtrip_8() {
    let e = Env::default();
    let token = setup_token(&e, 8);
    let cases = [0, 1, 100_000_000, 100_000_000_000_i128];
    for &native in &cases {
        let normalized = normalize(&e, &token, native);
        let result = denormalize(&e, &token, normalized);
        assert_eq!(result, native, "roundtrip failed for 8-dec native={}", native);
    }
}

#[test]
fn test_normalize_denormalize_roundtrip_0() {
    let e = Env::default();
    let token = setup_token(&e, 0);
    let cases = [0, 1, 100, 1_000_000_000_000_000_000_i128];
    for &native in &cases {
        let normalized = normalize(&e, &token, native);
        let result = denormalize(&e, &token, normalized);
        assert_eq!(result, native, "roundtrip failed for 0-dec native={}", native);
    }
}

#[test]
fn test_normalize_denormalize_roundtrip_18() {
    let e = Env::default();
    let token = setup_token(&e, 18);
    let cases = [0, 1, 42, 1_000_000_000_000_000_000_i128, i128::MAX];
    for &native in &cases {
        let normalized = normalize(&e, &token, native);
        let result = denormalize(&e, &token, normalized);
        assert_eq!(result, native, "roundtrip failed for 18-dec native={}", native);
    }
}

// ── Cross-decimal consistency ─────────────────────────────────────────────

#[test]
fn test_1000_tokens_normalize_same_across_decimals() {
    // 1000 tokens in any decimal config should normalize to 1000 * 10^18
    let expectations = [
        (0u32, 1_000_i128),
        (6u32, 1_000_000_000_i128),
        (8u32, 100_000_000_000_i128),
        (18u32, 1_000_000_000_000_000_000_000_i128),
    ];

    for (decimals, native) in expectations {
        let e = Env::default();
        let token = setup_token(&e, decimals);
        let normalized = normalize(&e, &token, native);
        assert_eq!(
            normalized,
            1_000_000_000_000_000_000_000_i128,
            "1000 tokens mismatch for {} decimals (native={})",
            decimals,
            native,
        );
    }
}

// ── Unsupported decimals ──────────────────────────────────────────────────

#[test]
#[should_panic(expected = "UnsupportedDecimals")]
fn test_normalize_rejects_unsupported_high_decimals() {
    let e = Env::default();
    let token = setup_token(&e, 19);
    normalize(&e, &token, 1_000);
}

#[test]
#[should_panic(expected = "UnsupportedDecimals")]
fn test_normalize_rejects_unsupported_low_decimals() {
    let e = Env::default();
    let token = setup_token(&e, MAX_SUPPORTED_DECIMALS + 1);
    normalize(&e, &token, 1_000);
}

// ── Edge: balance = scale - 1 (largest amount that truncates to zero) ─────

#[test]
fn test_denormalize_largest_truncation_to_zero() {
    let e = Env::default();
    // For 6 decimals: scale = 10^12
    // scale - 1 = 999_999_999_999 → denormalizes to 0
    let t6 = setup_token(&e, 6);
    let just_below_scale = 1_000_000_000_000_i128 - 1;
    assert_eq!(denormalize(&e, &t6, just_below_scale), 0);
    assert!(would_denormalize_to_zero(&e, &t6, just_below_scale));

    // At exactly scale → denormalizes to 1
    assert_eq!(denormalize(&e, &t6, 1_000_000_000_000_i128), 1);
    assert!(!would_denormalize_to_zero(&e, &t6, 1_000_000_000_000_i128));
}

#[test]
fn test_denormalize_8_decimals_largest_truncation_to_zero() {
    let e = Env::default();
    let t8 = setup_token(&e, 8);
    // scale = 10^10
    let just_below = 10_000_000_000_i128 - 1;
    assert_eq!(denormalize(&e, &t8, just_below), 0);
    assert_eq!(denormalize(&e, &t8, 10_000_000_000_i128), 1);
}

// ── Edge: maximum safe native amount before overflow ──────────────────────

#[test]
fn test_normalize_max_safe_amount_6_decimals() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    let max_safe = i128::MAX / 1_000_000_000_000_i128;
    assert!(can_normalize_safely(&e, &token, max_safe));
    let normalized = normalize(&e, &token, max_safe);
    assert!(normalized > 0);
}

#[test]
fn test_normalize_max_safe_amount_0_decimals() {
    let e = Env::default();
    let token = setup_token(&e, 0);
    let max_safe = i128::MAX / 1_000_000_000_000_000_000_i128;
    assert!(can_normalize_safely(&e, &token, max_safe));
    let normalized = normalize(&e, &token, max_safe);
    assert!(normalized > 0);
}

// ── Edge: denormalize Rounding::Up overflow protection ────────────────────

#[test]
#[should_panic(expected = "denormalization overflow: rounding up exceeds i128")]
fn test_denormalize_rounding_up_overflow() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    // i128::MAX / 10^12 = ~1.7e26 → remainder exists → Rounding::Up adds 1 → overflow
    denormalize_with_rounding(&e, &token, i128::MAX, Rounding::Up);
}