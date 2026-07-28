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
    let bond = client.create_bond(
        &identity,
        &MIN_BOND_AMOUNT,
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );
    assert_eq!(bond.bonded_amount, MIN_BOND_AMOUNT);
    assert!(bond.active);

    // Test with the largest amount allowed under the default leverage cap.
    let leverage_valid_amount = DEFAULT_MAX_LEVERAGE as i128 * MIN_BOND_AMOUNT;
    let bond2 = client.create_bond(
        &identity,
        &leverage_valid_amount,
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );
    assert_eq!(bond2.bonded_amount, leverage_valid_amount);
    assert!(bond2.active);
}

#[test]
#[should_panic(expected = "bond amount below minimum required")]
fn test_create_bond_with_amount_below_minimum() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    client.create_bond(
        &identity,
        &(MIN_BOND_AMOUNT - 1),
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );
}

#[test]
#[should_panic(expected = "bond amount below minimum required")]
fn test_create_bond_with_zero_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    client.create_bond(
        &identity,
        &0_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );
}

#[test]
#[should_panic(expected = "bond amount cannot be negative")]
fn test_create_bond_with_negative_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    client.create_bond(
        &identity,
        &(-1000_i128),
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );
}

#[test]
#[should_panic(expected = "bond amount exceeds maximum allowed")]
fn test_create_bond_with_amount_above_maximum() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    client.create_bond(
        &identity,
        &(MAX_BOND_AMOUNT + 1),
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );
}

// ============================================================================
// INTEGRATION TESTS WITH TOP_UP
// ============================================================================

#[test]
fn test_top_up_with_valid_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    // Create initial bond
    client.create_bond(
        &identity,
        &MIN_BOND_AMOUNT,
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );

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
    client.create_bond(
        &identity,
        &MIN_BOND_AMOUNT,
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );

    // Try to top up with zero amount
    client.top_up(&identity, &0_i128);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_top_up_with_negative_amount() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_token(&e);

    // Create initial bond
    client.create_bond(
        &identity,
        &MIN_BOND_AMOUNT,
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );

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
    let bond = client.create_bond(
        &identity,
        &MIN_BOND_AMOUNT,
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );
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
    client.create_bond(
        &identity,
        &MIN_BOND_AMOUNT,
        &credence_math::Timestamp::SECONDS_PER_DAY,
    );

    // Try to top up with zero (should fail)
    client.top_up(&identity, &0_i128);
}

// ============================================================================
// ADDRESS VALIDATION REGRESSION TESTS
// ============================================================================

#[test]
fn accepts_valid_recipient_address() {
    let e = Env::default();
    let recipient = Address::generate(&e);
    let contract = Address::generate(&e);

    // Non-self recipient address validation must succeed without panic
    super::validation::validate_recipient(&recipient, &contract);
}

#[test]
#[should_panic(expected = "recipient cannot be the contract itself")]
fn rejects_invalid_self_recipient_address() {
    let e = Env::default();
    let contract = Address::generate(&e);

    // Self recipient validation must panic
    super::validation::validate_recipient(&contract, &contract);
}

#[test]
fn accepts_valid_slash_treasury_address() {
    let e = Env::default();
    let (client, admin, _) = setup_with_token(&e);
    let treasury = Address::generate(&e);

    e.mock_all_auths();
    client.set_slash_treasury(&admin, &treasury);

    let configured = client.get_slash_treasury();
    assert_eq!(configured, Some(treasury));
}

#[test]
fn returns_none_when_slash_treasury_is_unset() {
    let e = Env::default();
    let (client, _, _) = setup_with_token(&e);

    // Unset optional address should return None without panicking
    assert_eq!(client.get_slash_treasury(), None);
}

#[test]
#[should_panic]
fn rejects_unauthorized_slash_treasury_setter() {
    let e = Env::default();
    let (client, _, _) = setup_with_token(&e);
    let stranger = Address::generate(&e);
    let treasury = Address::generate(&e);

    // Unauthorized caller attempting to configure slash treasury must be rejected
    client.set_slash_treasury(&stranger, &treasury);
}

#[test]
fn accepts_valid_liquidation_treasury_address() {
    let e = Env::default();
    let (client, admin, _) = setup_with_token(&e);
    let treasury = Address::generate(&e);

    e.mock_all_auths();
    client.set_liquidation_treasury(&admin, &treasury);

    assert_eq!(client.get_liquidation_treasury(), Some(treasury));
}

#[test]
fn returns_none_when_liquidation_treasury_is_unset() {
    let e = Env::default();
    let (client, _, _) = setup_with_token(&e);

    // Unset liquidation treasury must safely return None
    assert_eq!(client.get_liquidation_treasury(), None);
}

#[test]
#[should_panic]
fn rejects_unauthorized_liquidation_treasury_setter() {
    let e = Env::default();
    let (client, _, _) = setup_with_token(&e);
    let stranger = Address::generate(&e);
    let treasury = Address::generate(&e);

    client.set_liquidation_treasury(&stranger, &treasury);
}

#[test]
fn accepts_valid_pending_upgrade_admin() {
    let e = Env::default();
    let (client, admin, _) = setup_with_token(&e);
    let new_admin = Address::generate(&e);

    e.mock_all_auths();
    client.propose_upgrade_admin(&admin, &new_admin);

    assert_eq!(client.get_pending_upgrade_admin(), Some(new_admin));
}

#[test]
fn returns_none_when_pending_upgrade_admin_is_unset() {
    let e = Env::default();
    let (client, _, _) = setup_with_token(&e);

    assert_eq!(client.get_pending_upgrade_admin(), None);
}

#[test]
#[should_panic]
fn rejects_unauthorized_upgrade_admin_proposer() {
    let e = Env::default();
    let (client, _, _) = setup_with_token(&e);
    let stranger = Address::generate(&e);
    let new_admin = Address::generate(&e);

    client.propose_upgrade_admin(&stranger, &new_admin);
}

#[test]
fn verifies_repeated_address_validation() {
    let e = Env::default();
    let (client, admin, _) = setup_with_token(&e);
    let treasury_1 = Address::generate(&e);
    let treasury_2 = Address::generate(&e);

    e.mock_all_auths();
    client.set_slash_treasury(&admin, &treasury_1);
    assert_eq!(client.get_slash_treasury(), Some(treasury_1));

    client.set_slash_treasury(&admin, &treasury_2);
    assert_eq!(client.get_slash_treasury(), Some(treasury_2));
}

#[test]
fn verifies_authorization_after_state_changes() {
    let e = Env::default();
    let (client, admin, _) = setup_with_token(&e);
    let new_admin = Address::generate(&e);

    e.mock_all_auths();
    client.propose_upgrade_admin(&admin, &new_admin);
    assert_eq!(client.get_pending_upgrade_admin(), Some(new_admin.clone()));

    // Accept upgrade admin transition
    client.accept_upgrade_admin(&new_admin);
    assert_eq!(client.get_pending_upgrade_admin(), None);

    // New admin can now perform admin operations
    let treasury = Address::generate(&e);
    client.set_slash_treasury(&new_admin, &treasury);
    assert_eq!(client.get_slash_treasury(), Some(treasury));
}
