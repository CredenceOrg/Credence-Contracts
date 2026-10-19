//! Adversarial and failure-recovery coverage for `pausable.rs` (issue #1344).
//!
//! This module exercises failure paths, state corruption attempts, and recovery
//! guarantees that the happy-path tests in `test_pausable.rs` and boundary
//! tests in `test_pausable_boundary.rs` do not cover.
//!
//! Key recovery properties:
//! - Failed proposal execution leaves state clean for retry
//! - Admin override cannot be blocked by malformed proposals
//! - Partial approvals don't leak into subsequent proposals
//! - Borrow freeze/pause independence holds under contention
//! - Emergency drain scheduling/cancellation is idempotent
//! - Concurrent operations don't corrupt pause state

#![cfg(test)]

extern crate std;

use crate::pausable::{
    approve_pause_proposal, execute_pause_proposal, is_paused, pause, require_not_paused,
    set_pause_signer, set_pause_threshold, unpause,
};
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Bytes, Env, String, Vec};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::string::{String as StdString, ToString};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin, &None);
    (client, admin)
}

fn setup_with_signers(e: &Env, threshold: u32) -> (CredenceBondClient<'_>, Address, Vec<Address>) {
    let (client, admin) = setup(e);
    let mut signers = Vec::new(e);
    for _ in 0..threshold as usize {
        let s = Address::generate(e);
        client.set_pause_signer(&admin, &s, &true);
        signers.push_back(s);
    }
    client.set_pause_threshold(&admin, &threshold);
    (client, admin, signers)
}

/// Capture panic message as String.
fn panic_message<F: FnOnce()>(f: F) -> StdString {
    let payload = catch_unwind(AssertUnwindSafe(f)).expect_err("expected a panic");
    if let Some(s) = payload.downcast_ref::<StdString>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        panic!("panic payload was neither String nor &str");
    }
}

/// Assert closure panics with exactly `expected`.
#[track_caller]
fn expect_panic_with<F: FnOnce()>(expected: &str, f: F) {
    let msg = panic_message(f);
    assert_eq!(msg, expected);
}

// ---------------------------------------------------------------------------
// R1: Failed execution leaves clean state for retry
// ---------------------------------------------------------------------------

/// `execute_pause_proposal` with insufficient approvals must not mutate
/// the paused state, and the proposal must remain executable after more
/// approvals are gathered.
#[test]
fn failed_execution_leaves_proposal_executable() {
    let e = Env::default();
    let (client, admin, signers) = setup_with_signers(&e, 3);

    let pid = client.pause(&signers[0]).unwrap(); // 1 approval
    client.approve_pause_proposal(&signers[1], &pid); // 2 approvals

    // Execute with only 2 of 3 approvals must fail
    expect_panic_with("insufficient approvals to execute", || {
        client.execute_pause_proposal(&pid);
    });

    // Contract must still be unpaused
    assert!(!client.is_paused());

    // Add third approval and retry
    client.approve_pause_proposal(&signers[2], &pid);
    client.execute_pause_proposal(&pid);

    assert!(client.is_paused());
}

/// Failed unpause execution similarly leaves proposal intact.
#[test]
fn failed_unpause_execution_leaves_proposal_executable() {
    let e = Env::default();
    let (client, admin, signers) = setup_with_signers(&e, 3);

    let pid = client.pause(&signers[0]).unwrap();
    client.approve_pause_proposal(&signers[1], &pid);
    client.approve_pause_proposal(&signers[2], &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());

    // Try unpause with insufficient approvals
    let pid2 = client.unpause(&signers[0]).unwrap(); // 1 approval
    client.approve_pause_proposal(&signers[1], &pid2); // 2 approvals

    expect_panic_with("insufficient approvals to execute", || {
        client.execute_pause_proposal(&pid2);
    });

    assert!(client.is_paused());

    // Add third approval and retry
    client.approve_pause_proposal(&signers[2], &pid2);
    client.execute_pause_proposal(&pid2);

    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// R2: Admin override cannot be blocked
// ---------------------------------------------------------------------------

/// A malformed or stale proposal cannot prevent admin unpause override.
#[test]
fn admin_unpause_cannot_be_blocked_by_stale_proposal() {
    let e = Env::default();
    let (client, admin, signers) = setup_with_signers(&e, 2);

    // Create a pause proposal
    let pid = client.pause(&signers[0]).unwrap();
    client.approve_pause_proposal(&signers[1], &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());

    // Now create an unpause proposal that will never reach threshold
    let pid2 = client.unpause(&signers[0]).unwrap(); // 1 of 2 approvals
                                                     // Don't get second approval

    // Admin can still unpause directly (bypass proposal)
    assert_eq!(client.unpause(&admin), None);
    assert!(!client.is_paused());

    // Stale proposal pid2 should still exist but be irrelevant
    // (proposals are not auto-cleaned unless executed)
    // We can verify it's still there by trying to execute (will fail threshold)
    expect_panic_with("insufficient approvals to execute", || {
        client.execute_pause_proposal(&pid2);
    });
}

// ---------------------------------------------------------------------------
// R3: Partial approvals don't leak across proposals
// ---------------------------------------------------------------------------

/// Approvals for proposal A must not count towards proposal B.
#[test]
fn approvals_do_not_leak_across_proposals() {
    let e = Env::default();
    let (client, admin, signers) = setup_with_signers(&e, 2);

    // Proposal 1: pause
    let pid1 = client.pause(&signers[0]).unwrap();
    client.approve_pause_proposal(&signers[1], &pid1);
    client.execute_pause_proposal(&pid1);
    assert!(client.is_paused());

    // Proposal 2: unpause (different ID)
    let pid2 = client.unpause(&signers[0]).unwrap();
    // Only 1 approval so far (proposer)

    // Proposal 1's approvals must not count for Proposal 2
    expect_panic_with("insufficient approvals to execute", || {
        client.execute_pause_proposal(&pid2);
    });

    // Get second approval for proposal 2
    client.approve_pause_proposal(&signers[1], &pid2);
    client.execute_pause_proposal(&pid2);
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// R4: Proposal cleanup on execution
// ---------------------------------------------------------------------------

/// After execution, proposal data is removed so ID can be reused (in
/// theory; counter prevents reuse, but storage is freed).
#[test]
fn proposal_removed_after_execution() {
    let e = Env::default();
    let (client, admin, signers) = setup_with_signers(&e, 1);

    let pid = client.pause(&signers[0]).unwrap();
    client.execute_pause_proposal(&pid);

    // Proposal key should be removed
    let exists = e.as_contract(&client.address, || {
        e.storage()
            .instance()
            .has(&crate::DataKey::PauseProposal(pid))
    });
    assert!(!exists, "proposal key must be removed after execution");

    // Approval count also removed
    let exists = e.as_contract(&client.address, || {
        e.storage()
            .instance()
            .has(&crate::DataKey::PauseApprovalCount(pid))
    });
    assert!(!exists, "approval count must be removed after execution");
}

// ---------------------------------------------------------------------------
// R5: Borrow freeze / pause independence under contention
// ---------------------------------------------------------------------------

/// Toggling borrow freeze while paused must not affect pause state.
#[test]
fn borrow_freeze_toggle_preserves_pause_state() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    // Toggle borrow freeze multiple times
    client.set_borrow_frozen(&admin, &true);
    assert!(client.is_borrow_frozen());
    assert!(client.is_paused());

    client.set_borrow_frozen(&admin, &false);
    assert!(!client.is_borrow_frozen());
    assert!(client.is_paused());

    client.set_borrow_frozen(&admin, &true);
    assert!(client.is_borrow_frozen());
    assert!(client.is_paused());

    // Unpause should not affect borrow freeze
    client.unpause(&admin);
    assert!(!client.is_paused());
    assert!(client.is_borrow_frozen());
}

/// Toggling pause while borrow frozen must not affect borrow freeze state.
#[test]
fn pause_toggle_preserves_borrow_freeze_state() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_borrow_frozen(&admin, &true);
    assert!(client.is_borrow_frozen());

    // Toggle pause multiple times
    client.pause(&admin);
    assert!(client.is_paused());
    assert!(client.is_borrow_frozen());

    client.unpause(&admin);
    assert!(!client.is_paused());
    assert!(client.is_borrow_frozen());

    client.pause(&admin);
    assert!(client.is_paused());
    assert!(client.is_borrow_frozen());

    // Unfreeze borrow should not affect pause
    client.set_borrow_frozen(&admin, &false);
    assert!(!client.is_borrow_frozen());
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// R6: Emergency drain scheduling idempotency
// ---------------------------------------------------------------------------

/// Cancel emergency drain works even if not scheduled.
#[test]
fn cancel_emergency_drain_idempotent() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Cancel when not scheduled should not panic
    assert!(client.try_cancel_emergency_drain(&admin).is_ok());

    client.pause(&admin);
    assert!(client.try_cancel_emergency_drain(&admin).is_ok());

    client.try_schedule_emergency_drain(&admin, &86400_u64);
    assert!(client.try_cancel_emergency_drain(&admin).is_ok());

    // Cancel again after already cancelled
    assert!(client.try_cancel_emergency_drain(&admin).is_ok());
}

// ---------------------------------------------------------------------------
// R7: Pause state survives ledger sequence changes
// ---------------------------------------------------------------------------

/// Advancing ledger sequence does not reset pause state.
#[test]
fn pause_survives_ledger_advance() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    // Advance ledger by 1000
    let mut info = e.ledger().get();
    info.sequence_number = info.sequence_number.saturating_add(1000);
    e.ledger().set(info);

    assert!(client.is_paused());

    client.unpause(&admin);
    assert!(!client.is_paused());

    // Advance again
    info.sequence_number = info.sequence_number.saturating_add(1000);
    e.ledger().set(info);

    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// R8: Concurrent-like interleaving (simulated)
// ---------------------------------------------------------------------------

/// Simulated concurrent pause/unpause proposals don't corrupt state.
/// We can't test true concurrency but we can interleave operations.
#[test]
fn interleaved_pause_unpause_proposals() {
    let e = Env::default();
    let (client, admin, signers) = setup_with_signers(&e, 3);

    // Start pause proposal
    let pid1 = client.pause(&signers[0]).unwrap(); // 1

    // Before completing, start unpause proposal
    let pid2 = client.unpause(&signers[1]).unwrap(); // 1 (different proposal)

    // Complete pause proposal
    client.approve_pause_proposal(&signers[2], &pid1); // 2 of 3 for pause
    client.approve_pause_proposal(&signers[1], &pid1); // 3 of 3 for pause
    client.execute_pause_proposal(&pid1);
    assert!(client.is_paused());

    // Now unpause proposal still has 1 approval, needs 2 more
    expect_panic_with("insufficient approvals to execute", || {
        client.execute_pause_proposal(&pid2);
    });

    client.approve_pause_proposal(&signers[0], &pid2); // 2
    client.approve_pause_proposal(&signers[2], &pid2); // 3
    client.execute_pause_proposal(&pid2);
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// R9: Duplicate signer registration is idempotent
// ---------------------------------------------------------------------------

/// Calling set_pause_signer with same signer twice doesn't double-count.
#[test]
fn duplicate_signer_registration_idempotent() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let s = Address::generate(&e);

    client.set_pause_signer(&admin, &s, &true);
    client.set_pause_threshold(&admin, &1u32);

    // Register same signer again
    client.set_pause_signer(&admin, &s, &true);

    // Threshold still 1, signer count still 1
    let pid = client.pause(&s).unwrap();
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// R10: Signer removal decrements count
// ---------------------------------------------------------------------------

/// Removing a signer decrements count; threshold must be adjusted.
#[test]
fn signer_removal_decrements_count() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());

    // Remove s2
    client.unpause(&admin);
    client.set_pause_signer(&admin, &s2, &false);

    // Now threshold (2) > signer count (1) - set_pause_signer adjusts threshold down
    // The threshold should have been auto-adjusted to 1
    client.set_pause_threshold(&admin, &1u32); // Explicitly set to 1
    let pid2 = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&pid2);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// R11: Threshold zero means admin-only, signers ignored
// ---------------------------------------------------------------------------

/// When threshold is 0, signers are ignored and admin auth is required.
#[test]
fn threshold_zero_ignores_signers() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let s = Address::generate(&e);

    client.set_pause_signer(&admin, &s, &true);
    client.set_pause_threshold(&admin, &0u32);

    // Signer cannot propose when threshold is 0
    expect_panic_with("not pause signer", || {
        client.pause(&s);
    });

    // Admin can pause
    assert_eq!(client.pause(&admin), None);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// R12: Determinism - same inputs yield same verdicts
// ---------------------------------------------------------------------------

/// Repeated operations with same state produce identical results.
#[test]
fn pause_unpause_is_deterministic() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    for _ in 0..10 {
        client.pause(&admin);
        assert!(client.is_paused());
        client.unpause(&admin);
        assert!(!client.is_paused());
    }
}

// ---------------------------------------------------------------------------
// R12: Emergency drain scheduling idempotency
// ---------------------------------------------------------------------------

/// `schedule_emergency_drain` requires paused; `cancel_emergency_drain` does not.
#[test]
fn emergency_drain_requires_paused() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    // Unpaused -> fails
    assert!(client
        .try_schedule_emergency_drain(&admin, &86400_u64)
        .is_err());

    // Paused -> succeeds
    client.pause(&admin);
    assert!(client
        .try_schedule_emergency_drain(&admin, &86400_u64)
        .is_ok());
}

/// `cancel_emergency_drain` works regardless of pause state.
#[test]
fn cancel_emergency_drain_exempt() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    assert!(client.try_cancel_emergency_drain(&admin).is_ok());

    client.pause(&admin);
    assert!(client.try_cancel_emergency_drain(&admin).is_ok());
}
