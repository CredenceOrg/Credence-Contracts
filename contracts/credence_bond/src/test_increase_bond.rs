#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Env;

/// Test successful bond increase with valid parameters
#[test]
fn test_increase_bond_success() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let initial_amount = 1000_i128;
    let increase_amount = 500_i128;

    // Create initial bond
    let bond = client.create_bond(&identity, &initial_amount, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, initial_amount);

    // Increase bond
    let updated_bond = client.increase_bond(&identity, &increase_amount);
    assert_eq!(updated_bond.bonded_amount, initial_amount + increase_amount);
}

/// Test increase bond fails when caller is not the bond owner
#[test]
#[should_panic(expected = "not bond owner")]
fn test_increase_bond_unauthorized() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let unauthorized = Address::generate(&e);

    // Create bond for identity
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    // Try to increase bond from unauthorized address
    client.increase_bond(&unauthorized, &500_i128);
}

/// Test increase bond fails when amount is zero
#[test]
#[should_panic(expected = "amount must be positive")]
fn test_increase_bond_zero_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);

    // Create initial bond
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    // Try to increase with zero amount
    client.increase_bond(&identity, &0_i128);
}

/// Test increase bond fails when amount is negative
#[test]
#[should_panic(expected = "amount must be positive")]
fn test_increase_bond_negative_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);

    // Create initial bond
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    // Try to increase with negative amount
    client.increase_bond(&identity, &(-100_i128));
}

/// Test increase bond fails when no bond exists
#[test]
#[should_panic(expected = "no bond")]
fn test_increase_bond_no_bond() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);

    // Try to increase bond without creating one first
    client.increase_bond(&identity, &500_i128);
}

/// Test increase bond with maximum amount (overflow protection)
#[test]
#[should_panic(expected = "bond increase caused overflow")]
fn test_increase_bond_overflow() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);

    // Create bond with max amount
    let bond = client.create_bond(&identity, &i128::MAX, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, i128::MAX);

    // Try to increase (should overflow)
    client.increase_bond(&identity, &1_i128);
}

/// Test increase bond with typical USDC amount (6 decimals)
#[test]
fn test_increase_bond_usdc_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let initial_usdc = 1000_000000_i128; // 1000 USDC
    let increase_usdc = 500_000000_i128; // 500 USDC

    // Create bond
    let bond = client.create_bond(&identity, &initial_usdc, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, initial_usdc);

    // Increase bond
    let updated_bond = client.increase_bond(&identity, &increase_usdc);
    assert_eq!(updated_bond.bonded_amount, initial_usdc + increase_usdc);
}

/// Test multiple sequential bond increases
#[test]
fn test_increase_bond_sequential() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let initial_amount = 1000_i128;

    // Create initial bond
    client.create_bond(&identity, &initial_amount, &86400_u64, &false, &0_u64);

    // Multiple increases
    let increases = [100_i128, 200_i128, 300_i128];
    let mut total = initial_amount;

    for increase in increases {
        let bond = client.increase_bond(&identity, &increase);
        total += increase;
        assert_eq!(bond.bonded_amount, total);
    }

    // Verify final state
    let final_bond = client.get_identity_state();
    assert_eq!(final_bond.bonded_amount, initial_amount + 600);
}

/// Test increase bond maintains other bond properties
#[test]
fn test_increase_bond_preserves_properties() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let duration = 86400_u64;
    let initial_amount = 1000_i128;
    let increase_amount = 500_i128;

    // Create bond
    let bond = client.create_bond(&identity, &initial_amount, &duration, &false, &0_u64);
    let start_time = bond.bond_start;

    // Increase bond
    let updated_bond = client.increase_bond(&identity, &increase_amount);

    // Verify properties are preserved
    assert_eq!(updated_bond.identity, identity);
    assert_eq!(updated_bond.bond_start, start_time);
    assert_eq!(updated_bond.bond_duration, duration);
    assert_eq!(updated_bond.slashed_amount, 0);
    assert!(updated_bond.active);
    assert_eq!(updated_bond.bonded_amount, initial_amount + increase_amount);
}

/// Test increase bond with reentrancy guard
#[test]
fn test_increase_bond_reentrancy() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);

    // Create initial bond
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    // Verify not locked initially
    assert!(!client.is_locked());

    // Increase bond (should acquire and release lock)
    client.increase_bond(&identity, &500_i128);

    // Verify not locked after operation
    assert!(!client.is_locked());
}

/// Test increase bond with large amount
#[test]
fn test_increase_bond_large_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let initial_amount = 100_i128;
    let large_increase = 1_000_000_000_i128; // 1 billion

    // Create bond
    client.create_bond(&identity, &initial_amount, &86400_u64, &false, &0_u64);

    // Increase with large amount
    let bond = client.increase_bond(&identity, &large_increase);
    assert_eq!(bond.bonded_amount, initial_amount + large_increase);
}

/// Test increase bond on inactive bond
#[test]
fn test_increase_bond_inactive_bond() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);

    // Create and then withdraw (makes bond inactive)
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let _ = client.withdraw_bond(&identity);

    // Try to increase on inactive bond - should still work (bond exists, owner is correct)
    // The contract doesn't prevent increasing on inactive bonds
    let bond = client.increase_bond(&identity, &500_i128);
    assert_eq!(bond.bonded_amount, 500_i128);
}
