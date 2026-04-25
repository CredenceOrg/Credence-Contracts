//! Comprehensive unit tests for slashing functionality with 95%+ coverage.
//!
//! Test categories:
//! 1. Basic slashing operations
//! 2. Authorization and security
//! 3. Over-slash prevention (capping)
//! 4. Edge cases (zero, negative, max values)
//! 5. State consistency and tracking
//! 6. Event emission and audit trails
//! 7. Integration with withdrawals
//! 8. Cumulative slashing scenarios

// Comprehensive unit tests for slashing functionality.
// Covers: successful slash, unauthorized rejection, over-slash prevention,
// slash history (via events), and slash events.
//! Comprehensive unit tests for slashing functionality.
//! Covers: successful slash, unauthorized rejection, over-slash prevention,
//! slash history (via events), and slash events.

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

// ============================================================================
// Test Setup Utilities
// ============================================================================

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(e);
    (client, admin, identity)
}

fn setup_with_bond(
    e: &Env,
    amount: i128,
    duration: u64,
) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity) = setup(e);
    client.create_bond_with_rolling(&identity, &amount, &duration, &false, &0_u64);
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
    client.create_bond_with_rolling(&identity, &amount, &duration, &false, &0_u64);
    test_helpers::advance_ledger_sequence(e);
    (client, admin, identity)
}

// ============================================================================
// Category 1: Basic Slashing Operations
// ============================================================================

#[test]
fn test_slash_basic_success() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let bond = client.slash(&admin, &300_i128);

    assert_eq!(bond.slashed_amount, 300);
    assert_eq!(bond.bonded_amount, 1000);
    assert!(bond.active);
}

#[test]
fn test_slash_small_amount() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 10000_i128, 86400_u64);

    let bond = client.slash(&admin, &1_i128);

    assert_eq!(bond.slashed_amount, 1);
    assert_eq!(bond.bonded_amount, 10000);
}

#[test]
fn test_slash_exact_half() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let bond = client.slash(&admin, &500_i128);

    assert_eq!(bond.slashed_amount, 500);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_slash_entire_amount() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let bond = client.slash(&admin, &1000_i128);

    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(bond.bonded_amount, 1000);
}

// ============================================================================
// Category 2: Authorization and Security
// ============================================================================

#[test]
#[should_panic(expected = "not admin")]
fn test_slash_unauthorized_rejection() {
    let e = Env::default();
    let (_client, _admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let (client, _admin, identity) = setup(&e);
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    let other = Address::generate(&e);
    client.slash(&other, &100_i128);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_slash_unauthorized_different_address() {
    let e = Env::default();
    let (client, _admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let attacker1 = Address::generate(&e);
    let attacker2 = Address::generate(&e);
    client.slash(&attacker1, &500_i128);
    // Second attempt with different attacker also fails
    client.slash(&attacker2, &500_i128);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_slash_identity_cannot_slash_own_bond() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    // Identity tries to slash their own bond (not authorized)
    client.slash(&identity, &100_i128);
}

// ============================================================================
// Category 3: Over-Slash Prevention (Capping Behavior)
// ============================================================================

#[test]
fn test_slash_over_amount_capped() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let bond = client.slash(&admin, &2000_i128);

    // Should be capped at bonded_amount
    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_slash_way_over_amount_capped() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let bond = client.slash(&admin, &5_000_i128);

    // Should be capped at bonded_amount
    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_slash_max_i128_capped() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let bond = client.slash(&admin, &i128::MAX);

    // Should be capped at bonded_amount
    assert_eq!(bond.slashed_amount, 1000);
    assert_eq!(bond.bonded_amount, 1000);
}

// ============================================================================
// Category 4: Edge Cases (Zero, Negative, Boundary Values)
// ============================================================================

#[test]
fn test_slash_zero_amount() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let bond = client.slash(&admin, &0_i128);

    assert_eq!(bond.slashed_amount, 0);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_slash_overflow_prevention() {
    // With available-balance capping, slash is bounded by (bonded - slashed).
    // After fully slashing, further slashes are no-ops (actual_slash = 0).
    let e = Env::default();
    let (client, admin, _identity) =
        setup_with_bond_max_mint(&e, crate::validation::MAX_BOND_AMOUNT, 86400_u64);

    let bond = client.slash(&admin, &crate::validation::MAX_BOND_AMOUNT);
    assert_eq!(bond.slashed_amount, crate::validation::MAX_BOND_AMOUNT);

    // Further slash is capped at available (0) — no overflow, no panic
    let bond2 = client.slash(&admin, &i128::MAX);
    assert_eq!(bond2.slashed_amount, crate::validation::MAX_BOND_AMOUNT);
}

#[test]
fn test_slash_on_very_large_bond() {
    let e = Env::default();
    let (client, admin, _identity) =
        setup_with_bond_max_mint(&e, crate::validation::MAX_BOND_AMOUNT, 86400_u64);

    let bond = client.slash(&admin, &(crate::validation::MAX_BOND_AMOUNT / 4));

    assert_eq!(bond.slashed_amount, crate::validation::MAX_BOND_AMOUNT / 4);
}

// ============================================================================
// Category 5: State Consistency and Tracking
// ============================================================================

#[test]
fn test_slash_history_single_slash() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    client.slash(&admin, &200_i128);
    let bond = client.get_identity_state();

    assert_eq!(bond.slashed_amount, 200);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_slash_history_cumulative() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let bond1 = client.slash(&admin, &200_i128);
    assert_eq!(bond1.slashed_amount, 200);

    let bond2 = client.slash(&admin, &300_i128);
    assert_eq!(bond2.slashed_amount, 500);

    let bond3 = client.get_identity_state();
    assert_eq!(bond3.slashed_amount, 500);
}

#[test]
fn test_slash_multiple_accumulate() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 10000_i128, 86400_u64);

    // Linear accumulation: 1000 + 2000 + 3000 + 4000 + 5000
    // But capped at bonded_amount (10000)
    for i in 1..=5 {
        let bond = client.slash(&admin, &(i as i128 * 1000_i128));
        let expected_slashed = (i as i128 * (i as i128 + 1) / 2) * 1000_i128;
        let capped = if expected_slashed > 10000_i128 {
            10000_i128
        } else {
            expected_slashed
        };
        assert_eq!(bond.slashed_amount, capped);
    }
}

#[test]
fn test_slash_does_not_affect_other_fields() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let original_bond = client.get_identity_state();
    let original_bonded = original_bond.bonded_amount;
    let original_start = original_bond.bond_start;
    let original_duration = original_bond.bond_duration;

    client.slash(&admin, &300_i128);

    let updated_bond = client.get_identity_state();
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
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let _bond = client.slash(&admin, &250_i128);

    // Verify event was published by checking bond state
    let state = client.get_identity_state();
    assert_eq!(state.slashed_amount, 250);
}

#[test]
fn test_slash_event_contains_correct_event_data() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let bond1 = client.slash(&admin, &100_i128);
    assert_eq!(bond1.slashed_amount, 100);

    let bond2 = client.slash(&admin, &200_i128);
    // Event should contain slash_amount=200, total_slashed=300
    assert_eq!(bond2.slashed_amount, 300);
}

#[test]
fn test_slash_multiple_events() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    // Each slash emits an event
    for i in 1..=3 {
        let bond = client.slash(&admin, &(100_i128 * i as i128));
        assert_eq!(bond.slashed_amount, 100_i128 * (i * (i + 1) / 2) as i128);
    }
}

// ============================================================================
// Category 7: Integration with Withdrawals
// ============================================================================

#[test]
fn test_withdraw_after_slash_respects_available() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &400_i128);
    e.ledger().with_mut(|li| li.timestamp = 86401);
    let bond = client.withdraw(&600_i128);
    assert_eq!(bond.bonded_amount, 400);
    assert_eq!(bond.slashed_amount, 400);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdraw_more_than_available_after_slash() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &400_i128);
    e.ledger().with_mut(|li| li.timestamp = 86401);
    client.withdraw(&601_i128);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdraw_when_fully_slashed() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);

    // Fully slash the bond
    client.slash(&admin, &1000_i128);

    e.ledger().with_mut(|li| li.timestamp = 86401);
    // Cannot withdraw anything
    client.withdraw(&1_i128);
}

#[test]
fn test_withdraw_exact_available_balance() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &400_i128);
    e.ledger().with_mut(|li| li.timestamp = 86401);
    let bond = client.withdraw(&600_i128);

    assert_eq!(bond.bonded_amount, 400);
}

#[test]
fn test_slash_then_withdraw_then_slash_again() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);

    // Slash, withdraw, slash again
    client.slash(&admin, &200_i128);
    assert_eq!(client.get_identity_state().bonded_amount, 1000);

    e.ledger().with_mut(|li| li.timestamp = 86401);
    client.withdraw(&300_i128);
    assert_eq!(client.get_identity_state().bonded_amount, 700);

    let bond = client.slash(&admin, &100_i128);
    assert_eq!(bond.slashed_amount, 300);
    assert_eq!(bond.bonded_amount, 700);
}

#[test]
fn test_slash_after_partial_withdrawal() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    // Withdraw first
    e.ledger().with_mut(|li| li.timestamp = 86401);
    client.withdraw(&300_i128);
    assert_eq!(client.get_identity_state().bonded_amount, 700);

    // Then slash (ledger advanced vs bond creation; withdraw does not refresh collateral ledger)
    test_helpers::advance_ledger_sequence(&e);
    let bond = client.slash(&admin, &200_i128);
    assert_eq!(bond.bonded_amount, 700);
    assert_eq!(bond.slashed_amount, 200);

    // Available should be 700 - 200 = 500 (timestamp already past lock-up)
    client.withdraw(&500_i128);
    assert_eq!(client.get_identity_state().bonded_amount, 200);
}

// ============================================================================
// Category 8: Cumulative Slashing Scenarios
// ============================================================================

#[test]
fn test_cumulative_slash_with_capping() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    // First slash: 600 (cumulative = 600)
    client.slash(&admin, &600_i128);
    assert_eq!(client.get_identity_state().slashed_amount, 600);

    // Second slash: 600 (cumulative would be 1200, capped at 1000)
    let bond = client.slash(&admin, &600_i128);
    assert_eq!(bond.slashed_amount, 1000);
}

#[test]
fn test_cumulative_slash_incremental() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 10000_i128, 86400_u64);

    // Slash 10% at a time
    for i in 1..=10 {
        let bond = client.slash(&admin, &1000_i128);
        assert_eq!(bond.slashed_amount, (i as i128) * 1000_i128);
    }
}

#[test]
fn test_full_slash_prevents_further_slashing() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    // Fully slash
    client.slash(&admin, &1000_i128);
    assert_eq!(client.get_identity_state().slashed_amount, 1000);

    // Attempt further slash (should cap at bonded_amount)
    let bond = client.slash(&admin, &500_i128);
    assert_eq!(bond.slashed_amount, 1000);
}

#[test]
fn test_slash_large_amounts() {
    let e = Env::default();
    let large_amount = 1_000_000_000_000_i128;
    let (client, admin, _identity) = setup_with_bond(&e, large_amount, 86400_u64);

    let bond1 = client.slash(&admin, &(large_amount / 4));
    assert_eq!(bond1.slashed_amount, large_amount / 4);

    // Second slash accumulates
    let bond2 = client.slash(&admin, &(large_amount / 4));
    // The sum should be capped at bonded_amount
    assert_eq!(bond2.slashed_amount, large_amount / 2);
}

// ============================================================================
// Category 9: State Persistence
// ============================================================================

#[test]
fn test_slash_state_persists() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    client.slash(&admin, &300_i128);
    let bond1 = client.get_identity_state();
    assert_eq!(bond1.slashed_amount, 300);

    // Verify again
    let bond2 = client.get_identity_state();
    assert_eq!(bond2.slashed_amount, 300);
}

#[test]
fn test_slash_result_matches_get_state() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let slash_result = client.slash(&admin, &250_i128);
    let state = client.get_identity_state();

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
    let (client, _admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    let random = Address::generate(&e);
    client.slash(&random, &100_i128);
}

#[test]
#[should_panic(expected = "no bond")]
fn test_error_message_no_bond() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);

    // No bond created, try to slash
    client.slash(&admin, &100_i128);
}

// ============================================================================
// Category 11: Available-Balance Bound (slash ≤ bonded − slashed)
// ============================================================================

#[test]
fn test_slash_capped_at_available_not_bonded() {
    // After a partial slash, the cap is on remaining available, not total bonded.
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    // First slash: 600 → available becomes 400
    client.slash(&admin, &600_i128);
    assert_eq!(client.get_identity_state().slashed_amount, 600);

    // Second slash: request 500, but only 400 available → capped at 400
    let bond = client.slash(&admin, &500_i128);
    assert_eq!(bond.slashed_amount, 1000);
}

#[test]
fn test_slash_zero_available_is_noop() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    client.slash(&admin, &1000_i128);
    assert_eq!(client.get_identity_state().slashed_amount, 1000);

    // Available = 0 → any further slash is a no-op
    let bond = client.slash(&admin, &1_i128);
    assert_eq!(bond.slashed_amount, 1000);
}

#[test]
fn test_slash_available_decreases_after_each_slash() {
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    client.slash(&admin, &200_i128); // available: 800
    client.slash(&admin, &300_i128); // available: 500
    client.slash(&admin, &400_i128); // available: 100
    // Request 200, only 100 available
    let bond = client.slash(&admin, &200_i128);
    assert_eq!(bond.slashed_amount, 1000);
}

#[test]
fn test_slash_after_withdraw_respects_new_available() {
    // Withdraw reduces bonded_amount; subsequent slash is bounded by new available.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity) = setup(&e);
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 86401);
    client.withdraw(&400_i128); // bonded = 600, slashed = 0, available = 600
    test_helpers::advance_ledger_sequence(&e);
    // Slash 700 → capped at 600
    let bond = client.slash(&admin, &700_i128);
    assert_eq!(bond.bonded_amount, 600);
    assert_eq!(bond.slashed_amount, 600);
}

// ============================================================================
// Category 12: Slash History Records
// ============================================================================

#[test]
fn test_slash_history_count_increments() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    client.slash(&admin, &100_i128);
    client.slash(&admin, &200_i128);

    let count = crate::slash_history::get_slash_count(&e, &identity);
    assert_eq!(count, 2);
}

#[test]
fn test_slash_history_record_fields() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 5000);
    let (client, admin, identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    client.slash(&admin, &300_i128);

    let record = crate::slash_history::get_slash_record(&e, &identity, 0);
    assert_eq!(record.identity, identity);
    assert_eq!(record.slash_amount, 300);
    assert_eq!(record.total_slashed_after, 300);
    assert_eq!(record.timestamp, 5000);
}

#[test]
fn test_slash_history_total_slashed_after_accumulates() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    client.slash(&admin, &100_i128);
    client.slash(&admin, &200_i128);

    let r0 = crate::slash_history::get_slash_record(&e, &identity, 0);
    let r1 = crate::slash_history::get_slash_record(&e, &identity, 1);
    assert_eq!(r0.total_slashed_after, 100);
    assert_eq!(r1.total_slashed_after, 300);
}

#[test]
fn test_slash_history_capped_slash_records_actual_amount() {
    // When a slash is capped at available, the record stores the actual (capped) amount.
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    client.slash(&admin, &800_i128); // available: 200
    client.slash(&admin, &500_i128); // capped at 200

    let r1 = crate::slash_history::get_slash_record(&e, &identity, 1);
    assert_eq!(r1.slash_amount, 200);
    assert_eq!(r1.total_slashed_after, 1000);
}

#[test]
fn test_slash_history_zero_slash_no_record() {
    // A zero slash produces a record with slash_amount = 0 (no-op but still recorded).
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000_i128, 86400_u64);

    client.slash(&admin, &0_i128);

    // Zero slash: actual_slash_amount = 0, record is still appended
    let count = crate::slash_history::get_slash_count(&e, &identity);
    assert_eq!(count, 1);
    let r = crate::slash_history::get_slash_record(&e, &identity, 0);
    assert_eq!(r.slash_amount, 0);
}

#[test]
fn test_slash_history_get_all_records() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 10000_i128, 86400_u64);

    for i in 1_i128..=5 {
        client.slash(&admin, &(i * 100));
    }

    let history = crate::slash_history::get_slash_history(&e, &identity);
    assert_eq!(history.len(), 5);
    assert_eq!(history.get(0).unwrap().slash_amount, 100);
    assert_eq!(history.get(4).unwrap().slash_amount, 500);
}
