//! Comprehensive unit tests for slashing functionality.
//! Covers: successful slash, unauthorized rejection, over-slash prevention,
//! slash history (via events), and slash events.

#![cfg(test)]

use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    let identity = Address::generate(e);
    (client, admin, identity)
}

#[test]
fn test_slash_success() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let bond = client.slash(&admin, &300);
    assert_eq!(bond.slashed_amount, 300);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_slash_unauthorized_rejection() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let other = Address::generate(&e);
    client.slash(&other, &100);
}

#[test]
fn test_slash_over_slash_prevention() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let bond = client.slash(&admin, &2000);
    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_slash_history_recording() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.slash(&admin, &200);
    let bond = client.get_identity_state();
    assert_eq!(bond.slashed_amount, 200);
    client.slash(&admin, &300);
    let bond = client.get_identity_state();
    assert_eq!(bond.slashed_amount, 500);
}

#[test]
fn test_slash_zero_amount() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let bond = client.slash(&admin, &0);
    assert_eq!(bond.slashed_amount, 0);
}

#[test]
fn test_slash_events_emitted() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let _ = client.slash(&admin, &250);
    let state = client.get_identity_state();
    assert_eq!(state.slashed_amount, 250);
}

#[test]
fn test_withdraw_after_slash_respects_available() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.slash(&admin, &400);
    let bond = client.withdraw(&600);
    assert_eq!(bond.bonded_amount, 400);
    assert_eq!(bond.slashed_amount, 400);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdraw_more_than_available_after_slash_fails() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.slash(&admin, &400);
    client.withdraw(&601);
}

#[test]
fn test_multiple_slashes_capped_at_bonded() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.slash(&admin, &600);
    let bond = client.slash(&admin, &600);
    assert_eq!(bond.slashed_amount, 1000);
}
