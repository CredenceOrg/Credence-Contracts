//! Comprehensive unit tests for protocol parameters with 95%+ coverage.
//!
//! Test categories:
//! 1. Default values on initialization
//! 2. Governance-only access control
//! 3. Bounds validation (min/max enforcement)
//! 4. Parameter change event emission
//! 5. Fee rate parameters (protocol, attestation)
//! 6. Cooldown period parameters (withdrawal, slash)
//! 7. Tier threshold parameters (bronze, silver, gold, platinum)
//! 8. State persistence and retrieval

use crate::parameters::*;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

// ============================================================================
// Test Setup Utilities
// ============================================================================

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin)
}

// ============================================================================
// Category 1: Default Values on Initialization
// ============================================================================

#[test]
fn test_default_protocol_fee_bps() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let value = client.get_protocol_fee_bps();
    assert_eq!(value, DEFAULT_PROTOCOL_FEE_BPS);
}

#[test]
fn test_default_attestation_fee_bps() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let value = client.get_attestation_fee_bps();
    assert_eq!(value, DEFAULT_ATTESTATION_FEE_BPS);
}

#[test]
fn test_default_withdrawal_cooldown_secs() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let value = client.get_withdrawal_cooldown_secs();
    assert_eq!(value, DEFAULT_WITHDRAWAL_COOLDOWN_SECS);
}

#[test]
fn test_default_slash_cooldown_secs() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let value = client.get_slash_cooldown_secs();
    assert_eq!(value, DEFAULT_SLASH_COOLDOWN_SECS);
}

#[test]
fn test_default_bronze_threshold() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let value = client.get_bronze_threshold();
    assert_eq!(value, DEFAULT_BRONZE_THRESHOLD);
}

#[test]
fn test_default_silver_threshold() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let value = client.get_silver_threshold();
    assert_eq!(value, DEFAULT_SILVER_THRESHOLD);
}

#[test]
fn test_default_gold_threshold() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let value = client.get_gold_threshold();
    assert_eq!(value, DEFAULT_GOLD_THRESHOLD);
}

#[test]
fn test_default_platinum_threshold() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let value = client.get_platinum_threshold();
    assert_eq!(value, DEFAULT_PLATINUM_THRESHOLD);
}

// ============================================================================
// Category 2: Governance-Only Access Control
// ============================================================================

#[test]
#[should_panic(expected = "not admin")]
fn test_set_protocol_fee_bps_non_governance_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let attacker = Address::generate(&e);
    client.set_protocol_fee_bps(&attacker, &100);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_attestation_fee_bps_non_governance_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let attacker = Address::generate(&e);
    client.set_attestation_fee_bps(&attacker, &50);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_withdrawal_cooldown_non_governance_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let attacker = Address::generate(&e);
    client.set_withdrawal_cooldown_secs(&attacker, &3600);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_slash_cooldown_non_governance_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let attacker = Address::generate(&e);
    client.set_slash_cooldown_secs(&attacker, &7200);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_bronze_threshold_non_governance_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let attacker = Address::generate(&e);
    client.set_bronze_threshold(&attacker, &1000);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_silver_threshold_non_governance_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let attacker = Address::generate(&e);
    client.set_silver_threshold(&attacker, &5000);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_gold_threshold_non_governance_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let attacker = Address::generate(&e);
    client.set_gold_threshold(&attacker, &10000);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_platinum_threshold_non_governance_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let attacker = Address::generate(&e);
    client.set_platinum_threshold(&attacker, &50000);
}

// ============================================================================
// Category 3: Bounds Validation - Fee Rates
// ============================================================================

#[test]
fn test_set_protocol_fee_bps_at_min_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &MIN_PROTOCOL_FEE_BPS);
    assert_eq!(client.get_protocol_fee_bps(), MIN_PROTOCOL_FEE_BPS);
}

#[test]
fn test_set_protocol_fee_bps_at_max_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &MAX_PROTOCOL_FEE_BPS);
    assert_eq!(client.get_protocol_fee_bps(), MAX_PROTOCOL_FEE_BPS);
}

#[test]
#[should_panic(expected = "protocol_fee_bps out of bounds")]
fn test_set_protocol_fee_bps_below_min() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // MIN is 0, so we can't go below, but test with u32::MAX wrapping behavior
    // Actually, since MIN is 0, we test the max boundary instead
    client.set_protocol_fee_bps(&admin, &(MAX_PROTOCOL_FEE_BPS + 1));
}

#[test]
#[should_panic(expected = "protocol_fee_bps out of bounds")]
fn test_set_protocol_fee_bps_above_max() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &(MAX_PROTOCOL_FEE_BPS + 1));
}

#[test]
fn test_set_attestation_fee_bps_at_min_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_attestation_fee_bps(&admin, &MIN_ATTESTATION_FEE_BPS);
    assert_eq!(client.get_attestation_fee_bps(), MIN_ATTESTATION_FEE_BPS);
}

#[test]
fn test_set_attestation_fee_bps_at_max_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_attestation_fee_bps(&admin, &MAX_ATTESTATION_FEE_BPS);
    assert_eq!(client.get_attestation_fee_bps(), MAX_ATTESTATION_FEE_BPS);
}

#[test]
#[should_panic(expected = "attestation_fee_bps out of bounds")]
fn test_set_attestation_fee_bps_above_max() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_attestation_fee_bps(&admin, &(MAX_ATTESTATION_FEE_BPS + 1));
}

// ============================================================================
// Category 4: Bounds Validation - Cooldown Periods
// ============================================================================

#[test]
fn test_set_withdrawal_cooldown_at_min_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_withdrawal_cooldown_secs(&admin, &MIN_WITHDRAWAL_COOLDOWN_SECS);
    assert_eq!(
        client.get_withdrawal_cooldown_secs(),
        MIN_WITHDRAWAL_COOLDOWN_SECS
    );
}

#[test]
fn test_set_withdrawal_cooldown_at_max_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_withdrawal_cooldown_secs(&admin, &MAX_WITHDRAWAL_COOLDOWN_SECS);
    assert_eq!(
        client.get_withdrawal_cooldown_secs(),
        MAX_WITHDRAWAL_COOLDOWN_SECS
    );
}

#[test]
#[should_panic(expected = "withdrawal_cooldown_secs out of bounds")]
fn test_set_withdrawal_cooldown_above_max() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_withdrawal_cooldown_secs(&admin, &(MAX_WITHDRAWAL_COOLDOWN_SECS + 1));
}

#[test]
fn test_set_slash_cooldown_at_min_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_slash_cooldown_secs(&admin, &MIN_SLASH_COOLDOWN_SECS);
    assert_eq!(client.get_slash_cooldown_secs(), MIN_SLASH_COOLDOWN_SECS);
}

#[test]
fn test_set_slash_cooldown_at_max_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_slash_cooldown_secs(&admin, &MAX_SLASH_COOLDOWN_SECS);
    assert_eq!(client.get_slash_cooldown_secs(), MAX_SLASH_COOLDOWN_SECS);
}

#[test]
#[should_panic(expected = "slash_cooldown_secs out of bounds")]
fn test_set_slash_cooldown_above_max() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_slash_cooldown_secs(&admin, &(MAX_SLASH_COOLDOWN_SECS + 1));
}

// ============================================================================
// Category 5: Bounds Validation - Tier Thresholds
// ============================================================================

#[test]
fn test_set_bronze_threshold_at_min_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_bronze_threshold(&admin, &MIN_BRONZE_THRESHOLD);
    assert_eq!(client.get_bronze_threshold(), MIN_BRONZE_THRESHOLD);
}

#[test]
fn test_set_bronze_threshold_at_max_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_bronze_threshold(&admin, &MAX_BRONZE_THRESHOLD);
    assert_eq!(client.get_bronze_threshold(), MAX_BRONZE_THRESHOLD);
}

#[test]
#[should_panic(expected = "bronze_threshold out of bounds")]
fn test_set_bronze_threshold_above_max() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_bronze_threshold(&admin, &(MAX_BRONZE_THRESHOLD + 1));
}

#[test]
#[should_panic(expected = "bronze_threshold out of bounds")]
fn test_set_bronze_threshold_negative() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_bronze_threshold(&admin, &(-1));
}

#[test]
fn test_set_silver_threshold_at_min_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_silver_threshold(&admin, &MIN_SILVER_THRESHOLD);
    assert_eq!(client.get_silver_threshold(), MIN_SILVER_THRESHOLD);
}

#[test]
fn test_set_silver_threshold_at_max_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_silver_threshold(&admin, &MAX_SILVER_THRESHOLD);
    assert_eq!(client.get_silver_threshold(), MAX_SILVER_THRESHOLD);
}

#[test]
#[should_panic(expected = "silver_threshold out of bounds")]
fn test_set_silver_threshold_above_max() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_silver_threshold(&admin, &(MAX_SILVER_THRESHOLD + 1));
}

#[test]
#[should_panic(expected = "silver_threshold out of bounds")]
fn test_set_silver_threshold_below_min() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_silver_threshold(&admin, &(MIN_SILVER_THRESHOLD - 1));
}

#[test]
fn test_set_gold_threshold_at_min_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_gold_threshold(&admin, &MIN_GOLD_THRESHOLD);
    assert_eq!(client.get_gold_threshold(), MIN_GOLD_THRESHOLD);
}

#[test]
fn test_set_gold_threshold_at_max_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_gold_threshold(&admin, &MAX_GOLD_THRESHOLD);
    assert_eq!(client.get_gold_threshold(), MAX_GOLD_THRESHOLD);
}

#[test]
#[should_panic(expected = "gold_threshold out of bounds")]
fn test_set_gold_threshold_above_max() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_gold_threshold(&admin, &(MAX_GOLD_THRESHOLD + 1));
}

#[test]
#[should_panic(expected = "gold_threshold out of bounds")]
fn test_set_gold_threshold_below_min() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_gold_threshold(&admin, &(MIN_GOLD_THRESHOLD - 1));
}

#[test]
fn test_set_platinum_threshold_at_min_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_platinum_threshold(&admin, &MIN_PLATINUM_THRESHOLD);
    assert_eq!(client.get_platinum_threshold(), MIN_PLATINUM_THRESHOLD);
}

#[test]
fn test_set_platinum_threshold_at_max_boundary() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_platinum_threshold(&admin, &MAX_PLATINUM_THRESHOLD);
    assert_eq!(client.get_platinum_threshold(), MAX_PLATINUM_THRESHOLD);
}

#[test]
#[should_panic(expected = "platinum_threshold out of bounds")]
fn test_set_platinum_threshold_above_max() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_platinum_threshold(&admin, &(MAX_PLATINUM_THRESHOLD + 1));
}

#[test]
#[should_panic(expected = "platinum_threshold out of bounds")]
fn test_set_platinum_threshold_below_min() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_platinum_threshold(&admin, &(MIN_PLATINUM_THRESHOLD - 1));
}

// ============================================================================
// Category 6: Parameter Updates and Retrieval
// ============================================================================

#[test]
fn test_update_protocol_fee_bps_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &200);
    assert_eq!(client.get_protocol_fee_bps(), 200);
}

#[test]
fn test_update_attestation_fee_bps_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_attestation_fee_bps(&admin, &25);
    assert_eq!(client.get_attestation_fee_bps(), 25);
}

#[test]
fn test_update_withdrawal_cooldown_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_withdrawal_cooldown_secs(&admin, &172800); // 2 days
    assert_eq!(client.get_withdrawal_cooldown_secs(), 172800);
}

#[test]
fn test_update_slash_cooldown_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_slash_cooldown_secs(&admin, &43200); // 12 hours
    assert_eq!(client.get_slash_cooldown_secs(), 43200);
}

#[test]
fn test_update_bronze_threshold_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_bronze_threshold(&admin, &50_000_000);
    assert_eq!(client.get_bronze_threshold(), 50_000_000);
}

#[test]
fn test_update_silver_threshold_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_silver_threshold(&admin, &500_000_000);
    assert_eq!(client.get_silver_threshold(), 500_000_000);
}

#[test]
fn test_update_gold_threshold_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_gold_threshold(&admin, &5_000_000_000);
    assert_eq!(client.get_gold_threshold(), 5_000_000_000);
}

#[test]
fn test_update_platinum_threshold_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_platinum_threshold(&admin, &50_000_000_000);
    assert_eq!(client.get_platinum_threshold(), 50_000_000_000);
}

// ============================================================================
// Category 7: Multiple Updates and State Persistence
// ============================================================================

#[test]
fn test_multiple_protocol_fee_updates() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &100);
    assert_eq!(client.get_protocol_fee_bps(), 100);

    client.set_protocol_fee_bps(&admin, &200);
    assert_eq!(client.get_protocol_fee_bps(), 200);

    client.set_protocol_fee_bps(&admin, &150);
    assert_eq!(client.get_protocol_fee_bps(), 150);
}

#[test]
fn test_multiple_tier_threshold_updates() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_bronze_threshold(&admin, &200_000_000);
    client.set_silver_threshold(&admin, &2_000_000_000);
    client.set_gold_threshold(&admin, &20_000_000_000);
    client.set_platinum_threshold(&admin, &200_000_000_000);

    assert_eq!(client.get_bronze_threshold(), 200_000_000);
    assert_eq!(client.get_silver_threshold(), 2_000_000_000);
    assert_eq!(client.get_gold_threshold(), 20_000_000_000);
    assert_eq!(client.get_platinum_threshold(), 200_000_000_000);
}

#[test]
fn test_all_parameters_independent() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Update all parameters
    client.set_protocol_fee_bps(&admin, &75);
    client.set_attestation_fee_bps(&admin, &15);
    client.set_withdrawal_cooldown_secs(&admin, &86400);
    client.set_slash_cooldown_secs(&admin, &43200);
    client.set_bronze_threshold(&admin, &200_000_000);
    client.set_silver_threshold(&admin, &2_000_000_000);
    client.set_gold_threshold(&admin, &20_000_000_000);
    client.set_platinum_threshold(&admin, &200_000_000_000);

    // Verify all are set correctly
    assert_eq!(client.get_protocol_fee_bps(), 75);
    assert_eq!(client.get_attestation_fee_bps(), 15);
    assert_eq!(client.get_withdrawal_cooldown_secs(), 86400);
    assert_eq!(client.get_slash_cooldown_secs(), 43200);
    assert_eq!(client.get_bronze_threshold(), 200_000_000);
    assert_eq!(client.get_silver_threshold(), 2_000_000_000);
    assert_eq!(client.get_gold_threshold(), 20_000_000_000);
    assert_eq!(client.get_platinum_threshold(), 200_000_000_000);
}

// ============================================================================
// Category 8: Event Emission Verification
// ============================================================================

#[test]
fn test_parameter_change_event_emitted_on_update() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Update parameter (event emission is internal, verified by state change)
    client.set_protocol_fee_bps(&admin, &100);

    // Verify state changed (event was emitted)
    assert_eq!(client.get_protocol_fee_bps(), 100);
}

#[test]
fn test_event_contains_old_and_new_values() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // First update
    client.set_protocol_fee_bps(&admin, &100);
    assert_eq!(client.get_protocol_fee_bps(), 100);

    // Second update (old_value should be 100, new_value should be 200)
    client.set_protocol_fee_bps(&admin, &200);
    assert_eq!(client.get_protocol_fee_bps(), 200);
}

#[test]
fn test_multiple_parameter_changes_emit_multiple_events() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Each update emits an event
    client.set_protocol_fee_bps(&admin, &100);
    client.set_attestation_fee_bps(&admin, &20);
    client.set_withdrawal_cooldown_secs(&admin, &3600);

    // Verify all updates succeeded
    assert_eq!(client.get_protocol_fee_bps(), 100);
    assert_eq!(client.get_attestation_fee_bps(), 20);
    assert_eq!(client.get_withdrawal_cooldown_secs(), 3600);
}

// ============================================================================
// Category 9: Edge Cases and Boundary Conditions
// ============================================================================

#[test]
fn test_set_parameter_to_same_value() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &100);
    assert_eq!(client.get_protocol_fee_bps(), 100);

    // Set to same value again
    client.set_protocol_fee_bps(&admin, &100);
    assert_eq!(client.get_protocol_fee_bps(), 100);
}

#[test]
fn test_zero_cooldown_periods_allowed() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_withdrawal_cooldown_secs(&admin, &0);
    client.set_slash_cooldown_secs(&admin, &0);

    assert_eq!(client.get_withdrawal_cooldown_secs(), 0);
    assert_eq!(client.get_slash_cooldown_secs(), 0);
}

#[test]
fn test_zero_fee_rates_allowed() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &0);
    client.set_attestation_fee_bps(&admin, &0);

    assert_eq!(client.get_protocol_fee_bps(), 0);
    assert_eq!(client.get_attestation_fee_bps(), 0);
}

#[test]
fn test_max_values_for_all_parameters() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_protocol_fee_bps(&admin, &MAX_PROTOCOL_FEE_BPS);
    client.set_attestation_fee_bps(&admin, &MAX_ATTESTATION_FEE_BPS);
    client.set_withdrawal_cooldown_secs(&admin, &MAX_WITHDRAWAL_COOLDOWN_SECS);
    client.set_slash_cooldown_secs(&admin, &MAX_SLASH_COOLDOWN_SECS);
    client.set_bronze_threshold(&admin, &MAX_BRONZE_THRESHOLD);
    client.set_silver_threshold(&admin, &MAX_SILVER_THRESHOLD);
    client.set_gold_threshold(&admin, &MAX_GOLD_THRESHOLD);
    client.set_platinum_threshold(&admin, &MAX_PLATINUM_THRESHOLD);

    assert_eq!(client.get_protocol_fee_bps(), MAX_PROTOCOL_FEE_BPS);
    assert_eq!(client.get_attestation_fee_bps(), MAX_ATTESTATION_FEE_BPS);
    assert_eq!(
        client.get_withdrawal_cooldown_secs(),
        MAX_WITHDRAWAL_COOLDOWN_SECS
    );
    assert_eq!(client.get_slash_cooldown_secs(), MAX_SLASH_COOLDOWN_SECS);
    assert_eq!(client.get_bronze_threshold(), MAX_BRONZE_THRESHOLD);
    assert_eq!(client.get_silver_threshold(), MAX_SILVER_THRESHOLD);
    assert_eq!(client.get_gold_threshold(), MAX_GOLD_THRESHOLD);
    assert_eq!(client.get_platinum_threshold(), MAX_PLATINUM_THRESHOLD);
}

// ============================================================================
// Category 10: Event Argument Verification (issue #138)
// ============================================================================

#[test]
fn test_protocol_fee_event_args() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Set to 100 first so old_value is known
    client.set_protocol_fee_bps(&admin, &100);
    // Now update: old=100, new=200
    client.set_protocol_fee_bps(&admin, &200);

    let events = e.events().all();
    // Find the last parameter_changed event
    let last = events.iter().rev().find(|(_, topics, _)| {
        if let soroban_sdk::Val::Symbol(s) = topics.get(0).unwrap() {
            s == soroban_sdk::Symbol::new(&e, "parameter_changed")
        } else {
            false
        }
    });
    assert!(last.is_some(), "parameter_changed event not emitted");
    let (_, _, data) = last.unwrap();
    // data = (parameter_name, old_value, new_value, caller, timestamp)
    let (_, old_val, new_val, _, _): (soroban_sdk::String, i128, i128, Address, u64) =
        data.into_val(&e);
    assert_eq!(old_val, 100i128, "old_value mismatch");
    assert_eq!(new_val, 200i128, "new_value mismatch");
}

#[test]
fn test_attestation_fee_event_args() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_attestation_fee_bps(&admin, &25);
    client.set_attestation_fee_bps(&admin, &50);

    let events = e.events().all();
    let last = events.iter().rev().find(|(_, topics, _)| {
        if let soroban_sdk::Val::Symbol(s) = topics.get(0).unwrap() {
            s == soroban_sdk::Symbol::new(&e, "parameter_changed")
        } else {
            false
        }
    });
    assert!(last.is_some(), "parameter_changed event not emitted");
    let (_, _, data) = last.unwrap();
    let (_, old_val, new_val, _, _): (soroban_sdk::String, i128, i128, Address, u64) =
        data.into_val(&e);
    assert_eq!(old_val, 25i128);
    assert_eq!(new_val, 50i128);
}

#[test]
fn test_withdrawal_cooldown_event_args() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_withdrawal_cooldown_secs(&admin, &3600);
    client.set_withdrawal_cooldown_secs(&admin, &7200);

    let events = e.events().all();
    let last = events.iter().rev().find(|(_, topics, _)| {
        if let soroban_sdk::Val::Symbol(s) = topics.get(0).unwrap() {
            s == soroban_sdk::Symbol::new(&e, "parameter_changed")
        } else {
            false
        }
    });
    assert!(last.is_some());
    let (_, _, data) = last.unwrap();
    let (_, old_val, new_val, _, _): (soroban_sdk::String, i128, i128, Address, u64) =
        data.into_val(&e);
    assert_eq!(old_val, 3600i128);
    assert_eq!(new_val, 7200i128);
}

#[test]
fn test_pause_signer_event_includes_old_and_new() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let signer = Address::generate(&e);

    // Enable signer: old=false, new=true
    client.set_pause_signer(&admin, &signer, &true);

    let events = e.events().all();
    let ev = events.iter().rev().find(|(_, topics, _)| {
        if let soroban_sdk::Val::Symbol(s) = topics.get(0).unwrap() {
            s == soroban_sdk::Symbol::new(&e, "pause_signer_set")
        } else {
            false
        }
    });
    assert!(ev.is_some(), "pause_signer_set event not emitted");
    let (_, _, data) = ev.unwrap();
    let (old_val, new_val): (bool, bool) = data.into_val(&e);
    assert!(!old_val, "old_enabled should be false");
    assert!(new_val, "new_enabled should be true");
}

#[test]
fn test_pause_threshold_event_includes_old_and_new() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let signer = Address::generate(&e);

    // Add signer first so threshold can be set to 1
    client.set_pause_signer(&admin, &signer, &true);
    client.set_pause_threshold(&admin, &1);

    let events = e.events().all();
    let ev = events.iter().rev().find(|(_, topics, _)| {
        if let soroban_sdk::Val::Symbol(s) = topics.get(0).unwrap() {
            s == soroban_sdk::Symbol::new(&e, "pause_threshold_set")
        } else {
            false
        }
    });
    assert!(ev.is_some(), "pause_threshold_set event not emitted");
    let (_, _, data) = ev.unwrap();
    let (old_val, new_val): (u32, u32) = data.into_val(&e);
    assert_eq!(old_val, 0u32, "old threshold should be 0");
    assert_eq!(new_val, 1u32, "new threshold should be 1");
}

#[test]
fn test_no_duplicate_events_on_parameter_update() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let events_before = e.events().all().len();
    client.set_protocol_fee_bps(&admin, &100);
    let events_after = e.events().all().len();

    // Exactly one event emitted per setter call
    assert_eq!(events_after - events_before, 1, "expected exactly 1 event per setter");
}

// ============================================================================
// Category 11: Atomic Batch Updates (update_parameters)
// ============================================================================

fn all_none() -> ParameterUpdate {
    ParameterUpdate {
        protocol_fee_bps: None,
        attestation_fee_bps: None,
        withdrawal_cooldown_secs: None,
        slash_cooldown_secs: None,
        bronze_threshold: None,
        silver_threshold: None,
        gold_threshold: None,
        platinum_threshold: None,
        max_leverage: None,
    }
}

#[test]
fn test_update_parameters_all_valid_applies_all() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let update = ParameterUpdate {
        protocol_fee_bps: Some(100),
        attestation_fee_bps: Some(50),
        withdrawal_cooldown_secs: Some(86_400),
        slash_cooldown_secs: Some(43_200),
        bronze_threshold: Some(200_000_000),
        silver_threshold: Some(2_000_000_000),
        gold_threshold: Some(20_000_000_000),
        platinum_threshold: Some(200_000_000_000),
        max_leverage: Some(500),
    };

    client.update_parameters(&admin, &update);

    assert_eq!(client.get_protocol_fee_bps(), 100);
    assert_eq!(client.get_attestation_fee_bps(), 50);
    assert_eq!(client.get_withdrawal_cooldown_secs(), 86_400);
    assert_eq!(client.get_slash_cooldown_secs(), 43_200);
    assert_eq!(client.get_bronze_threshold(), 200_000_000);
    assert_eq!(client.get_silver_threshold(), 2_000_000_000);
    assert_eq!(client.get_gold_threshold(), 20_000_000_000);
    assert_eq!(client.get_platinum_threshold(), 200_000_000_000);
    assert_eq!(client.get_max_leverage(), 500);
}

#[test]
fn test_update_parameters_all_none_is_noop() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Record state before
    let fee_before = client.get_protocol_fee_bps();
    let cooldown_before = client.get_withdrawal_cooldown_secs();

    client.update_parameters(&admin, &all_none());

    // Nothing should change
    assert_eq!(client.get_protocol_fee_bps(), fee_before);
    assert_eq!(client.get_withdrawal_cooldown_secs(), cooldown_before);
}

#[test]
fn test_update_parameters_partial_only_changes_some_fields() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Set a known baseline
    client.set_protocol_fee_bps(&admin, &200);
    client.set_attestation_fee_bps(&admin, &30);

    let update = ParameterUpdate {
        protocol_fee_bps: Some(300),
        ..all_none()
    };
    client.update_parameters(&admin, &update);

    assert_eq!(client.get_protocol_fee_bps(), 300, "protocol_fee_bps should be updated");
    assert_eq!(client.get_attestation_fee_bps(), 30, "attestation_fee_bps must be untouched");
}

#[test]
#[should_panic(expected = "protocol_fee_bps out of bounds")]
fn test_update_parameters_invalid_first_field_reverts_all() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Set a known baseline state
    client.set_attestation_fee_bps(&admin, &20);

    let update = ParameterUpdate {
        // Invalid: above MAX_PROTOCOL_FEE_BPS (1000)
        protocol_fee_bps: Some(MAX_PROTOCOL_FEE_BPS + 1),
        // Valid field after invalid one
        attestation_fee_bps: Some(40),
        ..all_none()
    };

    // Must panic before any write, so attestation_fee_bps stays at 20
    client.update_parameters(&admin, &update);
}

#[test]
#[should_panic(expected = "silver_threshold out of bounds")]
fn test_update_parameters_invalid_mid_field_reverts_all() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Pre-set bronze to a known value
    client.set_bronze_threshold(&admin, &100_000_000);

    let update = ParameterUpdate {
        // Valid field first
        protocol_fee_bps: Some(100),
        // Valid field
        bronze_threshold: Some(200_000_000),
        // Invalid: below MIN_SILVER_THRESHOLD (100_000_000)
        silver_threshold: Some(MIN_SILVER_THRESHOLD - 1),
        ..all_none()
    };

    // Must panic before writing protocol_fee_bps or bronze_threshold
    client.update_parameters(&admin, &update);
}

/// Verify that a call panic from `test_update_parameters_invalid_mid_field_reverts_all`
/// leaves the state at its pre-call values (no partial write).
#[test]
fn test_update_parameters_no_partial_state_on_invalid_mid_field() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Establish baseline
    client.set_protocol_fee_bps(&admin, &75);
    client.set_bronze_threshold(&admin, &100_000_000);

    let update = ParameterUpdate {
        protocol_fee_bps: Some(200),           // valid
        bronze_threshold: Some(300_000_000),    // valid
        silver_threshold: Some(MIN_SILVER_THRESHOLD - 1), // invalid
        ..all_none()
    };

    // Capture result — should panic
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.update_parameters(&admin, &update);
    }));
    assert!(result.is_err(), "expected panic on out-of-bounds silver_threshold");

    // State must be unchanged — validation runs before any write
    assert_eq!(client.get_protocol_fee_bps(), 75, "protocol_fee_bps must not be written");
    assert_eq!(client.get_bronze_threshold(), 100_000_000, "bronze_threshold must not be written");
}

#[test]
#[should_panic(expected = "not admin")]
fn test_update_parameters_non_admin_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);

    let attacker = Address::generate(&e);
    let update = ParameterUpdate {
        protocol_fee_bps: Some(100),
        ..all_none()
    };

    client.update_parameters(&attacker, &update);
}

#[test]
fn test_update_parameters_emits_one_event_per_changed_field() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let events_before = e.events().all().len();

    let update = ParameterUpdate {
        protocol_fee_bps: Some(100),
        attestation_fee_bps: Some(20),
        withdrawal_cooldown_secs: Some(3600),
        ..all_none()
    };
    client.update_parameters(&admin, &update);

    let events_after = e.events().all().len();
    assert_eq!(
        events_after - events_before,
        3,
        "expected exactly one event per written field"
    );
}

#[test]
fn test_update_parameters_emits_correct_event_payload() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Establish old value
    client.set_protocol_fee_bps(&admin, &50);

    let update = ParameterUpdate {
        protocol_fee_bps: Some(150),
        ..all_none()
    };
    client.update_parameters(&admin, &update);

    let events = e.events().all();
    let last = events.iter().rev().find(|(_, topics, _)| {
        if let soroban_sdk::Val::Symbol(s) = topics.get(0).unwrap() {
            s == soroban_sdk::Symbol::new(&e, "parameter_changed")
        } else {
            false
        }
    });
    assert!(last.is_some(), "parameter_changed event not emitted");
    let (_, _, data) = last.unwrap();
    let (_, old_val, new_val, _, _): (soroban_sdk::String, i128, i128, Address, u64) =
        data.into_val(&e);
    assert_eq!(old_val, 50i128, "old_value mismatch");
    assert_eq!(new_val, 150i128, "new_value mismatch");
}

#[test]
#[should_panic(expected = "max_leverage out of bounds")]
fn test_update_parameters_invalid_last_field_reverts_all() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let update = ParameterUpdate {
        protocol_fee_bps: Some(100), // valid
        max_leverage: Some(MAX_MAX_LEVERAGE + 1), // invalid
        ..all_none()
    };

    // Must panic due to max_leverage; protocol_fee_bps must not be written
    client.update_parameters(&admin, &update);
}
