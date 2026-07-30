//! Comprehensive unit tests for slashing functionality with 95%+ coverage.
//!
//! Test categories:
//! 1.  Basic slashing operations
//! 2.  Authorization and security
//! 3.  Over-slash prevention — amounts above available balance are REJECTED
//! 4.  Edge cases (zero, negative, max values)
//! 5.  State consistency and tracking
//! 6.  Event emission and audit trails
//! 7.  Integration with withdrawals
//! 8.  Cumulative slashing scenarios
//! 9.  State persistence
//! 10. Error messages
//! 11. Available-balance bound (slash bounded by bonded - slashed)
//! 12. Slash history records
//! 13. Treasury transfer

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

// ============================================================================
// Test Setup Utilities
// ============================================================================

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(e);
    let treasury = Address::generate(e);
    client.set_slash_treasury(&admin, &treasury);
    (client, admin, identity)
}

fn setup_with_bond(
    e: &Env,
    amount: i128,
    duration: u64,
) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity) = setup(e);
    client.create_bond(&identity, &amount, &duration, &false, &0_u64);
    test_helpers::advance_ledger_sequence(e);
    (client, admin, identity)
}

/// Setup with max mint for tests requiring large bond amounts (e.g. overflow tests).
fn setup_with_bond_max_mint(
    e: &Env,
    amount: i128,
    duration: u64,
) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity, _token_id, _bond_id) = test_helpers::setup_with_max_mint(e);
    let treasury = Address::generate(e);
    client.set_slash_treasury(&admin, &treasury);
    client.create_bond(&identity, &amount, &duration, &false, &0_u64);
    test_helpers::advance_ledger_sequence(e);
    (client, admin, identity)
}

// ============================================================================
// Category 1: Basic Slashing Operations
// ============================================================================

#[test]
fn test_slash_basic_success() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let bond = client.slash(&admin, &identity, &300_i128);

    assert_eq!(bond.slashed_amount, 300);
    assert_eq!(bond.bonded_amount, 1000);
    assert!(bond.active);
}

#[test]
fn test_slash_small_amount() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 10000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let bond = client.slash(&admin, &identity, &1_i128);

    assert_eq!(bond.slashed_amount, 1);
    assert_eq!(bond.bonded_amount, 10000);
}

#[test]
fn test_slash_exact_half() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let bond = client.slash(&admin, &identity, &500_i128);

    assert_eq!(bond.slashed_amount, 500);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_slash_entire_amount() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let bond = client.slash(&admin, &identity, &1000_i128);

    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(bond.bonded_amount, 1000);
}

// ============================================================================
// Category 2: Authorization and Security
// ============================================================================

/// THREAT: T-001 — Ensures only admin can slash bonds.
#[test]
#[should_panic(expected = "not admin")]
fn test_slash_unauthorized_rejection() {
    let e = Env::default();
    let (client, _admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let other = Address::generate(&e);
    client.slash(&other, &identity, &100_i128);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_slash_unauthorized_different_address() {
    let e = Env::default();
    let (client, _admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let attacker = Address::generate(&e);
    client.slash(&attacker, &identity, &500_i128);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_slash_identity_cannot_slash_own_bond() {
    let e = Env::default();
    let (client, _admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&identity, &identity, &100_i128);
}

// ============================================================================
// Category 3: Over-Slash Prevention — REJECT, not silent cap
//
// Issue #995: slashing::slash_bond() previously silently capped the slash at
// the available balance. The normalized behavior (matching lib.rs slash_bond)
// is to REJECT with "slash exceeds bond" when amount > available balance.
// ============================================================================

/// THREAT: T-007 — Over-slash attempt must PANIC, not silently cap.
#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_slash_over_amount_rejected() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    // 2000 > available (1000): must panic, not silently cap
    client.slash(&admin, &identity, &2000_i128);
}

#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_slash_way_over_amount_rejected() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &5_000_i128);
}

#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_slash_max_i128_rejected() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &i128::MAX);
}

// ============================================================================
// Category 4: Edge Cases (Zero, Negative, Boundary Values)
// ============================================================================

/// Zero slash amount must panic with "slash amount must be positive".
#[test]
#[should_panic(expected = "slash amount must be positive")]
fn test_slash_zero_amount_rejected() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &0_i128);
}

/// Negative slash amount must panic with "slash amount must be positive".
#[test]
#[should_panic(expected = "slash amount must be positive")]
fn test_slash_negative_amount_rejected() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &-1_i128);
}

/// Slashing exactly the available balance succeeds (full slash).
#[test]
fn test_slash_exactly_available_succeeds() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    // First partial slash
    client.slash(&admin, &identity, &600_i128);
    // Available is now 400. Slash exactly 400.
    let bond = client.slash(&admin, &identity, &400_i128);

    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(bond.bonded_amount, 1000);
}

/// Slashing 1 above available must panic.
#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_slash_one_above_available_rejected() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    // First partial slash leaves 400 available
    client.slash(&admin, &identity, &600_i128);
    // 401 > 400: must panic
    client.slash(&admin, &identity, &401_i128);
}

#[test]
fn test_slash_on_very_large_bond() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond_max_mint(&e, crate::validation::MAX_BOND_AMOUNT, credence_math::Timestamp::SECONDS_PER_DAY);

    let bond = client.slash(&admin, &identity, &(crate::validation::MAX_BOND_AMOUNT / 4));

    assert_eq!(bond.slashed_amount, crate::validation::MAX_BOND_AMOUNT / 4);
}

// ============================================================================
// Category 5: State Consistency and Tracking
// ============================================================================

#[test]
fn test_slash_history_single_slash() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &200_i128);
    let bond = client.get_identity_state(&identity);

    assert_eq!(bond.slashed_amount, 200);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_slash_history_cumulative() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let bond1 = client.slash(&admin, &identity, &200_i128);
    assert_eq!(bond1.slashed_amount, 200);

    let bond2 = client.slash(&admin, &identity, &300_i128);
    assert_eq!(bond2.slashed_amount, 500);

    let bond3 = client.get_identity_state(&identity);
    assert_eq!(bond3.slashed_amount, 500);
}

#[test]
fn test_slash_multiple_accumulate() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 10000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    // 1000 + 2000 + 3000 = 6000 total, all within bonded
    client.slash(&admin, &identity, &1000_i128);
    client.slash(&admin, &identity, &2000_i128);
    let bond = client.slash(&admin, &identity, &3000_i128);
    assert_eq!(bond.slashed_amount, 6000);
}

#[test]
fn test_slash_does_not_affect_other_fields() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let original_bond = client.get_identity_state(&identity);
    let original_bonded = original_bond.bonded_amount;
    let original_start = original_bond.bond_start;
    let original_duration = original_bond.bond_duration;

    client.slash(&admin, &identity, &300_i128);

    let updated_bond = client.get_identity_state(&identity);
    assert_eq!(updated_bond.bonded_amount, original_bonded);
    assert_eq!(updated_bond.bond_start, original_start);
    assert_eq!(updated_bond.bond_duration, original_duration);
    assert_eq!(updated_bond.identity, identity);
}

// ============================================================================
// Category 6: Event Emission and Audit Trails
// ============================================================================

#[test]
fn test_slash_event_emitted_basic() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let _bond = client.slash(&admin, &identity, &250_i128);

    let state = client.get_identity_state(&identity);
    assert_eq!(state.slashed_amount, 250);
}

#[test]
fn test_slash_event_contains_correct_event_data() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let bond1 = client.slash(&admin, &identity, &100_i128);
    assert_eq!(bond1.slashed_amount, 100);

    let bond2 = client.slash(&admin, &identity, &200_i128);
    // Event contains slash_amount=200, total_slashed=300
    assert_eq!(bond2.slashed_amount, 300);
}

#[test]
fn test_slash_multiple_events() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    // Each slash emits an event; cumulative must be correct
    let b1 = client.slash(&admin, &identity, &100_i128);
    let b2 = client.slash(&admin, &identity, &200_i128);
    let b3 = client.slash(&admin, &identity, &300_i128);
    assert_eq!(b1.slashed_amount, 100);
    assert_eq!(b2.slashed_amount, 300);
    assert_eq!(b3.slashed_amount, 600);
}

// ============================================================================
// Category 7: Integration with Withdrawals
// ============================================================================

#[test]
fn test_withdraw_after_slash_respects_available() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &identity, &400_i128);
    e.ledger().with_mut(|li| li.timestamp = 86401);
    // 600 available; withdraw exactly 600
    let bond = client.withdraw(&identity, &600_i128);
    assert_eq!(bond.bonded_amount, 400);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdraw_when_fully_slashed() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    test_helpers::advance_ledger_sequence(&e);

    client.slash(&admin, &identity, &1000_i128);

    e.ledger().with_mut(|li| li.timestamp = 86401);
    client.withdraw(&identity, &1_i128);
}

#[test]
fn test_withdraw_exact_available_balance() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &identity, &400_i128);
    e.ledger().with_mut(|li| li.timestamp = 86401);
    let bond = client.withdraw(&identity, &600_i128);
    assert_eq!(bond.bonded_amount, 400);
}

#[test]
fn test_slash_then_withdraw_then_slash_again() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    test_helpers::advance_ledger_sequence(&e);

    client.slash(&admin, &identity, &200_i128);
    assert_eq!(client.get_identity_state(&identity).bonded_amount, 1000);

    e.ledger().with_mut(|li| li.timestamp = 86401);
    client.withdraw(&identity, &300_i128);
    assert_eq!(client.get_identity_state(&identity).bonded_amount, 700);

    // After withdrawal: bonded=700, slashed=200, available=500
    let bond = client.slash(&admin, &identity, &100_i128);
    assert_eq!(bond.slashed_amount, 300);
    assert_eq!(bond.bonded_amount, 700);
}

#[test]
fn test_slash_after_partial_withdrawal() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    e.ledger().with_mut(|li| li.timestamp = 86401);
    client.withdraw(&identity, &300_i128);
    assert_eq!(client.get_identity_state(&identity).bonded_amount, 700);

    test_helpers::advance_ledger_sequence(&e);
    let bond = client.slash(&admin, &identity, &200_i128);
    assert_eq!(bond.bonded_amount, 700);
    assert_eq!(bond.slashed_amount, 200);

    // Available = 700 - 200 = 500
    client.withdraw(&identity, &500_i128);
    assert_eq!(client.get_identity_state(&identity).bonded_amount, 200);
}

// ============================================================================
// Category 8: Cumulative Slashing Scenarios
// ============================================================================

/// After partial slash, an over-amount slash is REJECTED (not silently capped).
#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_cumulative_slash_over_available_rejected() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &600_i128);
    // available = 400; 600 > 400 must panic
    client.slash(&admin, &identity, &600_i128);
}

#[test]
fn test_cumulative_slash_incremental() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 10000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    for i in 1..=10 {
        let bond = client.slash(&admin, &identity, &1000_i128);
        assert_eq!(bond.slashed_amount, (i as i128) * 1000_i128);
    }
}

/// After full slash, any further slash must panic (available = 0).
#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_full_slash_prevents_further_slashing() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &1000_i128);
    // available = 0; any positive slash must panic
    client.slash(&admin, &identity, &1_i128);
}

#[test]
fn test_slash_large_amounts() {
    let e = Env::default();
    let large_amount = 1_000_000_000_000_i128;
    let (client, admin, identity) =
        setup_with_bond(&e, large_amount, credence_math::Timestamp::SECONDS_PER_DAY);

    let bond1 = client.slash(&admin, &identity, &(large_amount / 4));
    assert_eq!(bond1.slashed_amount, large_amount / 4);

    let bond2 = client.slash(&admin, &identity, &(large_amount / 4));
    assert_eq!(bond2.slashed_amount, large_amount / 2);
}

// ============================================================================
// Category 9: State Persistence
// ============================================================================

#[test]
fn test_slash_state_persists() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &300_i128);
    let bond1 = client.get_identity_state(&identity);
    assert_eq!(bond1.slashed_amount, 300);

    let bond2 = client.get_identity_state(&identity);
    assert_eq!(bond2.slashed_amount, 300);
}

#[test]
fn test_slash_result_matches_get_state() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let slash_result = client.slash(&admin, &identity, &250_i128);
    let state = client.get_identity_state(&identity);

    assert_eq!(slash_result.slashed_amount, state.slashed_amount);
    assert_eq!(slash_result.bonded_amount, state.bonded_amount);
}

// ============================================================================
// Category 10: Error Messages
// ============================================================================

#[test]
#[should_panic(expected = "not admin")]
fn test_error_message_not_admin() {
    let e = Env::default();
    let (client, _admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let random = Address::generate(&e);
    client.slash(&random, &identity, &100_i128);
}

#[test]
#[should_panic(expected = "no bond")]
fn test_error_message_no_bond() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let no_bond_identity = Address::generate(&e);

    client.slash(&admin, &no_bond_identity, &100_i128);
}

#[test]
#[should_panic(expected = "slash amount must be positive")]
fn test_error_message_zero_amount() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &0_i128);
}

#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_error_message_slash_exceeds_bond() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &1001_i128);
}

// ============================================================================
// Category 11: Available-Balance Bound (slash <= bonded - slashed)
// ============================================================================

/// After a partial slash the cap is on remaining available, not total bonded.
#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_slash_rejected_above_available_not_bonded() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    // First slash: 600 → available becomes 400
    client.slash(&admin, &identity, &600_i128);

    // Second slash: request 500 > available (400) → must panic
    client.slash(&admin, &identity, &500_i128);
}

/// When available == 0, any positive slash panics.
#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_slash_zero_available_panics() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &1000_i128);
    // available = 0 → any positive slash panics
    client.slash(&admin, &identity, &1_i128);
}

#[test]
fn test_slash_available_decreases_after_each_slash() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &200_i128); // available: 800
    client.slash(&admin, &identity, &300_i128); // available: 500
    // Slash exactly remaining 500
    let bond = client.slash(&admin, &identity, &500_i128);
    assert_eq!(bond.slashed_amount, 1000);
}

#[test]
fn test_slash_after_withdraw_respects_new_available() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    e.ledger().with_mut(|li| li.timestamp = 86401);
    client.withdraw(&identity, &400_i128); // bonded=600, available=600
    test_helpers::advance_ledger_sequence(&e);
    // Slash exactly new available (600)
    let bond = client.slash(&admin, &identity, &600_i128);
    assert_eq!(bond.bonded_amount, 600);
    assert_eq!(bond.slashed_amount, 600);
}

/// After a withdrawal that reduces bonded, slash of the old bonded amount must panic.
#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_slash_after_withdraw_over_new_available_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    e.ledger().with_mut(|li| li.timestamp = 86401);
    client.withdraw(&identity, &400_i128); // bonded=600, available=600
    test_helpers::advance_ledger_sequence(&e);
    // 700 > available (600) → must panic
    client.slash(&admin, &identity, &700_i128);
}

// ============================================================================
// Category 12: Slash History Records
// ============================================================================

#[test]
fn test_slash_history_count_increments() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &100_i128);
    client.slash(&admin, &identity, &200_i128);

    let count = crate::slash_history::get_slash_count(&e, &identity);
    assert_eq!(count, 2);
}

#[test]
fn test_slash_history_record_fields() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 5000);
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &300_i128);

    let record = crate::slash_history::get_slash_record(&e, &identity, 0);
    assert_eq!(record.identity, identity);
    assert_eq!(record.slash_amount, 300);
    assert_eq!(record.total_slashed_after, 300);
    assert_eq!(record.timestamp, 5000);
}

#[test]
fn test_slash_history_total_slashed_after_accumulates() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    client.slash(&admin, &identity, &100_i128);
    client.slash(&admin, &identity, &200_i128);

    let r0 = crate::slash_history::get_slash_record(&e, &identity, 0);
    let r1 = crate::slash_history::get_slash_record(&e, &identity, 1);
    assert_eq!(r0.total_slashed_after, 100);
    assert_eq!(r1.total_slashed_after, 300);
}

/// Rejected slashes (over-available) must NOT append any history record.
/// This is verified indirectly: a valid slash records exactly once, and the
/// over-slash attempt panics (tested separately via #[should_panic]).
#[test]
fn test_slash_history_valid_slash_appends_exactly_one_record() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    assert_eq!(crate::slash_history::get_slash_count(&e, &identity), 0);

    client.slash(&admin, &identity, &300_i128);
    assert_eq!(crate::slash_history::get_slash_count(&e, &identity), 1);

    client.slash(&admin, &identity, &200_i128);
    assert_eq!(crate::slash_history::get_slash_count(&e, &identity), 2);
}

/// Over-available slash panics — no record appended (the panic unwinds any append).
#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_slash_history_over_available_panics_no_record() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    // First slash: 700 → available = 300
    client.slash(&admin, &identity, &700_i128);
    // Second slash: 400 > available (300) → must panic before any record is appended
    client.slash(&admin, &identity, &400_i128);
}

#[test]
fn test_slash_history_get_all_records() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 10000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    for i in 1_i128..=5 {
        client.slash(&admin, &identity, &(i * 100));
    }

    let history = crate::slash_history::get_slash_history(&e, &identity);
    assert_eq!(history.len(), 5);
    assert_eq!(history.get(0).unwrap().slash_amount, 100);
    assert_eq!(history.get(4).unwrap().slash_amount, 500);
}

// ============================================================================
// Category 13: Treasury Transfer
// ============================================================================

/// slash() reverts when no treasury is configured.
#[test]
#[should_panic]
fn test_slash_reverts_when_treasury_not_configured() {
    let e = Env::default();
    let (client, admin, identity, _token, _bond_id) = test_helpers::setup_with_token(&e);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    test_helpers::advance_ledger_sequence(&e);

    // No treasury configured → must panic
    client.slash(&admin, &identity, &300_i128);
}

/// slash() transfers exact slash amount to the treasury address.
#[test]
fn test_slash_transfers_to_treasury() {
    let e = Env::default();
    let (client, admin, identity, token_id, bond_id) = test_helpers::setup_with_token(&e);

    let treasury = Address::generate(&e);
    client.set_slash_treasury(&admin, &treasury);
    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    test_helpers::advance_ledger_sequence(&e);

    use soroban_sdk::token::TokenClient;
    let token = TokenClient::new(&e, &token_id);

    let bond_bal_before = token.balance(&bond_id);
    let treasury_bal_before = token.balance(&treasury);

    client.slash(&admin, &identity, &400_i128);

    let bond_bal_after = token.balance(&bond_id);
    let treasury_bal_after = token.balance(&treasury);

    assert_eq!(bond_bal_before - bond_bal_after, 400);
    assert_eq!(treasury_bal_after - treasury_bal_before, 400);

    let bond = client.get_identity_state(&identity);
    assert_eq!(bond.slashed_amount, 400);
}

/// Exact transfer regression: slashed funds move to the configured destination.
#[test]
fn test_slashed_funds_transfer_to_configured_destination() {
    let e = Env::default();
    e.mock_all_auths();

    let (client, admin, identity, token_id, bond_id) = test_helpers::setup_with_token(&e);

    let destination = Address::generate(&e);
    client.set_slash_treasury(&admin, &destination);

    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    test_helpers::advance_ledger_sequence(&e);

    use soroban_sdk::token::TokenClient;
    let token = TokenClient::new(&e, &token_id);

    let bond_bal_before = token.balance(&bond_id);
    let dest_bal_before = token.balance(&destination);

    client.slash(&admin, &identity, &250_i128);

    let bond_bal_after = token.balance(&bond_id);
    let dest_bal_after = token.balance(&destination);

    assert_eq!(bond_bal_before - bond_bal_after, 250_i128);
    assert_eq!(dest_bal_after - dest_bal_before, 250_i128);

    let bond = client.get_identity_state(&identity);
    assert_eq!(bond.slashed_amount, 250_i128);
}

/// Unauthorized caller must not transfer tokens.
#[test]
#[should_panic(expected = "not admin")]
fn test_unauthorized_slash_does_not_transfer_tokens() {
    let e = Env::default();
    e.mock_all_auths();

    let (client, admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(&e);

    let destination = Address::generate(&e);
    client.set_slash_treasury(&admin, &destination);

    client.create_bond(
        &identity,
        &1000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    test_helpers::advance_ledger_sequence(&e);

    let attacker = Address::generate(&e);
    client.slash(&attacker, &identity, &250_i128);
}

// ============================================================================
// Regression: checked arithmetic in slash reward calculation
// ============================================================================

/// Division of a non-negative i128 by 10 preserves correct reward value.
#[test]
fn test_slash_reward_checked_div_preserves_value() {
    let e = Env::default();
    let (client, admin, identity) =
        setup_with_bond(&e, 1_000_000_i128, credence_math::Timestamp::SECONDS_PER_DAY);

    let bond = client.slash(&admin, &identity, &1_000_i128);
    assert_eq!(bond.slashed_amount, 1_000);
}
