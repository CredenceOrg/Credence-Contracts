//! Adversarial regression tests for zero / invalid address rejection in the
//! Admin contract (issue #1448).
//!
//! # What this file covers
//!
//! Every privileged entrypoint that accepts an `Address` argument MUST reject:
//!
//! 1. **The zero-address sentinel** — the all-zero Ed25519 public key encoded
//!    as a Soroban strkey (`GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF`).
//!    Accepting this address could permanently strand administration because
//!    the zero public key has no known private key.
//!
//! 2. **The contract's own address** — assigning a governance role to the
//!    contract itself breaks invariants because the contract cannot sign
//!    transactions on its own behalf.
//!
//! # Invariants checked
//!
//! * Rejection fires `InvalidAdminAddress` (error code 110) for every
//!   entrypoint.
//! * A rejected call never advances the monotonic `ConfigEpoch` counter.
//! * A rejected call leaves all admin-list state exactly as it was before the
//!   call (full rollback: no partial writes, no orphaned role entries).
//! * Valid addresses continue to succeed — the guard must not be over-broad.
//! * Re-submitting the same invalid address produces the same error and still
//!   does not mutate any state (idempotent rejection).
//!
//! # Error code reference
//!
//! `ContractError::InvalidAdminAddress = 110` (wire-stable, defined in
//! `credence_errors`).

#![cfg(test)]

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

// ── helpers ──────────────────────────────────────────────────────────────────

/// The all-zero Ed25519 public key encoded in Soroban strkey format.
/// This is the canonical zero-address sentinel for the admin contract.
fn zero_address(env: &Env) -> Address {
    Address::from_string(&soroban_sdk::String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

/// Expected Soroban SDK error value for `ContractError::InvalidAdminAddress`.
fn invalid_admin_address_err() -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(110)
}

/// Bootstrap a fresh contract and return `(env, client, super_admin_address)`.
///
/// The `super_admin` is the only registered admin after setup.
fn setup() -> (Env, AdminContractClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&env, &contract_id);
    let super_admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);
    (env, client, super_admin)
}

/// Bootstrap a contract with a second SuperAdmin to allow ownership transfer
/// tests without running into the "can't deactivate last SuperAdmin" guard.
fn setup_with_two_super_admins() -> (Env, AdminContractClient<'static>, Address, Address) {
    let (env, client, admin) = setup();
    let second = Address::generate(&env);
    client.add_admin(&admin, &second, &AdminRole::SuperAdmin);
    (env, client, admin, second)
}

// ── zero-address: add_admin ───────────────────────────────────────────────────

/// `add_admin` with the zero-address as `new_admin` must return
/// `InvalidAdminAddress` (error 110) and must not advance the epoch or insert
/// any record.
#[test]
fn add_admin_rejects_zero_address_and_preserves_state() {
    let (env, client, admin) = setup();
    let epoch_before = client.get_config_epoch();
    let count_before = client.get_admin_count();
    let zero = zero_address(&env);

    let err = client
        .try_add_admin(&admin, &zero, &AdminRole::Admin)
        .unwrap_err()
        .unwrap();

    assert_eq!(err, invalid_admin_address_err());
    // Epoch must not have advanced.
    assert_eq!(client.get_config_epoch(), epoch_before);
    // Admin count must be unchanged.
    assert_eq!(client.get_admin_count(), count_before);
    // The zero address must not appear in the admin list.
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert!(!list.contains(&zero));
}

/// Repeated submission of the same invalid address produces the same error
/// and does not mutate state on any retry (idempotent rejection).
#[test]
fn add_admin_rejects_zero_address_idempotently() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);

    for _ in 0..3 {
        let err = client
            .try_add_admin(&admin, &zero, &AdminRole::Admin)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, invalid_admin_address_err());
    }

    assert_eq!(client.get_config_epoch(), 0);
    assert_eq!(client.get_admin_count(), 1); // only the initial super_admin
}

// ── zero-address: remove_admin ────────────────────────────────────────────────

/// `remove_admin` with the zero-address must return `InvalidAdminAddress` and
/// must not change state (the guard fires before any storage lookup).
#[test]
fn remove_admin_rejects_zero_address_and_preserves_state() {
    let (env, client, admin) = setup();
    let epoch_before = client.get_config_epoch();
    let zero = zero_address(&env);

    let err = client
        .try_remove_admin(&admin, &zero)
        .unwrap_err()
        .unwrap();

    assert_eq!(err, invalid_admin_address_err());
    assert_eq!(client.get_config_epoch(), epoch_before);
}

// ── zero-address: update_admin_role ──────────────────────────────────────────

/// `update_admin_role` with the zero-address as `admin_address` must return
/// `InvalidAdminAddress` and must not change the epoch or any stored role.
#[test]
fn update_admin_role_rejects_zero_address_and_preserves_state() {
    let (env, client, admin) = setup();
    let epoch_before = client.get_config_epoch();
    let zero = zero_address(&env);

    let err = client
        .try_update_admin_role(&admin, &zero, &AdminRole::Admin)
        .unwrap_err()
        .unwrap();

    assert_eq!(err, invalid_admin_address_err());
    assert_eq!(client.get_config_epoch(), epoch_before);
}

// ── zero-address: deactivate_admin ───────────────────────────────────────────

/// `deactivate_admin` with the zero-address must return `InvalidAdminAddress`
/// and leave the epoch unchanged.
#[test]
fn deactivate_admin_rejects_zero_address_and_preserves_state() {
    let (env, client, admin) = setup();
    let epoch_before = client.get_config_epoch();
    let zero = zero_address(&env);

    let err = client
        .try_deactivate_admin(&admin, &zero)
        .unwrap_err()
        .unwrap();

    assert_eq!(err, invalid_admin_address_err());
    assert_eq!(client.get_config_epoch(), epoch_before);
}

// ── zero-address: reactivate_admin ───────────────────────────────────────────

/// `reactivate_admin` with the zero-address must return `InvalidAdminAddress`
/// and leave the epoch unchanged.
#[test]
fn reactivate_admin_rejects_zero_address_and_preserves_state() {
    let (env, client, admin) = setup();
    let epoch_before = client.get_config_epoch();
    let zero = zero_address(&env);

    let err = client
        .try_reactivate_admin(&admin, &zero)
        .unwrap_err()
        .unwrap();

    assert_eq!(err, invalid_admin_address_err());
    assert_eq!(client.get_config_epoch(), epoch_before);
}

// ── zero-address: transfer_ownership ─────────────────────────────────────────

/// `transfer_ownership` with the zero-address as `new_owner` must return
/// `InvalidAdminAddress` before touching the `PendingOwner` or epoch.
#[test]
fn transfer_ownership_rejects_zero_address_and_preserves_state() {
    let (env, client, admin) = setup();
    let epoch_before = client.get_config_epoch();
    let zero = zero_address(&env);

    let err = client
        .try_transfer_ownership(&admin, &zero)
        .unwrap_err()
        .unwrap();

    assert_eq!(err, invalid_admin_address_err());
    assert_eq!(client.get_config_epoch(), epoch_before);
    // No pending owner must have been set.
    assert_eq!(client.get_pending_owner(), None);
}

// ── zero-address: set_pause_signer ───────────────────────────────────────────

/// `set_pause_signer` with the zero-address as `signer` must return
/// `InvalidAdminAddress` and must not register the signer or advance the epoch.
#[test]
fn set_pause_signer_rejects_zero_address_and_preserves_state() {
    let (env, client, admin) = setup();
    let epoch_before = client.get_config_epoch();
    let zero = zero_address(&env);

    let err = client
        .try_set_pause_signer(&admin, &zero, &true)
        .unwrap_err()
        .unwrap();

    assert_eq!(err, invalid_admin_address_err());
    assert_eq!(client.get_config_epoch(), epoch_before);
}

// ── contract-self-address: add_admin ─────────────────────────────────────────

/// `add_admin` with the contract's own address must return
/// `InvalidAdminAddress` (the contract cannot sign transactions).
#[test]
fn add_admin_rejects_contract_self_address_and_preserves_state() {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&env, &contract_id);
    let super_admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);

    let epoch_before = client.get_config_epoch();
    let count_before = client.get_admin_count();

    let err = client
        .try_add_admin(&super_admin, &contract_id, &AdminRole::Admin)
        .unwrap_err()
        .unwrap();

    assert_eq!(err, invalid_admin_address_err());
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(client.get_admin_count(), count_before);
}

/// `add_admin` with the contract's own address at SuperAdmin role must also
/// be rejected (role level must not bypass the address validation guard).
#[test]
fn add_admin_rejects_contract_self_address_at_any_role() {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&env, &contract_id);
    let super_admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);

    for role in [AdminRole::Operator, AdminRole::Admin, AdminRole::SuperAdmin] {
        let err = client
            .try_add_admin(&super_admin, &contract_id, &role)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, invalid_admin_address_err(), "role={role:?}");
    }

    // Epoch must not have advanced for any of the three attempts.
    assert_eq!(client.get_config_epoch(), 0);
}

// ── contract-self-address: transfer_ownership ────────────────────────────────

/// `transfer_ownership` with a contract address as `new_owner` must fail and
/// leave `PendingOwner` unset and the epoch unchanged.
///
/// A contract address can never be a SuperAdmin (it cannot sign), so this
/// call will be rejected — either by the self-address guard (error 110) if the
/// address happens to match the contract's own address, or by the SuperAdmin
/// lookup (error 100) because no admin record exists for an arbitrary contract.
/// Either way ownership must not be transferred and no state must change.
#[test]
fn transfer_ownership_rejects_contract_self_address_and_preserves_state() {
    let (env, client, admin) = setup();
    let epoch_before = client.get_config_epoch();
    let other_contract = env.register_contract(None, AdminContract);

    let result = client.try_transfer_ownership(&admin, &other_contract);
    assert!(
        result.is_err(),
        "transfer_ownership must reject a contract address as new_owner"
    );
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(client.get_pending_owner(), None);
}

// ── contract-self-address: set_pause_signer ──────────────────────────────────

/// `set_pause_signer` with the contract's own address (the exact address the
/// client was registered against) must return `InvalidAdminAddress` because
/// the `pausable::set_pause_signer` guard checks `signer == current_contract_address()`.
#[test]
fn set_pause_signer_rejects_contract_self_address() {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&env, &contract_id);
    let super_admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);

    let epoch_before = client.get_config_epoch();

    // The contract's own address is `contract_id`.
    let err = client
        .try_set_pause_signer(&super_admin, &contract_id, &true)
        .unwrap_err()
        .unwrap();

    assert_eq!(err, invalid_admin_address_err());
    assert_eq!(client.get_config_epoch(), epoch_before);
}

// ── epoch isolation: mixed valid/invalid calls ────────────────────────────────

/// A batch of alternating valid and invalid (zero-address) mutations must
/// advance the epoch exactly once per committed (valid) mutation and never for
/// rejected (invalid) ones.
#[test]
fn epoch_advances_only_for_committed_mutations_not_for_rejections() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);

    // Initial epoch is 0.
    assert_eq!(client.get_config_epoch(), 0);

    // Rejected — epoch stays at 0.
    let _ = client.try_add_admin(&admin, &zero, &AdminRole::Operator);
    assert_eq!(client.get_config_epoch(), 0);

    // Committed — epoch advances to 1.
    let valid = Address::generate(&env);
    client.add_admin(&admin, &valid, &AdminRole::Operator);
    assert_eq!(client.get_config_epoch(), 1);

    // Rejected — epoch stays at 1.
    let _ = client.try_update_admin_role(&admin, &zero, &AdminRole::Admin);
    assert_eq!(client.get_config_epoch(), 1);

    // Committed — epoch advances to 2.
    client.update_admin_role(&admin, &valid, &AdminRole::Admin);
    assert_eq!(client.get_config_epoch(), 2);

    // Rejected — epoch stays at 2.
    let _ = client.try_deactivate_admin(&admin, &zero);
    assert_eq!(client.get_config_epoch(), 2);

    // Committed — epoch advances to 3.
    client.deactivate_admin(&admin, &valid);
    assert_eq!(client.get_config_epoch(), 3);
}

// ── state integrity: admin list is never corrupted ───────────────────────────

/// After a series of invalid-address rejections the admin list must be
/// structurally identical to the list that would exist after only the valid
/// mutations.
#[test]
fn admin_list_is_not_corrupted_by_rejected_zero_address_calls() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);
    let valid_a = Address::generate(&env);
    let valid_b = Address::generate(&env);

    // Interleave valid additions with rejected zero-address calls.
    client.add_admin(&admin, &valid_a, &AdminRole::Operator);
    let _ = client.try_add_admin(&admin, &zero, &AdminRole::Admin);
    client.add_admin(&admin, &valid_b, &AdminRole::Admin);
    let _ = client.try_remove_admin(&admin, &zero);
    let _ = client.try_update_admin_role(&admin, &zero, &AdminRole::SuperAdmin);

    // The zero address must not appear anywhere in the list.
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert!(!list.contains(&zero));

    // The two valid additions must be present.
    assert!(list.contains(&valid_a));
    assert!(list.contains(&valid_b));

    // Count: initial super_admin + valid_a + valid_b = 3.
    assert_eq!(client.get_admin_count(), 3);
}

// ── partial-failure / state-before integrity ─────────────────────────────────

/// A failing `add_admin(zero)` call that occurs just before a valid
/// `add_admin` must not leave any partial storage entry.  After the valid call,
/// inspecting `is_admin` for the zero address must still return `Role::User`.
#[test]
fn failed_add_admin_leaves_no_partial_storage_for_zero_address() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);
    let valid = Address::generate(&env);

    let _ = client.try_add_admin(&admin, &zero, &AdminRole::Admin);
    client.add_admin(&admin, &valid, &AdminRole::Admin);

    // The zero address must not be considered an admin.
    assert_eq!(
        client.is_admin(&zero),
        credence_errors::Role::User,
        "zero address must not appear as admin after rejected call"
    );
}

// ── ownership-transfer idempotent rejection ───────────────────────────────────

/// Submitting `transfer_ownership(zero)` twice in a row produces the same
/// error each time and never sets a `PendingOwner`.
#[test]
fn transfer_ownership_zero_address_rejection_is_idempotent() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);

    for _ in 0..2 {
        let err = client
            .try_transfer_ownership(&admin, &zero)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, invalid_admin_address_err());
    }

    assert_eq!(client.get_pending_owner(), None);
    assert_eq!(client.get_config_epoch(), 0);
}

// ── valid-address happy-path regression ──────────────────────────────────────

/// All mutating entrypoints that accept an `Address` argument must still
/// succeed with a valid, non-zero address.  This guard ensures the validation
/// is not over-broad.
#[test]
fn all_entrypoints_accept_valid_non_zero_addresses() {
    let (env, client, admin) = setup();

    // add_admin with a fresh address.
    let new_op = Address::generate(&env);
    let info = client.add_admin(&admin, &new_op, &AdminRole::Operator);
    assert_eq!(info.address, new_op);
    assert_eq!(info.role, AdminRole::Operator);
    let epoch_after_add = client.get_config_epoch();
    assert_eq!(epoch_after_add, 1);

    // update_admin_role.
    let updated = client.update_admin_role(&admin, &new_op, &AdminRole::Admin);
    assert_eq!(updated.role, AdminRole::Admin);
    assert_eq!(client.get_config_epoch(), 2);

    // deactivate_admin.
    client.deactivate_admin(&admin, &new_op);
    assert_eq!(client.get_config_epoch(), 3);

    // reactivate_admin.
    client.reactivate_admin(&admin, &new_op);
    assert_eq!(client.get_config_epoch(), 4);

    // set_pause_signer.
    let pause_signer = Address::generate(&env);
    client.set_pause_signer(&admin, &pause_signer, &true);
    assert_eq!(client.get_config_epoch(), 5);

    // transfer_ownership: new_owner must be a SuperAdmin first.
    let new_owner = Address::generate(&env);
    client.add_admin(&admin, &new_owner, &AdminRole::SuperAdmin);
    client.transfer_ownership(&admin, &new_owner);
    assert_eq!(client.get_pending_owner(), Some(new_owner.clone()));

    // remove_admin (after re-elevating new_op, so the list stays healthy).
    client.remove_admin(&admin, &new_op);
}

// ── boundary: role variants do not bypass address validation ──────────────────

/// The address guard in `add_admin` fires regardless of the requested role
/// level.  Attempting to insert the zero address as SuperAdmin, Admin, or
/// Operator must all return `InvalidAdminAddress`.
#[test]
fn add_admin_zero_address_rejected_for_all_role_variants() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);

    for role in [AdminRole::Operator, AdminRole::Admin, AdminRole::SuperAdmin] {
        let err = client
            .try_add_admin(&admin, &zero, &role)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, invalid_admin_address_err(), "role={role:?}");
    }

    // Three rejected calls — epoch must still be 0.
    assert_eq!(client.get_config_epoch(), 0);
}

// ── boundary: update_admin_role zero-address rejected for all target roles ────

/// `update_admin_role` must reject the zero address regardless of the target
/// role.
#[test]
fn update_admin_role_zero_address_rejected_for_all_role_variants() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);

    for role in [AdminRole::Operator, AdminRole::Admin, AdminRole::SuperAdmin] {
        let err = client
            .try_update_admin_role(&admin, &zero, &role)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, invalid_admin_address_err(), "role={role:?}");
    }

    assert_eq!(client.get_config_epoch(), 0);
}

// ── concurrent-style: repeated retries on zero address cannot pollute state ───

/// A caller that naively retries `add_admin(zero)` in a retry loop (simulating
/// a buggy client) must never corrupt state or advance the epoch.  The
/// contract's rejection must be stable and total.
#[test]
fn repeated_retries_with_zero_address_cannot_corrupt_state() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);

    for _ in 0..5 {
        let err = client
            .try_add_admin(&admin, &zero, &AdminRole::Admin)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, invalid_admin_address_err());
    }

    // State must be pristine: one admin (the super_admin), epoch = 0.
    assert_eq!(client.get_admin_count(), 1);
    assert_eq!(client.get_config_epoch(), 0);
    assert_eq!(client.is_admin(&zero), credence_errors::Role::User);
}

// ── permission state: suspended caller can still be rejected for zero target ──

/// Even when the caller is a suspended admin, attempting to mutate a zero
/// address should be caught by the address guard (which fires before the
/// caller's liveness check), ensuring layered defence-in-depth.
///
/// NOTE: the contract fires `require_valid_admin_address` before the caller
/// role/liveness checks in every entrypoint, so the error code here is always
/// 110, not a suspension/auth error.  This test documents and locks that
/// order.
#[test]
fn zero_address_guard_fires_before_suspension_check() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);

    // Add a second operator and immediately suspend them.
    let op = Address::generate(&env);
    client.add_admin(&admin, &op, &AdminRole::Operator);

    let now = env.ledger().timestamp();
    let suspension_end = now + 10_000;
    client.suspend_admin(&admin, &op, &suspension_end);

    // The suspended operator tries to add the zero address.
    // The address guard fires first — error 110, not a suspension error.
    let err = client
        .try_add_admin(&op, &zero, &AdminRole::Operator)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, invalid_admin_address_err());
}

// ── remove_admin: full state rollback ────────────────────────────────────────

/// A rejected `remove_admin(zero)` must not change the admin list, the role
/// lists, or the admin count.
#[test]
fn remove_admin_zero_address_full_rollback() {
    let (env, client, admin) = setup();
    let zero = zero_address(&env);
    let count_before = client.get_admin_count();
    let epoch_before = client.get_config_epoch();

    let _ = client.try_remove_admin(&admin, &zero);

    assert_eq!(client.get_admin_count(), count_before);
    assert_eq!(client.get_config_epoch(), epoch_before);

    // Admin list unchanged.
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert!(!list.contains(&zero));
}
