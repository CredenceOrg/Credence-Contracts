#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Env;

/// Test successful bond creation with valid parameters
#[test]
fn test_create_bond_success() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let amount = 1000_i128;
    let duration = 86400_u64;

    let bond = client.create_bond(&identity, &amount, &duration);

    assert!(bond.active);
    assert_eq!(bond.bonded_amount, amount);
    assert_eq!(bond.slashed_amount, 0);
    assert_eq!(bond.identity, identity);
    assert_eq!(bond.bond_duration, duration);
}

/// Test bond creation with zero amount (should succeed as no validation exists)
#[test]
fn test_create_bond_zero_amount() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &0_i128, &86400_u64);

    assert_eq!(bond.bonded_amount, 0);
    assert!(bond.active);
}

/// Test bond creation with negative amount (should succeed as no validation exists)
#[test]
fn test_create_bond_negative_amount() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &(-100_i128), &86400_u64);

    assert_eq!(bond.bonded_amount, -100);
}

/// Test bond creation with maximum valid amount
#[test]
fn test_create_bond_max_amount() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let max_amount = i128::MAX;
    let bond = client.create_bond(&identity, &max_amount, &86400_u64);

    assert_eq!(bond.bonded_amount, max_amount);
}

/// Test bond creation with zero duration
#[test]
fn test_create_bond_zero_duration() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &1000_i128, &0_u64);

    assert_eq!(bond.bond_duration, 0);
    assert!(bond.active);
}

/// Test bond creation with maximum duration that doesn't overflow
#[test]
fn test_create_bond_max_duration() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let duration = u64::MAX / 2; // Safe duration that won't overflow with typical timestamps
    let bond = client.create_bond(&identity, &1000_i128, &duration);

    assert_eq!(bond.bond_duration, duration);
}

/// Test bond creation with duration that causes timestamp overflow
#[test]
#[should_panic(expected = "bond end timestamp would overflow")]
fn test_create_bond_duration_overflow() {
    let e = Env::default();
    e.ledger().with_mut(|li| {
        li.timestamp = u64::MAX - 1000; // Set timestamp close to max
    });
    
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let duration = 2000_u64; // Will overflow when added to timestamp
    client.create_bond(&identity, &1000_i128, &duration);
}

/// Test duplicate bond creation (overwrites previous bond)
#[test]
fn test_create_bond_duplicate() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    
    // Create first bond
    let bond1 = client.create_bond(&identity, &1000_i128, &86400_u64);
    assert_eq!(bond1.bonded_amount, 1000);

    // Create second bond (overwrites first)
    let bond2 = client.create_bond(&identity, &2000_i128, &172800_u64);
    assert_eq!(bond2.bonded_amount, 2000);
    assert_eq!(bond2.bond_duration, 172800);

    // Verify storage contains second bond
    let stored_bond = client.get_identity_state();
    assert_eq!(stored_bond.bonded_amount, 2000);
}

/// Test bond creation with different identities (overwrites due to single bond storage)
#[test]
fn test_create_bond_different_identities() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity1 = Address::generate(&e);
    let identity2 = Address::generate(&e);

    client.create_bond(&identity1, &1000_i128, &86400_u64);
    let _bond2 = client.create_bond(&identity2, &2000_i128, &172800_u64);

    // Due to single bond storage, only the last bond is stored
    let stored_bond = client.get_identity_state();
    assert_eq!(stored_bond.identity, identity2);
    assert_eq!(stored_bond.bonded_amount, 2000);
}

/// Test bond creation initializes all fields correctly
#[test]
fn test_create_bond_field_initialization() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &5000_i128, &604800_u64);

    assert_eq!(bond.identity, identity);
    assert_eq!(bond.bonded_amount, 5000);
    assert_eq!(bond.bond_duration, 604800);
    assert_eq!(bond.slashed_amount, 0);
    assert!(bond.active);
}

/// Test bond creation persists to storage
#[test]
fn test_create_bond_storage_persistence() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let amount = 3000_i128;
    let duration = 259200_u64;

    client.create_bond(&identity, &amount, &duration);

    let retrieved_bond = client.get_identity_state();
    assert_eq!(retrieved_bond.identity, identity);
    assert_eq!(retrieved_bond.bonded_amount, amount);
    assert_eq!(retrieved_bond.bond_duration, duration);
}

/// Test bond creation with minimum positive amount
#[test]
fn test_create_bond_min_positive_amount() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &1_i128, &86400_u64);

    assert_eq!(bond.bonded_amount, 1);
    assert!(bond.active);
}

/// Test bond creation with typical USDC amount (6 decimals)
#[test]
fn test_create_bond_usdc_amount() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let usdc_amount = 1000_000000_i128; // 1000 USDC with 6 decimals
    let bond = client.create_bond(&identity, &usdc_amount, &86400_u64);

    assert_eq!(bond.bonded_amount, usdc_amount);
}

/// Test bond_start timestamp is set correctly
#[test]
fn test_create_bond_timestamp() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &1000_i128, &86400_u64);

    // bond_start should be set to ledger timestamp (can be 0 in test env)
    let ledger_time = e.ledger().timestamp();
    assert_eq!(bond.bond_start, ledger_time);
}

/// Test multiple sequential bond creations
#[test]
fn test_create_bond_sequential() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);

    for i in 1..=5 {
        let amount = i * 1000;
        let bond = client.create_bond(&identity, &amount, &86400_u64);
        assert_eq!(bond.bonded_amount, amount);
    }

    // Last bond should be stored
    let stored_bond = client.get_identity_state();
    assert_eq!(stored_bond.bonded_amount, 5000);
}
