//! Integration tests covering full bond lifecycle: create, top-up, slash, withdraw.
//! Verifies state consistency and happy path / edge scenarios.

#![cfg(test)]

use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin, Address::generate(e))
}

/// Happy path: create bond -> withdraw full after lock-up.
#[test]
fn test_lifecycle_create_then_withdraw() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let amount = 1000_i128;
    let duration = 86400_u64;
    client.create_bond(&identity, &amount, &duration, &false, &0_u64);
    let state = client.get_identity_state();
    assert_eq!(state.bonded_amount, amount);
    assert_eq!(state.slashed_amount, 0);
    assert!(state.active);

    let withdrawn = client.withdraw(&amount);
    assert_eq!(withdrawn.bonded_amount, 0);
    assert_eq!(withdrawn.slashed_amount, 0);
}

/// Create -> top-up -> withdraw.
#[test]
fn test_lifecycle_create_topup_withdraw() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &500_i128, &86400_u64, &false, &0_u64);
    let after_topup = client.top_up(&300_i128);
    assert_eq!(after_topup.bonded_amount, 800);

    client.withdraw(&800_i128);
    let state = client.get_identity_state();
    assert_eq!(state.bonded_amount, 0);
}

/// Create -> slash -> withdraw remaining.
#[test]
fn test_lifecycle_slash_then_withdraw_remaining() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let after_slash = client.slash(&admin, &400_i128);
    assert_eq!(after_slash.slashed_amount, 400);
    assert_eq!(after_slash.bonded_amount, 1000);

    let remaining = 1000_i128 - 400_i128;
    let after_withdraw = client.withdraw(&remaining);
    assert_eq!(after_withdraw.bonded_amount, 400);
    assert_eq!(after_withdraw.slashed_amount, 400);
}

/// Multiple operations: create, top-up, slash, withdraw.
#[test]
fn test_lifecycle_create_topup_slash_withdraw() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.top_up(&500_i128);
    client.slash(&admin, &300_i128);
    let state = client.get_identity_state();
    assert_eq!(state.bonded_amount, 1500);
    assert_eq!(state.slashed_amount, 300);
    let available = 1500 - 300;
    client.withdraw(&available);
    let final_state = client.get_identity_state();
    assert_eq!(final_state.bonded_amount, 300);
}

/// State consistency: get_identity_state matches after each step.
#[test]
fn test_lifecycle_state_consistency() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &2000_i128, &86400_u64, &false, &0_u64);
    let s1 = client.get_identity_state();
    let s2 = client.get_identity_state();
    assert_eq!(s1.bonded_amount, s2.bonded_amount);
    assert_eq!(s1.slashed_amount, s2.slashed_amount);

    client.slash(&admin, &500_i128);
    let s3 = client.get_identity_state();
    assert_eq!(s3.slashed_amount, 500);
    assert_eq!(s3.bonded_amount, 2000);

    client.withdraw(&1500_i128);
    let s4 = client.get_identity_state();
    assert_eq!(s4.bonded_amount, 500);
    assert_eq!(s4.slashed_amount, 500);
}

/// Extend duration then verify bond fields unchanged where expected.
#[test]
fn test_lifecycle_extend_duration() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let before = client.get_identity_state();
    client.extend_duration(&86400_u64);
    let after = client.get_identity_state();
    assert_eq!(after.bond_duration, before.bond_duration + 86400);
    assert_eq!(after.bonded_amount, before.bonded_amount);
}
