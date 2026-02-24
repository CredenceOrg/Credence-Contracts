//! Comprehensive tests for bond duration validation.
//!
//! Covers minimum and maximum duration enforcement, boundary conditions,
//! and clear error message verification on bond creation.

#![cfg(test)]

use crate::validation::{self, MAX_BOND_DURATION, MIN_BOND_DURATION};
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> CredenceBondClient<'_> {
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    client
}

// ────────────────────────────────────────────────────────────────
// Unit tests for validate_bond_duration
// ────────────────────────────────────────────────────────────────

/// Duration at the exact minimum boundary should pass.
#[test]
fn test_validate_duration_at_minimum() {
    validation::validate_bond_duration(MIN_BOND_DURATION);
}

/// Duration at the exact maximum boundary should pass.
#[test]
fn test_validate_duration_at_maximum() {
    validation::validate_bond_duration(MAX_BOND_DURATION);
}

/// Duration above minimum should pass.
#[test]
fn test_validate_duration_above_minimum() {
    validation::validate_bond_duration(MIN_BOND_DURATION + 1);
}

/// Duration below maximum should pass.
#[test]
fn test_validate_duration_below_maximum() {
    validation::validate_bond_duration(MAX_BOND_DURATION - 1);
}

/// Duration in the middle of the valid range should pass.
#[test]
fn test_validate_duration_mid_range() {
    // 30 days
    validation::validate_bond_duration(2_592_000);
}

/// Zero duration must be rejected.
#[test]
#[should_panic(expected = "bond duration too short: minimum is 86400 seconds (1 day)")]
fn test_validate_duration_zero() {
    validation::validate_bond_duration(0);
}

/// Duration one second below minimum must be rejected.
#[test]
#[should_panic(expected = "bond duration too short: minimum is 86400 seconds (1 day)")]
fn test_validate_duration_just_below_minimum() {
    validation::validate_bond_duration(MIN_BOND_DURATION - 1);
}

/// Very small duration (1 second) must be rejected.
#[test]
#[should_panic(expected = "bond duration too short: minimum is 86400 seconds (1 day)")]
fn test_validate_duration_one_second() {
    validation::validate_bond_duration(1);
}

/// Duration one second above maximum must be rejected.
#[test]
#[should_panic(expected = "bond duration too long: maximum is 31536000 seconds (365 days)")]
fn test_validate_duration_just_above_maximum() {
    validation::validate_bond_duration(MAX_BOND_DURATION + 1);
}

/// u64::MAX duration must be rejected.
#[test]
#[should_panic(expected = "bond duration too long: maximum is 31536000 seconds (365 days)")]
fn test_validate_duration_u64_max() {
    validation::validate_bond_duration(u64::MAX);
}

// ────────────────────────────────────────────────────────────────
// Integration tests: create_bond with duration validation
// ────────────────────────────────────────────────────────────────

/// Bond creation with minimum valid duration succeeds.
#[test]
fn test_create_bond_min_duration() {
    let e = Env::default();
    e.mock_all_auths();
    let client = setup(&e);
    let identity = Address::generate(&e);
    let bond = client.create_bond(
        &identity,
        &1000_i128,
        &MIN_BOND_DURATION,
        &false,
        &0_u64,
    );
    assert!(bond.active);
    assert_eq!(bond.bond_duration, MIN_BOND_DURATION);
}

/// Bond creation with maximum valid duration succeeds.
#[test]
fn test_create_bond_max_duration() {
    let e = Env::default();
    e.mock_all_auths();
    let client = setup(&e);
    let identity = Address::generate(&e);
    let bond = client.create_bond(
        &identity,
        &1000_i128,
        &MAX_BOND_DURATION,
        &false,
        &0_u64,
    );
    assert!(bond.active);
    assert_eq!(bond.bond_duration, MAX_BOND_DURATION);
}

/// Bond creation with typical 30-day duration succeeds.
#[test]
fn test_create_bond_typical_duration() {
    let e = Env::default();
    e.mock_all_auths();
    let client = setup(&e);
    let identity = Address::generate(&e);
    let thirty_days = 30 * 86_400_u64;
    let bond = client.create_bond(&identity, &1000_i128, &thirty_days, &false, &0_u64);
    assert!(bond.active);
    assert_eq!(bond.bond_duration, thirty_days);
}

/// Bond creation with zero duration must be rejected.
#[test]
#[should_panic(expected = "bond duration too short: minimum is 86400 seconds (1 day)")]
fn test_create_bond_zero_duration_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let client = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &0_u64, &false, &0_u64);
}

/// Bond creation with duration below minimum must be rejected.
#[test]
#[should_panic(expected = "bond duration too short: minimum is 86400 seconds (1 day)")]
fn test_create_bond_below_min_duration_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let client = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &(MIN_BOND_DURATION - 1),
        &false,
        &0_u64,
    );
}

/// Bond creation with duration above maximum must be rejected.
#[test]
#[should_panic(expected = "bond duration too long: maximum is 31536000 seconds (365 days)")]
fn test_create_bond_above_max_duration_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let client = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &(MAX_BOND_DURATION + 1),
        &false,
        &0_u64,
    );
}

/// Rolling bond creation with valid duration succeeds.
#[test]
fn test_create_rolling_bond_valid_duration() {
    let e = Env::default();
    e.mock_all_auths();
    let client = setup(&e);
    let identity = Address::generate(&e);
    let bond = client.create_bond(
        &identity,
        &1000_i128,
        &MIN_BOND_DURATION,
        &true,
        &3600_u64,
    );
    assert!(bond.active);
    assert!(bond.is_rolling);
    assert_eq!(bond.bond_duration, MIN_BOND_DURATION);
}

/// Rolling bond creation with invalid duration must be rejected.
#[test]
#[should_panic(expected = "bond duration too short: minimum is 86400 seconds (1 day)")]
fn test_create_rolling_bond_invalid_duration_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let client = setup(&e);
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &3600_u64, &true, &1800_u64);
}

/// Constants have expected values.
#[test]
fn test_duration_constants() {
    assert_eq!(MIN_BOND_DURATION, 86_400);
    assert_eq!(MAX_BOND_DURATION, 31_536_000);
    assert!(MIN_BOND_DURATION < MAX_BOND_DURATION);
}
