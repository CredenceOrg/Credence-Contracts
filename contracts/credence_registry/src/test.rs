use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Helper to create a test environment with initialized registry
fn setup_registry() -> (Env, Address, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceRegistry, ());

    let client = CredenceRegistryClient::new(&env, &contract_id);

    // Mock authorization for admin
    env.mock_all_auths();

    client.initialize(&admin);

    (env, contract_id, admin)
}

#[test]
fn test_initialize() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceRegistry, ());

    let client = CredenceRegistryClient::new(&env, &contract_id);

    env.mock_all_auths();

    client.initialize(&admin);

    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, admin);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice_should_fail() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceRegistry, ());

    let client = CredenceRegistryClient::new(&env, &contract_id);

    env.mock_all_auths();

    client.initialize(&admin);
    client.initialize(&admin); // Should panic
}

#[test]
fn test_register_identity() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    let entry = client.register(&identity, &bond_contract);

    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert_eq!(entry.active, true);
}

#[test]
#[should_panic(expected = "identity already registered")]
fn test_register_duplicate_identity() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract1 = Address::generate(&env);
    let bond_contract2 = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract1);
    client.register(&identity, &bond_contract2); // Should panic
}

#[test]
#[should_panic(expected = "bond contract already registered")]
fn test_register_duplicate_bond_contract() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity1, &bond_contract);
    client.register(&identity2, &bond_contract); // Should panic
}

#[test]
fn test_get_bond_contract() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract);

    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert_eq!(entry.identity, identity);
}

#[test]
#[should_panic(expected = "identity not registered")]
fn test_get_bond_contract_not_registered() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);

    client.get_bond_contract(&identity); // Should panic
}

#[test]
fn test_get_identity_reverse_lookup() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract);

    let found_identity = client.get_identity(&bond_contract);
    assert_eq!(found_identity, identity);
}

#[test]
#[should_panic(expected = "bond contract not registered")]
fn test_get_identity_not_registered() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let bond_contract = Address::generate(&env);

    client.get_identity(&bond_contract); // Should panic
}

#[test]
fn test_is_registered() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    // Not registered initially
    assert_eq!(client.is_registered(&identity), false);

    // Register
    client.register(&identity, &bond_contract);

    // Now registered
    assert_eq!(client.is_registered(&identity), true);
}

#[test]
fn test_deactivate() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract);
    assert_eq!(client.is_registered(&identity), true);

    client.deactivate(&identity);
    assert_eq!(client.is_registered(&identity), false);

    // Entry should still exist but be inactive
    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.active, false);
}

#[test]
#[should_panic(expected = "already deactivated")]
fn test_deactivate_twice() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract);
    client.deactivate(&identity);
    client.deactivate(&identity); // Should panic
}

#[test]
fn test_reactivate() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract);
    client.deactivate(&identity);
    assert_eq!(client.is_registered(&identity), false);

    client.reactivate(&identity);
    assert_eq!(client.is_registered(&identity), true);

    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.active, true);
}

#[test]
#[should_panic(expected = "already active")]
fn test_reactivate_already_active() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract);
    client.reactivate(&identity); // Should panic
}

#[test]
fn test_get_all_identities() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    env.mock_all_auths();

    // Initially empty
    let identities = client.get_all_identities();
    assert_eq!(identities.len(), 0);

    // Register multiple identities
    let identity1 = Address::generate(&env);
    let bond_contract1 = Address::generate(&env);
    client.register(&identity1, &bond_contract1);

    let identity2 = Address::generate(&env);
    let bond_contract2 = Address::generate(&env);
    client.register(&identity2, &bond_contract2);

    let identity3 = Address::generate(&env);
    let bond_contract3 = Address::generate(&env);
    client.register(&identity3, &bond_contract3);

    let identities = client.get_all_identities();
    assert_eq!(identities.len(), 3);

    // Verify all identities are in the list
    assert!(identities.iter().any(|addr| addr == identity1));
    assert!(identities.iter().any(|addr| addr == identity2));
    assert!(identities.iter().any(|addr| addr == identity3));
}

#[test]
fn test_transfer_admin() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);

    env.mock_all_auths();

    client.transfer_admin(&new_admin);

    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, new_admin);
}

#[test]
fn test_admin_only_operations() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    // Admin can register
    client.register(&identity, &bond_contract);

    // Admin can deactivate
    client.deactivate(&identity);

    // Admin can reactivate
    client.reactivate(&identity);

    // Admin can transfer admin rights
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
}

#[test]
fn test_bidirectional_lookup() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract);

    // Forward lookup: identity -> bond contract
    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond_contract);

    // Reverse lookup: bond contract -> identity
    let found_identity = client.get_identity(&bond_contract);
    assert_eq!(found_identity, identity);
}

#[test]
fn test_multiple_registrations() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    env.mock_all_auths();

    // Register 5 different identity-bond pairs
    for _i in 0..5 {
        let identity = Address::generate(&env);
        let bond_contract = Address::generate(&env);

        client.register(&identity, &bond_contract);

        // Verify forward lookup
        let entry = client.get_bond_contract(&identity);
        assert_eq!(entry.bond_contract, bond_contract);

        // Verify reverse lookup
        let found_identity = client.get_identity(&bond_contract);
        assert_eq!(found_identity, identity);

        // Verify registration status
        assert_eq!(client.is_registered(&identity), true);
    }

    // Verify all 5 are in the list
    let identities = client.get_all_identities();
    assert_eq!(identities.len(), 5);
}

#[test]
fn test_deactivate_and_reactivate_preserves_mapping() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract);

    // Deactivate
    client.deactivate(&identity);

    // Mappings should still exist
    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert_eq!(entry.active, false);

    let found_identity = client.get_identity(&bond_contract);
    assert_eq!(found_identity, identity);

    // Reactivate
    client.reactivate(&identity);

    // Verify everything is back to active
    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert_eq!(entry.active, true);
}

#[test]
fn test_timestamp_on_registration() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    let before_timestamp = env.ledger().timestamp();

    client.register(&identity, &bond_contract);

    let entry = client.get_bond_contract(&identity);

    // Timestamp should be >= before registration
    assert!(entry.registered_at >= before_timestamp);
}
