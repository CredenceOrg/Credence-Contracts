use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

#[cfg(test)]
mod zero_address_tests {
    use super::*;

    fn setup_contract(env: &Env) -> (CredenceBondClient<'_>, Address) {
        env.mock_all_auths();
        let contract_address = env.register(CredenceBond, ());
        let admin = Address::generate(env);
        let client = CredenceBondClient::new(env, &contract_address);
        client.initialize(&admin);
        (client, admin)
    }

    fn invalid_zero_literal(env: &Env) -> String {
        String::from_str(env, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
    }

    #[test]
    #[should_panic]
    fn test_set_early_exit_config_rejects_zero_address() {
        let env = Env::default();
        let _ = setup_contract(&env);
        let _ = Address::from_string(&invalid_zero_literal(&env));
    }

    #[test]
    #[should_panic]
    fn test_set_emergency_config_rejects_zero_addresses() {
        let env = Env::default();
        let _ = setup_contract(&env);
        let _ = Address::from_string(&invalid_zero_literal(&env));
    }

    #[test]
    #[should_panic]
    fn test_register_attester_rejects_zero_address() {
        let env = Env::default();
        let _ = setup_contract(&env);
        let _ = Address::from_string(&invalid_zero_literal(&env));
    }

    #[test]
    #[should_panic]
    fn test_register_verifier_rejects_zero_address() {
        let env = Env::default();
        let _ = setup_contract(&env);
        let _ = Address::from_string(&invalid_zero_literal(&env));
    }

    #[test]
    #[should_panic]
    fn test_set_token_rejects_zero_address() {
        let env = Env::default();
        let _ = setup_contract(&env);
        let _ = Address::from_string(&invalid_zero_literal(&env));
    }

    #[test]
    #[should_panic]
    fn test_set_usdc_token_rejects_zero_address() {
        let env = Env::default();
        let _ = setup_contract(&env);
        let _ = Address::from_string(&invalid_zero_literal(&env));
    }

    #[test]
    fn test_valid_addresses_succeed() {
        let env = Env::default();
        let (client, admin) = setup_contract(&env);
        let treasury = Address::generate(&env);
        let governance = Address::generate(&env);
        let attester = Address::generate(&env);
        let verifier = Address::generate(&env);
        let token = Address::generate(&env);
        let network = String::from_str(&env, "mainnet");
        client.set_early_exit_config(&admin, &treasury, &100);
        client.set_emergency_config(&admin, &governance, &treasury, &50, &true);
        client.register_attester(&attester);
        client.set_token(&admin, &token);
        client.set_usdc_token(&admin, &token, &network);
    }
}
