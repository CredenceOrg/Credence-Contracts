//! Tests for Rolling Bond: auto-renewal, withdrawal request with notice period, renewal events.

#![cfg(test)]

use crate::test_helpers;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(e);
    (client, admin, identity)
}

#[test]
fn test_rolling_bond_creation() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity) = setup(&e);
    let bond = client.create_bond(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    assert!(bond.is_rolling);
    assert_eq!(bond.notice_period_duration, 10);
    assert_eq!(bond.withdrawal_requested_at, 0);
}

#[test]
fn test_request_withdrawal() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    let bond = client.request_withdrawal();
    assert_eq!(bond.withdrawal_requested_at, 1000);
}

#[test]
#[should_panic(expected = "not a rolling bond")]
fn test_request_withdrawal_non_rolling() {
    let e = Env::default();
    let (client, _admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.request_withdrawal();
}

#[test]
#[should_panic(expected = "withdrawal already requested")]
fn test_request_withdrawal_twice() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    client.request_withdrawal();
    client.request_withdrawal();
}

#[test]
fn test_renew_if_rolling_advances_period() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    let bond = client.get_identity_state();
    assert_eq!(bond.bond_start, 1000);

    e.ledger().with_mut(|li| li.timestamp = 87401);
    let bond = client.renew_if_rolling();
    assert_eq!(bond.bond_start, 87401);
    assert_eq!(bond.withdrawal_requested_at, 0);
}

#[test]
fn test_renew_if_rolling_no_op_before_period_end() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    e.ledger().with_mut(|li| li.timestamp = 44200);
    let bond = client.renew_if_rolling();
    assert_eq!(bond.bond_start, 1000);
}

#[test]
fn test_renew_if_rolling_no_op_for_non_rolling() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);
    let bond = client.renew_if_rolling();
    assert_eq!(bond.bond_start, 1000);
}

#[test]
fn test_withdraw_after_notice_period() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity) = setup(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    client.request_withdrawal();
    e.ledger().with_mut(|li| li.timestamp = 1011);
    let bond = client.withdraw(&500);
    assert_eq!(bond.bonded_amount, 500);
}
