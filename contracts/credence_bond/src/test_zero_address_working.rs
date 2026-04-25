use crate::*;
use soroban_sdk::{Address, Env, String};

#[cfg(test)]
mod zero_address_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup_contract(env: &Env) -> (CredenceBondClient<'_>, Address) {
        let admin = Address::generate(env);
        let contract_address = env.register(CredenceBond, ());
        env.mock_all_auths();
        let client = CredenceBondClient::new(env, &contract_address);
        client.initialize(&admin);
        (client, admin)
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
