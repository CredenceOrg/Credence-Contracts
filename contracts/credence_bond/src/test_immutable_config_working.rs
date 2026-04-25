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
}
