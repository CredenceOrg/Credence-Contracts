//! Comprehensive tests for withdraw_bond functionality.
//! Covers: lock-up enforcement, cooldown (notice period), partial withdrawals,
//! insufficient balance, slashing interaction, and edge cases.

#![cfg(test)]

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env};

fn setup_with_token(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    test_helpers::setup_with_token(e)
}

#[test]
fn test_withdraw_bond_after_lockup_non_rolling() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    e.ledger().with_mut(|li| li.timestamp = 87401);
    let bond = client.withdraw_bond(&500);
    assert_eq!(bond.bonded_amount, 500);
}

#[test]
#[should_panic(expected = "lock-up period not elapsed; use withdraw_early")]
fn test_withdraw_bond_before_lockup_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    e.ledger().with_mut(|li| li.timestamp = 44200);
    client.withdraw_bond(&500);
}

#[test]
#[should_panic(expected = "cooldown window not elapsed; request_withdrawal first")]
fn test_withdraw_bond_rolling_before_notice_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    e.ledger().with_mut(|li| li.timestamp = 1101);

    client.withdraw_bond(&500);
}

#[test]
#[should_panic(expected = "cooldown window not elapsed; request_withdrawal first")]
fn test_withdraw_bond_rolling_before_cooldown_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    client.request_withdrawal();
    e.ledger().with_mut(|li| li.timestamp = 1005);

    client.withdraw_bond(&500);
}

#[test]
fn test_withdraw_bond_rolling_after_cooldown() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    client.request_withdrawal();
    e.ledger().with_mut(|li| li.timestamp = 1011);

    let bond = client.withdraw_bond(&500);
    assert_eq!(bond.bonded_amount, 500);
}

#[test]
fn test_withdraw_bond_partial_withdrawal() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw_bond(&300);
    assert_eq!(bond.bonded_amount, 700);
    let bond = client.withdraw_bond(&200);
    assert_eq!(bond.bonded_amount, 500);
    let bond = client.withdraw_bond(&500);
    assert_eq!(bond.bonded_amount, 0);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdraw_bond_insufficient_balance() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    client.withdraw_bond(&1001);
}

#[test]
fn test_withdraw_bond_after_slash() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.slash(&admin, &400);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw_bond(&600);
    assert_eq!(bond.bonded_amount, 400);
    assert_eq!(bond.slashed_amount, 400);
}

#[test]
fn test_withdraw_bond_zero_amount() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw_bond(&0);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_withdraw_bond_full_withdrawal() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, token_id, bond_contract_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw_bond(&1000);
    assert_eq!(bond.bonded_amount, 0);

    let token_client = TokenClient::new(&e, &token_id);
    let balance = token_client.balance(&bond_contract_id);
    assert_eq!(balance, 0);
}

#[test]
fn test_withdraw_alias_calls_withdraw_bond() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw(&500);
    assert_eq!(bond.bonded_amount, 500);
}
