#![cfg(test)]

use crate::test_helpers;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env};

fn setup_no_token(e: &Env) -> (CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin)
}

// ── default state ────────────────────────────────────────────────────────────

#[test]
fn test_borrow_freeze_default_false() {
    let e = Env::default();
    let (client, _admin) = setup_no_token(&e);
    assert!(!client.is_borrow_frozen());
}

// ── governance set / unset ────────────────────────────────────────────────────

#[test]
fn test_set_borrow_frozen_true_and_false() {
    let e = Env::default();
    let (client, admin) = setup_no_token(&e);

    client.set_borrow_frozen(&admin, &true);
    assert!(client.is_borrow_frozen());

    client.set_borrow_frozen(&admin, &false);
    assert!(!client.is_borrow_frozen());
}

#[test]
fn test_set_borrow_frozen_non_admin_rejected() {
    let e = Env::default();
    let (client, _admin) = setup_no_token(&e);
    let rando = Address::generate(&e);

    assert!(client.try_set_borrow_frozen(&rando, &true).is_err());
    assert!(!client.is_borrow_frozen());
}

// ── create_bond blocked ───────────────────────────────────────────────────────

#[test]
fn test_create_bond_blocked_when_frozen() {
    let e = Env::default();
    let (client, admin, identity, _token, _bond_id) = test_helpers::setup_with_token(&e);

    client.set_borrow_frozen(&admin, &true);

    assert!(client
        .try_create_bond(&identity, &1_000_i128, &86_400_u64)
        .is_err());
}

#[test]
fn test_create_bond_with_rolling_blocked_when_frozen() {
    let e = Env::default();
    let (client, admin, identity, _token, _bond_id) = test_helpers::setup_with_token(&e);

    client.set_borrow_frozen(&admin, &true);

    assert!(client
        .try_create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64)
        .is_err());
}

// ── top_up blocked ────────────────────────────────────────────────────────────

#[test]
fn test_top_up_blocked_when_frozen() {
    let e = Env::default();
    let (client, admin, identity, _token, _bond_id) = test_helpers::setup_with_token(&e);

    // Create bond first (freeze not yet active)
    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);

    client.set_borrow_frozen(&admin, &true);

    assert!(client.try_top_up(&1_000_i128).is_err());
}

// ── withdrawals still allowed ─────────────────────────────────────────────────

#[test]
fn test_withdraw_allowed_when_frozen() {
    let e = Env::default();
    let (client, admin, identity, _token, _bond_id) = test_helpers::setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);

    client.set_borrow_frozen(&admin, &true);

    // withdraw_bond_full should succeed even while frozen
    let withdrawn = client.withdraw_bond_full(&identity);
    assert!(withdrawn > 0);
}

// ── unfreeze restores create_bond ─────────────────────────────────────────────

#[test]
fn test_create_bond_allowed_after_unfreeze() {
    let e = Env::default();
    let (client, admin, identity, _token, _bond_id) = test_helpers::setup_with_token(&e);

    client.set_borrow_frozen(&admin, &true);
    assert!(client
        .try_create_bond(&identity, &1_000_i128, &86_400_u64)
        .is_err());

    client.set_borrow_frozen(&admin, &false);
    let bond = client.create_bond(&identity, &1_000_i128, &86_400_u64);
    assert!(bond.active);
}

// ── event emitted ─────────────────────────────────────────────────────────────

#[test]
fn test_set_borrow_frozen_emits_event() {
    let e = Env::default();
    let (client, admin) = setup_no_token(&e);

    client.set_borrow_frozen(&admin, &true);

    // Verify the event was published (at least one event exists after the call)
    let events = e.events().all();
    assert!(!events.is_empty());
}

// ── paused contract blocks set_borrow_frozen ──────────────────────────────────

#[test]
fn test_set_borrow_frozen_blocked_when_paused() {
    let e = Env::default();
    let (client, admin) = setup_no_token(&e);

    client.pause(&admin);
    assert!(client.try_set_borrow_frozen(&admin, &true).is_err());
}
