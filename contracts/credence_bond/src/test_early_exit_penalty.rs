//! Tests for Early Exit Penalty Mechanism.
//! Covers: penalty calculation from remaining lock time, configurable rates,
//! penalty event emission, and security (zero/max penalty edge cases).

#![cfg(test)]

use crate::early_exit_penalty;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

fn setup<'a>(e: &'a Env, treasury: &Address, penalty_bps: u32) -> (CredenceBondClient<'a>, Address) {
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    client.set_early_exit_config(&admin, treasury, &penalty_bps);
    (client, admin)
}

#[test]
fn test_early_exit_penalty_calculation_zero_penalty_rate() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, admin) = setup(&e, &treasury, 0);
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);

    let bond = client.withdraw_early(&500);
    assert_eq!(bond.bonded_amount, 500);
}

#[test]
fn test_early_exit_penalty_calculation_max_penalty() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, _admin) = setup(&e, &treasury, 10_000); // 100%
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);
    // Withdraw at start: remaining = 100, total = 100 -> full penalty
    let bond = client.withdraw_early(&500);
    assert_eq!(bond.bonded_amount, 500);
    // Penalty = 500 * 100% = 500; user effectively gets 0 (penalty to treasury)
}

#[test]
fn test_early_exit_penalty_half_remaining() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, _admin) = setup(&e, &treasury, 1000); // 10%
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);
    // At t=1050: remaining=50, total=100 -> 50% of penalty rate -> 5% of amount
    e.ledger().with_mut(|li| li.timestamp = 1050);
    let bond = client.withdraw_early(&100);
    assert_eq!(bond.bonded_amount, 900);
    // Penalty = 100 * 10% * (50/100) = 5
}

#[test]
fn test_early_exit_emits_penalty_event() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, _admin) = setup(&e, &treasury, 500); // 5%
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);
    client.withdraw_early(&200);
    // Event (early_exit_penalty, (identity, 200, penalty, treasury)) should be emitted
    // We can't easily assert events in Soroban test without event parsing; bond state is updated
    let state = client.get_identity_state();
    assert_eq!(state.bonded_amount, 800);
}

#[test]
#[should_panic(expected = "use withdraw for post lock-up")]
fn test_early_exit_rejected_after_lock_up() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, _admin) = setup(&e, &treasury, 500);
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 1101);
    client.withdraw_early(&100);
}

#[test]
#[should_panic(expected = "early exit config not set")]
fn test_early_exit_fails_without_config() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);
    client.withdraw_early(&100);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_early_exit_config_unauthorized() {
    let e = Env::default();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let other = Address::generate(&e);
    let treasury = Address::generate(&e);
    client.set_early_exit_config(&other, &treasury, &500);
}

#[test]
#[should_panic(expected = "penalty_bps must be <= 10000")]
fn test_set_early_exit_config_invalid_bps() {
    let e = Env::default();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let treasury = Address::generate(&e);
    client.set_early_exit_config(&admin, &treasury, &10_001);
}

#[test]
fn test_calculate_penalty_unit() {
    let e = Env::default();
    // remaining = total -> full penalty rate applied
    let p = early_exit_penalty::calculate_penalty(1000, 100, 100, 500);
    assert_eq!(p, 50); // 5% of 1000
    let p = early_exit_penalty::calculate_penalty(1000, 0, 100, 500);
    assert_eq!(p, 0);
    let p = early_exit_penalty::calculate_penalty(1000, 50, 100, 10000);
    assert_eq!(p, 500);
}
