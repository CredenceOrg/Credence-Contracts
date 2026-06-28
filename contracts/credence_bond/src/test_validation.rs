//! Tests for Bond Amount Validation Module
//!
//! Tests the validation functions for bond amounts to ensure they properly enforce
//! minimum and maximum limits.
//!

#![cfg(test)]

use super::parameters::DEFAULT_MAX_LEVERAGE;
use super::validation::{validate_bond_amount, MAX_BOND_AMOUNT, MIN_BOND_AMOUNT};
use super::CredenceBondClient;
use crate::test_helpers;

use soroban_sdk::{Address, Env};

fn setup_with_token(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity, ..) = test_helpers::setup_with_token(e);
    (client, admin, identity)
}

// ============================================================================
// UNIT TESTS FOR VALIDATION MODULE
// ============================================================================

#[test]
fn test_validate_bond_amount_valid() {
    // Test valid amounts within range
    validate_bond_amount(MIN_BOND_AMOUNT);
    validate_bond_amount(MAX_BOND_AMOUNT);
    validate_bond_amount((MIN_BOND_AMOUNT + MAX_BOND_AMOUNT) / 2);
    validate_bond_amount(MIN_BOND_AMOUNT + 1);
    validate_bond_amount(MAX_BOND_AMOUNT - 1);
}

#[test]
#[should_panic(expected = "bond amount below minimum required")]
fn test_validate_bond_amount_below_minimum() {
    validate_bond_amount(MIN_BOND_AMOUNT - 1);
}

#[test]
#[should_panic(expected = "bond amount below minimum required")]
fn test_validate_bond_amount_zero() {
    validate_bond_amount(0);
}

#[test]
#[should_panic(expected = "bond amount cannot be negative")]
fn test_validate_bond_amount_negative() {
    validate_bond_amount(-1);
}

#[test]
#[should_panic(expected = "bond amount cannot be negative")]
fn test_validate_bond_amount_large_negative() {
    validate_bond_amount(-1000);
}

#[test]
#[should_panic(expected = "bond amount exceeds maximum allowed")]
fn test_validate_bond_amount_above_maximum() {
    validate_bond_amount(MAX_BOND_AMOUNT + 1);
}

#[test]
#[should_panic(expected = "bond amount exceeds maximum allowed")]
fn test_validate_bond_amount_max_i128() {
    validate_bond_amount(i128::MAX);
}

// ============================================================================
// INTEGRATION TESTS WITH CREATE_BOND
// ============================================================================

#[test]
fn test_create_bond_with_valid_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    // Test with minimum valid amount
    let bond = client.create_bond(&identity, &MIN_BOND_AMOUNT, &86400_u64);
    assert_eq!(bond.bonded_amount, MIN_BOND_AMOUNT);
    assert!(bond.active);

    // Test with the largest amount allowed under the default leverage cap.
    let leverage_valid_amount = DEFAULT_MAX_LEVERAGE as i128 * MIN_BOND_AMOUNT;
    let bond2 = client.create_bond(&identity, &leverage_valid_amount, &86400_u64);
    assert_eq!(bond2.bonded_amount, leverage_valid_amount);
    assert!(bond2.active);
}

#[test]
#[should_panic(expected = "bond amount below minimum required")]
fn test_create_bond_with_amount_below_minimum() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    client.create_bond(&identity, &(MIN_BOND_AMOUNT - 1), &86400_u64);
}

#[test]
#[should_panic(expected = "bond amount below minimum required")]
fn test_create_bond_with_zero_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    client.create_bond(&identity, &0_i128, &86400_u64);
}

#[test]
#[should_panic(expected = "bond amount cannot be negative")]
fn test_create_bond_with_negative_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    client.create_bond(&identity, &(-1000_i128), &86400_u64);
}

#[test]
#[should_panic(expected = "bond amount exceeds maximum allowed")]
fn test_create_bond_with_amount_above_maximum() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    client.create_bond(&identity, &(MAX_BOND_AMOUNT + 1), &86400_u64);
}

// ============================================================================
// INTEGRATION TESTS WITH TOP_UP
// ============================================================================

#[test]
fn test_top_up_with_valid_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    // Create initial bond
    client.create_bond(&identity, &MIN_BOND_AMOUNT, &86400_u64);

    // Top up with valid amount
    let bond = client.top_up(&identity, &1000); // 1 additional unit
    assert_eq!(bond.bonded_amount, MIN_BOND_AMOUNT + 1000);
    assert!(bond.active);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_top_up_with_zero_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    // Create initial bond
    client.create_bond(&identity, &MIN_BOND_AMOUNT, &86400_u64);

    // Try to top up with zero amount
    client.top_up(&identity, &0_i128);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_top_up_with_negative_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    // Create initial bond
    client.create_bond(&identity, &MIN_BOND_AMOUNT, &86400_u64);

    // Try to top up with negative amount
    client.top_up(&identity, &(-1000_i128));
}

// ============================================================================
// BOUNDARY VALUE TESTS
// ============================================================================

#[test]
fn test_boundary_values() {
    // Test exactly at minimum boundary
    validate_bond_amount(MIN_BOND_AMOUNT);

    // Test exactly at maximum boundary
    validate_bond_amount(MAX_BOND_AMOUNT);

    // Test just above minimum
    validate_bond_amount(MIN_BOND_AMOUNT + 1);

    // Test just below maximum
    validate_bond_amount(MAX_BOND_AMOUNT - 1);
}

// ============================================================================
// ERROR MESSAGE VERIFICATION
// ============================================================================

#[test]
#[should_panic(expected = "bond amount below minimum required: 999 (minimum: 1000)")]
fn test_error_message_includes_amount_and_minimum() {
    validate_bond_amount(999); // MIN_BOND_AMOUNT - 1
}

#[test]
#[should_panic(
    expected = "bond amount exceeds maximum allowed: 100000000000001 (maximum: 100000000000000)"
)]
fn test_error_message_includes_amount_and_maximum() {
    validate_bond_amount(MAX_BOND_AMOUNT + 1);
}

// ============================================================================
// COMBINATION SCENARIOS
// ============================================================================

#[test]
fn test_create_bond_then_top_up_valid_scenario() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    // Create bond with minimum amount
    let bond = client.create_bond(&identity, &MIN_BOND_AMOUNT, &86400_u64);
    assert_eq!(bond.bonded_amount, MIN_BOND_AMOUNT);

    // Top up with valid amount
    let bond = client.top_up(&identity, &1000); // 1 additional unit
    assert_eq!(bond.bonded_amount, MIN_BOND_AMOUNT + 1000);

    // Top up again with another valid amount
    let bond = client.top_up(&identity, &5000); // 5 additional units
    assert_eq!(bond.bonded_amount, MIN_BOND_AMOUNT + 1000 + 5000);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_create_bond_with_min_amount_then_invalid_top_up() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    // Create bond with minimum amount
    client.create_bond(&identity, &MIN_BOND_AMOUNT, &86400_u64);

    // Try to top up with zero (should fail)
    client.top_up(&identity, &0_i128);
}
