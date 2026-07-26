//! Tests for Early Exit Penalty Mechanism.
//! Covers: penalty calculation from remaining lock time, configurable rates,
//! penalty event emission, and security (zero/max penalty edge cases).

use crate::early_exit_penalty;
use crate::math;
use crate::test_helpers;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env, Symbol, TryFromVal};

fn setup<'a>(
    e: &'a Env,
    treasury: &Address,
    penalty_bps: u32,
) -> (CredenceBondClient<'a>, Address, Address) {
    let (client, admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(e);
    client.set_early_exit_config(&admin, treasury, &penalty_bps);
    (client, admin, identity)
}

#[test]
fn test_early_exit_penalty_calculation_zero_penalty_rate() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, _admin, identity, token_id, bond_contract_id) = test_helpers::setup_with_token(&e);
    client.set_early_exit_config(&_admin, &treasury, &0);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    let token_client = TokenClient::new(&e, &token_id);
    let before_identity = token_client.balance(&identity);
    let before_treasury = token_client.balance(&treasury);
    let before_contract = token_client.balance(&bond_contract_id);

    let bond = client.withdraw_early(&identity, &500);
    assert_eq!(bond.bonded_amount, 500);
    assert_eq!(token_client.balance(&identity), before_identity + 500);
    assert_eq!(token_client.balance(&treasury), before_treasury);
    assert_eq!(
        token_client.balance(&bond_contract_id),
        before_contract - 500
    );
}

#[test]
fn test_early_exit_penalty_calculation_max_penalty() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, admin, identity, token_id, bond_contract_id) = test_helpers::setup_with_token(&e);
    client.set_early_exit_config(&admin, &treasury, &10_000); // 100%
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let token_client = TokenClient::new(&e, &token_id);
    let before_identity = token_client.balance(&identity);
    let before_treasury = token_client.balance(&treasury);
    let before_contract = token_client.balance(&bond_contract_id);

    // Withdraw at start: remaining = 86400, total = 86400 -> full penalty
    let bond = client.withdraw_early(&identity, &500);
    assert_eq!(bond.bonded_amount, 500);
    // Penalty = 500 * 100% = 500; user effectively gets 0 (penalty to treasury)
    assert_eq!(token_client.balance(&identity), before_identity);
    assert_eq!(token_client.balance(&treasury), before_treasury + 500);
    assert_eq!(
        token_client.balance(&bond_contract_id),
        before_contract - 500
    );
}

#[test]
fn test_early_exit_penalty_half_remaining() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, _admin, identity) = setup(&e, &treasury, 1000); // 10%
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    // At t=44200: remaining=43200, total=86400 -> 50% of penalty rate -> 5% of amount
    e.ledger().with_mut(|li| li.timestamp = 44200);
    let bond = client.withdraw_early(&identity, &100);
    assert_eq!(bond.bonded_amount, 900);
    // Penalty = 100 * 10% * (43200/86400) = 5
}

#[test]
fn test_early_exit_emits_penalty_event() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, admin, identity, _token_id, bond_contract_id) = test_helpers::setup_with_token(&e);
    client.set_early_exit_config(&admin, &treasury, &500); // 5%
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.withdraw_early(&identity, &200);
    let expected_penalty = early_exit_penalty::calculate_penalty(200, 86_400, 86_400, 500);
    let events = e.events().all();
    let found = events.iter().any(|(contract_id, topics, data)| {
        contract_id == bond_contract_id
            && !topics.is_empty()
            && Symbol::try_from_val(&e, &topics.get(0).unwrap())
                .map(|topic| topic == Symbol::new(&e, "early_exit_penalty"))
                .unwrap_or(false)
            && <(Address, i128, i128, Address)>::try_from_val(&e, &data)
                .map(|payload| {
                    payload == (identity.clone(), 200, expected_penalty, treasury.clone())
                })
                .unwrap_or(false)
    });
    assert!(found, "early_exit_penalty event was not emitted");

    let state = client.get_identity_state();
    assert_eq!(state.bonded_amount, 800);
}

#[test]
#[should_panic(expected = "Error(Contract, #204)")]
fn test_early_exit_rejected_after_lock_up() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let treasury = Address::generate(&e);
    let (client, _admin, identity) = setup(&e, &treasury, 500);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);
    client.withdraw_early(&identity, &100);
}

#[test]
fn test_early_exit_fails_without_config_and_reverts_state() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, token_id, bond_contract_id) = test_helpers::setup_with_token(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    let token_client = TokenClient::new(&e, &token_id);
    let before_bond = client.get_identity_state();
    let before_identity = token_client.balance(&identity);
    let before_contract = token_client.balance(&bond_contract_id);

    let result = client.try_withdraw_early(&identity, &100);
    assert!(
        result.is_err(),
        "withdraw_early must revert when early-exit treasury is unset"
    );

    let after_bond = client.get_identity_state();
    assert_eq!(after_bond.bonded_amount, before_bond.bonded_amount);
    assert_eq!(after_bond.slashed_amount, before_bond.slashed_amount);
    assert_eq!(token_client.balance(&identity), before_identity);
    assert_eq!(token_client.balance(&bond_contract_id), before_contract);
}

#[test]
#[should_panic(expected = "Error(Contract, #210)")]
fn test_early_exit_without_config_uses_typed_error() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, ..) = test_helpers::setup_with_token(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.withdraw_early(&identity, &100);
}

#[test]
fn test_early_exit_penalty_and_payout_sum_to_gross_withdrawal() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity, token_id, bond_contract_id) = test_helpers::setup_with_token(&e);
    let treasury = Address::generate(&e);
    client.set_early_exit_config(&admin, &treasury, &1000); // 10%
    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);

    e.ledger().with_mut(|li| li.timestamp = 1050);
    let token_client = TokenClient::new(&e, &token_id);
    let before_identity = token_client.balance(&identity);
    let before_treasury = token_client.balance(&treasury);
    let before_contract = token_client.balance(&bond_contract_id);

    let gross = 400_i128;
    let expected_penalty = early_exit_penalty::calculate_penalty(gross, 50, 100, 1000);
    let expected_payout = gross.checked_sub(expected_penalty).unwrap();

    let bond = client.withdraw_early(&identity, &gross);
    assert_eq!(bond.bonded_amount, 600);
    assert_eq!(expected_penalty + expected_payout, gross);
    assert_eq!(
        token_client.balance(&identity),
        before_identity + expected_payout
    );
    assert_eq!(
        token_client.balance(&treasury),
        before_treasury + expected_penalty
    );
    assert_eq!(
        token_client.balance(&bond_contract_id),
        before_contract - gross
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_set_early_exit_config_unauthorized() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin, &None);
    let other = Address::generate(&e);
    let treasury = Address::generate(&e);
    client.set_early_exit_config(&other, &treasury, &500);
}

#[test]
#[should_panic(expected = "penalty_bps must be <= 10000")]
fn test_set_early_exit_config_invalid_bps() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin, &None);
    let treasury = Address::generate(&e);
    client.set_early_exit_config(&admin, &treasury, &10_001);
}

#[test]
fn test_calculate_penalty_unit() {
    // remaining = total -> full penalty rate applied
    let p = early_exit_penalty::calculate_penalty(1000, 100, 100, 500);
    assert_eq!(p, 50); // 5% of 1000
    let p = early_exit_penalty::calculate_penalty(1000, 0, 100, 500);
    assert_eq!(p, 0);
    let p = early_exit_penalty::calculate_penalty(1000, 50, 100, math::BPS_DENOMINATOR as u32);
    assert_eq!(p, 500);
}
