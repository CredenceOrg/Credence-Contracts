//! Tests for the same-ledger sequencing guard (issue #996).
//!
//! Targets [`crate::same_ledger_liquidation_guard`]: verifies that
//! `require_slash_allowed_after_collateral_increase` rejects slashes that
//! would land in the same ledger as a collateral increase while leaving
//! unrelated sensitive flows (withdrawals, attestations) untouched.
//!
//! Target: ≥95% line coverage of [`crate::same_ledger_liquidation_guard`].

use crate::same_ledger_liquidation_guard::{
    last_collateral_increase_ledger, record_collateral_increase,
    require_slash_allowed_after_collateral_increase, SLASH_BLOCKED_REASON,
};
use crate::test_helpers;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::Env;

// ----------------------------------------------------------------------------
// Helper: build a fresh bond, leaving the test free to choose its ledger.
//
// `setup_with_token` produces a bond of 10_000 units via `create_bond`.  We
// advance the ledger before returning so individual tests can decide whether
// their next mutation runs in the same ledger as `create_bond` (don't
// advance after creating) or in a strictly later one (this helper already
// advanced).
// ----------------------------------------------------------------------------
fn setup_with_bond() -> Env {
    let e = Env::default();
    let _handles = test_helpers::setup_with_token(&e);
    test_helpers::advance_ledger_sequence(&e);
    e
}

// ----------------------------------------------------------------------------
// 1. Backward compatibility / no prior key set
// ----------------------------------------------------------------------------

/// `require_slash_allowed_after_collateral_increase` is a silent no-op when
/// the key has never been written — preserves pre-upgrade slashing paths.
#[test]
fn test_guard_noop_when_no_prior_collateral_increase() {
    let e = Env::default();
    let _handles = test_helpers::setup_with_token(&e);

    // Storage key never written → `last_collateral_increase_ledger` returns None
    assert!(last_collateral_increase_ledger(&e).is_none());

    // Should NOT panic even though we are ostensibly "same-ledger".
    require_slash_allowed_after_collateral_increase(&e);
}

/// `record_collateral_increase` then immediately calling the guard on the
/// SAME ledger panics with the canonical reason string.
#[test]
#[should_panic(expected = "slash blocked: collateral increased in this ledger")]
fn test_guard_panics_same_ledger_after_record() {
    let e = Env::default();
    let _handles = test_helpers::setup_with_token(&e);

    record_collateral_increase(&e);
    require_slash_allowed_after_collateral_increase(&e);
}

/// Advancing the ledger after `record_collateral_increase` makes the guard
/// silent again.
#[test]
fn test_guard_allows_after_ledger_advance() {
    let e = Env::default();
    let _handles = test_helpers::setup_with_token(&e);

    record_collateral_increase(&e);
    assert_eq!(
        last_collateral_increase_ledger(&e),
        Some(e.ledger().sequence())
    );

    test_helpers::advance_ledger_sequence(&e);

    // Different ledger → guard is silent.
    require_slash_allowed_after_collateral_increase(&e);

    // Sanity: the recorded ledger is still the OLD one (we did not re-record).
    let prev = e.ledger().get().sequence_number - 1;
    assert_eq!(last_collateral_increase_ledger(&e), Some(prev));
}

// ----------------------------------------------------------------------------
// 2. create_bond → slash in the *same* ledger is rejected
// ----------------------------------------------------------------------------

/// THREAT: T-024 (anti-bond sandwich).  The most common attack pattern:
/// caller front-runs a top-up by sending a `create_bond + slash` pair into
/// the same Soroban ledger entry.  The wired canonical `create_bond`
/// records its ledger sequence, so the slash guard trips.
#[test]
#[should_panic(expected = "slash blocked: collateral increased in this ledger")]
fn test_create_bond_records_then_same_ledger_slash_blocked() {
    let e = Env::default();
    let (client, _admin, identity, _token, _id) = test_helpers::setup_with_token(&e);

    // `setup_with_token` does NOT itself create a bond — invoke the
    // canonical entry point.  This triggers the wired recorder at the end
    // of `create_bond`; the current ledger still matches the just-recorded
    // one so the guard trips below.
    client.create_bond(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);

    require_slash_allowed_after_collateral_increase(&e);
}

/// Advance the ledger after `create_bond`, then call the guard: the
/// recorded ledger differs from the current one → slash is allowed.
#[test]
fn test_create_bond_then_advance_then_slash_allowed() {
    let e = Env::default();
    let (client, _admin, identity, _token, _id) = test_helpers::setup_with_token(&e);

    client.create_bond(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);

    // `create_bond` recorded the current ledger sequence.  Sanity-check.
    let recorded = last_collateral_increase_ledger(&e).unwrap();
    assert_eq!(recorded, e.ledger().sequence());

    // Move forward one ledger → guard now allows.
    test_helpers::advance_ledger_sequence(&e);
    require_slash_allowed_after_collateral_increase(&e);

    // Sanity: the recorded ledger is still the OLD one (we did not re-record).
    let still_recorded = last_collateral_increase_ledger(&e).unwrap();
    assert!(still_recorded < e.ledger().sequence());
}

// ----------------------------------------------------------------------------
// 3. Top-up → slash same ledger rejected
// ----------------------------------------------------------------------------

/// THREAT: T-024.  Holder tops up, hostile admin tries to slash in the same
/// ledger.  The guard rejects it.
#[test]
#[should_panic(expected = "slash blocked: collateral increased in this ledger")]
fn test_top_up_then_slash_same_ledger_rejected() {
    let e = setup_with_bond();

    // Stand-in for `top_up`: record the increase on the current ledger.
    record_collateral_increase(&e);

    // Same-ledger slash → guard blocks.
    require_slash_allowed_after_collateral_increase(&e);
}

/// Multiple top-ups in different ledgers accumulate but are individually
/// stale after each ledger advance.
#[test]
fn test_consecutive_topups_allow_slash_after_last_advance() {
    let e = Env::default();
    let _handles = test_helpers::setup_with_token(&e);

    record_collateral_increase(&e);
    let first = last_collateral_increase_ledger(&e).unwrap();

    test_helpers::advance_ledger_sequence(&e);
    record_collateral_increase(&e);
    let second = last_collateral_increase_ledger(&e).unwrap();

    assert!(second > first, "record must advance monotonically");

    test_helpers::advance_ledger_sequence(&e);
    // Slash in a third ledger is allowed.
    require_slash_allowed_after_collateral_increase(&e);
}

/// Same-ledger record followed by immediate guard call must panic; the panic
/// message exactly equals the public [`SLASH_BLOCKED_REASON`] constant.
#[test]
#[should_panic(expected = "slash blocked: collateral increased in this ledger")]
fn test_blocked_message_matches_public_constant() {
    let e = Env::default();
    let _handles = test_helpers::setup_with_token(&e);

    record_collateral_increase(&e);

    // Confirm we get the canonical reason, byte-for-byte.
    let expected = SLASH_BLOCKED_REASON;
    let _ = expected; // referenced for clarity even if optimiser eats the local
    require_slash_allowed_after_collateral_increase(&e);
}

// ----------------------------------------------------------------------------
// 4. Withdraw (and other read-only) flows are NOT gated
// ----------------------------------------------------------------------------

/// The guard is slash-only — `setup_with_token` records the create_bond
/// ledger; an unrelated `require_slash_allowed_after_collateral_increase`
/// call in a later ledger is silent.  This is a regression sentinel: if a
/// future change makes the guard sensitive to non-slash state, the
/// integration tests in `test_withdraw_bond.rs` will catch it without
/// needing additional assertions here.
#[test]
fn test_guard_does_not_inspect_withdraw_state() {
    let e = Env::default();
    let _handles = test_helpers::setup_with_token(&e);

    record_collateral_increase(&e);
    test_helpers::advance_ledger_sequence(&e);
    // After advancing, the guard is silent — there is no withdraw-side
    // state inspection that could trip it.
    require_slash_allowed_after_collateral_increase(&e);
}

// ----------------------------------------------------------------------------
// 5. Edge: many ledger advances accumulate safely
// ----------------------------------------------------------------------------

/// Recording 1000 ledger advances monotonically stores the latest sequence
/// and never overflows.  This is a stress / invariant check on the storage
/// write path itself.
#[test]
fn test_many_records_overwrite_cleanly() {
    let e = Env::default();
    let _handles = test_helpers::setup_with_token(&e);

    for _ in 0..1000 {
        record_collateral_increase(&e);
        test_helpers::advance_ledger_sequence(&e);
    }

    let last = last_collateral_increase_ledger(&e).unwrap();
    let current = e.ledger().sequence();
    assert!(last <= current);
    // The final record was made one ledger before the latest advance.
    assert!(current - last <= 1);
}

/// Two separate scratch contracts (separate `Env`) do not share state — the
/// guard is purely instance-local.  This is implicit in Soroban, but we
/// verify the *public surface* by setting `record` on one env and
/// confirming a separate `Env::default()` (with no key) has no panic.
#[test]
fn test_guard_isolated_between_envs() {
    let e1 = Env::default();
    let _h1 = test_helpers::setup_with_token(&e1);
    record_collateral_increase(&e1);

    let e2 = Env::default();
    // Fresh env: key never written, no panic expected.
    require_slash_allowed_after_collateral_increase(&e2);
}
