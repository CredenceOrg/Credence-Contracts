//! Cooldown Window Tests
//!
//! Covers the full cooldown lifecycle: configuration, request, execution,
//! cancellation, plus edge cases and authorization checks.

#![cfg(test)]

use crate::cooldown;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin)
}

// ---------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------

#[test]
fn test_set_and_get_cooldown_period() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin) = setup(&e);

    assert_eq!(client.get_cooldown_period(), 0);
    client.set_cooldown_period(&admin, &3600);
    assert_eq!(client.get_cooldown_period(), 3600);
}

#[test]
fn test_update_cooldown_period() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin) = setup(&e);

    client.set_cooldown_period(&admin, &100);
    assert_eq!(client.get_cooldown_period(), 100);

    client.set_cooldown_period(&admin, &200);
    assert_eq!(client.get_cooldown_period(), 200);
}

#[test]
fn test_set_cooldown_period_to_zero() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin) = setup(&e);

    client.set_cooldown_period(&admin, &100);
    client.set_cooldown_period(&admin, &0);
    assert_eq!(client.get_cooldown_period(), 0);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_cooldown_period_unauthorized() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin) = setup(&e);

    let other = Address::generate(&e);
    client.set_cooldown_period(&other, &3600);
}

// ---------------------------------------------------------------
// Request cooldown withdrawal
// ---------------------------------------------------------------

#[test]
fn test_request_cooldown_withdrawal() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 5000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &3600);

    let req = client.request_cooldown_withdrawal(&identity, &500);
    assert_eq!(req.requester, identity);
    assert_eq!(req.amount, 500);
    assert_eq!(req.requested_at, 5000);
}

#[test]
fn test_request_cooldown_withdrawal_full_amount() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);

    let req = client.request_cooldown_withdrawal(&identity, &1000);
    assert_eq!(req.amount, 1000);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_request_cooldown_zero_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);

    client.request_cooldown_withdrawal(&identity, &0);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_request_cooldown_negative_amount() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);

    client.request_cooldown_withdrawal(&identity, &-10);
}

#[test]
#[should_panic(expected = "amount exceeds available balance")]
fn test_request_cooldown_exceeds_balance() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);

    client.request_cooldown_withdrawal(&identity, &1001);
}

#[test]
#[should_panic(expected = "amount exceeds available balance")]
fn test_request_cooldown_exceeds_available_after_slash() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.slash(&admin, &300);
    client.set_cooldown_period(&admin, &100);

    // Available is 1000 - 300 = 700, requesting 701 should fail
    client.request_cooldown_withdrawal(&identity, &701);
}

#[test]
#[should_panic(expected = "cooldown request already pending")]
fn test_request_cooldown_duplicate() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);

    client.request_cooldown_withdrawal(&identity, &500);
    client.request_cooldown_withdrawal(&identity, &200);
}

#[test]
#[should_panic(expected = "no bond")]
fn test_request_cooldown_no_bond() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin) = setup(&e);

    client.set_cooldown_period(&admin, &100);
    let identity = Address::generate(&e);
    client.request_cooldown_withdrawal(&identity, &500);
}

#[test]
#[should_panic(expected = "requester is not the bond holder")]
fn test_request_cooldown_wrong_identity() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);

    let other = Address::generate(&e);
    client.request_cooldown_withdrawal(&other, &500);
}

// ---------------------------------------------------------------
// Execute cooldown withdrawal
// ---------------------------------------------------------------

#[test]
fn test_execute_cooldown_withdrawal_after_period() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &400);

    // Advance time past the cooldown
    e.ledger().with_mut(|li| li.timestamp = 1101);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 600);
}

#[test]
fn test_execute_cooldown_withdrawal_exact_boundary() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &250);

    // Exactly at the boundary (1000 + 100 = 1100)
    e.ledger().with_mut(|li| li.timestamp = 1100);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 750);
}

#[test]
fn test_execute_cooldown_removes_request() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &400);

    e.ledger().with_mut(|li| li.timestamp = 1101);
    client.execute_cooldown_withdrawal(&identity);

    // Request should be cleared; a new one can be made
    e.ledger().with_mut(|li| li.timestamp = 2000);
    let req = client.request_cooldown_withdrawal(&identity, &200);
    assert_eq!(req.amount, 200);
    assert_eq!(req.requested_at, 2000);
}

#[test]
fn test_execute_cooldown_with_zero_period() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    // Cooldown period defaults to 0 (instant)
    client.request_cooldown_withdrawal(&identity, &300);

    // Should succeed immediately since period is 0
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 700);
}

#[test]
#[should_panic(expected = "cooldown period has not elapsed")]
fn test_execute_cooldown_too_early() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    // Try to execute 1 second too early
    e.ledger().with_mut(|li| li.timestamp = 1099);
    client.execute_cooldown_withdrawal(&identity);
}

#[test]
#[should_panic(expected = "no cooldown request")]
fn test_execute_cooldown_no_request() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.execute_cooldown_withdrawal(&identity);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_execute_cooldown_balance_slashed_during_cooldown() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &800);

    // Slash the bond while cooldown is pending
    client.slash(&admin, &500);

    // Now available = 1000 - 500 = 500, but request is for 800
    e.ledger().with_mut(|li| li.timestamp = 1101);
    client.execute_cooldown_withdrawal(&identity);
}

// ---------------------------------------------------------------
// Cancel cooldown
// ---------------------------------------------------------------

#[test]
fn test_cancel_cooldown() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    client.cancel_cooldown(&identity);

    // Request should be gone; new one can be made
    let req = client.request_cooldown_withdrawal(&identity, &200);
    assert_eq!(req.amount, 200);
}

#[test]
#[should_panic(expected = "no cooldown request to cancel")]
fn test_cancel_cooldown_no_request() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin) = setup(&e);

    let identity = Address::generate(&e);
    client.cancel_cooldown(&identity);
}

#[test]
#[should_panic(expected = "no cooldown request")]
fn test_execute_after_cancel() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);
    client.cancel_cooldown(&identity);

    e.ledger().with_mut(|li| li.timestamp = 1101);
    client.execute_cooldown_withdrawal(&identity);
}

// ---------------------------------------------------------------
// Query
// ---------------------------------------------------------------

#[test]
fn test_get_cooldown_request() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 2000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &750);

    let req = client.get_cooldown_request(&identity);
    assert_eq!(req.requester, identity);
    assert_eq!(req.amount, 750);
    assert_eq!(req.requested_at, 2000);
}

#[test]
#[should_panic(expected = "no cooldown request")]
fn test_get_cooldown_request_none() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, _admin) = setup(&e);

    let identity = Address::generate(&e);
    client.get_cooldown_request(&identity);
}

// ---------------------------------------------------------------
// Pure helper function tests
// ---------------------------------------------------------------

#[test]
fn test_is_cooldown_active_no_request() {
    assert!(!cooldown::is_cooldown_active(100, 0, 50));
}

#[test]
fn test_is_cooldown_active_during_window() {
    assert!(cooldown::is_cooldown_active(1050, 1000, 100));
}

#[test]
fn test_is_cooldown_active_at_boundary() {
    // At the exact end time, the cooldown is no longer active
    assert!(!cooldown::is_cooldown_active(1100, 1000, 100));
}

#[test]
fn test_is_cooldown_active_after_window() {
    assert!(!cooldown::is_cooldown_active(1200, 1000, 100));
}

#[test]
fn test_can_withdraw_no_request() {
    assert!(!cooldown::can_withdraw(100, 0, 50));
}

#[test]
fn test_can_withdraw_during_cooldown() {
    assert!(!cooldown::can_withdraw(1050, 1000, 100));
}

#[test]
fn test_can_withdraw_at_boundary() {
    assert!(cooldown::can_withdraw(1100, 1000, 100));
}

#[test]
fn test_can_withdraw_after_cooldown() {
    assert!(cooldown::can_withdraw(2000, 1000, 100));
}

#[test]
fn test_can_withdraw_zero_period() {
    // With zero cooldown, withdrawal is immediately possible
    assert!(cooldown::can_withdraw(1000, 1000, 0));
}

#[test]
fn test_saturating_add_no_overflow() {
    // u64::MAX as request_time + large period should not panic
    assert!(cooldown::is_cooldown_active(
        u64::MAX - 1,
        u64::MAX - 10,
        100
    ));
    assert!(cooldown::can_withdraw(u64::MAX, u64::MAX - 10, 5));
}

// ---------------------------------------------------------------
// Full lifecycle
// ---------------------------------------------------------------

#[test]
fn test_full_cooldown_lifecycle() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &5000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &3600);

    // Request withdrawal
    let req = client.request_cooldown_withdrawal(&identity, &2000);
    assert_eq!(req.amount, 2000);
    assert_eq!(req.requested_at, 1000);

    // Verify bond unchanged
    let bond = client.get_identity_state();
    assert_eq!(bond.bonded_amount, 5000);

    // Advance past cooldown and execute
    e.ledger().with_mut(|li| li.timestamp = 4601);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 3000);

    // Request another withdrawal
    e.ledger().with_mut(|li| li.timestamp = 5000);
    client.request_cooldown_withdrawal(&identity, &1000);
    e.ledger().with_mut(|li| li.timestamp = 8601);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 2000);
}

#[test]
fn test_cancel_and_rerequest_lifecycle() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin) = setup(&e);

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &86400, &false, &0);
    client.set_cooldown_period(&admin, &100);

    client.request_cooldown_withdrawal(&identity, &800);
    client.cancel_cooldown(&identity);

    // New request at a later time
    e.ledger().with_mut(|li| li.timestamp = 2000);
    let req = client.request_cooldown_withdrawal(&identity, &500);
    assert_eq!(req.requested_at, 2000);
    assert_eq!(req.amount, 500);

    e.ledger().with_mut(|li| li.timestamp = 2100);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 500);
}
