use crate::*;
use std::panic::AssertUnwindSafe;
use soroban_sdk::{Address, Env, String};

#[cfg(test)]
mod zero_address_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn create_contract() -> CredenceBond {
        CredenceBond {}
    }

    fn setup_contract(env: &Env) -> (Address, Address) {
        let contract = create_contract();
        let admin = Address::generate(env);
        let contract_address = env.register(CredenceBond, ());

        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            CredenceBond::initialize(env.clone(), admin.clone());
        });

        (contract_address, admin)
    }

    #[test]
    fn test_set_early_exit_config_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(&env, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));

        env.mock_all_auths();
        
        env.as_contract(&contract_address, || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                CredenceBond::set_early_exit_config(
                    env.clone(),
                    admin.clone(),
                    zero_address.clone(),
                    100, // 1% penalty
                );
            }));
            
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_set_emergency_config_rejects_zero_addresses() {
        let env = Env::default();
        let (contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(&env, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));
        let valid_address = Address::generate(&env);

        env.mock_all_auths();
        
        env.as_contract(&contract_address, || {
            // Test zero governance address
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                CredenceBond::set_emergency_config(
                    env.clone(),
                    admin.clone(),
                    zero_address.clone(),
                    valid_address.clone(),
                    50, // 0.5% fee
                    true,
                );
            }));
            
            assert!(result.is_err());

            // Test zero treasury address
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                CredenceBond::set_emergency_config(
                    env.clone(),
                    admin.clone(),
                    valid_address.clone(),
                    zero_address.clone(),
                    50, // 0.5% fee
                    true,
                );
            }));
            
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_register_attester_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(&env, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));

        env.mock_all_auths();
        
        env.as_contract(&contract_address, || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                CredenceBond::register_attester(
                    env.clone(),
                    zero_address.clone(),
                );
            }));
            
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_register_verifier_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(&env, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));

        env.mock_all_auths();
        
        env.as_contract(&contract_address, || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                CredenceBond::register_verifier(
                    env.clone(),
                    zero_address.clone(),
                    1000, // stake deposit
                );
            }));
            
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_set_token_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(&env, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));

        env.mock_all_auths();
        
        env.as_contract(&contract_address, || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                CredenceBond::set_token(
                    env.clone(),
                    admin.clone(),
                    zero_address.clone(),
                );
            }));
            
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_set_usdc_token_rejects_zero_address() {
        let env = Env::default();
        let (contract_address, admin) = setup_contract(&env);
        let zero_address = Address::from_string(&String::from_str(&env, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));
        let network = String::from_str(&env, "mainnet");

        env.mock_all_auths();
        
        env.as_contract(&contract_address, || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                CredenceBond::set_usdc_token(
                    env.clone(),
                    admin.clone(),
                    zero_address.clone(),
                    network.clone(),
                );
            }));
            
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_valid_addresses_succeed() {
        let env = Env::default();
        let (contract_address, admin) = setup_contract(&env);
        let treasury = Address::generate(&env);
        let governance = Address::generate(&env);
        let attester = Address::generate(&env);
        let verifier = Address::generate(&env);
        let token = Address::generate(&env);
        let network = String::from_str(&env, "mainnet");

        env.mock_all_auths();
        
        env.as_contract(&contract_address, || {
            // These should all succeed
            CredenceBond::set_early_exit_config(
                env.clone(),
                admin.clone(),
                treasury.clone(),
                100, // 1% penalty
            );

            CredenceBond::set_emergency_config(
                env.clone(),
                admin.clone(),
                governance.clone(),
                treasury.clone(),
                50, // 0.5% fee
                true,
            );

            CredenceBond::register_attester(
                env.clone(),
                attester.clone(),
            );

            CredenceBond::register_verifier(
                env.clone(),
                verifier.clone(),
                1000, // stake deposit
            );

            CredenceBond::set_token(
                env.clone(),
                admin.clone(),
                token.clone(),
            );

            CredenceBond::set_usdc_token(
                env.clone(),
                admin.clone(),
                token.clone(),
                network.clone(),
            );
        });
    }
}
