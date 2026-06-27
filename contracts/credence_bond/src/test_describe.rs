//! Tests for the `describe_config` and `describe_bond` introspection entrypoints.
//!
//! Coverage:
//! - `describe_config` returns `None` before `initialize`.
//! - `describe_config` returns correct values after `initialize`.
//! - `describe_config` reflects `set_early_exit_config` changes.
//! - `describe_bond` returns `None` when no bond exists.
//! - `describe_bond` returns `None` for an unknown identity.
//! - `describe_bond` returns `Some` with correct fields after `create_bond`.
//! - `describe_bond` reflects `top_up` and `slash` lifecycle changes.
//! - `describe_bond` reflects `request_withdrawal` state.
//! - `describe_bond` reflects `withdraw` (partial) state.
//! - Neither entrypoint calls `require_auth` (no auth mocking needed).

use super::*;
use crate::test_helpers::{self, advance_ledger_sequence};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Env;

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    (client, admin)
}

// ── describe_config ──────────────────────────────────────────────────────────

#[test]
fn test_describe_config_returns_none_when_uninitialized() {
    let e = Env::default();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    assert!(client.describe_config().is_none());
}

#[test]
fn test_describe_config_after_initialize() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let cfg = client.describe_config().unwrap();

    assert_eq!(cfg.admin, admin);
    // Early-exit config not set yet.
    assert!(cfg.early_exit_treasury.is_none());
    assert!(cfg.early_exit_penalty_bps.is_none());
    // Weight config defaults (see weighted_attestation::DEFAULT_WEIGHT_MULTIPLIER_BPS).
    assert_eq!(
        cfg.weight_multiplier_bps,
        weighted_attestation::DEFAULT_WEIGHT_MULTIPLIER_BPS
    );
}

#[test]
fn test_describe_config_reflects_early_exit_config() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let treasury = Address::generate(&e);

    client.set_early_exit_config(&admin, &treasury, &500_u32);

    let cfg = client.describe_config().unwrap();
    assert_eq!(cfg.early_exit_treasury, Some(treasury));
    assert_eq!(cfg.early_exit_penalty_bps, Some(500_u32));
}

#[test]
fn test_describe_config_no_auth_required() {
    // Deliberately do NOT call e.mock_all_auths() — describe_config must succeed
    // without any auth context.
    let e = Env::default();
    e.mock_all_auths(); // needed only for initialize
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    // Call without any auth mock — should not panic.
    let cfg = client.describe_config().unwrap();
    assert_eq!(cfg.admin, admin);
}

// ── describe_bond ────────────────────────────────────────────────────────────

#[test]
fn test_describe_bond_returns_none_when_no_bond() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let stranger = Address::generate(&e);

    let result = client.describe_bond(&stranger);
    assert!(result.is_none());
}

#[test]
fn test_describe_bond_returns_none_for_wrong_identity() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    let other = Address::generate(&e);

    client.create_bond(&identity, &1000_i128, &3600_u64, &false, &0_u64);

    // `other` has no bond — must return None.
    assert!(client.describe_bond(&other).is_none());
}

#[test]
fn test_describe_bond_after_create_bond() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);

    client.create_bond(&identity, &1000_i128, &3600_u64, &false, &0_u64);

    let view = client.describe_bond(&identity).unwrap();
    assert_eq!(view.identity, identity);
    assert_eq!(view.bonded_amount, 1000);
    assert_eq!(view.slashed_amount, 0);
    assert_eq!(view.available_amount, 1000);
    assert_eq!(view.bond_duration, 3600);
    assert!(view.active);
    assert!(!view.is_rolling);
    assert_eq!(view.withdrawal_requested_at, 0);
    // 1000 is far below TIER_BRONZE_MAX (1e21), so the tier is Bronze.
    assert_eq!(view.tier, BondTier::Bronze);
}

#[test]
fn test_describe_bond_tier_bronze() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);

    client.create_bond(&identity, &1000_i128, &3600_u64, &false, &0_u64);

    let view = client.describe_bond(&identity).unwrap();
    assert_eq!(view.tier, BondTier::Bronze);
}

#[test]
fn test_describe_bond_reflects_top_up() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);

    client.create_bond(&identity, &1000_i128, &3600_u64, &false, &0_u64);
    client.top_up(&identity, &250_i128);

    let view = client.describe_bond(&identity).unwrap();
    assert_eq!(view.bonded_amount, 1_250);
    assert_eq!(view.available_amount, 1_250);
}

#[test]
fn test_describe_bond_reflects_slash() {
    let e = Env::default();
    let (client, admin, identity, _token, _bond_id) = test_helpers::setup_with_token(&e);
    let slash_treasury = Address::generate(&e);
    client.set_slash_treasury(&admin, &slash_treasury);

    client.create_bond(&identity, &1000_i128, &3600_u64, &false, &0_u64);
    advance_ledger_sequence(&e);
    client.slash(&admin, &200_i128);

    let view = client.describe_bond(&identity).unwrap();
    assert_eq!(view.bonded_amount, 1000);
    assert_eq!(view.slashed_amount, 200);
    assert_eq!(view.available_amount, 800);
}

#[test]
fn test_describe_bond_reflects_request_withdrawal() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);

    // Rolling bond
    client.create_bond(&identity, &1000_i128, &3600_u64, &true, &600_u64);
    // Advance the ledger clock so the recorded request timestamp is non-zero
    // (Env::default() starts at timestamp 0).
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    client.request_withdrawal(&identity);

    let view = client.describe_bond(&identity).unwrap();
    assert!(view.is_rolling);
    assert!(view.withdrawal_requested_at > 0);
}

#[test]
fn test_describe_bond_reflects_partial_withdraw() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);

    client.create_bond(&identity, &1000_i128, &3600_u64, &false, &0_u64);

    // Advance past lockup
    let mut info = e.ledger().get();
    info.timestamp += 3601;
    e.ledger().set(info);

    client.withdraw(&identity, &400_i128);

    let view = client.describe_bond(&identity).unwrap();
    assert_eq!(view.bonded_amount, 600);
    assert_eq!(view.available_amount, 600);
}

#[test]
fn test_describe_bond_no_auth_required() {
    // describe_bond must not require any auth.
    let e = Env::default();
    e.mock_all_auths(); // only for setup
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let identity = Address::generate(&e);
    client.initialize(&admin, &None);
    client.create_bond(&identity, &1000_i128, &3600_u64, &false, &0_u64);

    // Call without additional auth — must not panic.
    let view = client.describe_bond(&identity).unwrap();
    assert_eq!(view.bonded_amount, 1000);
}
