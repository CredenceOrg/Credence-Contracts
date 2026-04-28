//! Comprehensive tests for bond creation fee mechanism.
//!
//! Covers:
//!   - Fee calculation and treasury config
//!   - Governance fee bounds (`MIN_FEE_BPS`, `MAX_GOVERNANCE_FEE_BPS`)
//!   - Fee waiver (zero bps)
//!   - `fee_config_updated` event: old/new values in topics and data payload
//!   - Fee pool accumulation
//!   - Authorisation guard (non-admin rejected)

use crate::fees::{MAX_GOVERNANCE_FEE_BPS, MIN_FEE_BPS};
use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, IntoVal, Symbol, Val};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    // Shared helper configures token + approvals so create_bond works with fees.
    let (client, admin, identity, ..) = test_helpers::setup_with_token(e);
    (client, admin, identity)
}

// ─── basic config ─────────────────────────────────────────────────────────────

#[test]
fn test_fee_zero_when_not_configured() {
    let e = Env::default();
    let (client, _admin, identity) = setup(&e);
    let (treasury, fee_bps) = client.get_fee_config();
    assert!(treasury.is_none());
    assert_eq!(fee_bps, 0);
    let bond = client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_set_fee_config() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32);
    let (t, bps) = client.get_fee_config();
    assert_eq!(t, Some(treasury));
    assert_eq!(bps, 100);
}

// ─── fee calculation ──────────────────────────────────────────────────────────

#[test]
fn test_fee_calculated_on_create_bond() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32); // 1%
    let bond = client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 990); // 1% fee = 10
}

#[test]
fn test_fee_one_percent() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32);
    let bond = client.create_bond_with_rolling(&identity, &10000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 9_900);
}

#[test]
fn test_fee_zero_bps_no_deduction() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &0_u32);
    let bond = client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_fee_large_amount() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &50_u32); // 0.5%
    let amount = 1_000_000_000_i128;
    let bond = client.create_bond_with_rolling(&identity, &amount, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 995_000_000); // 0.5% fee
}

// ─── governance bounds ────────────────────────────────────────────────────────

/// `MAX_GOVERNANCE_FEE_BPS` (500) itself must be accepted.
#[test]
fn test_fee_at_governance_cap_accepted() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &MAX_GOVERNANCE_FEE_BPS);
    let (_, bps) = client.get_fee_config();
    assert_eq!(bps, MAX_GOVERNANCE_FEE_BPS);
}

/// One bps above the governance cap must be rejected.
#[test]
#[should_panic(expected = "fee_bps exceeds governance cap (max 500 bps = 5%)")]
fn test_fee_above_governance_cap_rejected() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &(MAX_GOVERNANCE_FEE_BPS + 1));
}

/// The old 10 001 bps input (100%+) must now be rejected by the governance cap.
#[test]
#[should_panic(expected = "fee_bps exceeds governance cap (max 500 bps = 5%)")]
fn test_fee_100_percent_rejected() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &10_001_u32);
}

/// `MIN_FEE_BPS` (1) must be accepted as the lowest non-zero value.
#[test]
fn test_fee_at_minimum_bps_accepted() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &MIN_FEE_BPS);
    let (_, bps) = client.get_fee_config();
    assert_eq!(bps, MIN_FEE_BPS);
}

/// Zero is always valid (disables the fee), even after a non-zero value was set.
#[test]
fn test_fee_config_zero_re_disables_fee() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32);
    client.set_fee_config(&admin, &treasury, &0_u32);
    let (_, bps) = client.get_fee_config();
    assert_eq!(bps, 0);
}

// ─── authorisation ────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "not admin")]
fn test_set_fee_config_unauthorized() {
    let e = Env::default();
    let (client, _admin, _identity) = setup(&e);
    let other = Address::generate(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&other, &treasury, &100_u32);
}

// ─── fee pool accumulation ────────────────────────────────────────────────────

#[test]
fn test_fee_accumulates_in_pool() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32); // 1%
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64); // fee 10
    client.create_bond_with_rolling(&identity, &2000_i128, &86400_u64, &false, &0_u64); // fee 20
    let collected = client.collect_fees(&admin);
    assert_eq!(collected, 10 + 20);
}

// ─── event emission: fee_config_updated ──────────────────────────────────────

fn is_fee_event(e: &Env, topics: &soroban_sdk::Vec<Val>, target: &Symbol) -> bool {
    if topics.len() == 0 { return false; }
    let sym: Symbol = topics.get(0).unwrap().into_val(e);
    &sym == target
}

#[test]
fn test_fee_config_updated_event_initial_set() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);

    client.set_fee_config(&admin, &treasury, &200_u32);

    let all_events = e.events().all();
    let target_sym = Symbol::new(&e, "fee_config_updated");

    let fee_event = all_events.iter().find(|(_, topics, _)| {
        is_fee_event(&e, topics, &target_sym)
    }).expect("fee_config_updated event not found");

    let (_, topics, _) = fee_event;
    let old_bps: u32 = topics.get(1).unwrap().into_val(&e);
    assert_eq!(old_bps, 0);
    let new_bps: u32 = topics.get(2).unwrap().into_val(&e);
    assert_eq!(new_bps, 200);
}

#[test]
fn test_fee_config_updated_event_carries_old_values() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _identity) = setup(&e);
    let treasury_a = Address::generate(&e);
    let treasury_b = Address::generate(&e);
    let target_sym = Symbol::new(&e, "fee_config_updated");

    client.set_fee_config(&admin, &treasury_a, &100_u32);
    client.set_fee_config(&admin, &treasury_b, &300_u32);

    let all_events = e.events().all();
    let fee_event = all_events.iter().filter(|(_, t, _)| is_fee_event(&e, t, &target_sym)).last().expect("event not found");
    
    let (_, topics, data) = fee_event;

    let old_bps: u32 = topics.get(1).unwrap().into_val(&e);
    assert_eq!(old_bps, 100, "old_fee_bps should be 100 in the latest event");
    let new_bps: u32 = topics.get(2).unwrap().into_val(&e);
    assert_eq!(new_bps, 300, "new_fee_bps mismatch");
    let old_treasury: Option<Address> = data.into_val(&e);
    assert_eq!(old_treasury, Some(treasury_a), "old_treasury mismatch");
}

#[test]
fn test_fee_config_updated_event_on_disable() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);

    client.set_fee_config(&admin, &treasury, &150_u32);
    client.set_fee_config(&admin, &treasury, &0_u32);

    let all_events = e.events().all();
    let target_sym = Symbol::new(&e, "fee_config_updated");

    let last_event = all_events.iter().filter(|(_, t, _)| is_fee_event(&e, t, &target_sym)).last().expect("event not found");
    let (_, topics, _) = last_event;
    let old_bps: u32 = topics.get(1).unwrap().into_val(&e);
    assert_eq!(old_bps, 150);
}
