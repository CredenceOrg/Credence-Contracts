use crate::*;
use soroban_sdk::{Address, Env};

#[cfg(test)]
mod zero_address_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn create_contract() -> AdminContract {
        AdminContract {}
    }

    fn setup_contract(env: &Env) -> (Address, Address) {
        let contract = create_contract();
        let super_admin = Address::generate(env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);
        });

        (contract_address, super_admin)
    }

    /// The all-zero Ed25519 public key in strkey format.
    fn zero_address(env: &Env) -> Address {
        Address::from_string(&String::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ))
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #110)")]
    fn test_add_admin_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let zero = zero_address(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(env.clone(), super_admin.clone(), zero, AdminRole::Admin);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #110)")]
    fn test_transfer_ownership_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let zero = zero_address(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(env.clone(), super_admin.clone(), zero);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #110)")]
    fn test_update_admin_role_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let zero = zero_address(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::update_admin_role(
                env.clone(),
                super_admin.clone(),
                zero,
                AdminRole::Admin,
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #110)")]
    fn test_reactivate_admin_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let zero = zero_address(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::reactivate_admin(env.clone(), super_admin.clone(), zero);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #110)")]
    fn test_deactivate_admin_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let zero = zero_address(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), zero);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #110)")]
    fn test_remove_admin_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let zero = zero_address(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::remove_admin(env.clone(), super_admin.clone(), zero);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #110)")]
    fn test_set_pause_signer_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let zero = zero_address(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::set_pause_signer(env.clone(), super_admin.clone(), zero, true);
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #110)")]
    fn test_add_admin_rejects_contract_self_address() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                contract_address.clone(),
                AdminRole::Admin,
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #110)")]
    fn test_transfer_ownership_rejects_contract_self_address() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        // Add another SuperAdmin so the check in transfer_ownership passes
        let other = Address::generate(&env);
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                other.clone(),
                AdminRole::SuperAdmin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin.clone(),
                contract_address.clone(),
            );
        });
    }

    #[test]
    fn test_valid_addresses_succeed() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let new_admin = Address::generate(&env);
        let new_owner = Address::generate(&env);
        let pause_signer = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            let admin_info = AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                new_admin.clone(),
                AdminRole::Admin,
            );
            assert_eq!(admin_info.address, new_admin);
            assert_eq!(admin_info.role, AdminRole::Admin);
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                new_owner.clone(),
                AdminRole::SuperAdmin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(env.clone(), super_admin.clone(), new_owner.clone());
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::set_pause_signer(
                env.clone(),
                new_owner.clone(),
                pause_signer.clone(),
                true,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::update_admin_role(
                env.clone(),
                super_admin.clone(),
                new_admin.clone(),
                AdminRole::Operator,
            );
        });
    }
}
