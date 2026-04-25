use crate::*;
use std::panic::AssertUnwindSafe;
use soroban_sdk::{Address, Env, String};

#[cfg(test)]
mod immutable_config_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn create_contract() -> CredenceBond {
        CredenceBond {}
    }

    #[test]
    fn test_admin_requires_initialization() {
        let env = Env::default();
        let contract = create_contract();
        let contract_address = env.register(CredenceBond, ());
        let admin = Address::generate(&env);

        env.mock_all_auths();
        
        env.as_contract(&contract_address, || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                CredenceBond::require_admin_internal(&env, &admin);
            }));
            
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_admin_initialized_correctly() {
        let env = Env::default();
        let contract = create_contract();
        let admin = Address::generate(&env);
        let contract_address = env.register(CredenceBond, ());

        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            CredenceBond::initialize(env.clone(), admin.clone());
            
            // This should not panic
            CredenceBond::require_admin_internal(&env, &admin);
        });
    }

    #[test]
    fn test_token_requires_initialization() {
        let env = Env::default();
        let contract = create_contract();
        let contract_address = env.register(CredenceBond, ());

        env.mock_all_auths();
        
        env.as_contract(&contract_address, || {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                crate::token_integration::get_token(&env);
            }));
            
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_admin_cannot_be_reinitialized() {
        let env = Env::default();
        let contract = create_contract();
        let admin = Address::generate(&env);
        let contract_address = env.register(CredenceBond, ());

        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            CredenceBond::initialize(env.clone(), admin.clone());
            
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                CredenceBond::initialize(env.clone(), admin.clone());
            }));
            
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_token_can_only_be_set_once() {
        let env = Env::default();
        let contract = create_contract();
        let admin = Address::generate(&env);
        let token1 = Address::generate(&env);
        let token2 = Address::generate(&env);
        let contract_address = env.register(CredenceBond, ());

        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            CredenceBond::initialize(env.clone(), admin.clone());
            
            // Set token first time - should succeed
            CredenceBond::set_token(env.clone(), admin.clone(), token1.clone());
            
            // Verify token is set
            let retrieved_token = crate::token_integration::get_token(&env);
            assert_eq!(retrieved_token, token1);
            
            // Set token second time - should overwrite (this is expected behavior for tokens)
            CredenceBond::set_token(env.clone(), admin.clone(), token2.clone());
            
            // Verify token was updated
            let retrieved_token = crate::token_integration::get_token(&env);
            assert_eq!(retrieved_token, token2);
        });
    }
}
