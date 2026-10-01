#![cfg(test)]

//! Authentication boundary tests for AdminContract.
//!
//! Complements the existing test_authorization.rs by locking authentication
//! on functions not yet fully covered:
//!   - update_admin_role
//!   - deactivate_admin / reactivate_admin
//!   - suspend_admin
//!   - transfer_ownership / accept_ownership
//!
//! Rule: every non-view #[contractimpl] function must require an authenticated
//! address arg.  Happy-path asserts the operation succeeds; sad-path asserts
//! an unauthorised caller is rejected.

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};
use testutils::{admin as test_admin, user};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_env() -> (Env, Address, Address) {
    let env = Env::default();
    let contract_address = env.register_contract(None, AdminContract);
    let super_admin = test_admin(&env);
    env.mock_all_auths();
    env.as_contract(&contract_address, || {
        AdminContract::initialize(env.clone(), super_admin.clone(), 1, 10);
    });
    (env, contract_address, super_admin)
}

fn add_admin(
    env: &Env,
    contract: &Address,
    caller: &Address,
    new_admin: &Address,
    role: AdminRole,
) {
    env.as_contract(contract, || {
        AdminContract::add_admin(env.clone(), caller.clone(), new_admin.clone(), role);
    });
}

fn advance(env: &Env, secs: u64) {
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: env.ledger().timestamp() + secs,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });
}

// ---------------------------------------------------------------------------
// update_admin_role — caller must be a higher-level admin
// ---------------------------------------------------------------------------

/// Happy path: SuperAdmin promotes an Operator to Admin.
#[test]
fn update_admin_role_succeeds_when_super_admin_authorizes() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    let info = env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Admin,
        )
    });
    assert_eq!(info.role, AdminRole::Admin);
}

/// Sad path: an Operator cannot promote another Operator.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn update_admin_role_rejected_when_operator_tries_to_promote() {
    let (env, contract, super_admin) = setup_env();
    let op1 = user(&env);
    let op2 = user(&env);
    add_admin(&env, &contract, &super_admin, &op1, AdminRole::Operator);
    add_admin(&env, &contract, &super_admin, &op2, AdminRole::Operator);

    env.as_contract(&contract, || {
        // op1 tries to give op2 a higher role — must be rejected.
        AdminContract::update_admin_role(env.clone(), op1.clone(), op2.clone(), AdminRole::Admin);
    });
}

// ---------------------------------------------------------------------------
// deactivate_admin — caller must outrank target
// ---------------------------------------------------------------------------

/// Happy path: SuperAdmin deactivates an Admin.
#[test]
fn deactivate_admin_succeeds_when_caller_outranks_target() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    let info = env.as_contract(&contract, || {
        AdminContract::get_admin_info(env.clone(), admin.clone())
    });
    assert!(!info.active);
}

/// Sad path: an Operator cannot deactivate an Admin.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn deactivate_admin_rejected_when_caller_does_not_outrank_target() {
    let (env, contract, super_admin) = setup_env();
    let admin1 = test_admin(&env);
    let admin2 = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin1, AdminRole::Admin);
    add_admin(&env, &contract, &super_admin, &admin2, AdminRole::Admin);

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), admin1.clone(), admin2.clone());
    });
}

// ---------------------------------------------------------------------------
// reactivate_admin — caller must outrank target
// ---------------------------------------------------------------------------

/// Happy path: SuperAdmin reactivates a previously deactivated Admin.
#[test]
fn reactivate_admin_succeeds_when_super_admin_authorizes() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    // Deactivate first.
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    // Reactivate.
    env.as_contract(&contract, || {
        AdminContract::reactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    assert_eq!(
        env.as_contract(&contract, || AdminContract::is_admin(env.clone(), admin)),
        Role::Admin
    );
}

/// Sad path: an Operator cannot reactivate an Admin.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn reactivate_admin_rejected_when_caller_does_not_outrank_target() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    let operator = user(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    // Operator tries to reactivate the Admin — must be rejected.
    env.as_contract(&contract, || {
        AdminContract::reactivate_admin(env.clone(), operator.clone(), admin.clone());
    });
}

// ---------------------------------------------------------------------------
// suspend_admin — caller must be an admin and outrank the target
// ---------------------------------------------------------------------------

/// Happy path: SuperAdmin suspends an Admin for a future timestamp.
#[test]
fn suspend_admin_succeeds_when_super_admin_authorizes() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let until_ts = env.ledger().timestamp() + 3600;
    env.as_contract(&contract, || {
        AdminContract::suspend_admin(env.clone(), super_admin.clone(), admin.clone(), until_ts);
    });

    // Admin should appear inactive while timestamp < until_ts.
    let admin_role = env.as_contract(&contract, || {
        AdminContract::is_admin(env.clone(), admin.clone())
    });
    assert_eq!(
        admin_role,
        Role::User,
        "suspended admin must not be active before expiry"
    );
}

/// Sad path: suspension with a past timestamp must be rejected.
#[test]
#[should_panic]
fn suspend_admin_rejected_when_until_ts_is_in_the_past() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    advance(&env, 10_000);
    let past_ts = env.ledger().timestamp() - 1;
    env.as_contract(&contract, || {
        AdminContract::suspend_admin(env.clone(), super_admin.clone(), admin.clone(), past_ts);
    });
}

/// Sad path: an Operator cannot suspend an Admin (lower rank).
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn suspend_admin_rejected_when_caller_does_not_outrank_target() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    let operator = user(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    let until_ts = env.ledger().timestamp() + 3600;
    env.as_contract(&contract, || {
        // Operator tries to suspend Admin — caller role (1) < target role (2).
        AdminContract::suspend_admin(env.clone(), operator.clone(), admin.clone(), until_ts);
    });
}

// ---------------------------------------------------------------------------
// transfer_ownership — caller must be the current owner
// ---------------------------------------------------------------------------

/// Happy path: owner initiates a transfer to a SuperAdmin; pending owner is set.
#[test]
fn transfer_ownership_succeeds_when_owner_authorizes() {
    let (env, contract, super_admin) = setup_env();
    // Create a second SuperAdmin to transfer ownership to.
    let new_super = test_admin(&env);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &new_super,
        AdminRole::SuperAdmin,
    );

    env.as_contract(&contract, || {
        AdminContract::transfer_ownership(env.clone(), super_admin.clone(), new_super.clone());
    });

    let pending = env.as_contract(&contract, || AdminContract::get_pending_owner(env.clone()));
    assert_eq!(pending, Some(new_super));
}

/// Sad path: a non-owner caller (Admin) cannot initiate an ownership transfer.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn transfer_ownership_rejected_when_caller_is_not_owner() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    let new_super = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &new_super,
        AdminRole::SuperAdmin,
    );

    env.as_contract(&contract, || {
        // admin is not the owner — must be rejected.
        AdminContract::transfer_ownership(env.clone(), admin.clone(), new_super.clone());
    });
}

// ---------------------------------------------------------------------------
// accept_ownership — pending owner must authorize
// ---------------------------------------------------------------------------

/// Happy path: pending owner accepts and becomes the new owner.
#[test]
fn accept_ownership_succeeds_when_pending_owner_authorizes() {
    let (env, contract, super_admin) = setup_env();
    let new_super = test_admin(&env);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &new_super,
        AdminRole::SuperAdmin,
    );

    env.as_contract(&contract, || {
        AdminContract::transfer_ownership(env.clone(), super_admin.clone(), new_super.clone());
    });
    // Ownership transfer is two-step: the timelock must elapse before the
    // pending owner can accept.
    advance(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    env.as_contract(&contract, || {
        AdminContract::accept_ownership(env.clone(), new_super.clone());
    });

    let owner = env.as_contract(&contract, || AdminContract::get_owner(env.clone()));
    assert_eq!(owner, new_super);
}

/// Sad path: a stranger (not the pending owner) cannot accept the transfer.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn accept_ownership_rejected_when_caller_is_not_pending_owner() {
    let (env, contract, super_admin) = setup_env();
    let new_super = test_admin(&env);
    let stranger = user(&env);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &new_super,
        AdminRole::SuperAdmin,
    );

    env.as_contract(&contract, || {
        AdminContract::transfer_ownership(env.clone(), super_admin.clone(), new_super.clone());
    });
    env.as_contract(&contract, || {
        // stranger is not the pending owner.
        AdminContract::accept_ownership(env.clone(), stranger.clone());
    });
}

// ---------------------------------------------------------------------------
// Adversarial regression cases
//
// These cases harden the entrypoints against inputs that the happy/sad-path
// pairs above do not exercise:
//
//   - Idempotency invariants (no-op paths must not advance the epoch or
//     mutate state)
//   - Duplicate-state rejections (already-active, already-deactivated)
//   - Unknown-target rejections (non-registered address)
//   - Self-mutation edge cases (self-suspension, self-demotion)
//   - Re-suspension that would shorten or equal the existing window
//   - Deactivated caller attempting a privileged operation
//   - Capacity boundary (max-admin cap, min-admins guard on remove)
//   - Config-epoch stability across all rejected operations
//
// Each test documents the threat it models so reviewers can reason about
// whether the invariant is still enforced after a refactor.
// ---------------------------------------------------------------------------

// ── update_admin_role: idempotency ──────────────────────────────────────────

/// Idempotency: assigning the same role that an admin already holds must be a
/// strict no-op — no state change, no event, and crucially no epoch advance.
///
/// Threat: a concurrent observer that reads epoch N and re-reads after a
/// spurious same-role update would incorrectly conclude that governance state
/// changed and discard a valid cached snapshot.
#[test]
fn update_admin_role_no_op_when_role_is_unchanged() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    let epoch_before = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });
    let events_before = env.events().all().len();

    // Same role — must be a no-op.
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Operator, // identical to current role
        );
    });

    let epoch_after =
        env.as_contract(&contract, || AdminContract::get_config_epoch(env.clone()));
    assert_eq!(
        epoch_before, epoch_after,
        "no-op role update must not advance the config epoch"
    );
    assert_eq!(
        env.events().all().len(),
        events_before,
        "no-op role update must not emit events"
    );

    // Stored role must be unchanged.
    let info = env.as_contract(&contract, || {
        AdminContract::get_admin_info(env.clone(), operator.clone())
    });
    assert_eq!(info.role, AdminRole::Operator);
}

// ── update_admin_role: unknown target ───────────────────────────────────────

/// Rejection: updating the role of an address that has never been registered
/// as an admin must be rejected with NotAdmin(#100).
///
/// Threat: an off-chain race could send an update before the add_admin
/// transaction is included.  The contract must not create a ghost record.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn update_admin_role_rejected_when_target_is_not_registered() {
    let (env, contract, super_admin) = setup_env();
    let ghost = user(&env); // never added

    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            ghost.clone(),
            AdminRole::Operator,
        );
    });
}

// ── update_admin_role: self-demotion ────────────────────────────────────────

/// Boundary: a SuperAdmin attempting to lower their own role must be rejected.
///
/// Threat: a compromised key tries to demote itself before the operator
/// notices, stranding the governance hierarchy.  The contract must reject
/// self-assignment of an equal-or-lower role.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn update_admin_role_rejected_when_super_admin_demotes_self() {
    let (env, contract, super_admin) = setup_env();

    env.as_contract(&contract, || {
        // SuperAdmin tries to downgrade themselves to Admin — must be rejected.
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            super_admin.clone(),
            AdminRole::Admin,
        );
    });
}

// ── deactivate_admin: duplicate deactivation ────────────────────────────────

/// Rejection: deactivating an admin that is already deactivated must be
/// rejected with AlreadyDeactivated(#404).
///
/// Threat: a naïve retry loop could double-deactivate, potentially
/// triggering an event that misleads an indexer into logging a second
/// deactivation or spuriously advancing the epoch.
#[test]
#[should_panic(expected = "Error(Contract, #404)")]
fn deactivate_admin_rejected_when_already_deactivated() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    // First deactivation — should succeed.
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    // Second deactivation — must be rejected.
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });
}

// ── deactivate_admin: same-rank peer ────────────────────────────────────────

/// Rejection: an Admin-role caller cannot deactivate another Admin-role target
/// because deactivation requires the caller to *strictly* outrank the target.
///
/// This is distinct from the existing sad-path that tests operator-vs-admin;
/// it specifically pins the strict-outranks predicate at the Admin/Admin
/// boundary to prevent lateral privilege abuse within the same tier.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn deactivate_admin_rejected_when_caller_is_same_rank_as_target() {
    let (env, contract, super_admin) = setup_env();
    let admin_a = test_admin(&env);
    let admin_b = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin_a, AdminRole::Admin);
    add_admin(&env, &contract, &super_admin, &admin_b, AdminRole::Admin);

    env.as_contract(&contract, || {
        // admin_a and admin_b are the same rank — lateral deactivation must fail.
        AdminContract::deactivate_admin(env.clone(), admin_a.clone(), admin_b.clone());
    });
}

// ── deactivate_admin: epoch stability on rejection ──────────────────────────

/// Regression: a rejected deactivation must not advance the config epoch.
///
/// Threat: if a failed deactivation bumped the epoch, any client that
/// successfully read the pre-call state would treat its snapshot as stale
/// and unnecessarily re-fetch governance data, opening a window for a
/// race-condition exploit.
#[test]
fn deactivate_admin_rejected_call_does_not_advance_epoch() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    // Capture epoch after the add_admin call.
    let epoch_before = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    // Attempt a same-rank lateral deactivation (will panic).
    let second_admin = test_admin(&env);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &second_admin,
        AdminRole::Admin,
    );
    let epoch_after_add = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    // Wrap the rejected call; epoch must equal epoch_after_add.
    let env2 = env.clone();
    let contract2 = contract.clone();
    let admin_clone = admin.clone();
    let second_clone = second_admin.clone();

    // Use try_invoke pattern via the client — build a client to call try_*
    let client = AdminContractClient::new(&env, &contract);
    let result = client.try_deactivate_admin(&admin_clone, &second_clone);
    assert!(result.is_err(), "lateral deactivation must be rejected");

    let epoch_unchanged = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });
    assert_eq!(
        epoch_after_add, epoch_unchanged,
        "rejected deactivation must not advance the config epoch"
    );
}

// ── reactivate_admin: duplicate reactivation ────────────────────────────────

/// Rejection: reactivating an admin that is already active must be rejected
/// with AlreadyActive(#405).
///
/// Threat: a confused caller could reactivate an active admin (e.g. after a
/// lag in reading state), which must not create a spurious event or epoch bump.
#[test]
#[should_panic(expected = "Error(Contract, #405)")]
fn reactivate_admin_rejected_when_already_active() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    // admin is already active — reactivation must be rejected immediately.
    env.as_contract(&contract, || {
        AdminContract::reactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });
}

// ── reactivate_admin: unknown target ────────────────────────────────────────

/// Rejection: reactivating an address that was never registered must be
/// rejected with NotAdmin(#100).
///
/// Threat: an off-chain script issues a reactivate before the corresponding
/// add_admin is confirmed.  The contract must not create a ghost record.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn reactivate_admin_rejected_when_target_is_not_registered() {
    let (env, contract, super_admin) = setup_env();
    let ghost = user(&env); // never added

    env.as_contract(&contract, || {
        AdminContract::reactivate_admin(env.clone(), super_admin.clone(), ghost.clone());
    });
}

// ── reactivate_admin: deactivated caller cannot reactivate ──────────────────

/// Rejection: a caller whose own `active` flag is false must not be able to
/// invoke reactivate_admin, because `require_role_at_least` treats inactive
/// callers as having no effective role.
///
/// Threat: a temporarily deactivated admin leverages a window before an
/// on-chain deactivation to queue a reactivation and restore an ally's access.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn reactivate_admin_rejected_when_caller_is_deactivated() {
    let (env, contract, super_admin) = setup_env();
    let admin_a = test_admin(&env);
    let admin_b = test_admin(&env);

    // Both admins start active.
    add_admin(&env, &contract, &super_admin, &admin_a, AdminRole::Admin);
    add_admin(&env, &contract, &super_admin, &admin_b, AdminRole::Admin);

    // Deactivate admin_b so admin_a has someone to (try to) reactivate.
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin_b.clone());
    });

    // Deactivate admin_a (the would-be caller).
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin_a.clone());
    });

    // admin_a is deactivated — must not be able to reactivate admin_b.
    env.as_contract(&contract, || {
        AdminContract::reactivate_admin(env.clone(), admin_a.clone(), admin_b.clone());
    });
}

// ── suspend_admin: self-suspension ──────────────────────────────────────────

/// Rejection: an admin must not be able to suspend themselves.
///
/// Threat: a compromised key could self-suspend to manufacture an alibi
/// ("I was suspended — I couldn't have done that") while still having been
/// the effective caller at the time of a prior action.  The contract rejects
/// self-suspension with AdminUnchanged(#111).
#[test]
#[should_panic(expected = "Error(Contract, #111)")]
fn suspend_admin_rejected_when_caller_and_target_are_the_same() {
    let (env, contract, super_admin) = setup_env();
    let until_ts = env.ledger().timestamp() + 3_600;

    env.as_contract(&contract, || {
        AdminContract::suspend_admin(
            env.clone(),
            super_admin.clone(),
            super_admin.clone(), // self
            until_ts,
        );
    });
}

// ── suspend_admin: re-suspension must strictly extend the window ─────────────

/// Rejection: a second suspension whose `until_ts` does not strictly extend
/// the existing window must be rejected with AdminSuspended(#113).
///
/// Threat: an attacker who can call suspend_admin repeatedly attempts to
/// re-suspend with the same deadline so an audit trail shows two suspension
/// events for what is effectively one, or to probe whether the idempotency
/// check fires.  The contract must reject non-extending re-suspensions.
///
/// Note: test_suspension.rs#test_re_suspend_extends_suspension covers the
/// *success* path (strictly larger window).  This case covers the *rejection*
/// boundary at equality.
#[test]
#[should_panic(expected = "Error(Contract, #113)")]
fn suspend_admin_rejected_when_re_suspension_does_not_extend_window() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let until_ts = env.ledger().timestamp() + 3_600;

    // First suspension establishes the window.
    env.as_contract(&contract, || {
        AdminContract::suspend_admin(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            until_ts,
        );
    });

    // Second suspension with the same deadline — must be rejected.
    env.as_contract(&contract, || {
        AdminContract::suspend_admin(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            until_ts, // equal, not strictly greater
        );
    });
}

/// Rejection: a second suspension with a *shorter* deadline (earlier than the
/// existing window) must also be rejected with AdminSuspended(#113).
///
/// Threat: an attacker attempts to shrink an existing suspension so that the
/// target regains privileges sooner than intended.
#[test]
#[should_panic(expected = "Error(Contract, #113)")]
fn suspend_admin_rejected_when_re_suspension_shortens_window() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let until_ts = env.ledger().timestamp() + 7_200;

    // First suspension: window ends at +7200.
    env.as_contract(&contract, || {
        AdminContract::suspend_admin(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            until_ts,
        );
    });

    // Attempt to shorten to +3600 — must be rejected.
    env.as_contract(&contract, || {
        AdminContract::suspend_admin(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            env.ledger().timestamp() + 3_600, // shorter
        );
    });
}

// ── add_admin: capacity boundary ────────────────────────────────────────────

/// Rejection: adding an admin when the max-admin cap has been reached must be
/// rejected with ThresholdExceedsSigners(#601).
///
/// Threat: an unchecked loop or automated script keeps calling add_admin,
/// driving unbounded storage growth and inflating per-read gas costs for every
/// operation that iterates the admin list.
#[test]
#[should_panic(expected = "Error(Contract, #601)")]
fn add_admin_rejected_when_max_admin_cap_is_reached() {
    let env = Env::default();
    let contract = env.register_contract(None, AdminContract);
    let super_admin = test_admin(&env);
    env.mock_all_auths();

    // Initialize with a cap of 2 (super_admin already counts as 1).
    env.as_contract(&contract, || {
        AdminContract::initialize(env.clone(), super_admin.clone(), 1, 2);
    });

    // Add one more admin to fill the cap.
    let second = user(&env);
    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            second.clone(),
            AdminRole::Admin,
        );
    });

    // Third add must be rejected — cap is 2.
    let third = user(&env);
    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            third.clone(),
            AdminRole::Operator,
        );
    });
}

// ── remove_admin: min-admins guard ──────────────────────────────────────────

/// Rejection: removing the last SuperAdmin must be rejected when it would
/// drop the SuperAdmin count below `min_admins`.
///
/// Threat: a caller with sufficient privilege (i.e. a second SuperAdmin)
/// removes the last remaining SuperAdmin, making the contract ungovernable.
/// The min-admins guard must block this regardless of the caller's role.
#[test]
#[should_panic(expected = "Error(Contract, #107)")]
fn remove_admin_rejected_when_it_would_violate_min_admins_for_super_admin_tier() {
    let (env, contract, super_admin) = setup_env();
    // setup_env initializes with min_admins=1; super_admin is the only SuperAdmin.
    // A second SuperAdmin is required to call remove_admin on the first.
    let second_super = test_admin(&env);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &second_super,
        AdminRole::SuperAdmin,
    );

    // Manually set the super_admin role to be the last one by removing second_super
    // from the role list directly, so that super_admin is the sole SuperAdmin.
    env.as_contract(&contract, || {
        let mut role_list: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(AdminRole::SuperAdmin))
            .unwrap();
        // Remove second_super from the role list to leave only super_admin.
        let idx = role_list
            .iter()
            .position(|a| a == second_super)
            .unwrap();
        role_list.remove(idx as u32);
        env.storage()
            .instance()
            .set(&DataKey::RoleAdmins(AdminRole::SuperAdmin), &role_list);
        // Also remove the admin record so remove_admin finds only super_admin.
        env.storage()
            .instance()
            .remove(&DataKey::AdminInfo(second_super.clone()));
    });

    // Now super_admin is the last SuperAdmin; attempting to remove them must fail.
    // We need a caller that outranks super_admin — inject a ghost SuperAdmin record
    // directly (same technique as test_ownership_transfer.rs uses for fault injection).
    let caller = test_admin(&env);
    env.as_contract(&contract, || {
        let ghost_info = AdminInfo {
            address: caller.clone(),
            role: AdminRole::SuperAdmin,
            assigned_at: env.ledger().timestamp(),
            assigned_by: caller.clone(),
            active: true,
            suspended_until: 0,
        };
        env.storage()
            .instance()
            .set(&DataKey::AdminInfo(caller.clone()), &ghost_info);
    });

    env.as_contract(&contract, || {
        AdminContract::remove_admin(env.clone(), caller.clone(), super_admin.clone());
    });
}

// ── deactivated caller cannot invoke any privileged entrypoint ───────────────

/// Regression: a caller whose `active` flag is `false` must be treated as
/// having no effective role and must be rejected from *all* privileged
/// entrypoints.  This case exercises `add_admin` as a representative
/// write path; the same rejection is expected for every other mutating call
/// because they all route through `require_role_at_least`.
///
/// Threat: a deactivated admin exploits a window between the deactivation
/// transaction confirming on-chain and the caller's local auth cache
/// expiring to submit a privileged operation.
#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn deactivated_caller_cannot_add_admin() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    // Deactivate the caller.
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    // Deactivated admin tries to add an Operator — must be rejected.
    let new_op = user(&env);
    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            admin.clone(),
            new_op.clone(),
            AdminRole::Operator,
        );
    });
}

// ── config epoch: no advance on any rejected operation ───────────────────────

/// Regression: the config epoch must remain stable across a sequence of
/// rejected calls covering each entrypoint category.  This pins the
/// serialization contract documented in lib.rs: "rejected, stale, repeated,
/// and failed operations never advance the epoch".
///
/// This is the only test that checks epoch invariance across multiple
/// entrypoints in a single sequence; it is not redundant with the per-entrypoint
/// sad-path tests above, which only check functional rejection.
#[test]
fn config_epoch_does_not_advance_on_any_rejected_call() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    let operator = user(&env);
    let ghost = user(&env);

    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);
    add_admin(&env, &contract, &super_admin, &operator, AdminRole::Operator);

    let epoch_baseline = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    let client = AdminContractClient::new(&env, &contract);

    // 1. update_admin_role on unregistered target
    let _ = client.try_update_admin_role(&super_admin, &ghost, &AdminRole::Operator);

    // 2. deactivate_admin: same-rank lateral
    let _ = client.try_deactivate_admin(&admin, &operator);

    // 3. reactivate_admin: target already active
    let _ = client.try_reactivate_admin(&super_admin, &admin);

    // 4. suspend_admin: self-suspension
    let until_ts = env.ledger().timestamp() + 3_600;
    let _ = client.try_suspend_admin(&super_admin, &super_admin, &until_ts);

    // 5. update_admin_role: same role (no-op — epoch must not advance)
    let _ = client.try_update_admin_role(&super_admin, &operator, &AdminRole::Operator);

    let epoch_after = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    assert_eq!(
        epoch_baseline, epoch_after,
        "config epoch must not advance on rejected or no-op operations"
    );
}

// ── accept_ownership: timelock boundary is inclusive at exactly TIMELOCK ─────

/// Boundary: `accept_ownership` must succeed when the ledger is advanced to
/// *exactly* `proposed_at + OWNERSHIP_TRANSFER_TIMELOCK` (inclusive lower
/// bound).
///
/// The dual `test_accept_ownership_rejects_before_timelock_elapses` in
/// test_ownership_transfer.rs covers the `now < eligible_at` side; this pins
/// the `now == eligible_at` side — the precise moment the gate opens.
#[test]
fn accept_ownership_succeeds_at_exact_timelock_boundary() {
    let (env, contract, super_admin) = setup_env();
    let new_super = test_admin(&env);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &new_super,
        AdminRole::SuperAdmin,
    );

    let proposed_at = env.ledger().timestamp();
    env.as_contract(&contract, || {
        AdminContract::transfer_ownership(env.clone(), super_admin.clone(), new_super.clone());
    });

    // Advance to exactly the timelock boundary.
    advance(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    assert_eq!(env.ledger().timestamp(), proposed_at + crate::OWNERSHIP_TRANSFER_TIMELOCK);

    env.as_contract(&contract, || {
        AdminContract::accept_ownership(env.clone(), new_super.clone());
    });

    let owner = env.as_contract(&contract, || AdminContract::get_owner(env.clone()));
    assert_eq!(owner, new_super, "owner must be updated at the exact timelock boundary");

    // Pending owner slot must be cleared atomically.
    let pending = env.as_contract(&contract, || AdminContract::get_pending_owner(env.clone()));
    assert_eq!(pending, None, "pending owner must be cleared after acceptance");
}

// ── update_admin_role: promotion then demotion preserves state integrity ─────

/// Regression: promoting an admin and then demoting them back to their
/// original role must leave the role-list and stored role consistent.
/// Specifically, the admin must appear exactly once in each role list —
/// not duplicated in the new list or orphaned in the old list.
///
/// Threat: a role-list bug that fails to clean up the old-role entry on
/// update would cause the admin to appear in two role lists simultaneously,
/// making get_admins_by_role return an inflated count and violating the
/// single-role invariant.
#[test]
fn update_admin_role_round_trip_preserves_role_list_integrity() {
    let (env, contract, super_admin) = setup_env();
    let target = user(&env);
    add_admin(
        &env,
        &contract,
        &super_admin,
        &target,
        AdminRole::Operator,
    );

    // Promote to Admin.
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            target.clone(),
            AdminRole::Admin,
        );
    });

    // Demote back to Operator.
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            target.clone(),
            AdminRole::Operator,
        );
    });

    // Stored role must reflect the final value.
    let info = env.as_contract(&contract, || {
        AdminContract::get_admin_info(env.clone(), target.clone())
    });
    assert_eq!(info.role, AdminRole::Operator, "role must reflect last update");

    // target must appear exactly once in the Operator list and not at all in Admin.
    let op_list: soroban_sdk::Vec<Address> = env.as_contract(&contract, || {
        AdminContract::get_admins_by_role(env.clone(), AdminRole::Operator)
    });
    let admin_list: soroban_sdk::Vec<Address> = env.as_contract(&contract, || {
        AdminContract::get_admins_by_role(env.clone(), AdminRole::Admin)
    });

    let op_count = op_list.iter().filter(|a| a == target).count();
    let admin_count = admin_list.iter().filter(|a| a == target).count();

    assert_eq!(op_count, 1, "target must appear exactly once in Operator list");
    assert_eq!(admin_count, 0, "target must not appear in Admin list after demotion");
}

// ── suspend_admin: suspension expiry is auto-detected, no second tx needed ───

/// Boundary: an admin whose suspension window has passed must be treated as
/// fully effective again by `is_admin` and `has_role_at_least` without any
/// explicit reactivation call.
///
/// This is already covered by test_suspension.rs#test_auto_reactivation_after_expiry.
/// The version here exercises the *exact boundary second* (`suspended_until`)
/// rather than one second after, pinning the `>=` comparison used in
/// `is_admin` and `has_role_at_least`.
#[test]
fn suspend_admin_expiry_boundary_is_inclusive() {
    let (env, contract, super_admin) = setup_env();
    let admin = test_admin(&env);
    add_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let now = env.ledger().timestamp();
    let until_ts = now + 1_000;

    env.as_contract(&contract, || {
        AdminContract::suspend_admin(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            until_ts,
        );
    });

    // One second before expiry: still suspended.
    env.ledger().with_mut(|li| li.timestamp = until_ts - 1);
    assert_eq!(
        env.as_contract(&contract, || { AdminContract::is_admin(env.clone(), admin.clone()) }),
        credence_errors::Role::User,
        "admin must be inactive one second before expiry"
    );

    // At exactly suspended_until: auto-reactivated (>= comparison).
    env.ledger().with_mut(|li| li.timestamp = until_ts);
    assert_eq!(
        env.as_contract(&contract, || { AdminContract::is_admin(env.clone(), admin.clone()) }),
        credence_errors::Role::Admin,
        "admin must be active at exactly suspended_until (>= boundary)"
    );
}
