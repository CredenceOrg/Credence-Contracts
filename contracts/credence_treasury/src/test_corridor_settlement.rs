//! Tests for the corridor-gated `settle` entrypoint (issue #911).
//!
//! `settle` lets the admin move treasury funds directly to a destination,
//! but only when that destination has been explicitly registered as a
//! corridor. These tests cover the happy path (registered corridor
//! succeeds) and the primary rejection mode (unregistered corridor
//! reverts), plus the surrounding lifecycle (removal, re-registration,
//! liquidity floor interaction).

use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceTreasuryClient<'_>, Address, Address) {
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);

    let token_admin = Address::generate(e);
    let token_id = e.register_stellar_asset_contract(token_admin.clone());

    e.mock_all_auths();
    client.initialize(&admin, &token_id);

    let stellar_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);
    stellar_client.mint(&admin, &(i128::MAX / 2));

    (client, admin, token_id)
}

fn setup_with_balance(e: &Env, initial_balance: i128) -> (CredenceTreasuryClient<'_>, Address, Address) {
    let (client, admin, token_id) = setup(e);
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);
    stellar_client.mint(&admin, &initial_balance);
    client.receive_fee(&admin, &initial_balance, &FundSource::ProtocolFee);
    (client, admin, token_id)
}

#[test]
fn test_corridor_not_registered_by_default() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    let destination = Address::generate(&e);

    assert!(!client.is_corridor_registered(&destination));
}

#[test]
fn test_register_corridor_then_check() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    let destination = Address::generate(&e);

    client.register_corridor(&admin, &destination);
    assert!(client.is_corridor_registered(&destination));
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_register_corridor_unauthorized_caller() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    let unauthorized = Address::generate(&e);
    let destination = Address::generate(&e);

    client.register_corridor(&unauthorized, &destination);
}

// ── Happy path ──────────────────────────────────────────────────────────

#[test]
fn test_settle_succeeds_for_registered_corridor() {
    let e = Env::default();
    let (client, admin, _token) = setup_with_balance(&e, 10_000);
    let destination = Address::generate(&e);

    client.register_corridor(&admin, &destination);
    let actual = client.settle(&admin, &destination, &4_000);

    assert_eq!(actual, 4_000);
    assert_eq!(client.get_balance(), 6_000);
}

// ── Primary failure mode: unregistered corridor ────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #611)")]
fn test_settle_rejects_unregistered_corridor() {
    let e = Env::default();
    let (client, admin, _token) = setup_with_balance(&e, 10_000);
    let destination = Address::generate(&e);

    // destination was never registered via register_corridor
    client.settle(&admin, &destination, &1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #611)")]
fn test_settle_rejects_after_corridor_removed() {
    let e = Env::default();
    let (client, admin, _token) = setup_with_balance(&e, 10_000);
    let destination = Address::generate(&e);

    client.register_corridor(&admin, &destination);
    client.remove_corridor(&admin, &destination);

    client.settle(&admin, &destination, &1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_settle_unauthorized_caller() {
    let e = Env::default();
    let (client, admin, _token) = setup_with_balance(&e, 10_000);
    let destination = Address::generate(&e);
    let unauthorized = Address::generate(&e);

    client.register_corridor(&admin, &destination);
    client.settle(&unauthorized, &destination, &1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #602)")]
fn test_settle_respects_min_liquidity_floor() {
    let e = Env::default();
    let (client, admin, _token) = setup_with_balance(&e, 10_000);
    let destination = Address::generate(&e);

    client.register_corridor(&admin, &destination);
    client.set_min_liquidity(&admin, &5_000);

    // Would leave 4_000, below the 5_000 floor.
    client.settle(&admin, &destination, &6_000);
}

#[test]
fn test_settle_idempotent_registration() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    let destination = Address::generate(&e);

    // Registering twice must not error or double-count anything.
    client.register_corridor(&admin, &destination);
    client.register_corridor(&admin, &destination);
    assert!(client.is_corridor_registered(&destination));
}
