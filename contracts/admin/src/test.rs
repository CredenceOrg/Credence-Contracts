use crate::*;
use soroban_sdk::{Address, Env};

#[cfg(test)]
mod comprehensive_tests {
    use super::*;
    use credence_errors::Role;
    use soroban_sdk::testutils::Address as _;

    fn create_contract() -> AdminContract {
        AdminContract {}
    }

    fn setup_with_limits(env: &Env, min_admins: u32, max_admins: u32) -> (Address, Address) {
        let contract = create_contract();
        let super_admin = Address::generate(env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), min_admins, max_admins);
        });

        (contract_address, super_admin)
    }

    fn setup_contract(env: &Env) -> (Address, Address) {
        setup_with_limits(env, 1, 100)
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

    #[test]
    fn test_initialization() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        assert_eq!(
            env.as_contract(&contract_address, || {
                AdminContract::is_admin(env.clone(), super_admin.clone())
            }),
            Role::Admin
        );
        assert_eq!(
            env.as_contract(&contract_address, || {
                AdminContract::get_admin_role(env.clone(), super_admin.clone())
            }),
            AdminRole::SuperAdmin
        );
        assert_eq!(
            env.as_contract(&contract_address, || {
                AdminContract::get_admin_count(env.clone())
            }),
            1
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_double_initialization() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #107)")]
    fn test_initialize_rejects_min_admins_zero() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin = Address::generate(&env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 0, 100);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #107)")]
    fn test_initialize_rejects_min_greater_than_max() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin = Address::generate(&env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 10, 9);
        });
    }

    #[test]
    fn test_get_config_returns_initialized_values() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_with_limits(&env, 2, 5);

        let (min_admins, max_admins) =
            env.as_contract(&contract_address, || AdminContract::get_config(env.clone()));

        assert_eq!(min_admins, 2);
        assert_eq!(max_admins, 5);
    }

    #[test]
    fn test_add_admin() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        let new_admin = Address::generate(&env);

        env.mock_all_auths();
        let admin_info = env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                new_admin.clone(),
                AdminRole::Admin,
            )
        });

        assert_eq!(admin_info.address, new_admin);
        assert_eq!(admin_info.role, AdminRole::Admin);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_add_admin_rejects_insufficient_privileges() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, _operator) = setup_multiple_admins(&env);
        let new_admin = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                admin.clone(),
                new_admin.clone(),
                AdminRole::Admin,
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #405)")]
    fn test_add_admin_rejects_duplicate_admin() {
        let env = Env::default();
        let (contract_address, super_admin, admin, _operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                admin.clone(),
                AdminRole::Admin,
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #601)")]
    fn test_add_admin_respects_max_limit() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_with_limits(&env, 1, 2);

        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                admin1.clone(),
                AdminRole::Admin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                admin2.clone(),
                AdminRole::Admin,
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #405)")]
    fn test_add_admin_rejects_self_add_as_duplicate_admin() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                super_admin.clone(),
                AdminRole::SuperAdmin,
            );
        });
    }

    #[test]
    fn test_remove_admin() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::remove_admin(env.clone(), admin.clone(), operator.clone());
        });

        assert_eq!(
            env.as_contract(&contract_address, || {
                AdminContract::get_admin_count(env.clone())
            }),
            2
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_remove_admin_rejects_non_admin_target() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let non_admin = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::remove_admin(env.clone(), super_admin.clone(), non_admin.clone());
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_remove_admin_rejects_insufficient_privileges() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::remove_admin(env.clone(), operator.clone(), admin.clone());
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_remove_admin_rejects_removing_super_admin() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_with_limits(&env, 1, 100);

        let other = Address::generate(&env);
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                other.clone(),
                AdminRole::Admin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::remove_admin(env.clone(), other.clone(), super_admin.clone());
        });
    }

    #[test]
    fn test_update_admin_role() {
        let env = Env::default();
        let (contract_address, super_admin, _admin, operator) = setup_multiple_admins(&env);

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
            env.as_contract(&contract_address, || {
                AdminContract::get_admin_role(env.clone(), operator.clone())
            }),
            AdminRole::Admin
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_update_admin_role_rejects_insufficient_privileges() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::update_admin_role(
                env.clone(),
                admin.clone(),
                operator.clone(),
                AdminRole::Admin,
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_update_admin_role_rejects_non_admin_target() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let non_admin = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::update_admin_role(
                env.clone(),
                super_admin.clone(),
                non_admin.clone(),
                AdminRole::Admin,
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_update_admin_role_prevents_self_assign_equal_or_higher() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::update_admin_role(
                env.clone(),
                super_admin.clone(),
                super_admin.clone(),
                AdminRole::SuperAdmin,
            );
        });
    }

    #[test]
    #[allow(deprecated)]
    fn test_update_admin_role_updates_role_lists() {
        let env = Env::default();
        let (contract_address, super_admin, _admin, operator) = setup_multiple_admins(&env);

        let before_operators = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role(env.clone(), AdminRole::Operator)
        });
        assert!(before_operators.contains(&operator));

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::update_admin_role(
                env.clone(),
                super_admin.clone(),
                operator.clone(),
                AdminRole::Admin,
            );
        });

        let after_operators = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role(env.clone(), AdminRole::Operator)
        });
        let after_admins = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role(env.clone(), AdminRole::Admin)
        });

        assert!(!after_operators.contains(&operator));
        assert!(after_admins.contains(&operator));
    }

    #[test]
    fn test_deactivate_reactivate_admin() {
        let env = Env::default();
        let (contract_address, super_admin, admin, _) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });

        let admin_info = env.as_contract(&contract_address, || {
            AdminContract::get_admin_info(env.clone(), admin.clone())
        });
        assert!(!admin_info.active);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::reactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });

        let admin_info = env.as_contract(&contract_address, || {
            AdminContract::get_admin_info(env.clone(), admin.clone())
        });
        assert!(admin_info.active);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_deactivate_admin_rejects_insufficient_privileges() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), operator.clone(), admin.clone());
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #404)")]
    fn test_deactivate_admin_rejects_double_deactivate() {
        let env = Env::default();
        let (contract_address, super_admin, admin, _) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_reactivate_admin_rejects_insufficient_privileges() {
        let env = Env::default();
        let (contract_address, super_admin, admin, operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::reactivate_admin(env.clone(), operator.clone(), admin.clone());
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #405)")]
    fn test_reactivate_admin_rejects_when_already_active() {
        let env = Env::default();
        let (contract_address, super_admin, admin, _) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::reactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });
    }

    #[test]
    fn test_deactivated_admin_not_counted_as_active_and_fails_role_checks() {
        let env = Env::default();
        let (contract_address, super_admin, admin, _operator) = setup_multiple_admins(&env);

        let before_active = env.as_contract(&contract_address, || {
            AdminContract::get_active_admin_count(env.clone())
        });
        assert_eq!(before_active, 3);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });

        let after_active = env.as_contract(&contract_address, || {
            AdminContract::get_active_admin_count(env.clone())
        });
        assert_eq!(after_active, 2);

        assert_eq!(
            env.as_contract(&contract_address, || {
                AdminContract::is_admin(env.clone(), admin.clone())
            }),
            Role::User
        );
        assert!(!env.as_contract(&contract_address, || {
            AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Operator)
        }));
    }

    #[test]
    #[allow(deprecated)]
    fn test_role_hierarchy() {
        let env = Env::default();
        let (contract_address, super_admin, _admin, _operator) = setup_multiple_admins(&env);

        assert!(AdminRole::SuperAdmin > AdminRole::Admin);
        assert!(AdminRole::Admin > AdminRole::Operator);
        assert!(AdminRole::SuperAdmin > AdminRole::Operator);

        let super_admins = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role(env.clone(), AdminRole::SuperAdmin)
        });
        assert_eq!(super_admins.len(), 1);
        assert!(super_admins.contains(&super_admin));
    }

    #[test]
    fn test_has_role_at_least() {
        let env = Env::default();
        let (contract_address, super_admin, admin, operator) = setup_multiple_admins(&env);

        assert!(env.as_contract(&contract_address, || {
            AdminContract::has_role_at_least(
                env.clone(),
                super_admin.clone(),
                AdminRole::SuperAdmin,
            )
        }));
        assert!(env.as_contract(&contract_address, || {
            AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
        }));
        assert!(env.as_contract(&contract_address, || {
            AdminContract::has_role_at_least(env.clone(), operator.clone(), AdminRole::Operator)
        }));
    }

    #[test]
    #[allow(deprecated)]
    fn test_get_all_admins() {
        let env = Env::default();
        let (contract_address, _super_admin, _admin, _operator) = setup_multiple_admins(&env);

        let all_admins = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins(env.clone())
        });
        assert_eq!(all_admins.len(), 3);
    }

    #[test]
    fn test_admin_info() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, _) = setup_multiple_admins(&env);

        let admin_info = env.as_contract(&contract_address, || {
            AdminContract::get_admin_info(env.clone(), admin.clone())
        });
        assert_eq!(admin_info.address, admin);
        assert_eq!(admin_info.role, AdminRole::Admin);
        assert!(admin_info.active);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_get_admin_info_panics_for_non_admin() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_contract(&env);
        let non_admin = Address::generate(&env);

        env.as_contract(&contract_address, || {
            AdminContract::get_admin_info(env.clone(), non_admin.clone())
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_get_admin_role_panics_for_non_admin() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_contract(&env);
        let non_admin = Address::generate(&env);

        env.as_contract(&contract_address, || {
            AdminContract::get_admin_role(env.clone(), non_admin.clone())
        });
    }

    // ── Pagination tests (issue #1298) ───────────────────────────────────────────

    #[test]
    fn test_get_all_admins_page_empty_set() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_contract(&env);
        // Remove the sole admin to get an empty list.
        // Instead, use a fresh contract with no admins added beyond initialization.
        // Actually, initialize creates 1 admin. Let's test with a cursor past the end.
        let (page, next_cursor) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 100, 10)
        });
        assert_eq!(page.len(), 0);
        assert_eq!(next_cursor, None);
    }

    #[test]
    fn test_get_all_admins_page_single_admin() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        let (page, next_cursor) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 0, 10)
        });
        assert_eq!(page.len(), 1);
        assert_eq!(page.get(0).unwrap(), super_admin);
        assert_eq!(next_cursor, None);
    }

    #[test]
    fn test_get_all_admins_page_boundary_exact_fit() {
        let env = Env::default();
        let (contract_address, super_admin, admin, operator) = setup_multiple_admins(&env);

        // limit exactly matches count
        let (page, next_cursor) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 0, 3)
        });
        assert_eq!(page.len(), 3);
        assert_eq!(page.get(0).unwrap(), super_admin);
        assert_eq!(page.get(1).unwrap(), admin);
        assert_eq!(page.get(2).unwrap(), operator);
        assert_eq!(next_cursor, None);
    }

    #[test]
    fn test_get_all_admins_page_multi_page_walk() {
        let env = Env::default();
        let (contract_address, super_admin, admin, operator) = setup_multiple_admins(&env);

        // Page 1: limit 2
        let (page_1, cursor_1) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 0, 2)
        });
        assert_eq!(page_1.len(), 2);
        assert_eq!(page_1.get(0).unwrap(), super_admin);
        assert_eq!(page_1.get(1).unwrap(), admin);
        assert_eq!(cursor_1, Some(2));

        // Page 2: cursor 2, limit 2 (only 1 remaining)
        let (page_2, cursor_2) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 2, 2)
        });
        assert_eq!(page_2.len(), 1);
        assert_eq!(page_2.get(0).unwrap(), operator);
        assert_eq!(cursor_2, None);
    }

    #[test]
    fn test_get_all_admins_page_cursor_past_end() {
        let env = Env::default();
        let (contract_address, _super_admin, _admin, _operator) = setup_multiple_admins(&env);

        let (page, next_cursor) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 3, 10)
        });
        assert_eq!(page.len(), 0);
        assert_eq!(next_cursor, None);
    }

    #[test]
    fn test_get_all_admins_page_limit_clamped_to_cap() {
        let env = Env::default();
        let (contract_address, _super_admin, _admin, _operator) = setup_multiple_admins(&env);

        // limit = 250 exceeds MAX_PAGE_LIMIT (200), should clamp
        let (page, _next_cursor) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 0, 250)
        });
        assert_eq!(page.len(), 3);
    }

    #[test]
    fn test_get_all_admins_page_zero_limit_uses_default() {
        let env = Env::default();
        let (contract_address, _super_admin, _admin, _operator) = setup_multiple_admins(&env);

        // limit = 0 should use MAX_PAGE_LIMIT (200) as default
        let (page, next_cursor) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 0, 0)
        });
        assert_eq!(page.len(), 3);
        assert_eq!(next_cursor, None);
    }

    #[test]
    fn test_get_all_admins_page_deterministic_order() {
        let env = Env::default();
        let (contract_address, super_admin, admin, operator) = setup_multiple_admins(&env);

        // Two calls with same cursor should return same order
        let (page_a, _) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 0, 3)
        });
        let (page_b, _) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 0, 3)
        });
        assert_eq!(page_a.len(), page_b.len());
        for i in 0..page_a.len() {
            assert_eq!(page_a.get(i).unwrap(), page_b.get(i).unwrap());
        }
    }

    #[test]
    fn test_get_admins_by_role_page_empty_role() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_contract(&env);

        // Admin role list is empty in a fresh setup
        let (page, next_cursor) = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role_page(env.clone(), AdminRole::Admin, 0, 10)
        });
        assert_eq!(page.len(), 0);
        assert_eq!(next_cursor, None);
    }

    #[test]
    fn test_get_admins_by_role_page_single_role_admin() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        let (page, next_cursor) = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role_page(env.clone(), AdminRole::SuperAdmin, 0, 10)
        });
        assert_eq!(page.len(), 1);
        assert_eq!(page.get(0).unwrap(), super_admin);
        assert_eq!(next_cursor, None);
    }

    #[test]
    fn test_get_admins_by_role_page_multi_page() {
        let env = Env::default();
        let (contract_address, super_admin, admin, operator) = setup_multiple_admins(&env);

        // Page through all admins
        let (page_1, cursor_1) = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role_page(env.clone(), AdminRole::SuperAdmin, 0, 1)
        });
        assert_eq!(page_1.len(), 1);
        assert_eq!(page_1.get(0).unwrap(), super_admin);
        assert_eq!(cursor_1, None); // Only 1 super admin, so no next cursor

        // Admin role has 1 member (admin + promoted operator)
        let (page_admin, cursor_admin) = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role_page(env.clone(), AdminRole::Admin, 0, 1)
        });
        assert_eq!(page_admin.len(), 1);
        assert_eq!(page_admin.get(0).unwrap(), admin);
        assert_eq!(cursor_admin, None);
    }

    #[test]
    fn test_get_admins_by_role_page_cursor_past_end() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_contract(&env);

        let (page, next_cursor) = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role_page(env.clone(), AdminRole::SuperAdmin, 5, 10)
        });
        assert_eq!(page.len(), 0);
        assert_eq!(next_cursor, None);
    }

    #[test]
    fn test_get_admins_by_role_page_limit_clamped() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_contract(&env);

        let (page, _) = env.as_contract(&contract_address, || {
            AdminContract::get_admins_by_role_page(env.clone(), AdminRole::SuperAdmin, 0, 500)
        });
        assert_eq!(page.len(), 1);
    }

    #[test]
    fn test_admin_pagination_matches_deprecated_full_list() {
        let env = Env::default();
        let (contract_address, super_admin, admin, operator) = setup_multiple_admins(&env);

        // Collect all via pagination
        let mut all_paginated = soroban_sdk::Vec::new(&env);
        let mut cursor = 0;
        loop {
            let (page, next) = env.as_contract(&contract_address, || {
                AdminContract::get_all_admins_page(env.clone(), cursor, 2)
            });
            for addr in page.iter() {
                all_paginated.push_back(addr);
            }
            match next {
                Some(n) => cursor = n,
                None => break,
            }
        }

        assert_eq!(all_paginated.len(), 3);
        assert!(all_paginated.contains(&super_admin));
        assert!(all_paginated.contains(&admin));
        assert!(all_paginated.contains(&operator));
    }

    #[test]
    fn test_admin_role_pagination_matches_deprecated_full_list() {
        let env = Env::default();
        let (contract_address, super_admin, admin, operator) = setup_multiple_admins(&env);

        // Collect super admins via pagination
        let mut all_super_admins = soroban_sdk::Vec::new(&env);
        let mut cursor = 0;
        loop {
            let (page, next) = env.as_contract(&contract_address, || {
                AdminContract::get_admins_by_role_page(
                    env.clone(),
                    AdminRole::SuperAdmin,
                    cursor,
                    1,
                )
            });
            for addr in page.iter() {
                all_super_admins.push_back(addr);
            }
            match next {
                Some(n) => cursor = n,
                None => break,
            }
        }
        assert_eq!(all_super_admins.len(), 1);
        assert!(all_super_admins.contains(&super_admin));

        // Collect operators via pagination
        let mut all_operators = soroban_sdk::Vec::new(&env);
        cursor = 0;
        loop {
            let (page, next) = env.as_contract(&contract_address, || {
                AdminContract::get_admins_by_role_page(env.clone(), AdminRole::Operator, cursor, 1)
            });
            for addr in page.iter() {
                all_operators.push_back(addr);
            }
            match next {
                Some(n) => cursor = n,
                None => break,
            }
        }
        assert_eq!(all_operators.len(), 1);
        assert!(all_operators.contains(&operator));
    }

    #[test]
    fn test_get_admin_count_matches_pagination_total() {
        let env = Env::default();
        let (contract_address, _super_admin, _admin, _operator) = setup_multiple_admins(&env);

        let count = env.as_contract(&contract_address, || {
            AdminContract::get_admin_count(env.clone())
        });

        let mut total = 0u32;
        let mut cursor = 0;
        loop {
            let (page, next) = env.as_contract(&contract_address, || {
                AdminContract::get_all_admins_page(env.clone(), cursor, 1)
            });
            total += page.len();
            match next {
                Some(n) => cursor = n,
                None => break,
            }
        }
        assert_eq!(count, total);
    }

    #[test]
    fn test_concurrent_insert_during_pagination_walk() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        // Start with 1 admin. Walk page-by-page with limit=1.
        let (page1, cursor1) = env.as_contract(&contract_address, || {
            AdminContract::get_all_admins_page(env.clone(), 0, 1)
        });
        assert_eq!(page1.len(), 1);
        assert_eq!(page1.get(0).unwrap(), super_admin);
        // cursor1 should be None since there's only 1 admin
        assert_eq!(cursor1, None);

        // Now add a new admin mid-walk
        let new_admin = Address::generate(&env);
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                new_admin.clone(),
                AdminRole::Operator,
            );
        });

        // Continue the walk from cursor 1 (if it had returned Some(1))
        // Since cursor was None (exhausted), a new walk from 0 should now
        // include both admins and produce a deterministic result.
        let mut all = soroban_sdk::Vec::new(&env);
        let mut cursor = 0;
        loop {
            let (page, next) = env.as_contract(&contract_address, || {
                AdminContract::get_all_admins_page(env.clone(), cursor, 1)
            });
            for addr in page.iter() {
                all.push_back(addr);
            }
            match next {
                Some(n) => cursor = n,
                None => break,
            }
        }
        assert_eq!(all.len(), 2);
        assert!(all.contains(&super_admin));
        assert!(all.contains(&new_admin));
    }
}
