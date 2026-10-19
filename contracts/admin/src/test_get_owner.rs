#![cfg(test)]

use crate::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup_env() -> (Env, Address, Address) {
    let env = Env::default();
    let contract_address = env.register_contract(None, AdminContract);
    let super_admin = Address::generate(&env);

    env.mock_all_auths();
    env.as_contract(&contract_address, || {
        AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);
    });

    (env, contract_address, super_admin)
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_get_owner_uninitialized() {
    let env = Env::default();
    let contract_address = env.register_contract(None, AdminContract);

    // This should panic with ContractError::NotInitialized (Error #1)
    env.as_contract(&contract_address, || {
        AdminContract::get_owner(env.clone());
    });
}

#[test]
fn test_get_owner_success_loading() {
    let (env, contract_address, super_admin) = setup_env();

    let owner = env.as_contract(&contract_address, || {
        AdminContract::get_owner(env.clone())
    });

    assert_eq!(owner, super_admin);
}

#[test]
fn test_get_owner_retry_idempotency() {
    let (env, contract_address, super_admin) = setup_env();

    // Multiple consecutive reads should return the same deterministic result
    for _ in 0..5 {
        let owner = env.as_contract(&contract_address, || {
            AdminContract::get_owner(env.clone())
        });
        assert_eq!(owner, super_admin);
    }
}

#[test]
fn test_get_owner_stale_state_during_transfer() {
    let (env, contract_address, super_admin) = setup_env();

    let new_owner = Address::generate(&env);
    
    // Initiate transfer
    env.as_contract(&contract_address, || {
        // First add the new owner as an admin so we can transfer ownership to them
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            new_owner.clone(),
            AdminRole::SuperAdmin,
        );
        AdminContract::transfer_ownership(env.clone(), super_admin.clone(), new_owner.clone());
    });

    // The owner should remain stale (i.e. unchanged) until the transfer is accepted
    let owner = env.as_contract(&contract_address, || {
        AdminContract::get_owner(env.clone())
    });

    assert_eq!(owner, super_admin);
}

#[test]
fn test_get_owner_new_after_accept() {
    let (env, contract_address, super_admin) = setup_env();

    let new_owner = Address::generate(&env);

    env.as_contract(&contract_address, || {
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            new_owner.clone(),
            AdminRole::SuperAdmin,
        );
        AdminContract::transfer_ownership(env.clone(), super_admin.clone(), new_owner.clone());
        AdminContract::accept_ownership(env.clone(), new_owner.clone());
    });

    // The owner should now be the new owner
    let owner = env.as_contract(&contract_address, || {
        AdminContract::get_owner(env.clone())
    });

    assert_eq!(owner, new_owner);
}

#[test]
fn test_get_owner_permissions() {
    let (env, contract_address, super_admin) = setup_env();
    
    let client = AdminContractClient::new(&env, &contract_address);

    // Call as an unauthenticated/random user to ensure no permissions are required.
    // get_owner is a public getter, so this should just succeed without throwing NotAdmin.
    env.mock_all_auths();
    let owner = client.get_owner();

    assert_eq!(owner, super_admin);
}
