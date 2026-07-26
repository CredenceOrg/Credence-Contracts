// ============================================================================
// FILE: contracts/credence_bond/src/fee_tests.rs
//
// Add to contracts/credence_bond/src/lib.rs:
//   #[cfg(test)]
//   mod fee_tests;
// ============================================================================

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, FromVal,
};

use crate::fee::{
    get_protocol_fee_bps, set_protocol_fee_bps, BPS_DENOMINATOR, DEFAULT_FEE_BPS, MAX_FEE_BPS,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Spin up a fresh test environment and return (env, admin_address).
fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    (env, admin)
}

// ---------------------------------------------------------------------------
// Constants sanity
// ---------------------------------------------------------------------------

#[test]
fn test_constants_are_consistent() {
    // MAX_FEE_BPS must be strictly less than BPS_DENOMINATOR (100 %)
    assert!(
        MAX_FEE_BPS < BPS_DENOMINATOR,
        "MAX_FEE_BPS ({MAX_FEE_BPS}) must be < BPS_DENOMINATOR ({BPS_DENOMINATOR})"
    );

    // DEFAULT_FEE_BPS must not exceed the cap
    assert!(
        DEFAULT_FEE_BPS <= MAX_FEE_BPS,
        "DEFAULT_FEE_BPS ({DEFAULT_FEE_BPS}) must be <= MAX_FEE_BPS ({MAX_FEE_BPS})"
    );
}

// ---------------------------------------------------------------------------
// Default / unset behaviour
// ---------------------------------------------------------------------------

#[test]
fn test_get_fee_returns_default_when_unset() {
    let (env, _admin) = setup();
    let fee = get_protocol_fee_bps(&env);
    assert_eq!(
        fee, DEFAULT_FEE_BPS,
        "Unset fee should return DEFAULT_FEE_BPS ({DEFAULT_FEE_BPS} bps)"
    );
}

// ---------------------------------------------------------------------------
// Valid fee values
// ---------------------------------------------------------------------------

#[test]
fn test_set_fee_to_zero_bps() {
    let (env, admin) = setup();
    set_protocol_fee_bps(&env, &admin, 0);
    assert_eq!(get_protocol_fee_bps(&env), 0, "Fee should be 0 bps");
}

#[test]
fn test_set_fee_to_default_bps() {
    let (env, admin) = setup();
    set_protocol_fee_bps(&env, &admin, DEFAULT_FEE_BPS);
    assert_eq!(
        get_protocol_fee_bps(&env),
        DEFAULT_FEE_BPS,
        "Fee should equal DEFAULT_FEE_BPS"
    );
}

#[test]
fn test_set_fee_to_exactly_max_bps() {
    let (env, admin) = setup();
    // MAX_FEE_BPS itself must be accepted (inclusive upper bound)
    set_protocol_fee_bps(&env, &admin, MAX_FEE_BPS);
    assert_eq!(
        get_protocol_fee_bps(&env),
        MAX_FEE_BPS,
        "Fee equal to MAX_FEE_BPS ({MAX_FEE_BPS} bps) must be accepted"
    );
}

#[test]
fn test_set_fee_to_midrange_value() {
    let (env, admin) = setup();
    let mid = MAX_FEE_BPS / 2; // 500 bps = 5 %
    set_protocol_fee_bps(&env, &admin, mid);
    assert_eq!(get_protocol_fee_bps(&env), mid);
}

#[test]
fn test_fee_update_overwrites_previous_value() {
    let (env, admin) = setup();
    set_protocol_fee_bps(&env, &admin, 100);
    assert_eq!(get_protocol_fee_bps(&env), 100);

    set_protocol_fee_bps(&env, &admin, 300);
    assert_eq!(
        get_protocol_fee_bps(&env),
        300,
        "Second write should overwrite first"
    );
}

// ---------------------------------------------------------------------------
// Invalid fee values — must panic / error
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_set_fee_one_bps_above_max_panics() {
    let (env, admin) = setup();
    // MAX_FEE_BPS + 1 must be rejected
    set_protocol_fee_bps(&env, &admin, MAX_FEE_BPS + 1);
}

#[test]
#[should_panic]
fn test_set_fee_to_half_bps_denominator_panics() {
    let (env, admin) = setup();
    // 50 % — well above the 10 % cap
    set_protocol_fee_bps(&env, &admin, BPS_DENOMINATOR / 2);
}

#[test]
#[should_panic]
fn test_set_fee_to_full_bps_denominator_panics() {
    let (env, admin) = setup();
    // 100 % — confiscatory
    set_protocol_fee_bps(&env, &admin, BPS_DENOMINATOR);
}

#[test]
#[should_panic]
fn test_set_fee_to_u32_max_panics() {
    let (env, admin) = setup();
    set_protocol_fee_bps(&env, &admin, u32::MAX);
}

// ---------------------------------------------------------------------------
// Event emission
// ---------------------------------------------------------------------------

#[test]
fn test_fee_update_emits_event_with_previous_and_new_values() {
    let (env, admin) = setup();

    // Establish a known initial state
    set_protocol_fee_bps(&env, &admin, 100); // previous = 100 bps

    // Clear event log so we only see the second call
    env.events().all(); // drain

    set_protocol_fee_bps(&env, &admin, 250); // new = 250 bps

    let events = env.events().all();
    assert!(!events.is_empty(), "An event should have been emitted");

    // The last event data must contain (previous=100, new=250)
    // Event structure: (topics, data) where data = (previous_bps, new_bps)
    let (_, _topics, data) = events.last().unwrap();
    let (prev, next): (u32, u32) = <(u32, u32)>::from_val(&env, &data);
    assert_eq!(prev, 100, "Event must carry previous fee (100 bps)");
    assert_eq!(next, 250, "Event must carry new fee (250 bps)");
}

#[test]
fn test_fee_update_from_default_emits_correct_previous() {
    let (env, admin) = setup();

    // First ever set — previous should be DEFAULT_FEE_BPS
    env.events().all(); // drain
    set_protocol_fee_bps(&env, &admin, 500);

    let events = env.events().all();
    let (_, _topics, data) = events.last().unwrap();
    let (prev, next): (u32, u32) = <(u32, u32)>::from_val(&env, &data);
    assert_eq!(
        prev, DEFAULT_FEE_BPS,
        "Previous should be DEFAULT_FEE_BPS when unset"
    );
    assert_eq!(next, 500);
}

// ---------------------------------------------------------------------------
// Migration regression test
// ---------------------------------------------------------------------------

#[test]
fn test_default_fee_constant_has_not_changed() {
    // This test will fail if DEFAULT_FEE_BPS is accidentally changed,
    // alerting contributors to update the migration guide.
    assert_eq!(
        DEFAULT_FEE_BPS, 200,
        "DEFAULT_FEE_BPS changed from 200 bps — update the migration guide in docs/"
    );
}

#[test]
fn test_max_fee_constant_has_not_changed() {
    assert_eq!(
        MAX_FEE_BPS, 1_000,
        "MAX_FEE_BPS changed from 1000 bps — verify no existing bonds are affected"
    );
}
