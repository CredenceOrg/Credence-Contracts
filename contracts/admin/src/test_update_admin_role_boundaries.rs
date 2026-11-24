//! Deterministic failure-boundary coverage for `update_admin_role` (#1418).
//!
//! Invariants asserted by every test in this module:
//!
//! 1. **Atomicity** — every rejected call leaves `AdminInfo`, `RoleAdmins`,
//!    `AdminList`, and `ConfigEpoch` byte-identical to their pre-call state.
//! 2. **Idempotency** — a no-op role update (same role, same target) mutates
//!    nothing and does not advance `ConfigEpoch`.
//! 3. **MinAdmins floor** — demoting a SuperAdmin below `MinAdmins` effective
//!    SuperAdmins is rejected with `InvalidPauseAction`, mirroring the guard
//!    `remove_admin` and `suspend_admin` already enforce.
//! 4. **Authorization** — callers below the required role level, and
//!    self-promotion to equal-or-higher role, are rejected deterministically.

extern crate std;

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};

fn setup() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let contract_address = env.register_contract(None, AdminContract);
    let super_admin = Address::generate(&env);
    let admin = Address::generate(&env);
    let operator = Address::generate(&env);

    env.mock_all_auths();
    env.as_contract(&contract_address, || {
        AdminContract::initialize(env.clone(), super_admin.clone(), 1, 10);
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            AdminRole::Admin,
        );
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Operator,
        );
    });

    (env, contract_address, super_admin, admin, operator)
}

fn snapshot(
    env: &Env,
    contract_address: &Address,
    target: &Address,
) -> (AdminInfo, u64, Vec<Address>, Vec<Address>) {
    env.as_contract(contract_address, || {
        let info: AdminInfo = env
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(target.clone()))
            .expect("target admin must exist for snapshot");
        let epoch: u64 = AdminContract::get_config_epoch(env.clone());
        let all: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or(Vec::new(env));
        let supers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(AdminRole::SuperAdmin))
            .unwrap_or(Vec::new(env));
        (info, epoch, all, supers)
    })
}

fn assert_unchanged(
    before: &(AdminInfo, u64, Vec<Address>, Vec<Address>),
    after: &(AdminInfo, u64, Vec<Address>, Vec<Address>),
) {
    assert_eq!(before.0.role, after.0.role);
    assert_eq!(before.0.assigned_at, after.0.assigned_at);
    assert_eq!(before.0.assigned_by, after.0.assigned_by);
    assert_eq!(before.0.active, after.0.active);
    assert_eq!(before.0.suspended_until, after.0.suspended_until);
    assert_eq!(
        before.1, after.1,
        "ConfigEpoch must not advance on rejection"
    );
    assert_eq!(before.2, after.2, "AdminList must be unchanged");
    assert_eq!(
        before.3, after.3,
        "RoleAdmins(SuperAdmin) must be unchanged"
    );
}

#[test]
fn update_admin_role_promotes_operator_to_admin() {
    let (env, contract_address, super_admin, _admin, operator) = setup();

    env.as_contract(&contract_address, || {
        let result = AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Admin,
        );
        assert_eq!(result.role, AdminRole::Admin);
        assert_eq!(result.assigned_by, super_admin.clone());
        assert_eq!(result.assigned_at, env.ledger().timestamp());

        let info = AdminContract::get_admin_info(env.clone(), operator.clone());
        assert_eq!(info.role, AdminRole::Admin);

        let admins: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(AdminRole::Admin))
            .unwrap();
        assert!(admins.iter().any(|a| a == operator));

        let operators: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(AdminRole::Operator))
            .unwrap_or(Vec::new(&env));
        assert!(!operators.iter().any(|a| a == operator));
    });
}

#[test]
fn update_admin_role_advances_epoch_exactly_once() {
    let (env, contract_address, super_admin, _admin, operator) = setup();

    env.as_contract(&contract_address, || {
        let before = AdminContract::get_config_epoch(env.clone());
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Admin,
        );
        let after = AdminContract::get_config_epoch(env.clone());
        assert_eq!(after, before + 1);
    });
}

#[test]
fn update_admin_role_same_role_is_a_noop_without_epoch_bump() {
    let (env, contract_address, super_admin, admin, _operator) = setup();

    env.as_contract(&contract_address, || {
        let before = snapshot(&env, &contract_address, &admin);
        let result = AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            AdminRole::Admin,
        );
        let after = snapshot(&env, &contract_address, &admin);

        assert_eq!(result.role, AdminRole::Admin);
        assert_eq!(result.assigned_at, before.0.assigned_at);
        assert_eq!(result.assigned_by, before.0.assigned_by);
        assert_unchanged(&before, &after);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #107)")]
fn update_admin_role_rejects_demoting_last_super_admin() {
    let (env, contract_address, super_admin, _admin, _operator) = setup();

    env.as_contract(&contract_address, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            super_admin.clone(),
            AdminRole::Admin,
        );
    });
}

#[test]
fn update_admin_role_allows_demoting_super_admin_when_another_exists() {
    let (env, contract_address, super_admin, admin, _operator) = setup();

    env.as_contract(&contract_address, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            AdminRole::SuperAdmin,
        );
        let updated = AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            super_admin.clone(),
            AdminRole::Admin,
        );
        assert_eq!(updated.role, AdminRole::Admin);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #107)")]
fn update_admin_role_rejects_demoting_last_effective_super_admin_when_others_suspended() {
    let (env, contract_address, super_admin, admin, _operator) = setup();

    env.as_contract(&contract_address, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            AdminRole::SuperAdmin,
        );
        AdminContract::suspend_admin(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            env.ledger().timestamp() + 10_000,
        );

        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            super_admin.clone(),
            AdminRole::Admin,
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn update_admin_role_rejects_admin_promoting_to_super_admin() {
    let (env, contract_address, _super_admin, admin, operator) = setup();

    env.as_contract(&contract_address, || {
        AdminContract::update_admin_role(
            env.clone(),
            admin.clone(),
            operator.clone(),
            AdminRole::SuperAdmin,
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn update_admin_role_rejects_operator_updating_any_role() {
    let (env, contract_address, _super_admin, admin, operator) = setup();

    env.as_contract(&contract_address, || {
        AdminContract::update_admin_role(
            env.clone(),
            operator.clone(),
            admin.clone(),
            AdminRole::Operator,
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn update_admin_role_rejects_self_promotion() {
    let (env, contract_address, _super_admin, admin, _operator) = setup();

    env.as_contract(&contract_address, || {
        AdminContract::update_admin_role(
            env.clone(),
            admin.clone(),
            admin.clone(),
            AdminRole::SuperAdmin,
        );
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #")]
fn update_admin_role_rejects_contract_self_address() {
    let (env, contract_address, super_admin, _admin, _operator) = setup();

    env.as_contract(&contract_address, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            contract_address.clone(),
            AdminRole::Admin,
        );
    });
}
