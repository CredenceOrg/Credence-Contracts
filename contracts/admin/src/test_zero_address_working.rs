use crate::*;
use soroban_sdk::{Address, Env, String};
use credence_errors::ContractError;

#[cfg(test)]
mod zero_address_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup_contract(env: &Env) -> (Address, Address, AdminContractClient) {
        let super_admin = Address::generate(env);
        let contract_address = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(env, &contract_address);

        env.mock_all_auths();
        client.initialize(&super_admin, &1, &100);

        (contract_address, super_admin, client)
    }

    fn zero_address(env: &Env) -> Address {
        Address::from_string(&String::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ))
    }

    #[test]
    fn test_valid_addresses_succeed() {
        let env = Env::default();
        let (_, super_admin, client) = setup_contract(&env);
        let new_admin = Address::generate(&env);

        env.mock_all_auths();
        let admin_info = client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
        assert_eq!(admin_info.address, new_admin);
        assert_eq!(admin_info.role, AdminRole::Admin);
    }

    #[test]
    fn test_add_admin_rejects_zero_address_and_preserves_state() {
        let env = Env::default();
        let (_, super_admin, client) = setup_contract(&env);
        let zero = zero_address(&env);

        let initial_epoch = client.get_config_epoch();
        let initial_count = client.get_admin_count();

        env.mock_all_auths();
        let err = client
            .try_add_admin(&super_admin, &zero, &AdminRole::Admin)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ContractError::InvalidAdminAddress);

        // Verify state is untouched
        assert_eq!(client.get_config_epoch(), initial_epoch);
        assert_eq!(client.get_admin_count(), initial_count);
    }

    #[test]
    fn test_transfer_ownership_rejects_zero_address_and_preserves_state() {
        let env = Env::default();
        let (_, super_admin, client) = setup_contract(&env);
        let zero = zero_address(&env);

        let initial_epoch = client.get_config_epoch();

        env.mock_all_auths();
        let err = client
            .try_transfer_ownership(&super_admin, &zero)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ContractError::InvalidAdminAddress);

        assert_eq!(client.get_config_epoch(), initial_epoch);
        assert!(client.get_pending_owner().is_none());
    }

    #[test]
    fn test_update_admin_role_rejects_zero_address_and_preserves_state() {
        let env = Env::default();
        let (_, super_admin, client) = setup_contract(&env);
        let zero = zero_address(&env);

        let initial_epoch = client.get_config_epoch();

        env.mock_all_auths();
        let err = client
            .try_update_admin_role(&super_admin, &zero, &AdminRole::Admin)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ContractError::InvalidAdminAddress);

        assert_eq!(client.get_config_epoch(), initial_epoch);
    }

    #[test]
    fn test_reactivate_admin_rejects_zero_address_and_preserves_state() {
        let env = Env::default();
        let (_, super_admin, client) = setup_contract(&env);
        let zero = zero_address(&env);

        let initial_epoch = client.get_config_epoch();

        env.mock_all_auths();
        let err = client
            .try_reactivate_admin(&super_admin, &zero)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ContractError::InvalidAdminAddress);

        assert_eq!(client.get_config_epoch(), initial_epoch);
    }

    #[test]
    fn test_deactivate_admin_rejects_zero_address_and_preserves_state() {
        let env = Env::default();
        let (_, super_admin, client) = setup_contract(&env);
        let zero = zero_address(&env);

        let initial_epoch = client.get_config_epoch();

        env.mock_all_auths();
        let err = client
            .try_deactivate_admin(&super_admin, &zero)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ContractError::InvalidAdminAddress);

        assert_eq!(client.get_config_epoch(), initial_epoch);
    }

    #[test]
    fn test_remove_admin_rejects_zero_address_and_preserves_state() {
        let env = Env::default();
        let (_, super_admin, client) = setup_contract(&env);
        let zero = zero_address(&env);

        let initial_epoch = client.get_config_epoch();
        let initial_count = client.get_admin_count();

        env.mock_all_auths();
        let err = client
            .try_remove_admin(&super_admin, &zero)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ContractError::InvalidAdminAddress);

        assert_eq!(client.get_config_epoch(), initial_epoch);
        assert_eq!(client.get_admin_count(), initial_count);
    }

    #[test]
    fn test_set_pause_signer_rejects_zero_address_and_preserves_state() {
        let env = Env::default();
        let (_, super_admin, client) = setup_contract(&env);
        let zero = zero_address(&env);

        let initial_epoch = client.get_config_epoch();

        env.mock_all_auths();
        let err = client
            .try_set_pause_signer(&super_admin, &zero, &true)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ContractError::InvalidAdminAddress);

        assert_eq!(client.get_config_epoch(), initial_epoch);
    }
}
