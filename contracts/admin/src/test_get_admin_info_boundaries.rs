//! Deterministic failure-boundary coverage for `AdminContract::get_admin_info` (#1427).
//!
//! `get_admin_info` is a pure read of `DataKey::AdminInfo(address)`. It either
//! returns a stable `AdminInfo` snapshot or panics with `ContractError::NotAdmin`
//! when no record exists. These tests pin the boundary between those two outcomes
//! across every state an admin can be in — unknown, removed, deactivated and
//! suspended — and prove the read is idempotent and side-effect free so a stale or
//! repeated call can never yield a different result.

use crate::*;
use soroban_sdk::{Address, Env};

#[cfg(test)]
mod get_admin_info_boundaries {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup_contract(env: &Env) -> (Address, Address) {
        let super_admin = Address::generate(env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);
        });

        (contract_address, super_admin)
    }

    fn setup_multiple_admins(env: &Env) -> (Address, Address, Address, Address) {
        let (contract_address, super_admin) = setup_contract(env);
        let admin = Address::generate(env);
        let operator = Address::generate(env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                admin.clone(),
                AdminRole::Admin,
            );
            AdminContract::add_admin(
                env.clone(),
                admin.clone(),
                operator.clone(),
                AdminRole::Operator,
            );
        });

        (contract_address, super_admin, admin, operator)
    }

    fn read_info(env: &Env, contract_address: &Address, who: &Address) -> AdminInfo {
        env.as_contract(contract_address, || {
            AdminContract::get_admin_info(env.clone(), who.clone())
        })
    }

    /// Success boundary: the snapshot returned matches exactly what was written.
    #[test]
    fn returns_exact_snapshot_for_an_active_admin() {
        let env = Env::default();
        let (contract_address, super_admin, admin, _operator) = setup_multiple_admins(&env);

        let info = read_info(&env, &contract_address, &admin);
        assert_eq!(info.address, admin);
        assert_eq!(info.role, AdminRole::Admin);
        assert_eq!(info.assigned_by, super_admin);
        assert!(info.active);
        assert_eq!(info.suspended_until, 0);
    }

    /// Failure boundary: no admin record exists on a never-initialized contract.
    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn panics_on_uninitialized_contract() {
        let env = Env::default();
        let contract_address = env.register_contract(None, AdminContract);
        let stranger = Address::generate(&env);

        let _ = read_info(&env, &contract_address, &stranger);
    }

    /// Failure boundary: an unknown address never leaks another admin's record,
    /// even while other admins are registered.
    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn panics_for_unknown_address_beside_existing_admins() {
        let env = Env::default();
        let (contract_address, _super_admin, _admin, _operator) = setup_multiple_admins(&env);
        let stranger = Address::generate(&env);

        let _ = read_info(&env, &contract_address, &stranger);
    }

    /// Failure boundary: once removed, the record is gone and the read panics.
    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn panics_after_the_admin_is_removed() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::remove_admin(env.clone(), admin.clone(), operator.clone());
        });

        let _ = read_info(&env, &contract_address, &operator);
    }

    /// A deactivated admin still has a record: the read succeeds with
    /// `active = false` and is stable across repeated calls (no caching/staleness).
    #[test]
    fn returns_stable_snapshot_for_deactivated_admin() {
        let env = Env::default();
        let (contract_address, super_admin, admin, _operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });

        let first = read_info(&env, &contract_address, &admin);
        let second = read_info(&env, &contract_address, &admin);

        assert!(!first.active);
        assert_eq!(first.role, AdminRole::Admin);
        assert_eq!(first.active, second.active);
        assert_eq!(first.assigned_at, second.assigned_at);
        assert_eq!(first.assigned_by, second.assigned_by);
        assert_eq!(first.suspended_until, second.suspended_until);
    }

    /// A suspended admin is still readable and the suspension window survives.
    #[test]
    fn returns_record_with_suspension_window() {
        let env = Env::default();
        let (contract_address, super_admin, admin, _operator) = setup_multiple_admins(&env);
        let until = env.ledger().timestamp() + 3_600;

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::suspend_admin(env.clone(), super_admin.clone(), admin.clone(), until);
        });

        let info = read_info(&env, &contract_address, &admin);
        assert_eq!(info.address, admin);
        assert_eq!(info.suspended_until, until);
    }

    /// Role changes are observed immediately — there is no stale cached read.
    #[test]
    fn reflects_role_change_without_stale_reads() {
        let env = Env::default();
        let (contract_address, super_admin, _admin, operator) = setup_multiple_admins(&env);

        assert_eq!(
            read_info(&env, &contract_address, &operator).role,
            AdminRole::Operator
        );

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::update_admin_role(
                env.clone(),
                super_admin.clone(),
                operator.clone(),
                AdminRole::Admin,
            );
        });

        assert_eq!(
            read_info(&env, &contract_address, &operator).role,
            AdminRole::Admin
        );
    }

    /// The read is idempotent and never mutates admin bookkeeping.
    #[test]
    fn is_read_only_and_idempotent() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, _operator) = setup_multiple_admins(&env);

        let before = env.as_contract(&contract_address, || {
            AdminContract::get_admin_count(env.clone())
        });
        let baseline = read_info(&env, &contract_address, &admin);

        for _ in 0..5 {
            let snapshot = read_info(&env, &contract_address, &admin);
            assert_eq!(snapshot.address, baseline.address);
            assert_eq!(snapshot.role, baseline.role);
            assert_eq!(snapshot.active, baseline.active);
            assert_eq!(snapshot.suspended_until, baseline.suspended_until);
        }

        let after = env.as_contract(&contract_address, || {
            AdminContract::get_admin_count(env.clone())
        });
        assert_eq!(before, after);
    }
}
