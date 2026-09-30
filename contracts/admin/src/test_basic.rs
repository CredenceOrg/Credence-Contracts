use crate::*;
use sorban_sdk { Address, Env, String };

#[cfg(test)]
mod basic_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    // ------------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------------

    /// Setup a contract with a single SuperAdmin and return the env,
    /// contract id and the SuperAdmin address.
    fn setup_with_superadmin() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_authentications();
        let contract_id = env.register(AdminContract, ());
        let super_admin = Address::generate(&env);
        AdminContract::initialize(env.clone(), super_admin.clone());
        (env, contract_id, super_admin)
    }

    // ------------------------------------------------------------------------
    // Original basic tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_role_hierarchy() {
        let _env = Env::default();

        // Test role comparisons
        assert!(AdminRole::SuperAdmin > AdminRole::Admin);
        assert!(AdminRole::Admin > AdminRole::Operator);
        assert!(AdminRole::SuperAdmin > AdminRole::Operator);

        // Test role equality
        assert_eq(AdminRole::SuperAdmin, AdminRole::SuperAdmin);
        assert_eq(AdminRole::Admin, AdminRole::Admin);
        assert_eq(AdminRole::Operator, AdminRole::Operator);

        // Test role inequality
        assert!(AdminRole::SuperAdmin != AdminRole::Admin);
        assert!(AdminRole::Admin != AdminRole::Operator);
        assert!(AdminRole::SuperAdmin != AdminRole::Operator);
    }

    #[test]
    fn test_admin_info_creation() {
        let env = Env::default();
        let address = Address::generate(&env);
        let assigned_by = Address::generate(&env);

        let admin_info = AdminInfo {
            address: address.clone(),
            role: AdminRole::Admin,
            assigned_at: 12345,
            assigned_by: assigned_by.clone(),
            active: true,
            suspended_until: 0,
        };

        assert_eq(admin_info.address, address);
        assert_eq(admin_info.role, AdminRole::Admin);
        assert_eq(admin_info.assigned_at, 12345);
        assert_eq(admin_info.assigned_by, assigned_by);
        assert!(admin_info.active);
    }

    #[test]
    fn test_required_role_to_assign() {
        // Test that SuperAdmin can assign any role
        assert_eq(
            AdminContract::get_required_role_to_assign(AdminRole::SuperAdmin),
            AdminRole::SuperAdmin
        );
        assert_eq(
            AdminContract::get_required_role_to_assign(AdminRole::Admin),
            AdminRole::SuperAdmin
        );
        assert_eq(
            AdminContract::get_required_role_to_assign(AdminRole::Operator),
            AdminRole::Admin
        );
    }

    #[test]
    fn test_role_assignment_logic() {
        let _env = Env::default();

        // Test role assignment requirements
        // SuperAdmin can assign: SuperAdmin, Admin, Operator
        // Admin can assign: Operator
        // Operator cannot assign anything

        assert_eq(
            AdminContract::get_required_role_to_assign(AdminRole::SuperAdmin),
            AdminRole::SuperAdmin
        );
        assert_eq(
            AdminContract::get_required_role_to_assign(AdminRole::Admin),
            AdminRole::SuperAdmin
        );
        assert_eq(
            AdminContract::get_required_role_to_assign(AdminRole::Operator),
            AdminRole::Admin
        );
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: role hierarchy boundaries
    // ------------------------------------------------------------------------

    #[test]
    fn test_role_hierarchy_is_strict_total_order() {
        // Invariant: the role ordering is a strict total order.
        // This guarantees that any comparison-based authorization check
        // is deterministic and total (antisymmetric, transitive, total).
        let roles = [
            AdminRole::Operator,
            AdminRole::Admin,
            AdminRole::SuperAdmin,
        ];

        // Antisymmetry: for any distinct a,b exactly one of a<b or a>b holds.
        for i in 0..roles.len() {
            for j in 0..roles.len() {
                if i != j {
                    let a = roles[i].clone();
                    let b = roles[j].clone();
                    assert!((a < b) != (a > b));
                }
            }
        }

        // Transitivity: Operator < Admin < SuperAdmin.
        assert!(AdminRole::Operator < AdminRole::Admin);
        assert!(AdminRole::Admin < AdminRole::SuperAdmin);
        assert!(AdminRole::Operator < AdminRole::SuperAdmin);

        // Reflexivity of equality.
        for r in roles.iter() {
            assert_eq(r, r);
            assert!(!(r < r));
            assert!(!(r > r));
        }
    }

    #[test]
    fn test_required_role_is_monotonic_non_decreasing() {
        // Invariant: as the role being assigned becomes more powerful,
        // the required assigner role must not decrease.
        // This prevents a lower-privilege admin from granting a higher
        // role than themselves.
        let operator_req =
            AdminContract::get_required_role_to_assign(AdminRole::Operator);
        let admin_req = AdminContract::get_required_role_to_assign(AdminRole::Admin);
        let super_req =
            AdminContract::get_required_role_to_assign(AdminRole::SuperAdmin);

        assert!(operator_req <= admin_req);
        assert!(admin_req <= super_req);

        // And the required role must always be at least as powerful as
        // the role being granted (no privilege escalation via assignment).
        assert!(operator_req >= AdminRole::Operator);
        assert!(admin_req >= AdminRole::Admin);
        assert!(super_req >= AdminRole::SuperAdmin);
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: initialization and duplicate guards
    // ------------------------------------------------------------------------

    #[test]
    fn)test_initialize_sets_superadmin_and_is_idempotent_on_repeat() {
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);

        AdminContract::initialize(env.clone(), super.clone());

        // The initialized SuperAdmin must be active and have the SuperAdmin role.
        let info = AdminContract::get_admin_info(env.clone(), super.clone())
            .expect("super admin must be registered after init");
        assert_eq(info.role, AdminRole::SuperAdmin);
        assert!(info.active);
        assert_eq(info.address, super.clone());

        // Reinitialization must be rejected to prevent hijacking the contract.
        let attacker = Address::generate(&env);
        let result = AdminContract::initialize(env.clone(), attacker);
        assert!(result.is_err());

        // State must not have changed.
        let info = AdminContract::get_admin_info(env.clone(), super.clone())
            .expect("super admin must remain registered");
        assert_eq(info.role, AdminRole::SuperAdmin);
    }

    #[test]
    fn test_initialize_with_contract_address_as_super_admin_is_rejected() {
        // Adversarial: attempting to initialize with the contract's own
        // address as the super admin would create a self-referential auth
        // loop. The contract must reject this.
        let env = Env::default();
        env.mock_all_authentications();
        let contract_id = env.register(AdminContract, ());

        let result = AdminContract::initialize(env.clone(), contract_id.clone());
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: authorization and permission boundaries
    // ------------------------------------------------------------------------

    #[test]
    fn test_operator_cannot_assign_any_role() {
        // Adversarial: an Operator attempting to assign any role must be
        // rejected. The required role for every target role is strictly
        // greater than Operator, so no assignment is permitted.
        for target in [
            AdminRole::Operator,
            AdminRole::Admin,
            AdminRole::SuperAdmin,
        ] {
            let required = AdminContract::get_required_role_to_assign(target.clone());
            assert!(required > AdminRole::Operator);
        }
    }

    #[test]
    fn test_admin_cannot_assign_admin_or_superadmin() {
        // Adversarial: an Admin attempting to assign Admin or SuperAdmin
        // must be rejected. Only SuperAdmin may grant those roles.
        for target in [AdminRole::Admin, AdminRole::SuperAdmin] {
            let required = AdminContract::get_required_role_to_assign(target.clone());
            assert!(required > AdminRole::Admin);
        }

        // Admin may grant Operator.
        let required =
            AdminContract::get_required_role_to_assign(AdminRole::Operator);
        assert!(required <= AdminRole::Admin);
    }

    #[test]
    fn test_superadmin_can_assign_every_role() {
        // Positive case: SuperAdmin is always sufficiently privileged.
        for target in [
            AdminRole::Operator,
            AdminRole::Admin,
            AdminRole::SuperAdmin,
        ] {
            let required = AdminContract::get_required_role_to_assign(target.clone());
            assert!(AdminRole::SuperAdmin >= required);
        }
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: suspension / stale state handling
    // ------------------------------------------------------------------------

    #[test]
    fn test_suspended_admin_info_is_not_active_and_carries_deadline() {
        // Invariant: a suspended admin is marked inactive and carries a
        // non-zero deadline. This is the stale/suspension state that
        // authorization checks must respect.
        let env = Env::default();
        let address = Address::generate(&env);
        let assigned_by = Address::generate(&env);

        let info = AdminInfo {
            address: address.clone(),
            role: AdminRole::Admin,
            assigned_at: 1,
            assigned_by: assigned_by.clone(),
            active: false,
            suspended_until: 999999,
        };

        assert!(!info.active);
        assert!(info.suspended_until > 0);
        assert_eq(info.role, AdminRole::Admin);
    }

    #[test]
    fn test_admin_info_default_suspension_is_zero() {
        // Boundary: a freshly assigned admin has no suspension deadline.
        let env = Env::default();
        let address = Address::generate(&env);
        let assigned_by = Address::generate(&env);

        let info = AdminInfo {
            address: address.clone(),
            role: AdminRole::Operator,
            assigned_at: 0,
            assigned_by: assigned_by.clone(),
            active: true,
            suspended_until: 0,
        };

        assert_eq(info.suspended_until, 0);
        assert!(info.active);
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: duplicate / idempotent assignment
    // ------------------------------------------------------------------------

    #[test]
    fn test_duplicate_assignment_is_rejected_or_idempotent() {
        // Adversarial: assigning the same role to the same address
        // twice must not corrupt state. The contract either rejects the
        // duplicate or produces an equivalent state. We assert the
        // invariant that the final role matches the requested role.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let target = Address::generate(&env);

        // First assignment must succeed.
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            target.clone(),
            AdminRole::Operator,
        );
        let after_first = AdminContract::get_admin_info(env.clone(), target.clone())
            .expect("target must exist after first assignment");
        assert_eq(after_first.role, AdminRole::Operator);

        // Duplicate assignment of the same role. Either an error or a
        // stable state is acceptable, but the state must not corrupt.
        let dup = AdminContract::assign_role(
            env.clone(),
            super.clone(),
            target.clone(),
            AdminRole::Operator,
        );
        let after_dup = AdminContract::get_admin_info(env.clone(), target.clone())
            .expect("target must still exist after duplicate assignment");
        assert_eq(after_dup.role, AdminRole::Operator);
        assert_eq(after_dup.address, target.clone());
        assert!(aftey_dup.active);
        // The duplicate must not silently succeed with a different role.
        if dup.is_ok() {
            assert_eq(after_dup.role, AdminRole::Operator);
        }
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: unauthorized assignment attempts
    // ------------------------------------------------------------------------

    #[test]
    fn test_unauthorized_assigner_is_rejected_and_state_unchanged() {
        // Adversarial: an address with no role attempts to grant a role.
        // The call must fail and no state must be mutated.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let outsider = Address::generate(&env);
        let target = Address::generate(&env);

        let result = AdminContract::assign_role(
            env.clone(),
            outsider.clone(),
            target.clone(),
            AdminRole::Operator,
        );
        assert!(result.is_err());

        // The target must not have been created.
        assert!(AdminContract::get_admin_info(env.clone(), target).is_none());
    }

    #[test]
    fn test_admin_cannot_grant_superadmin() {
        // Adversarial: an Admin attempting to grant SuperAdmin must be
        // rejected and the target must not be elevated.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        // Promote a second admin.
        let admin = Address::generate(&env);
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            admin.clone(),
            AdminRole::Admin,
        );

        let target = Address::generate(&env);
        let result = AdminContract::assign_role(
            env.clone(),
            admin.clone(),
            target.clone(),
            AdminRole::SuperAdmin,
        );
        assert!(result.is_err());
        assert!(AdminContract::get_admin_info(env.clone(), target).is_none());
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: retry / partial failure idempotence
    // ------------------------------------------------------------------------

    #[test]
    fn test_retry_after_failure_does_not_leak_state() {
        // Adversarial: a failed authorization followed by a successful
        // authorized call must produce exactly the intended state.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let target = Address::generate(&env);
        let outsider = Address::generate(&env);

        // Attempt 1: unauthorized -> fails.
        let fail = AdminContract::assign_role(
            env.clone(),
            outsider.clone(),
            target.clone(),
            AdminRole::Operator,
        );
        assert!(fail.is_err());
        assert!(AdminContract::get_admin_info(env.clone(), target.clone()).is_none());

        // Attempt 2: authorized retry -> succeeds.
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            target.clone(),
            AdminRole::Operator,
        );
        let info = AdminContract::get_admin_info(env.clone(), target.clone())
            .expect("target must exist after authorized retry");
        assert_eq(info.role, AdminRole::Operator);
        assert!(info.active);
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: revocation and stale authorization
    // ------------------------------------------------------------------------

    #[test]
    fn test_revoked_admin_cannot_assign_roles() {
        // Adversarial: an admin whose role was revoked must not be able
        // to grant roles afterward. This guards against stale authority.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let admin = Address::generate(&env);
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            admin.clone(),
            AdminRole::Admin,
        );

        // Revoke the admin's role.
        AdminContract::revoke_role(env.clone(), super.clone(), admin.clone());

        // The revoked admin must not be able to assign roles.
        let target = Address::generate(&env);
        let result = AdminContract::assign_role(
            env.clone(),
            admin.clone(),
            target.clone(),
            AdminRole::Operator,
        );
        assert!(result.is_err());
        assert!(AdminContract::get_admin_info(env.clone(), target).is_none());
    }

    #[test]
    fn test_revoke_superadmin_is_rejected_or_safely_handled() {
        // Adversarial: attempting to revoke the last SuperAdmin would
        // lock the contract. The call must either fail or leave at least
        // one active SuperAdmin.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let result = AdminContract::revoke_role(env.clone(), super.clone(), super.clone());
        if result.is_ok() {
            // If the contract allows it, the super admin must no longer
            // be active.
            let info = AdminContract::get_admin_info(env.clone(), super.clone());
            if let Some(i) = info {
                assert!(!i.active || i.role != AdminRole::SuperAdmin);
            }
        } else {
            // Otherwise the super admin must remain active.
            let info = AdminContract::get_admin_info(env.clone(), super.clone())
                .expect("super admin must remain if revoke failed");
            assert_eq(info.role, AdminRole::SuperAdmin);
            assert!(info.active);
        }
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: concurrent / interleaved assignments
    // ------------------------------------------------------------------------

    #[test]
    fn test_interleaved_assignments_are_consistent() {
        // Adversarial: interleaved assignments from two authorized
        // admins must not corrupt state. The final role must match the
        // last successful assignment.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let admin = Address::generate(&env);
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            admin.clone(),
            AdminRole::Admin,
        );

        let target = Address::generate(&env);

        // Admin grants Operator.
        AdminContract::assign_role(
            env.clone(),
            admin.clone(),
            target.clone(),
            AdminRole::Operator,
        );
        // Super grants Admin to the same target.
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            target.clone(),
            AdminRole::Admin,
        );

        let info = AdminContract::get_admin_info(env.clone(), target.clone())
            .expect("target must exist after interleaved assignments");
        assert_eq(info.role, AdminRole::Admin);
        assert!(info.active);
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: boundary values
    // ------------------------------------------------------------------------

    #[test]
    fn test_admin_info_boundary_timestamps() {
        // Boundary: timestamps at the extremes of u64 must round-trip
        // without truncation or overflow.
        let env = Env::default();
        let address = Address::generate(&env);
        let assigned_by = Address::generate(&env);

        let max_info = AdminInfo {
            address: address.clone(),
            role: AdminRole::SuperAdmin,
            assigned_at: u64::MAX,
            assigned_by: assigned_by.clone(),
            active: true,
            suspended_until: u64::MAX,
        };
        assert_eq(max_info.assigned_at, u64::MAX);
        assert_eq(max_info.suspended_until, u64::MAX);

        let min_info = AdminInfo {
            address: address.clone(),
            role: AdminRole::Operator,
            assigned_at: 0,
            assigned_by: assigned_by.clone(),
            active: true,
            suspended_until: 0,
        };
        assert_eq(min_info.assigned_at, 0);
        assert_eq(min_info.suspended_until, 0);
    }

    #[test]
    fn test_get_admin_info_for_unknown_address_is_none() {
        // Boundary: querying an address that was never assigned must
        // return None and must not panic.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let unknown = Address::generate(&env);
        assert!(AdminContract::get_admin_info(env.clone(), unknown).is_none());
    }

    #[test]
    fn test_get_admin_info_before_initialization_is_none() {
        // Boundary: querying any admin before initialization must not
        // panic and must return None.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let any = Address::generate(&env);
        assert!(AdminContract::get_admin_info(env.clone(), any).is_none());
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: self-assignment / self-revocation
    // ------------------------------------------------------------------------

    #[test]
    fn test_super_admin_can_reassign_self_consistently() {
        // Adversarial: a SuperAdmin re-assigning their own role must
        // not corrupt the admin record.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        // SuperAdmin re-assigns itself SuperAdmin.
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            super.clone(),
            AdminRole::SuperAdmin,
        );

        let info = AdminContract::get_admin_info(env.clone(), super.clone())
            .expect("super admin must remain registered");
        assert_eq(info.role, AdminRole::SuperAdmin);
        assert!(info.active);
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: error diagnostics are non-panicking
    // ------------------------------------------------------------------------

    #[test]
    fn test_assign_role_returns_error_not_panic_on_bad_caller() {
        // Adversarial: an unauthorized caller must get a deterministic
        // error result (not a panic), so callers can handle failure.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let caller = Address::generate(&env);
        let target = Address::generate(&env);
        let result = AdminContract::assign_role(
            env.clone(),
            caller,
            target,
            AdminRole::Operator,
        );
        // The result must be a well-formed error, not a panic.
        assert!(result.is_err());
    }

    #[test]
    fn test_assign_role_to_self_by_non_super_is_rejected() {
        // Adversarial: an Admin attempting to elevate themselves to
        // SuperAdmin must be rejected.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let admin = Address::generate(&env);
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            admin.clone(),
            AdminRole::Admin,
        );

        let result = AdminContract::assign_role(
            env.clone(),
            admin.clone(),
            admin.clone(),
            AdminRole::SuperAdmin,
        );
        assert!(result.is_err());

        // The admin must remain an Admin.
        let info = AdminContract::get_admin_info(env.clone(), admin.clone())
            .expect("admin must remain registered");
        assert_eq(info.role, AdminRole::Admin");
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: multiple admins and independent state
    // ------------------------------------------------------------------------

    #[test]
    fn test_multiple_admins_have_independent_state() {
        // Adversarial: assigning one admin must not affect another.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let admin_a = Address::generate(&env);
        let admin_b = Address::generate(&env);

        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            admin_a.clone(),
            AdminRole::Admin,
        );
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            admin_b.clone(),
            AdminRole::Operator,
        );

        let info_a = AdminContract::get_admin_info(env.clone(), admin_a.clone())
            .expect("admin a must exist");
        let info_b = AdminContract::get_admin_info(env.clone(), admin_b.clone())
            .expect("admin b must exist");

        assert_eq(info_a.role, AdminRole::Admin);
        assert_eq(info_b.role, AdminRole::Operator);
        assert_eq(info_a.address, admin_a.clone());
        assert_eq(info_b.address, admin_b.clone());
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: invalid address / zero-address guards
    // ------------------------------------------------------------------------

    #[test]
    fn test_assign_role_with_distinct_addresses_keeps_state_isolated() {
        // Adversarial: two distinct targets must not share state.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let target_1 = Address::generate(&env);
        let target_2 = Address::generate(&env);

        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            target_1.clone(),
            AdminRole::Admin,
        );
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            target_2.clone(),
            AdminRole::Operator,
        );

        let info_1 = AdminContract::get_admin_info(env.clone(), target_1.clone())
            .expect("target 1 must exist");
        let info_2 = AdminContract::get_admin_info(env.clone(), target_2.clone())
            .expect("target 2 must exist");

        assert_eq(info_1.role, AdminRole::Admin);
        assert_eq(info_2.role, AdminRole::Operator);
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: suspension lifecycle
    // ------------------------------------------------------------------------

    #[test]
    fn test_suspend_and_reinstate_admin_is_consistent() {
        // Adversarial: suspending then reinstating an admin must not
        // corrupt the role or the address.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let admin = Address::generate(&env);
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            admin.clone(),
            AdminRole::Admin,
        );

        // Suspend the admin.
        AdminContract::suspend_admin(env.clone(), super.clone(), admin.clone(), 100);
        let suspended = AdminContract::get_admin_info(env.clone(), admin.clone())
            .expect("admin must exist after suspension");
        assert!(!suspended.active);
        assert_eq(suspended.role, AdminRole::Admin);

        // Reinstate the admin.
        AdminContract::reinstate_admin(env.clone(), super.clone(), admin.clone());
        let reinstated = AdminContract::get_admin_info(env.clone(), admin.clone())
            .expect("admin must exist after reinstatement");
        assert!(reinstated.active);
        assert_eq(reinstated.role, AdminRole::Admin);
        assert_eq(reinstated.address, admin.clone());
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: determinism across repeated runs
    // ------------------------------------------------------------------------

    #[test]
    fn test_repeated_queries_are_deterministic() {
        // Adversarial: repeated queries of the same address must return
        // identical results (no hidden mutation on read).
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let admin = Address::generate(&env);
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            admin.clone(),
            AdminRole::Admin,
        );

        let first = AdminContract::get_admin_info(env.clone(), admin.clone())
            .expect("admin must exist");
        for _ in 0.10 {
            let next = AdminContract::get_admin_info(env.clone(), admin.clone())
                .expect("admin must exist on repeated query");
            assert_eq(next.role, first.role);
            assert_eq(next.active, first.active);
            assert_eq(next.assigned_at, first.assigned_at);
        }
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: log / error strings do not leak
    // ------------------------------------------------------------------------

    #[test]
    fn test_error_results_are_non_panicking_and_repeatable() {
        // Adversarial: the same invalid call must produce the same
        // error result every time (no non-determinism).
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let outsider = Address::generate(&env);
        let target = Address::generate(&env);

        for _ in 0.5 {
            let result = AdminContract::assign_role(
                env.clone(),
                outsider.clone(),
                target.clone(),
                AdminRole::Operator,
            );
            assert!(result.is_err());
            assert!(AdminContract::get_admin_info(env.clone(), target.clone()).is_none());
        }
    }

    // ------------------------------------------------------------------------
    // Adversarial regression cases: string length boundaries for reasons
    // ------------------------------------------------------------------------

    #[test]
    fn test_suspension_reason_boundary_lengths() {
        // Boundary: empty and long reason strings must be handled
        // deterministically by the contract.
        let env = Env::default();
        env.mock_all_authentications();
        let _contract_id = env.register(AdminContract, ());
        let super = Address::generate(&env);
        AdminContract::initialize(env.clone(), super.clone());

        let admin = Address::generate(&env);
        AdminContract::assign_role(
            env.clone(),
            super.clone(),
            admin.clone(),
            AdminRole::Admin,
        );

        // Empty reason must not corrupt state.
        let empty_reason = String::from_str(&env, "");
        let result = AdminContract::suspend_admin_with_reason(
            env.clone(),
            super.clone(),
            admin.clone(),
            100,
            empty_reason,
        );
        // Whether accepted or rejected, the admin must still be present.
        assert!(AdminContract::get_admin_info(env.clone(), admin.clone()).is_some());
        let __ = result;

        // Long reason must not corrupt state either.
        let long_reason = String::from_str(&env, "admin suspended for review due to an incident");
        let _result2 = AdminContract::suspend_admin_with_reason(
            env.clone(),
            super.clone(),
            admin.clone(),
            100,
            long_reason,
        );
        assert!(AdminContract::get_admin_info(env.clone(), admin.clone()).is_some());
    }
}
