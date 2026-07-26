#![cfg(test)]

use crate::*;
use soroban_sdk::{Address, Env, String};
use std::panic::AssertUnwindSafe;

mod zero_address_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup_contract(env: &Env) -> (CredenceBondClient<'_>, Address, Address) {
        let contract_address = env.register(CredenceBond, ());
        let client = CredenceBondClient::new(env, &contract_address);
        let admin = Address::generate(env);

        env.mock_all_auths();
        client.initialize(&admin, &None);

        (client, contract_address, admin)
    }

    #[test]
    fn test_set_early_exit_config_rejects_zero_address() {
        let env = Env::default();
        let (client, _contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ));

        env.mock_all_auths();

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            client.set_early_exit_config(&admin, &zero_address, &100);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_set_emergency_config_rejects_zero_addresses() {
        let env = Env::default();
        let (client, _contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ));
        let valid_address = Address::generate(&env);

        env.mock_all_auths();

        // Test zero governance address
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            client.set_emergency_config(&admin, &zero_address, &valid_address, &50, &true);
        }));

        assert!(result.is_err());

        // Test zero treasury address
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            client.set_emergency_config(&admin, &valid_address, &zero_address, &50, &true);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_register_attester_rejects_zero_address() {
        let env = Env::default();
        let (client, _contract_address, _admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ));

        env.mock_all_auths();

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            client.register_attester(&zero_address);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_register_verifier_rejects_zero_address() {
        let env = Env::default();
        let (client, _contract_address, _admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ));

        env.mock_all_auths();

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            client.register_verifier(&zero_address, &1000);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_set_token_rejects_zero_address() {
        let env = Env::default();
        let (client, _contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ));

        env.mock_all_auths();

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            client.set_token(&admin, &zero_address);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_set_usdc_token_rejects_zero_address() {
        let env = Env::default();
        let (client, _contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ));
        let network = String::from_str(&env, "mainnet");

        env.mock_all_auths();

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            client.set_usdc_token(&admin, &zero_address, &network);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_valid_addresses_succeed() {
        let env = Env::default();
        let (client, _contract_address, admin) = setup_contract(&env);
        let treasury = Address::generate(&env);
        let governance = Address::generate(&env);
        let attester = Address::generate(&env);
        let verifier = Address::generate(&env);
        let token = Address::generate(&env);
        let network = String::from_str(&env, "mainnet");

        env.mock_all_auths();

        // These should all succeed
        client.set_early_exit_config(&admin, &treasury, &100);

        client.set_emergency_config(&admin, &governance, &treasury, &50, &true);

        client.register_attester(&attester);

        client.register_verifier(&verifier, &1000);

        client.set_token(&admin, &token);

        // Re-register new contract or reset since set_token can only be called once
        let contract_address2 = env.register(CredenceBond, ());
        let client2 = CredenceBondClient::new(&env, &contract_address2);
        let admin2 = Address::generate(&env);
        client2.initialize(&admin2, &None);

        client2.set_usdc_token(&admin2, &token, &network);
    }
}
