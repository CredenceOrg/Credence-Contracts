//! Tests for Tiered Bond System: Bronze, Silver, Gold, Platinum by bonded amount.

use crate::test_helpers;
use crate::tiered_bond::{get_tier_for_amount, TIER_BRONZE_MAX, TIER_GOLD_MAX, TIER_SILVER_MAX};
use crate::{BondTier, CredenceBondClient};
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    test_helpers::setup_with_token(e)
}

#[test]
fn test_tier_thresholds() {
    let e = Env::default();
    assert_eq!(get_tier_for_amount(&e, 0), BondTier::Bronze);
    assert_eq!(
        get_tier_for_amount(&e, TIER_BRONZE_MAX - 1),
        BondTier::Bronze
    );
    assert_eq!(get_tier_for_amount(&e, TIER_BRONZE_MAX), BondTier::Silver);
    assert_eq!(
        get_tier_for_amount(&e, TIER_SILVER_MAX - 1),
        BondTier::Silver
    );
    assert_eq!(get_tier_for_amount(&e, TIER_SILVER_MAX), BondTier::Gold);
    assert_eq!(get_tier_for_amount(&e, TIER_GOLD_MAX - 1), BondTier::Gold);
    assert_eq!(get_tier_for_amount(&e, TIER_GOLD_MAX), BondTier::Platinum);
    assert_eq!(get_tier_for_amount(&e, i128::MAX), BondTier::Platinum);
}

#[test]
fn test_get_tier_after_create_bond() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &(TIER_SILVER_MAX), &86400_u64, &false, &0_u64);
    let tier = client.get_tier();
    assert_eq!(tier, BondTier::Gold);
}

#[test]
fn test_tier_upgrade_on_top_up() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &(TIER_BRONZE_MAX), &86400_u64, &false, &0_u64);
    assert_eq!(client.get_tier(), BondTier::Silver);
    client.top_up(&identity, &(TIER_SILVER_MAX - TIER_BRONZE_MAX));
    assert_eq!(client.get_tier(), BondTier::Gold);
}

#[test]
fn test_tier_downgrade_on_withdraw() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(&identity, &(TIER_GOLD_MAX), &86400_u64, &false, &0_u64);
    assert_eq!(client.get_tier(), BondTier::Platinum);
    e.ledger().with_mut(|li| li.timestamp = 86401);
    let withdraw_to_silver = TIER_GOLD_MAX - TIER_SILVER_MAX + 1;
    client.withdraw(&identity, &withdraw_to_silver);
    assert_eq!(client.get_tier(), BondTier::Silver);
}

#[test]
fn test_tier_unchanged_within_threshold() {
    let e = Env::default();
    let (client, _admin, identity, ..) = setup(&e);
    client.create_bond_with_rolling(
        &identity,
        &(TIER_BRONZE_MAX / 2),
        &86400_u64,
        &false,
        &0_u64,
    );
    assert_eq!(client.get_tier(), BondTier::Bronze);
    client.top_up(&identity, &(TIER_BRONZE_MAX / 2 - 1));
    assert_eq!(client.get_tier(), BondTier::Bronze);
}

#[test]
fn test_admin_set_tier_thresholds() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, identity, ..) = setup(&e);

    // Initial thresholds check
    let initial = client.get_tier_thresholds();
    assert_eq!(initial.bronze_max, TIER_BRONZE_MAX);
    assert_eq!(initial.silver_max, TIER_SILVER_MAX);
    assert_eq!(initial.gold_max, TIER_GOLD_MAX);

    // Update thresholds
    let new_bronze = 2_000_000_000_000_000_000_000i128; // 2000 * 10^18
    let new_silver = 10_000_000_000_000_000_000_000i128; // 10000 * 10^18
    let new_gold = 30_000_000_000_000_000_000_000i128; // 30000 * 10^18

    client.set_tier_thresholds(&admin, &new_bronze, &new_silver, &new_gold);

    let updated = client.get_tier_thresholds();
    assert_eq!(updated.bronze_max, new_bronze);
    assert_eq!(updated.silver_max, new_silver);
    assert_eq!(updated.gold_max, new_gold);

    // Check mapping uses new thresholds
    assert_eq!(get_tier_for_amount(&e, new_bronze - 1), BondTier::Bronze);
    assert_eq!(get_tier_for_amount(&e, new_bronze), BondTier::Silver);
    assert_eq!(get_tier_for_amount(&e, new_silver - 1), BondTier::Silver);
    assert_eq!(get_tier_for_amount(&e, new_silver), BondTier::Gold);
    assert_eq!(get_tier_for_amount(&e, new_gold - 1), BondTier::Gold);
    assert_eq!(get_tier_for_amount(&e, new_gold), BondTier::Platinum);
}

#[test]
#[should_panic(expected = "bronze max must be positive")]
fn test_set_tier_thresholds_invalid_bronze() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, ..) = setup(&e);
    client.set_tier_thresholds(&admin, &0, &5000, &20000);
}

#[test]
#[should_panic(expected = "silver max must be greater than bronze max")]
fn test_set_tier_thresholds_invalid_silver() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, ..) = setup(&e);
    client.set_tier_thresholds(&admin, &2000, &2000, &20000);
}

#[test]
#[should_panic(expected = "gold max must be greater than silver max")]
fn test_set_tier_thresholds_invalid_gold() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, ..) = setup(&e);
    client.set_tier_thresholds(&admin, &2000, &5000, &5000);
}

#[test]
fn test_fully_slashed_bond_tier() {
    let e = Env::default();
    e.mock_all_auths();
    let (client, admin, identity, ..) = setup(&e);

    // Create bond at Platinum tier
    client.create_bond_with_rolling(&identity, &(TIER_GOLD_MAX), &86400_u64, &false, &0_u64);
    assert_eq!(client.get_tier(), BondTier::Platinum);

    // Fully slash the bond
    client.slash(&admin, &TIER_GOLD_MAX);

    let state = client.get_identity_state();
    assert_eq!(state.slashed_amount, TIER_GOLD_MAX);
    assert_eq!(state.bonded_amount, TIER_GOLD_MAX);

    // Tier should still be Platinum
    assert_eq!(client.get_tier(), BondTier::Platinum);
}
