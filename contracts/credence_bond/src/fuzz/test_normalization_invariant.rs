//! Property-based tests for the normalize/denormalize roundtrip invariant.
//!
//! For any supported decimal in [0, 18] and any safe amount (no overflow),
//! `denormalize(env, token, normalize(env, token, amount)) == amount`.

extern crate std;

use crate::normalization::{
    denormalize, normalize, MAX_SUPPORTED_DECIMALS, MIN_SUPPORTED_DECIMALS, NORMALIZED_DECIMALS,
};
use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

/// Mock token that returns a configurable `decimals()` value.
#[contract]
pub struct MockDecimalToken;

#[contractimpl]
impl MockDecimalToken {
    pub fn decimals(e: Env) -> u32 {
        e.storage()
            .instance()
            .get(&Symbol::new(&e, "decimals"))
            .unwrap_or(18)
    }
    pub fn balance(_e: Env, _id: Address) -> i128 {
        0
    }
    pub fn transfer(_e: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn transfer_from(
        _e: Env,
        _spender: Address,
        _from: Address,
        _to: Address,
        _amount: i128,
    ) {
    }
    pub fn allowance(_e: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
}

fn setup_token(e: &Env, decimals: u32) -> Address {
    let token_id = e.register(MockDecimalToken, ());
    e.as_contract(&token_id, || {
        e.storage()
            .instance()
            .set(&Symbol::new(e, "decimals"), &decimals);
    });
    token_id
}

fn max_safe_amount(decimals: u32) -> i128 {
    let exponent = NORMALIZED_DECIMALS - decimals;
    if exponent == 0 {
        i128::MAX
    } else {
        i128::MAX / 10_i128.pow(exponent)
    }
}

fn roundtrip_strategy() -> impl Strategy<Value = (u32, i128)> {
    (MIN_SUPPORTED_DECIMALS..=MAX_SUPPORTED_DECIMALS)
        .prop_flat_map(|decimals| {
            let max_safe = max_safe_amount(decimals);
            (Just(decimals), 0_i128..=max_safe)
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Invariant: denormalize(normalize(amount)) == amount for all supported
    /// decimal values and all amounts that do not overflow i128 during scaling.
    #[test]
    fn prop_normalize_denormalize_roundtrip(
        (decimals, amount) in roundtrip_strategy(),
    ) {
        let e = Env::default();
        let token = setup_token(&e, decimals);
        let normalized = normalize(&e, &token, amount);
        let result = denormalize(&e, &token, normalized);
        prop_assert_eq!(result, amount);
    }
}

#[test]
#[should_panic(expected = "bond amount cannot be negative")]
fn test_normalize_rejects_negative() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    normalize(&e, &token, -1);
}

#[test]
#[should_panic(expected = "cannot denormalize negative amount")]
fn test_denormalize_rejects_negative() {
    let e = Env::default();
    let token = setup_token(&e, 6);
    denormalize(&e, &token, -1);
}

#[test]
#[should_panic(expected = "outside supported range")]
fn test_decimals_above_max_panics() {
    let e = Env::default();
    let token = setup_token(&e, MAX_SUPPORTED_DECIMALS + 1);
    normalize(&e, &token, 1_000);
}

#[test]
#[should_panic(expected = "outside supported range")]
fn test_decimals_below_min_panics() {
    let e = Env::default();
    let token = setup_token(&e, 255);
    normalize(&e, &token, 1_000);
}

#[test]
fn test_zero_decimals_boundary_roundtrip() {
    let e = Env::default();
    let token = setup_token(&e, 0);
    let max = max_safe_amount(0);
    let normalized = normalize(&e, &token, max);
    let result = denormalize(&e, &token, normalized);
    assert_eq!(result, max);
}

#[test]
#[should_panic(expected = "normalization overflow")]
fn test_overflow_panics_in_normalize() {
    let e = Env::default();
    let token = setup_token(&e, 0);
    let too_big = max_safe_amount(0) + 1;
    normalize(&e, &token, too_big);
}
