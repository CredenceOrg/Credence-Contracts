use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

use crate::normalization::{denormalize, get_scale_info, normalize, NORMALIZED_DECIMALS};

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(_e: Env) -> u32 {
        6
    }
}

#[contract]
struct MockTokenHigh;

#[contractimpl]
impl MockTokenHigh {
    pub fn decimals(_e: Env) -> u32 {
        NORMALIZED_DECIMALS + 1
    }
}

#[test]
fn test_get_scale_info_multiplier() {
    let e = Env::default();
    let token_id = e.register(MockToken, ());
    let (scale, is_multiplier) = get_scale_info(&e, &token_id);

    assert!(is_multiplier);
    assert_eq!(scale, 10_i128.pow(NORMALIZED_DECIMALS - 6));
}

#[test]
fn test_normalize_and_denormalize_roundtrip() {
    let e = Env::default();
    let token_id = e.register(MockToken, ());

    let amount = 1_000_000i128;
    let normalized = normalize(&e, &token_id, amount);
    let denorm = denormalize(&e, &token_id, normalized);

    assert_eq!(denorm, amount);
}

#[test]
#[should_panic(expected = "token decimals exceeds supported maximum")]
fn test_get_scale_info_rejects_high_decimals() {
    let e = Env::default();
    let token_id = e.register(MockTokenHigh, ());

    let _ = get_scale_info(&e, &token_id);
}

#[test]
fn test_normalize_divisor_branch() {
    let e = Env::default();
    let token_id = e.register(MockToken, ());

    let (scale, is_multiplier) = get_scale_info(&e, &token_id);
    let amount = scale * 10;
    let normalized = normalize(&e, &token_id, amount);

    assert!(is_multiplier);
    assert_eq!(normalized, amount * scale);
}

#[test]
fn test_denormalize_multiplier_branch() {
    let e = Env::default();
    let token_id = e.register(MockToken, ());

    let (scale, is_multiplier) = get_scale_info(&e, &token_id);
    let amount = scale * 10;
    let denorm = denormalize(&e, &token_id, amount);

    assert!(is_multiplier);
    assert_eq!(denorm, amount / scale);
}
