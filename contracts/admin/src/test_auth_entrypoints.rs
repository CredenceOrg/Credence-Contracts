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
use soroban_sdk::{testutils::Address as _, Address, Env};
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
    add_admin(&env, &contract, &super_admin, &operator, AdminRole::Operator);

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
        AdminContract::deactivate_admin(env.clone(), operator.clone(), admin.clone());
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
    let admin_role = env.as_contract(&contract, || AdminContract::is_admin(env.clone(), admin.clone()));
    assert_eq!(admin_role, Role::User, "suspended admin must not be active before expiry");
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
    add_admin(&env, &contract, &super_admin, &new_super, AdminRole::SuperAdmin);

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
    add_admin(&env, &contract, &super_admin, &new_super, AdminRole::SuperAdmin);

    env.as_contract(&contract, || {
        AdminContract::transfer_ownership(env.clone(), super_admin.clone(), new_super.clone());
    });
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
    add_admin(&env, &contract, &super_admin, &new_super, AdminRole::SuperAdmin);

    env.as_contract(&contract, || {
        AdminContract::transfer_ownership(env.clone(), super_admin.clone(), new_super.clone());
    });
    env.as_contract(&contract, || {
        // stranger is not the pending owner.
        AdminContract::accept_ownership(env.clone(), stranger.clone());
    });
}
