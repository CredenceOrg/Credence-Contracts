//! Emergency/pause mode invariant tests for the Admin contract.
//!
//! These tests prove that emergency toggles restrict writes as intended
//! and that exits preserve the correct invariants.
//!
//! When the contract is paused:
//! - All writable admin entrypoints must be blocked.
//! - Read-only entrypoints must still function.
//! - The pause state must be correctly toggled back.
//!
//! Invariants exercised here:
//! - Pause is a global write gate: every state-mutating admin entrypoint
//!   must reject with `ContractPaused` while paused, and must succeed again
//!   after unpause without losing prior state.
//! - Pause/unpause themselves are never gated, so an operator can always
//!   recover from an emergency (no unrecoverable stuck state).
//! - Pause toggles are idempotent: repeated pause/unpause calls are no-ops
//!   rather than errors, so retries and duplicate submissions are safe.
//! - Read-only entrypoints remain available while paused so monitoring and
//!   diagnostics keep working during an incident.
//! - Authorization is still enforced while paused: a non-admin cannot
//!   toggle pause or bypass the gate.

use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

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
// Pause state transitions
// ---------------------------------------------------------------------------

#[test]
fn emergency_pause_blocks_writes_allows_reads() {
    let (e, client, super_admin) = setup();

    assert!(!client.is_paused());
    client.pause(&super_admin);
    assert!(client.is_paused());

    // Reads must still work
    assert_eq!(client.get_admin_count(), 1);
    assert_eq!(
        client.version(),
        String::from_str(&e, credence_errors::VERSION)
    );
    assert_eq!(client.get_all_admins().len(), 1);
}

#[test]
fn emergency_unpause_restores_writes() {
    let (e, client, super_admin) = setup();

    client.pause(&super_admin);
    assert!(client.is_paused());

    client.unpause(&super_admin);
    assert!(!client.is_paused());

    // Write must succeed after unpause
    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);
}

// ---------------------------------------------------------------------------
// Pause blocks add_admin
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #106)")] // ContractPaused
fn emergency_pause_blocks_add_admin() {
    let (e, client, super_admin) = setup();
    client.pause(&super_admin);

    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
}

// ---------------------------------------------------------------------------
// Pause blocks remove_admin
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #106)")] // ContractPaused
fn emergency_pause_blocks_remove_admin() {
    let (e, client, super_admin) = setup();

    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);

    client.pause(&super_admin);
    client.remove_admin(&super_admin, &new_admin);
}

// ---------------------------------------------------------------------------
// Pause blocks update_admin_role
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #106)")] // ContractPaused
fn emergency_pause_blocks_update_admin_role() {
    let (e, client, super_admin) = setup();

    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);

    client.pause(&super_admin);
    client.update_admin_role(&super_admin, &new_admin, &AdminRole::Operator);
}

// ---------------------------------------------------------------------------
// Pause blocks deactivate_admin
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #106)")] // ContractPaused
fn emergency_pause_blocks_deactivate_admin() {
    let (e, client, super_admin) = setup();

    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);

    client.pause(&super_admin);
    client.deactivate_admin(&super_admin, &new_admin);
}

// ---------------------------------------------------------------------------
// Pause blocks reactivate_admin
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #106)")] // ContractPaused
fn emergency_pause_blocks_reactivate_admin() {
    let (e, client, super_admin) = setup();

    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
    client.deactivate_admin(&super_admin, &new_admin);

    client.pause(&super_admin);
    client.reactivate_admin(&super_admin, &new_admin);
}

// ---------------------------------------------------------------------------
// Pause blocks transfer_ownership
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #106)")] // ContractPaused
fn emergency_pause_blocks_transfer_ownership() {
    let (e, client, super_admin) = setup();

    // Create a second super admin to transfer to
    let new_owner = Address::generate(&e);
    client.add_admin(&super_admin, &new_owner, &AdminRole::SuperAdmin);

    client.pause(&super_admin);
    client.transfer_ownership(&super_admin, &new_owner);
}

// ---------------------------------------------------------------------------
// Pause blocks accept_ownership
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #106)")] // ContractPaused
fn emergency_pause_blocks_accept_ownership() {
    let (e, client, super_admin) = setup();

    let new_owner = Address::generate(&e);
    client.add_admin(&super_admin, &new_owner, &AdminRole::SuperAdmin);
    client.transfer_ownership(&super_admin, &new_owner);

    // Advance past the timelock
    e.ledger()
        .with_mut(|li| li.timestamp = li.timestamp + 86_401);

    client.pause(&super_admin);
    client.accept_ownership(&new_owner);
}

// ---------------------------------------------------------------------------
// Pause blocks suspend_admin
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #106)")] // ContractPaused
fn emergency_pause_blocks_suspend_admin() {
    let (e, client, super_admin) = setup();

    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);

    let until_ts = e.ledger().timestamp() + 3600;
    client.pause(&super_admin);
    client.suspend_admin(&super_admin, &new_admin, &until_ts);
}

// NOTE: set_pause_signer and set_pause_threshold are intentionally NOT
// blocked by pause (they call require_admin_auth, not require_not_paused).
// This ensures pause signers can always be managed during emergencies.

// ---------------------------------------------------------------------------
// Pause blocks set_pause_signer / set_pause_threshold? (regression guard)
// ---------------------------------------------------------------------------
//
// The comment above documents the intended behavior: pause-signer management
// is NOT gated by pause. This test locks that contract in so a future change
// cannot silently make the emergency path unreachable.

#[test]
fn emergency_pause_does_not_block_pause_signer_management() {
    let (e, client, super_admin) = setup();

    let signer = Address::generate(&e);
    client.pause(&super_admin);
    assert!(client.is_paused());

    // Must remain callable while paused so signers can be rotated during an
    // incident. If this ever starts panicking with ContractPaused, the
    // emergency recovery path has regressed.
    client.set_pause_signer(&super_admin, &signer);
    client.set_pause_threshold(&super_admin, &1u32);

    // Pause state must be untouched by signer management.
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Authorization is enforced while paused
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn emergency_pause_requires_admin_auth() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);

    // Drop auth mocking: an unauthenticated caller must not be able to pause.
    e.set_auths(&[]);
    let attacker = Address::generate(&e);
    client.pause(&attacker);
}

// ---------------------------------------------------------------------------
// Pause gate is enforced on every write, including after a failed write
// ---------------------------------------------------------------------------

#[test]
fn emergency_pause_gate_is_sticky_across_failed_writes() {
    let (e, client, super_admin) = setup();

    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
    let count_before = client.get_admin_count();

    client.pause(&super_admin);

    // A blocked write must not mutate state.
    let blocked = client.try_add_admin(&super_admin, &Address::generate(&e), &AdminRole::Admin);
    assert!(blocked.is_err());
    assert_eq!(client.get_admin_count(), count_before);
    assert!(client.is_paused());

    // A second blocked write must also be rejected (no partial state).
    let blocked_again =
        client.try_remove_admin(&super_admin, &new_admin);
    assert!(blocked_again.is_err());
    assert_eq!(client.get_admin_count(), count_before);
    assert!(client.is_paused());

    // Unpause restores writes and prior state is intact.
    client.unpause(&super_admin);
    assert!(!client.is_paused());
    client.remove_admin(&super_admin, &new_admin);
    assert_eq!(client.get_admin_count(), count_before - 1);
}

// ---------------------------------------------------------------------------
// Boundary: pause/unpause cycle repeated many times stays consistent
// ---------------------------------------------------------------------------

#[test]
fn emergency_pause_unpause_cycle_is_stable() {
    let (e, client, super_admin) = setup();

    for _ in 0..8 {
        client.pause(&super_admin);
        assert!(client.is_paused());
        client.unpause(&super_admin);
        assert!(!client.is_paused());
    }

    // State must still be usable after many toggles.
    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);
}

// ---------------------------------------------------------------------------
// Boundary: pause blocks writes at the exact toggle boundary
// ---------------------------------------------------------------------------

#[test]
fn emergency_pause_boundary_write_before_and_after_toggle() {
    let (e, client, super_admin) = setup();

    // Write immediately before pause succeeds.
    let a = Address::generate(&e);
    client.add_admin(&super_admin, &a, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);

    client.pause(&super_admin);

    // Write immediately after pause is rejected.
    let b = Address::generate(&e);
    let rejected = client.try_add_admin(&super_admin, &b, &AdminRole::Admin);
    assert!(rejected.is_err());
    assert_eq!(client.get_admin_count(), 2);

    client.unpause(&super_admin);

    // Write immediately after unpause succeeds again.
    client.add_admin(&super_admin, &b, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 3);
}

// ---------------------------------------------------------------------------
// Regression: pause does not corrupt admin records
// ---------------------------------------------------------------------------

#[test]
fn emergency_pause_preserves_admin_records() {
    let (e, client, super_admin) = setup();

    let a = Address::generate(&e);
    let b = Address::generate(&e);
    client.add_admin(&super_admin, &a, &AdminRole::Admin);
    client.add_admin(&super_admin, &b, &AdminRole::Operator);

    let before = client.get_all_admins();
    let count_before = client.get_admin_count();

    client.pause(&super_admin);
    // Reads during pause must reflect the same records.
    assert_eq!(client.get_admin_count(), count_before);
    assert_eq!(client.get_all_admins().len(), before.len());

    client.unpause(&super_admin);
    assert_eq!(client.get_admin_count(), count_before);
    assert_eq!(client.get_all_admins().len(), before.len());
}

// ---------------------------------------------------------------------------
// Regression: deactivated admins stay deactivated across pause cycles
// ---------------------------------------------------------------------------

#[test]
fn emergency_pause_preserves_deactivation_state() {
    let (e, client, super_admin) = setup();

    let a = Address::generate(&e);
    client.add_admin(&super_admin, &a, &AdminRole::Admin);
    client.deactivate_admin(&super_admin, &a);

    client.pause(&super_admin);
    client.unpause(&super_admin);

    // Reactivation must still work after the pause cycle, proving the
    // deactivation flag survived the emergency toggle.
    client.reactivate_admin(&super_admin, &a);
    assert_eq!(client.get_admin_count(), 2);
}

// ---------------------------------------------------------------------------
// Pause does NOT block pause/unpause themselves (admin can always toggle)
// ---------------------------------------------------------------------------

#[test]
fn emergency_pause_unpause_always_works_even_when_paused() {
    let (e, client, super_admin) = setup();

    // First pause
    client.pause(&super_admin);
    assert!(client.is_paused());

    // Unpause must still work (otherwise we'd be stuck)
    client.unpause(&super_admin);
    assert!(!client.is_paused());

    // Re-pause to verify cycle
    client.pause(&super_admin);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Pause invariant: pause state is preserved across read operations
// ---------------------------------------------------------------------------

#[test]
fn emergency_pause_state_preserved_after_reads() {
    let (e, client, super_admin) = setup();

    client.pause(&super_admin);
    assert!(client.is_paused());

    // Perform several reads
    let _ = client.get_admin_count();
    let _ = client.get_all_admins();
    let _ = client.get_config();

    // Pause state must still be true
    assert!(client.is_paused());

    // Unpause
    client.unpause(&super_admin);
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// Idempotent pause: pausing an already-paused contract is a no-op
// ---------------------------------------------------------------------------

#[test]
fn emergency_pause_idempotent() {
    let (e, client, super_admin) = setup();

    client.pause(&super_admin);
    assert!(client.is_paused());

    // Second pause should not error
    client.pause(&super_admin);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Idempotent unpause: unpausing an already-unpaused contract is a no-op
// ---------------------------------------------------------------------------

#[test]
fn emergency_unpause_idempotent() {
    let (e, client, super_admin) = setup();

    // Unpause when not paused should be a no-op (not an error)
    client.unpause(&super_admin);
    assert!(!client.is_paused());

    client.unpause(&super_admin);
    assert!(!client.is_paused());
}
