#![cfg(test)]

use crate::{test_helpers, CredenceBond, CredenceBondClient, DataKey};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_admin_transfer_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let contract_id = env.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&env, &contract_id);

    client.initialize(&admin);

    // Initial admin is correct
    let stored_admin: Address = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    });
    assert_eq!(stored_admin, admin);

    // Propose transfer
    client.transfer_admin(&admin, &new_admin);

    // Pending admin is set
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    // Old admin is still the admin
    let stored_admin: Address = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    });
    assert_eq!(stored_admin, admin);

    // Accept transfer
    client.accept_admin(&new_admin);

    // New admin is now the admin
    let stored_admin: Address = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    });
    assert_eq!(stored_admin, new_admin);

    // Pending admin is cleared
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
#[should_panic(expected = "only pending admin can accept the role")]
fn test_admin_transfer_wrong_acceptor() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let wrong_admin = Address::generate(&env);
    let contract_id = env.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.transfer_admin(&admin, &new_admin);

    // Wrong address tries to accept
    client.accept_admin(&wrong_admin);
}

#[test]
fn test_upgrade_admin_transfer_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let contract_id = env.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&env, &contract_id);

    client.initialize(&admin);
    
    // Initial upgrade admin is correct
    let stored: Address = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::UpgradeAdmin).unwrap()
    });
    assert_eq!(stored, admin);

    // Propose transfer
    client.transfer_upgrade_admin(&admin, &new_admin);

    // Pending is set
    assert_eq!(client.get_pending_upgrade_admin(), Some(new_admin.clone()));

    // Accept
    client.accept_upgrade_admin(&new_admin);

    // New admin is now the upgrade admin
    let stored: Address = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::UpgradeAdmin).unwrap()
    });
    assert_eq!(stored, new_admin);

    // New admin should also have the Upgrader role
    let is_upgrader = client.is_authorized_upgrader(&new_admin);
    assert!(is_upgrader);

    // Pending is cleared
    assert_eq!(client.get_pending_upgrade_admin(), None);
}

#[test]
#[should_panic(expected = "new admin must be different")]
fn test_admin_transfer_to_self() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.transfer_admin(&admin, &admin);
}

#[test]
#[should_panic(expected = "not upgrade admin")]
fn test_transfer_upgrade_admin_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let malicious = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let contract_id = env.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.transfer_upgrade_admin(&malicious, &new_admin);
}
