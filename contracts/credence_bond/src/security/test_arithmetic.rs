//! Arithmetic Security Tests
//!
//! This module contains comprehensive security tests for arithmetic operations
//! to verify overflow and underflow protection in the Credence Bond contract.
//!
//! Test Categories:
//! 1. i128 Overflow Tests - Verify safe handling of large bond amounts
//! 2. u64 Timestamp Overflow Tests - Verify safe handling of bond durations and timestamps
//! 3. Withdrawal Underflow Tests - Verify safe withdrawal operations
//! 4. Slashing Underflow Tests - Verify safe slashing operations
//!
//! All tests use boundary values (max/min values) to ensure robust protection.

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Env;

// ============================================================================
// i128 OVERFLOW TESTS
// ============================================================================

#[test]
fn test_i128_bond_amount_at_max() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Test creating bond with maximum i128 value
    let bond = client.create_bond(&identity, &i128::MAX, &86400_u64);

    assert_eq!(bond.bonded_amount, i128::MAX);
    assert!(bond.active);
}

#[test]
#[should_panic(expected = "top-up caused overflow")]
fn test_i128_overflow_on_top_up() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Create bond with max - 1000
    client.create_bond(&identity, &(i128::MAX - 1000), &86400_u64);

    // Attempt to top up by 2000, which should overflow
    client.top_up(&2000);
}

#[test]
#[should_panic(expected = "top-up caused overflow")]
fn test_i128_overflow_on_max_top_up() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Create bond with max value
    client.create_bond(&identity, &i128::MAX, &86400_u64);

    // Attempt to top up by 1, which should overflow
    client.top_up(&1);
}

#[test]
#[should_panic(expected = "slashing caused overflow")]
fn test_i128_overflow_on_massive_slashing() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Create bond with large amount
    client.create_bond(&identity, &(i128::MAX / 2), &86400_u64);

    // Slash near-maximum amount first
    client.slash(&(i128::MAX / 2));

    // Current slashed_amount is now i128::MAX / 2
    // Attempt to slash more than i128::MAX / 2, which will cause overflow in checked_add
    client.slash(&(i128::MAX / 2 + 2));
}

#[test]
fn test_i128_large_bond_operations() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let large_amount = i128::MAX / 2;

    // Create bond with large amount
    let bond = client.create_bond(&identity, &large_amount, &86400_u64);
    assert_eq!(bond.bonded_amount, large_amount);

    // Top up with another large amount (should succeed as sum < i128::MAX)
    let bond = client.top_up(&(large_amount / 2));
    assert_eq!(bond.bonded_amount, large_amount + (large_amount / 2));
}

#[test]
fn test_negative_bond_amount_handling() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);

    // Test with negative amount (technically allowed by i128, but may be business logic violation)
    // This documents current behavior
    let bond = client.create_bond(&identity, &(-1000), &86400_u64);
    assert_eq!(bond.bonded_amount, -1000);
}

// ============================================================================
// u64 TIMESTAMP OVERFLOW TESTS
// ============================================================================

#[test]
fn test_u64_max_duration() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Test creating bond with maximum u64 duration
    let bond = client.create_bond(&identity, &1000, &u64::MAX);

    assert_eq!(bond.bond_duration, u64::MAX);
}

#[test]
#[should_panic(expected = "duration extension caused overflow")]
fn test_u64_overflow_on_duration_extension() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Create bond with max - 1000 duration
    client.create_bond(&identity, &1000, &(u64::MAX - 1000));

    // Attempt to extend by 2000, which should overflow
    client.extend_duration(&2000);
}

#[test]
#[should_panic(expected = "bond end timestamp would overflow")]
fn test_u64_overflow_on_end_timestamp() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| {
        // Set current timestamp to a very high value
        li.timestamp = u64::MAX - 1000;
    });

    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Create bond with duration that would cause end timestamp to overflow
    // bond_start will be u64::MAX - 1000, adding 2000 duration will overflow
    client.create_bond(&identity, &1000, &2000);
}

#[test]
fn test_u64_large_duration_extension() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let duration = u64::MAX / 2;

    // Create bond with large duration
    let bond = client.create_bond(&identity, &1000, &duration);
    assert_eq!(bond.bond_duration, duration);

    // Extend with another large duration (should succeed as sum < u64::MAX)
    let bond = client.extend_duration(&(duration / 2));
    assert_eq!(bond.bond_duration, duration + (duration / 2));
}

#[test]
fn test_timestamp_boundary_conditions() {
    let e = Env::default();
    e.mock_all_auths();
    // Set timestamp to near-max value
    e.ledger().with_mut(|li| {
        li.timestamp = u64::MAX - 10000;
    });

    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Create bond with safe duration
    let bond = client.create_bond(&identity, &1000, &5000);

    assert_eq!(bond.bond_duration, 5000);
    assert!(bond.bond_start >= u64::MAX - 10000);
}

// ============================================================================
// WITHDRAWAL UNDERFLOW TESTS
// ============================================================================

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdrawal_exceeds_available_balance() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Attempt to withdraw more than available
    client.withdraw(&1001);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdrawal_after_slashing() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Slash 400
    client.slash(&400);

    // Available balance is now 600, attempt to withdraw 601
    client.withdraw(&601);
}

#[test]
fn test_withdrawal_exact_available_balance() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Withdraw exact available amount
    let bond = client.withdraw(&1000);
    assert_eq!(bond.bonded_amount, 0);
}

#[test]
fn test_withdrawal_zero_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Withdraw zero amount (should succeed)
    let bond = client.withdraw(&0);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_multiple_withdrawals_causing_underflow() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Multiple withdrawals
    client.withdraw(&400);
    client.withdraw(&400);
    // Available balance is now 200, this should fail
    client.withdraw(&300);
}

#[test]
fn test_withdrawal_with_max_i128_bond() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &i128::MAX, &86400_u64);

    // Withdraw large amount
    let bond = client.withdraw(&(i128::MAX / 2));
    assert_eq!(bond.bonded_amount, i128::MAX - (i128::MAX / 2));
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdrawal_when_fully_slashed() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Slash entire amount
    client.slash(&1000);

    // Attempt to withdraw when fully slashed (available = 0)
    client.withdraw(&1);
}

// ============================================================================
// SLASHING UNDERFLOW TESTS
// ============================================================================

#[test]
fn test_slashing_normal_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Slash normal amount
    let bond = client.slash(&300);
    assert_eq!(bond.slashed_amount, 300);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_slashing_exceeds_bonded_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Slash more than bonded amount (should cap at bonded amount)
    let bond = client.slash(&2000);
    assert_eq!(bond.slashed_amount, 1000); // Capped at bonded_amount
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_multiple_slashing_operations() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Multiple slashing operations
    let bond = client.slash(&200);
    assert_eq!(bond.slashed_amount, 200);

    let bond = client.slash(&300);
    assert_eq!(bond.slashed_amount, 500);

    let bond = client.slash(&100);
    assert_eq!(bond.slashed_amount, 600);
}

#[test]
fn test_slashing_zero_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Slash zero amount
    let bond = client.slash(&0);
    assert_eq!(bond.slashed_amount, 0);
}

#[test]
fn test_slashing_after_withdrawal() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Withdraw first
    client.withdraw(&300);

    // Then slash (should still reference original bonded amount)
    let bond = client.slash(&400);
    assert_eq!(bond.slashed_amount, 400);
    assert_eq!(bond.bonded_amount, 700); // After withdrawal
}

#[test]
fn test_slashing_with_max_values() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &i128::MAX, &86400_u64);

    // Slash large amount
    let bond = client.slash(&(i128::MAX / 2));
    assert_eq!(bond.slashed_amount, i128::MAX / 2);
}

// ============================================================================
// COMBINED SCENARIO TESTS
// ============================================================================

#[test]
fn test_complex_arithmetic_scenario() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Initial bond
    client.create_bond(&identity, &10000, &86400_u64);

    // Top up
    let bond = client.top_up(&5000);
    assert_eq!(bond.bonded_amount, 15000);

    // Slash some
    let bond = client.slash(&3000);
    assert_eq!(bond.slashed_amount, 3000);

    // Withdraw available (15000 - 3000 = 12000 available)
    let bond = client.withdraw(&8000);
    assert_eq!(bond.bonded_amount, 7000);

    // Verify final state
    assert_eq!(bond.slashed_amount, 3000);
    assert_eq!(bond.bonded_amount, 7000);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdrawal_leaves_insufficient_for_slashed() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400_u64);

    // Slash 500
    client.slash(&500);

    // Try to withdraw 600 (but only 500 is available after slashing)
    // This should panic with "insufficient balance for withdrawal"
    client.withdraw(&600);
}

#[test]
fn test_boundary_arithmetic_with_zero_values() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    // Create bond with zero amount
    let bond = client.create_bond(&identity, &0, &86400_u64);
    assert_eq!(bond.bonded_amount, 0);

    // Try operations on zero bond
    let bond = client.slash(&0);
    assert_eq!(bond.slashed_amount, 0);

    let bond = client.withdraw(&0);
    assert_eq!(bond.bonded_amount, 0);
}
