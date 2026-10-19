//! Deterministic failure-boundary coverage for `require_valid_admin_address`.
//!
//! The helper only validates the supplied address and does not read or write
//! admin state. Caller-facing tests additionally verify that its rejection is
//! atomic and that a later valid retry can still commit exactly one mutation.

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(env, INVALID_ADDRESS_SENTINEL))
}

#[test]
fn accepts_a_generated_address_without_mutating_state() {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let candidate = Address::generate(&env);

    env.as_contract(&contract_id, || {
        AdminContract::require_valid_admin_address(&env, &candidate);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #110)")]
fn rejects_the_invalid_address_sentinel() {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let invalid = zero_address(&env);

    env.as_contract(&contract_id, || {
        AdminContract::require_valid_admin_address(&env, &invalid);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #110)")]
fn rejects_the_contracts_own_address() {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);

    env.as_contract(&contract_id, || {
        AdminContract::require_valid_admin_address(&env, &contract_id);
    });
}

#[test]
fn rejected_public_call_preserves_admin_state_and_epoch() {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&env, &contract_id);
    let super_admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&super_admin, &1, &100);

    let epoch_before = client.get_config_epoch();
    let admins_before = client.get_all_admins();
    let invalid = zero_address(&env);

    assert!(client
        .try_add_admin(&super_admin, &invalid, &AdminRole::Admin)
        .is_err());

    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(client.get_admin_count(), 1);
    assert_eq!(client.get_all_admins(), admins_before);
}

#[test]
fn repeated_rejection_does_not_block_a_valid_retry() {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&env, &contract_id);
    let super_admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&super_admin, &1, &100);

    let invalid = zero_address(&env);
    assert!(client
        .try_add_admin(&super_admin, &invalid, &AdminRole::Admin)
        .is_err());
    assert!(client
        .try_add_admin(&super_admin, &invalid, &AdminRole::Admin)
        .is_err());
    assert_eq!(client.get_config_epoch(), 0);

    let valid = Address::generate(&env);
    client.add_admin(&super_admin, &valid, &AdminRole::Admin);
    assert_eq!(client.get_config_epoch(), 1);
    assert_eq!(client.get_admin_count(), 2);
}
