#![cfg(test)]

extern crate std;

use crate::test_helpers::setup_with_token;
use crate::CredenceBondClient;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Env, IntoVal};

#[test]
#[should_panic(expected = "Error(Contract, #210)")] // EarlyExitConfigNotSet
fn test_withdraw_early_reverts_if_config_not_set() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) = setup_with_token(&e);

    // Create a bond
    client.create_bond_with_rolling(&identity, &10_000, &86400, &false, &0);

    // Advance time but not past expiry
    e.ledger().with_mut(|l| l.timestamp += 43200);

    // Try to withdraw early without setting the early exit config
    client.withdraw_early(&1000);
}

#[test]
fn test_withdraw_early_transfers_penalty_to_treasury() {
    let e = Env::default();
    let (client, admin, identity, token_id, _contract_id) = setup_with_token(&e);
    let token_client = soroban_sdk::token::Client::new(&e, &token_id);

    let treasury = Address::generate(&e);

    // Configure early exit penalty
    client.set_early_exit_config(&admin, &treasury, &1000); // 10%

    // Create a bond
    client.create_bond_with_rolling(&identity, &10_000, &86400, &false, &0); // 1 day duration

    // Advance time halfway
    e.ledger().with_mut(|l| l.timestamp += 43200);

    // Withdraw early
    let withdraw_amount = 2000;
    client.withdraw_early(&withdraw_amount);

    // Penalty calculation:
    // base_penalty = 2000 * 1000 / 10000 = 200
    // time_factor = (86400 - 43200) / 86400 = 0.5
    // penalty = 200 * 0.5 = 100
    let expected_penalty = 100;
    let expected_net = withdraw_amount - expected_penalty;

    // Verify balances
    // User should have received net amount
    assert_eq!(token_client.balance(&identity), 9000 + expected_net); // 10000 (initial) - 10000 (bond) + 9000 (topup) + 1900 (net)

    // Treasury should have received the penalty
    assert_eq!(token_client.balance(&treasury), expected_penalty);

    // Check bond state
    let bond = client.get_identity_state();
    assert_eq!(bond.bonded_amount, 10_000 - withdraw_amount);
}

#[test]
#[should_panic(expected = "Error(Contract, #204)")] // LockupNotExpired
fn test_withdraw_early_fails_after_lockup() {
    let e = Env::default();
    let (client, admin, identity, _token_id, _contract_id) = setup_with_token(&e);
    let treasury = Address::generate(&e);

    client.set_early_exit_config(&admin, &treasury, &1000);
    client.create_bond_with_rolling(&identity, &10_000, &86400, &false, &0);

    // Advance time past expiry
    e.ledger().with_mut(|l| l.timestamp += 86401);

    client.withdraw_early(&1000);
}

#[test]
fn test_withdraw_early_full_amount() {
    let e = Env::default();
    let (client, admin, identity, token_id, _contract_id) = setup_with_token(&e);
    let token_client = soroban_sdk::token::Client::new(&e, &token_id);
    let treasury = Address::generate(&e);

    client.set_early_exit_config(&admin, &treasury, &10_000); // 100% penalty

    client.create_bond_with_rolling(&identity, &10_000, &86400, &false, &0);

    // Withdraw immediately
    let withdraw_amount = 10_000;
    client.withdraw_early(&withdraw_amount);

    // Penalty should be the full amount
    let expected_penalty = 10_000;
    let expected_net = 0;

    assert_eq!(token_client.balance(&identity), 9000 + expected_net);
    assert_eq!(token_client.balance(&treasury), expected_penalty);

    let bond = client.get_identity_state();
    assert_eq!(bond.bonded_amount, 0);
}

#[test]
fn test_withdraw_early_with_zero_penalty_rate() {
    let e = Env::default();
    let (client, admin, identity, token_id, _contract_id) = setup_with_token(&e);
    let token_client = soroban_sdk::token::Client::new(&e, &token_id);
    let treasury = Address::generate(&e);

    // Set penalty to 0
    client.set_early_exit_config(&admin, &treasury, &0);

    client.create_bond_with_rolling(&identity, &10_000, &86400, &false, &0);

    e.ledger().with_mut(|l| l.timestamp += 43200);

    let withdraw_amount = 2000;
    client.withdraw_early(&withdraw_amount);

    let expected_penalty = 0;
    let expected_net = withdraw_amount;

    assert_eq!(token_client.balance(&identity), 9000 + expected_net);
    assert_eq!(token_client.balance(&treasury), expected_penalty);

    let bond = client.get_identity_state();
    assert_eq!(bond.bonded_amount, 10_000 - withdraw_amount);
}