//! Tests for Tiered Bond System: Bronze, Silver, Gold, Platinum by bonded amount.

#![cfg(test)]

use crate::tiered_bond::{get_tier_for_amount, TIER_BRONZE_MAX, TIER_GOLD_MAX, TIER_SILVER_MAX};
use crate::{BondTier, CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_tier_thresholds() {
    assert_eq!(get_tier_for_amount(0), BondTier::Bronze);
    assert_eq!(get_tier_for_amount(TIER_BRONZE_MAX - 1), BondTier::Bronze);
    assert_eq!(get_tier_for_amount(TIER_BRONZE_MAX), BondTier::Silver);
    assert_eq!(get_tier_for_amount(TIER_SILVER_MAX - 1), BondTier::Silver);
    assert_eq!(get_tier_for_amount(TIER_SILVER_MAX), BondTier::Gold);
    assert_eq!(get_tier_for_amount(TIER_GOLD_MAX - 1), BondTier::Gold);
    assert_eq!(get_tier_for_amount(TIER_GOLD_MAX), BondTier::Platinum);
    assert_eq!(get_tier_for_amount(i128::MAX), BondTier::Platinum);
}

#[test]
fn test_get_tier_after_create_bond() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(&identity, &(TIER_SILVER_MAX), &86400_u64, &false, &0_u64);
    let tier = client.get_tier();
    assert_eq!(tier, BondTier::Gold);
}

#[test]
fn test_tier_upgrade_on_top_up() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(&identity, &(TIER_BRONZE_MAX), &86400_u64, &false, &0_u64);
    assert_eq!(client.get_tier(), BondTier::Silver);
    client.top_up(&(TIER_SILVER_MAX - TIER_BRONZE_MAX));
    assert_eq!(client.get_tier(), BondTier::Gold);
}

#[test]
fn test_tier_downgrade_on_withdraw() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(&identity, &(TIER_GOLD_MAX), &86400_u64, &false, &0_u64);
    assert_eq!(client.get_tier(), BondTier::Platinum);
    let withdraw_to_silver = TIER_GOLD_MAX - TIER_SILVER_MAX + 1;
    client.withdraw(&withdraw_to_silver);
    assert_eq!(client.get_tier(), BondTier::Silver);
}

#[test]
fn test_tier_unchanged_within_threshold() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &(TIER_BRONZE_MAX / 2),
        &86400_u64,
        &false,
        &0_u64,
    );
    assert_eq!(client.get_tier(), BondTier::Bronze);
    client.top_up(&(TIER_BRONZE_MAX / 2 - 1));
    assert_eq!(client.get_tier(), BondTier::Bronze);
}
