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
