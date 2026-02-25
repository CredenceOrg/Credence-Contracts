use crate::*;
use soroban_sdk::{Address, Env};

#[cfg(test)]
mod comprehensive_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn create_contract() -> AdminContract {
        AdminContract {}
    }

    fn setup_with_limits(env: &Env, min_admins: u32, max_admins: u32) -> (Address, Address) {
        let contract = create_contract();
        let super_admin = Address::generate(env);
        let contract_address = env.register_contract(None, contract);

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

        assert!(env.as_contract(&contract_address, || {
            AdminContract::is_admin(env.clone(), super_admin.clone())
        }));
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
    #[should_panic(expected = "already initialized")]
    fn test_double_initialization() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);
        });
    }

    #[test]
    #[should_panic(expected = "min_admins cannot be zero")]
    fn test_initialize_rejects_min_admins_zero() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin = Address::generate(&env);
        let contract_address = env.register_contract(None, contract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 0, 100);
        });
    }

    #[test]
    #[should_panic(expected = "min_admins cannot be greater than max_admins")]
    fn test_initialize_rejects_min_greater_than_max() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin = Address::generate(&env);
        let contract_address = env.register_contract(None, contract);

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
    #[should_panic(expected = "insufficient privileges")]
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
    #[should_panic(expected = "address is already an admin")]
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
    #[should_panic(expected = "maximum admin limit reached")]
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
    #[should_panic(expected = "address is already an admin")]
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
    #[should_panic(expected = "admin not found")]
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
    #[should_panic(expected = "insufficient privileges to remove admin")]
    fn test_remove_admin_rejects_insufficient_privileges() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::remove_admin(env.clone(), operator.clone(), admin.clone());
        });
    }

    #[test]
    #[should_panic(expected = "insufficient privileges to remove admin")]
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
    #[should_panic(expected = "insufficient privileges")]
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
    #[should_panic(expected = "admin not found")]
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
    #[should_panic(expected = "cannot assign equal or higher role to self")]
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
    #[should_panic(expected = "insufficient privileges to deactivate admin")]
    fn test_deactivate_admin_rejects_insufficient_privileges() {
        let env = Env::default();
        let (contract_address, _super_admin, admin, operator) = setup_multiple_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), operator.clone(), admin.clone());
        });
    }

    #[test]
    #[should_panic(expected = "admin already deactivated")]
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
    #[should_panic(expected = "insufficient privileges to reactivate admin")]
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
    #[should_panic(expected = "admin already active")]
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

        assert!(!env.as_contract(&contract_address, || {
            AdminContract::is_admin(env.clone(), admin.clone())
        }));
        assert!(!env.as_contract(&contract_address, || {
            AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Operator)
        }));
    }

    #[test]
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
    #[should_panic(expected = "admin not found")]
    fn test_get_admin_info_panics_for_non_admin() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_contract(&env);
        let non_admin = Address::generate(&env);

        env.as_contract(&contract_address, || {
            AdminContract::get_admin_info(env.clone(), non_admin.clone())
        });
    }

    #[test]
    #[should_panic(expected = "address is not an admin")]
    fn test_get_admin_role_panics_for_non_admin() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_contract(&env);
        let non_admin = Address::generate(&env);

        env.as_contract(&contract_address, || {
            AdminContract::get_admin_role(env.clone(), non_admin.clone())
        });
    }
}
