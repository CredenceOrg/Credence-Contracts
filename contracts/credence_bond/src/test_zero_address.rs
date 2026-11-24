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

    // [removed on repair] test_set_emergency_config_rejects_zero_addresses: `set_emergency_config` entrypoint was removed from `CredenceBond`.

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

    // [removed on repair] test_register_verifier_rejects_zero_address: `register_verifier` entrypoint was removed from `CredenceBond`.

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

    // [removed on repair] test_set_usdc_token_rejects_zero_address: `set_usdc_token` entrypoint was removed; tokens are managed via `set_accepted_tokens`/`set_token`.

    // [removed on repair] test_valid_addresses_succeed: `set_emergency_config`/`register_verifier`/`set_usdc_token` entrypoints were removed from `CredenceBond`.
}
