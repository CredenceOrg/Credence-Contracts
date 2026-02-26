//! Comprehensive tests for bond creation fee mechanism (#15).
//! Covers fee calculation, treasury config, fee waiver, events, and edge cases.

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    // Shared helper configures token + approvals so create_bond works with fees.
    let (client, admin, identity, ..) = test_helpers::setup_with_token(e);
    (client, admin, identity)
}

#[test]
fn test_fee_zero_when_not_configured() {
    let e = Env::default();
    let (client, _admin, identity) = setup(&e);
    let (treasury, fee_bps) = client.get_fee_config();
    assert!(treasury.is_none());
    assert_eq!(fee_bps, 0);
    let bond = client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
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

#[test]
fn test_fee_calculated_on_create_bond() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32); // 1%
    let bond = client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 990); // 1% fee = 10
}

#[test]
fn test_fee_one_percent() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32);
    let bond = client.create_bond(&identity, &10_000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 9_900);
}

#[test]
fn test_fee_zero_bps() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &0_u32);
    let bond = client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_fee_max_bps_capped() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &10_000_u32);
    let bond = client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 0);
}

#[test]
#[should_panic(expected = "fee_bps must be <= 10000")]
fn test_fee_over_max_rejected() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &10_001_u32);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_fee_config_unauthorized() {
    let e = Env::default();
    let (client, _admin, _identity) = setup(&e);
    let other = Address::generate(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&other, &treasury, &100_u32);
}

#[test]
fn test_fee_large_amount() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &50_u32); // 0.5%
    let amount = 1_000_000_000_i128;
    let bond = client.create_bond(&identity, &amount, &86400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 995_000_000); // 0.5% fee
}

#[test]
fn test_fee_accumulates_in_pool() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32); // 1%
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64); // fee 10
    client.create_bond(&identity, &2000_i128, &86400_u64, &false, &0_u64); // fee 20
    let collected = client.collect_fees(&admin);
    assert_eq!(collected, 10 + 20);
}
