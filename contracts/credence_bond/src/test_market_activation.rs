//! Market Activation Validation Tests
//!
//! Regression suite for pre-activation parameter validation (issue #271).
//! Ensures bonds cannot be activated with missing or invalid risk parameters.

use crate::test_helpers::setup_with_token;
use crate::parameters::{MAX_PROTOCOL_FEE_BPS, MAX_GOLD_THRESHOLD, MAX_PLATINUM_THRESHOLD};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Env;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Minimum valid bond amount (above bronze default of 100_000_000).
const VALID_AMOUNT: i128 = 1_000_000_000;
/// 30-day duration in seconds.
const VALID_DURATION: u64 = 2_592_000;

/// Configure all required risk params so `create_bond` succeeds.
fn configure_all(client: &crate::CredenceBondClient<'_>, admin: &soroban_sdk::Address) {
    client.set_fee_config(admin, admin, &50);
    client.set_bronze_threshold(admin, &100_000_000_i128);
    client.set_silver_threshold(admin, &1_000_000_000_i128);
    client.set_gold_threshold(admin, &10_000_000_000_i128);
    client.set_platinum_threshold(admin, &100_000_000_000_i128);
    client.set_max_leverage(admin, &100_000_u32);
}

// ── existing regression: missing token config ─────────────────────────────

#[test]
#[should_panic]
fn test_activation_fails_without_token_config() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(crate::CredenceBond, ());
    let client = crate::CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    let identity = soroban_sdk::Address::generate(&e);
    client.initialize(&admin);
    // No token set → should panic
    client.create_bond(&identity, &VALID_AMOUNT, &VALID_DURATION);
}

// ── gold threshold: missing (zero) ───────────────────────────────────────────

#[test]
#[should_panic]
fn test_activation_fails_with_zero_gold_threshold() {
    let e = Env::default();
    let (client, admin, identity, _token, _cid) = setup_with_token(&e);
    client.set_fee_config(&admin, &admin, &50);
    client.set_bronze_threshold(&admin, &100_000_000_i128);
    client.set_silver_threshold(&admin, &1_000_000_000_i128);
    // gold threshold left at default (10_000_000_000) but silver == gold would fail;
    // set gold == silver to trigger "gold threshold must be greater than silver threshold"
    client.set_gold_threshold(&admin, &1_000_000_000_i128); // equal to silver
    client.set_platinum_threshold(&admin, &100_000_000_000_i128);
    client.set_max_leverage(&admin, &100_000_u32);
    client.create_bond(&identity, &VALID_AMOUNT, &VALID_DURATION);
}

// ── gold threshold: above maximum bound ──────────────────────────────────────

#[test]
#[should_panic]
fn test_set_gold_threshold_above_max_panics() {
    let e = Env::default();
    let (client, admin, _identity, _token, _cid) = setup_with_token(&e);
    // MAX_GOLD_THRESHOLD + 1 must be rejected by the setter
    client.set_gold_threshold(&admin, &(MAX_GOLD_THRESHOLD + 1));
}

// ── platinum threshold: missing (equal to gold) ───────────────────────────────

#[test]
#[should_panic]
fn test_activation_fails_with_platinum_equal_to_gold() {
    let e = Env::default();
    let (client, admin, identity, _token, _cid) = setup_with_token(&e);
    client.set_fee_config(&admin, &admin, &50);
    client.set_bronze_threshold(&admin, &100_000_000_i128);
    client.set_silver_threshold(&admin, &1_000_000_000_i128);
    client.set_gold_threshold(&admin, &10_000_000_000_i128);
    // platinum == gold → invalid ordering
    client.set_platinum_threshold(&admin, &10_000_000_000_i128);
    client.set_max_leverage(&admin, &100_000_u32);
    client.create_bond(&identity, &VALID_AMOUNT, &VALID_DURATION);
}

// ── platinum threshold: above maximum bound ───────────────────────────────────

#[test]
#[should_panic]
fn test_set_platinum_threshold_above_max_panics() {
    let e = Env::default();
    let (client, admin, _identity, _token, _cid) = setup_with_token(&e);
    client.set_platinum_threshold(&admin, &(MAX_PLATINUM_THRESHOLD + 1));
}

// ── fee bps: above maximum bound ─────────────────────────────────────────────

#[test]
#[should_panic]
fn test_set_protocol_fee_bps_above_max_panics() {
    let e = Env::default();
    let (client, admin, _identity, _token, _cid) = setup_with_token(&e);
    // MAX_PROTOCOL_FEE_BPS is 1000; 1001 must be rejected
    client.set_protocol_fee_bps(&admin, &(MAX_PROTOCOL_FEE_BPS + 1));
}

// ── fee bps: zero is valid (no fee) ──────────────────────────────────────────

#[test]
fn test_set_protocol_fee_bps_zero_is_valid() {
    let e = Env::default();
    let (client, admin, _identity, _token, _cid) = setup_with_token(&e);
    client.set_protocol_fee_bps(&admin, &0);
    assert_eq!(client.get_protocol_fee_bps(), 0);
}

// ── duration: below minimum (< 1 day) ────────────────────────────────────────

#[test]
#[should_panic]
fn test_activation_fails_with_duration_below_minimum() {
    let e = Env::default();
    let (client, admin, identity, _token, _cid) = setup_with_token(&e);
    configure_all(&client, &admin);
    // 1 second < MIN_BOND_DURATION (86_400)
    client.create_bond(&identity, &VALID_AMOUNT, &1_u64);
}

// ── duration: above maximum (> 365 days) ─────────────────────────────────────

#[test]
#[should_panic]
fn test_activation_fails_with_duration_above_maximum() {
    let e = Env::default();
    let (client, admin, identity, _token, _cid) = setup_with_token(&e);
    configure_all(&client, &admin);
    // 366 days > MAX_BOND_DURATION (31_536_000)
    client.create_bond(&identity, &VALID_AMOUNT, &(31_536_000_u64 + 1));
}

// ── negative amount ───────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn test_activation_fails_with_negative_amount() {
    let e = Env::default();
    let (client, admin, identity, _token, _cid) = setup_with_token(&e);
    configure_all(&client, &admin);
    client.create_bond(&identity, &(-1_i128), &VALID_DURATION);
}

// ── zero amount ───────────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn test_activation_fails_with_zero_amount() {
    let e = Env::default();
    let (client, admin, identity, _token, _cid) = setup_with_token(&e);
    configure_all(&client, &admin);
    client.create_bond(&identity, &0_i128, &VALID_DURATION);
}

// ── valid activation succeeds ─────────────────────────────────────────────────

#[test]
fn test_valid_bond_activation_succeeds() {
    let e = Env::default();
    let (client, admin, identity, _token, _cid) = setup_with_token(&e);
    configure_all(&client, &admin);
    let bond = client.create_bond(&identity, &VALID_AMOUNT, &VALID_DURATION);
    assert!(bond.active);
    assert_eq!(bond.bond_duration, VALID_DURATION);
}
