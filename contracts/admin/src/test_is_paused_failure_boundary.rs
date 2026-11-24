//! Deterministic failure-boundary coverage for `is_paused` (issue #1406).
//!
//! # What this module proves
//!
//! `is_paused` is a pure storage read with no auth requirement.  Its
//! failure-boundary surface is therefore about *what state it reports* under
//! adversarial and edge-case conditions rather than about access control.
//! Every test below targets one specific invariant drawn from the acceptance
//! criteria:
//!
//! 1. **Default / uninitialized** — returns `false` when the storage key has
//!    never been written.
//! 2. **Post-initialize** — returns `false` after `initialize` (which writes
//!    `Paused = false`).
//! 3. **Deterministic after direct pause** — returns `true` immediately after
//!    `pause` with no intervening reads that could alter the result.
//! 4. **Deterministic after direct unpause** — returns `false` immediately
//!    after `unpause`.
//! 5. **Idempotent pause** — double-pause keeps state `true`; no panic, no
//!    state corruption.
//! 6. **Idempotent unpause** — double-unpause keeps state `false`.
//! 7. **Read-only under pause** — calling `is_paused` while paused never
//!    mutates state; it is safe to call it any number of times.
//! 8. **Epoch isolation** — `is_paused` never advances the config epoch (it
//!    must not be classified as a mutation).
//! 9. **Multisig path: false until executed** — under a threshold > 0 the
//!    pause state stays `false` through propose + approve; it only flips on
//!    `execute_pause_proposal`.
//! 10. **Multisig path: stale-proposal rejected** — an already-consumed
//!     proposal cannot flip the state a second time.
//! 11. **Insufficient approvals: execute rejected, state unchanged** — calling
//!     `execute_pause_proposal` before reaching threshold panics and leaves
//!     `is_paused` unchanged.
//! 12. **Non-signer cannot propose** — a caller that is not a registered pause
//!     signer is rejected; `is_paused` stays `false`.
//! 13. **State preserved across unrelated writes** — `is_paused` does not
//!     flip after admin roster changes.
//! 14. **Pause/unpause cycle is fully reversible** — multiple complete
//!     pause → unpause cycles each restore `false`.
//! 15. **Stale-epoch detection** — the config epoch advances exactly once on
//!     each committed toggle; clients can detect a stale snapshot.
//! 16. **Concurrent observability** — epoch snapshot taken before and after a
//!     pause detects the conflict without requiring a second `is_paused` call.
//! 17. **Permission: non-admin cannot directly pause** — a plain address that
//!     is not a SuperAdmin is rejected; `is_paused` stays `false`.
//! 18. **Permission: operator cannot directly pause** — an Operator-role admin
//!     is rejected; `is_paused` stays `false`.
//! 19. **Permission: admin-role cannot directly pause** — an Admin-role admin
//!     (below SuperAdmin) is rejected; `is_paused` stays `false`.
//! 20. **Retry safety** — a failed `pause` call (wrong role) can be retried
//!     with the correct caller without leaving partial state.
//! 21. **Threshold-0 path bypasses proposal** — with threshold 0 the
//!     direct-pause path is taken and `pause` returns `None`.
//! 22. **Suspended super-admin cannot pause** — a suspended SuperAdmin call
//!     to pause is rejected; state stays `false`.
//! 23. **Deactivated super-admin cannot pause** — a deactivated SuperAdmin
//!     call to pause is rejected.
//! 24. **Partial-write atomicity** — an invocation that panics mid-flight
//!     rolls back completely; `is_paused` reflects the pre-call state.

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Minimal single-super-admin setup via the public contract client.
fn setup() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, super_admin)
}

// ---------------------------------------------------------------------------
// 1. Default / uninitialized state
// ---------------------------------------------------------------------------

/// `is_paused` must return `false` on a freshly registered contract that has
/// never been initialized.  The `Paused` storage key does not exist; the
/// function defaults to `false` via `unwrap_or(false)`.
#[test]
fn is_paused_returns_false_when_storage_key_absent() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    // No `initialize` call — `Paused` key has never been written.
    assert!(!client.is_paused(), "uninitialized contract must report not-paused");
}

// ---------------------------------------------------------------------------
// 2. Post-initialize default
// ---------------------------------------------------------------------------

/// After `initialize` the contract explicitly writes `Paused = false`.
/// `is_paused` must surface that value.
#[test]
fn is_paused_false_after_initialize() {
    let (_e, client, _super_admin) = setup();
    assert!(!client.is_paused(), "freshly initialized contract must report not-paused");
}

// ---------------------------------------------------------------------------
// 3. Deterministic after direct pause
// ---------------------------------------------------------------------------

/// After a single `pause` call by a SuperAdmin (threshold 0), `is_paused`
/// must return `true` on the very next read.
#[test]
fn is_paused_true_immediately_after_direct_pause() {
    let (_e, client, super_admin) = setup();
    client.pause(&super_admin);
    assert!(client.is_paused(), "is_paused must be true immediately after pause");
}

// ---------------------------------------------------------------------------
// 4. Deterministic after direct unpause
// ---------------------------------------------------------------------------

/// After pause → unpause, `is_paused` must return `false` deterministically.
#[test]
fn is_paused_false_immediately_after_direct_unpause() {
    let (_e, client, super_admin) = setup();
    client.pause(&super_admin);
    assert!(client.is_paused());
    client.unpause(&super_admin);
    assert!(!client.is_paused(), "is_paused must be false immediately after unpause");
}

// ---------------------------------------------------------------------------
// 5. Idempotent pause
// ---------------------------------------------------------------------------

/// A second `pause` on an already-paused contract must be a no-op: state
/// stays `true` and no panic occurs.
#[test]
fn is_paused_idempotent_double_pause() {
    let (_e, client, super_admin) = setup();
    client.pause(&super_admin);
    client.pause(&super_admin); // idempotent — must not panic
    assert!(client.is_paused(), "double-pause must leave state as true");
}

// ---------------------------------------------------------------------------
// 6. Idempotent unpause
// ---------------------------------------------------------------------------

/// A second `unpause` on an already-unpaused contract must be a no-op: state
/// stays `false` and no panic occurs.
#[test]
fn is_paused_idempotent_double_unpause() {
    let (_e, client, super_admin) = setup();
    client.unpause(&super_admin); // idempotent — must not panic
    client.unpause(&super_admin);
    assert!(!client.is_paused(), "double-unpause must leave state as false");
}

// ---------------------------------------------------------------------------
// 7. Read-only: is_paused does not mutate state
// ---------------------------------------------------------------------------

/// Calling `is_paused` many times while the contract is paused must never
/// flip the state back.  Reads are side-effect-free.
#[test]
fn is_paused_is_read_only_and_does_not_alter_state() {
    let (_e, client, super_admin) = setup();
    client.pause(&super_admin);

    for _ in 0..10 {
        assert!(client.is_paused(), "repeated reads must all return true");
    }

    // State must still be true — reads must not have altered it.
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// 8. Epoch isolation: is_paused must not advance the config epoch
// ---------------------------------------------------------------------------

/// `is_paused` is a pure read; it must not bump the monotonic config epoch.
/// Callers use the epoch for conflict detection; a spurious bump would
/// incorrectly invalidate their snapshots.
#[test]
fn is_paused_does_not_advance_config_epoch() {
    let (_e, client, super_admin) = setup();

    let epoch_before = client.get_config_epoch();
    let _ = client.is_paused();
    let _ = client.is_paused();
    let _ = client.is_paused();
    let epoch_after = client.get_config_epoch();

    assert_eq!(
        epoch_before, epoch_after,
        "is_paused must not advance the config epoch"
    );

    // Contrast: an actual mutation *does* advance the epoch.
    client.pause(&super_admin);
    assert!(client.get_config_epoch() > epoch_after, "pause must advance epoch");

    // Reading again after pause still does not bump further.
    let epoch_mid = client.get_config_epoch();
    let _ = client.is_paused();
    assert_eq!(client.get_config_epoch(), epoch_mid);
}

// ---------------------------------------------------------------------------
// 9. Multisig: false until execute_pause_proposal
// ---------------------------------------------------------------------------

/// With threshold = 2 the pause state must stay `false` through
/// `pause` (propose + first approval) and `approve_pause_proposal`
/// (second approval).  Only `execute_pause_proposal` commits the flip.
#[test]
fn is_paused_false_until_execute_pause_proposal() {
    let (e, client, super_admin) = setup();

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin, &s2, &true);
    client.set_pause_threshold(&super_admin, &2u32);

    // Propose + first approval
    let pid = client.pause(&s1).unwrap();
    assert!(!client.is_paused(), "state must still be false after proposal");

    // Second approval — still not executed
    client.approve_pause_proposal(&s2, &pid);
    assert!(!client.is_paused(), "state must still be false after all approvals but before execute");

    // Now execute — state flips
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused(), "state must be true after execute_pause_proposal");
}

// ---------------------------------------------------------------------------
// 10. Multisig: consumed proposal cannot flip state a second time
// ---------------------------------------------------------------------------

/// After `execute_pause_proposal` the proposal is removed.  A second call
/// with the same `proposal_id` must panic and `is_paused` must stay `true`.
#[test]
fn is_paused_unchanged_after_stale_proposal_execution_attempt() {
    let (e, client, super_admin) = setup();

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin, &s2, &true);
    client.set_pause_threshold(&super_admin, &2u32);

    let pid = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());

    // Re-executing a consumed proposal must be rejected.
    let result = client.try_execute_pause_proposal(&pid);
    assert!(result.is_err(), "re-executing a consumed proposal must fail");

    // State must remain true — no second flip.
    assert!(client.is_paused(), "is_paused must remain true after stale-execute rejection");
}

// ---------------------------------------------------------------------------
// 11. Insufficient approvals: execute rejected, state unchanged
// ---------------------------------------------------------------------------

/// Calling `execute_pause_proposal` before the threshold is reached must
/// panic and leave `is_paused` unchanged.
#[test]
fn is_paused_unchanged_when_execute_called_before_threshold() {
    let (e, client, super_admin) = setup();

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin, &s2, &true);
    client.set_pause_threshold(&super_admin, &2u32);

    // Only one approval (from proposer)
    let pid = client.pause(&s1).unwrap();

    // Execute before second approval — must fail.
    let result = client.try_execute_pause_proposal(&pid);
    assert!(result.is_err(), "execute before threshold must be rejected");
    assert!(!client.is_paused(), "is_paused must remain false after rejected execute");

    // Completing the flow afterwards must succeed.
    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// 12. Non-signer cannot propose pause
// ---------------------------------------------------------------------------

/// An address that is not a registered pause signer must be rejected when it
/// calls `pause` (multisig path).  `is_paused` must stay `false`.
#[test]
fn is_paused_unchanged_when_non_signer_proposes_pause() {
    let (e, client, super_admin) = setup();

    let s1 = Address::generate(&e);
    let non_signer = Address::generate(&e);

    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_threshold(&super_admin, &1u32);

    // `non_signer` is not registered — must be rejected.
    let result = client.try_pause(&non_signer);
    assert!(result.is_err(), "non-signer must be rejected on pause");
    assert!(!client.is_paused(), "is_paused must stay false after non-signer rejection");
}

// ---------------------------------------------------------------------------
// 13. State preserved across unrelated writes
// ---------------------------------------------------------------------------

/// Adding, updating, or removing admins must not affect the pause state.
#[test]
fn is_paused_unaffected_by_admin_roster_changes() {
    let (e, client, super_admin) = setup();

    // Pause, then mutate the admin roster.
    client.pause(&super_admin);
    assert!(client.is_paused());

    let new_admin = Address::generate(&e);
    // Admin mutations are blocked while paused — unpause first.
    client.unpause(&super_admin);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
    // Re-pause.
    client.pause(&super_admin);
    assert!(client.is_paused(), "is_paused must be true after roster change");

    // Unpause and roster change must not silently re-pause.
    client.unpause(&super_admin);
    client.remove_admin(&super_admin, &new_admin);
    assert!(!client.is_paused(), "is_paused must be false after roster change while unpaused");
}

// ---------------------------------------------------------------------------
// 14. Full pause/unpause cycle is reversible
// ---------------------------------------------------------------------------

/// Running N complete pause → unpause cycles must always leave the contract
/// in the `false` state, proving the toggle is fully reversible.
#[test]
fn is_paused_fully_reversible_across_multiple_cycles() {
    let (_e, client, super_admin) = setup();

    for cycle in 0..5_u32 {
        client.pause(&super_admin);
        assert!(client.is_paused(), "cycle {cycle}: expected paused after pause");
        client.unpause(&super_admin);
        assert!(!client.is_paused(), "cycle {cycle}: expected unpaused after unpause");
    }
}

// ---------------------------------------------------------------------------
// 15. Stale-epoch detection after pause toggle
// ---------------------------------------------------------------------------

/// The config epoch must advance exactly once per committed toggle.  A client
/// that snapshots the epoch before a pause can detect the stale snapshot by
/// comparing epoch values.
#[test]
fn is_paused_stale_epoch_detected_after_pause() {
    let (_e, client, super_admin) = setup();

    let epoch_before = client.get_config_epoch();
    assert!(!client.is_paused());

    client.pause(&super_admin);
    let epoch_after_pause = client.get_config_epoch();

    assert_eq!(
        epoch_after_pause,
        epoch_before + 1,
        "pause must advance epoch by exactly 1"
    );

    // Unpause advances by 1 more.
    client.unpause(&super_admin);
    assert_eq!(
        client.get_config_epoch(),
        epoch_before + 2,
        "unpause must advance epoch by exactly 1"
    );

    // is_paused reads do not further advance epoch.
    let _ = client.is_paused();
    assert_eq!(client.get_config_epoch(), epoch_before + 2);
}

// ---------------------------------------------------------------------------
// 16. Concurrent observability via epoch snapshot
// ---------------------------------------------------------------------------

/// A client that snapshots the epoch can detect that a concurrent pause
/// happened without ever calling `is_paused` again — the epoch divergence is
/// the signal.  This test encodes that contract explicitly.
#[test]
fn is_paused_concurrent_toggle_detectable_via_epoch() {
    let (_e, client, super_admin) = setup();

    // Client A takes a snapshot.
    let snapshot_epoch = client.get_config_epoch();
    let snapshot_paused = client.is_paused();
    assert!(!snapshot_paused);

    // "Concurrent" mutation: Client B pauses the contract.
    client.pause(&super_admin);

    // Client A detects the stale snapshot through epoch divergence.
    let current_epoch = client.get_config_epoch();
    assert_ne!(
        current_epoch, snapshot_epoch,
        "epoch divergence signals a concurrent mutation"
    );

    // Re-reading confirms the actual state.
    assert!(client.is_paused(), "re-reading after epoch divergence must show true");
}

// ---------------------------------------------------------------------------
// 17. Permission: non-admin address cannot directly pause (threshold 0)
// ---------------------------------------------------------------------------

/// With threshold = 0 the `pause` path requires SuperAdmin via
/// `require_admin_auth`.  A random address must be rejected and `is_paused`
/// must remain `false`.
#[test]
fn is_paused_unchanged_when_non_admin_attempts_direct_pause() {
    let (e, client, _super_admin) = setup();

    let random = Address::generate(&e);
    let result = client.try_pause(&random);
    assert!(result.is_err(), "non-admin must be rejected on direct pause");
    assert!(!client.is_paused(), "is_paused must stay false after non-admin rejection");
}

// ---------------------------------------------------------------------------
// 18. Permission: Operator role cannot directly pause
// ---------------------------------------------------------------------------

/// An Operator-role admin does not satisfy `require_admin_auth` (requires
/// SuperAdmin).  Pause must be rejected.
#[test]
fn is_paused_unchanged_when_operator_attempts_direct_pause() {
    let (e, client, super_admin) = setup();

    let operator = Address::generate(&e);
    client.add_admin(&super_admin, &operator, &AdminRole::Operator);

    let result = client.try_pause(&operator);
    assert!(result.is_err(), "Operator must be rejected on direct pause");
    assert!(!client.is_paused(), "is_paused must stay false after Operator rejection");
}

// ---------------------------------------------------------------------------
// 19. Permission: Admin role cannot directly pause
// ---------------------------------------------------------------------------

/// An Admin-role address (below SuperAdmin) must also be rejected on the
/// direct-pause path.
#[test]
fn is_paused_unchanged_when_admin_role_attempts_direct_pause() {
    let (e, client, super_admin) = setup();

    let admin_addr = Address::generate(&e);
    client.add_admin(&super_admin, &admin_addr, &AdminRole::Admin);

    let result = client.try_pause(&admin_addr);
    assert!(result.is_err(), "Admin-role must be rejected on direct pause");
    assert!(!client.is_paused(), "is_paused must stay false after Admin-role rejection");
}

// ---------------------------------------------------------------------------
// 20. Retry safety: failed pause can be retried with correct caller
// ---------------------------------------------------------------------------

/// A failed `pause` call (wrong role) must leave no partial state, so a
/// subsequent call with the correct SuperAdmin caller succeeds normally.
#[test]
fn is_paused_retry_succeeds_after_initial_rejection() {
    let (e, client, super_admin) = setup();

    let wrong_caller = Address::generate(&e);

    // First attempt fails.
    assert!(client.try_pause(&wrong_caller).is_err());
    assert!(!client.is_paused());

    // Retry with the correct caller must succeed without any leftover state.
    client.pause(&super_admin);
    assert!(client.is_paused(), "retry with correct caller must succeed");
}

// ---------------------------------------------------------------------------
// 21. Threshold-0 path bypasses proposal and returns None
// ---------------------------------------------------------------------------

/// When `PauseThreshold` is 0, `pause` takes the direct path, writes
/// `Paused = true` immediately, and returns `None` (no proposal ID).
#[test]
fn is_paused_threshold_zero_uses_direct_pause_path() {
    let (_e, client, super_admin) = setup();
    // Default threshold is 0 after initialize.
    let result = client.pause(&super_admin);
    assert_eq!(result, None, "threshold-0 must return None (no proposal)");
    assert!(client.is_paused(), "direct pause must flip state immediately");
}

// ---------------------------------------------------------------------------
// 22. Suspended SuperAdmin cannot directly pause
// ---------------------------------------------------------------------------

/// A SuperAdmin whose suspension has not yet expired is treated as inactive
/// by `require_admin_auth` and must be rejected.  `is_paused` must stay
/// `false`.
#[test]
fn is_paused_unchanged_when_suspended_super_admin_attempts_pause() {
    let (e, client, super_admin) = setup();

    // Add a second SuperAdmin so we remain above MinAdmins after suspension.
    let super_admin2 = Address::generate(&e);
    client.add_admin(&super_admin, &super_admin2, &AdminRole::SuperAdmin);

    let now = e.ledger().timestamp();
    let until = now + 3_600; // 1 hour from now
    client.suspend_admin(&super_admin2, &super_admin, &until);

    // Suspended SuperAdmin must be rejected.
    let result = client.try_pause(&super_admin);
    assert!(result.is_err(), "suspended SuperAdmin must be rejected on pause");
    assert!(!client.is_paused(), "is_paused must stay false after suspended-admin rejection");

    // After suspension expires the same admin can pause successfully.
    e.ledger().with_mut(|l| l.timestamp = until + 1);
    client.pause(&super_admin);
    assert!(client.is_paused(), "is_paused must be true after suspension expired and pause called");
}

// ---------------------------------------------------------------------------
// 23. Deactivated SuperAdmin cannot directly pause
// ---------------------------------------------------------------------------

/// A deactivated admin must be rejected even if they hold the SuperAdmin role.
#[test]
fn is_paused_unchanged_when_deactivated_super_admin_attempts_pause() {
    let (e, client, super_admin) = setup();

    // Add a second SuperAdmin so we can deactivate the first.
    let super_admin2 = Address::generate(&e);
    client.add_admin(&super_admin, &super_admin2, &AdminRole::SuperAdmin);
    client.deactivate_admin(&super_admin2, &super_admin);

    // Deactivated SuperAdmin must be rejected.
    let result = client.try_pause(&super_admin);
    assert!(result.is_err(), "deactivated SuperAdmin must be rejected on pause");
    assert!(!client.is_paused(), "is_paused must stay false after deactivated-admin rejection");

    // The still-active super_admin2 can pause without issue.
    client.pause(&super_admin2);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// 24. Partial-write atomicity: panicking invocation rolls back completely
// ---------------------------------------------------------------------------

/// Any invocation that panics (e.g. `execute_pause_proposal` before threshold)
/// must roll back entirely.  `is_paused` must reflect the pre-invocation state
/// after the panic, not a partial intermediate write.
#[test]
fn is_paused_reflects_pre_call_state_after_panicking_execute() {
    let (e, client, super_admin) = setup();

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin, &s2, &true);
    client.set_pause_threshold(&super_admin, &2u32);

    let pid = client.pause(&s1).unwrap();

    // Record state before the panicking call.
    let paused_before = client.is_paused();
    let epoch_before = client.get_config_epoch();
    assert!(!paused_before);

    // Execute before threshold — must panic and roll back.
    let _ = client.try_execute_pause_proposal(&pid);

    // State and epoch must be exactly as before the failed call.
    assert_eq!(
        client.is_paused(),
        paused_before,
        "is_paused must equal pre-call state after atomic rollback"
    );
    assert_eq!(
        client.get_config_epoch(),
        epoch_before,
        "config epoch must not advance after rolled-back call"
    );
}
