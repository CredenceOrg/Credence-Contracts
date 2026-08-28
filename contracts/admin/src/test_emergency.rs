//! Emergency/pause mode invariant tests for the Admin contract.
//!
//! These tests prove that emergency toggles restrict writes as intended
//! and that exits preserve the correct invariants.
//!
//! When the contract is paused:
//! - All writable admin entrypoints must be blocked.
//! - Read-only entrypoints must still function.
//! - The pause state must be correctly toggled back.

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
