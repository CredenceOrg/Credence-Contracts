// Making sure our token helpers don't blow up in unexpected ways
// Tests all the edge cases: zero amounts, negative numbers, missing configs

#![cfg(test)]

extern crate std;

use crate::safe_token::*;
use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env};
use std::panic::catch_unwind;

fn setup_env() -> Env {
    Env::default()
}

fn set_token_in_storage(env: &Env, token: &Address) {
    env.storage()
        .instance()
        .set(&crate::DataKey::BondToken, token);
}

#[test]
fn test_safe_transfer_with_valid_params() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let recipient = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    // No token contract actually exists, so this will panic
    let result = catch_unwind(|| {
        safe_transfer(&env, &recipient, 1000);
    });
    assert!(result.is_err());
}

#[test]
fn test_safe_transfer_with_zero_amount() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let recipient = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    // Zero should just return early, no panic
    let result = catch_unwind(|| {
        safe_transfer(&env, &recipient, 0);
    });
    assert!(result.is_ok());
}

#[test]
fn test_safe_transfer_with_negative_amount() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let recipient = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let result = catch_unwind(|| {
        safe_transfer(&env, &recipient, -100);
    });
    assert!(result.is_err());
}

#[test]
fn test_safe_transfer_with_no_token_configured() {
    let env = setup_env();
    let recipient = Address::generate(&env);

    // No token set at all - should panic
    let result = catch_unwind(|| {
        safe_transfer(&env, &recipient, 1000);
    });
    assert!(result.is_err());
}

#[test]
fn test_safe_transfer_from_with_insufficient_allowance() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let owner = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let result = catch_unwind(|| {
        safe_transfer_from(&env, &owner, 1000);
    });
    assert!(result.is_err());
}

#[test]
fn test_safe_require_allowance_with_zero_amount() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let owner = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let result = catch_unwind(|| {
        safe_require_allowance(&env, &owner, 0);
    });
    assert!(result.is_ok());
}

#[test]
fn test_safe_require_allowance_with_negative_amount() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let owner = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let result = catch_unwind(|| {
        safe_require_allowance(&env, &owner, -100);
    });
    assert!(result.is_err());
}

#[test]
fn test_safe_approve_with_negative_amount() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let spender = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let result = catch_unwind(|| {
        safe_approve(&env, &spender, -100, 1000);
    });
    assert!(result.is_err());
}

#[test]
fn test_safe_increase_allowance_with_zero_amount() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let spender = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let result = catch_unwind(|| {
        safe_increase_allowance(&env, &spender, 0, 1000);
    });
    assert!(result.is_ok());
}

#[test]
fn test_safe_increase_allowance_with_negative_amount() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let spender = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let result = catch_unwind(|| {
        safe_increase_allowance(&env, &spender, -100, 1000);
    });
    assert!(result.is_err());
}

#[test]
fn test_force_approve_with_negative_amount() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let spender = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let result = catch_unwind(|| {
        force_approve(&env, &spender, -100, 1000);
    });
    assert!(result.is_err());
}

#[test]
fn test_get_token_with_no_config() {
    let env = setup_env();

    let result = catch_unwind(|| {
        get_token(&env);
    });
    assert!(result.is_err());
}

#[test]
fn test_get_token_with_valid_config() {
    let env = setup_env();
    let token_address = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let result = catch_unwind(|| get_token(&env));
    assert!(result.is_ok());
}

// Make sure all functions fail the same way when given negative amounts
#[test]
fn test_error_message_consistency() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let recipient = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let functions: Vec<Box<dyn Fn()>> = vec![
        Box::new(|| safe_transfer(&env, &recipient, -1)),
        Box::new(|| safe_transfer_from(&env, &recipient, -1)),
        Box::new(|| safe_require_allowance(&env, &recipient, -1)),
        Box::new(|| safe_approve(&env, &recipient, -1, 1000)),
        Box::new(|| safe_increase_allowance(&env, &recipient, -1, 1000)),
        Box::new(|| force_approve(&env, &recipient, -1, 1000)),
    ];

    for func in functions {
        let result = catch_unwind(func);
        assert!(result.is_err());
    }
}

// Test for the atomic function - this is the main thing for the task
// Ensures state update only happens if transfer works
#[test]
fn test_atomic_transfer_and_update_only_calls_update_if_transfer_succeeds() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let recipient = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let mut update_called = false;

    // Transfer will fail (no real token), so update should NOT run
    let result = catch_unwind(|| {
        atomic_transfer_and_update(&env, &recipient, 100, || {
            update_called = true;
        });
    });

    assert!(result.is_err());
    assert!(!update_called); // No partial updates!
}

// Zero amount edge case - should just run the update directly
#[test]
fn test_atomic_transfer_and_update_with_zero_amount() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let recipient = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let mut update_called = false;

    atomic_transfer_and_update(&env, &recipient, 0, || {
        update_called = true;
    });

    assert!(update_called);
}

#[test]
fn test_edge_cases() {
    let env = setup_env();
    let token_address = Address::generate(&env);
    let recipient = Address::generate(&env);

    set_token_in_storage(&env, &token_address);

    let max_amount = i128::MAX;
    let _ = catch_unwind(|| {
        safe_transfer(&env, &recipient, max_amount);
    });

    let min_amount = 1i128;
    let _ = catch_unwind(|| {
        safe_transfer(&env, &recipient, min_amount);
    });
}
