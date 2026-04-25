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
#[should_panic(expected = "Error(Contract, #2)")]
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

    let entry = client.register(&identity, &bond_contract, &true);

    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert!(entry.active);
}

#[test]
#[should_panic(expected = "Error(Contract, #400)")]
fn test_register_duplicate_identity() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract1 = Address::generate(&env);
    let bond_contract2 = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract1, &true);
    client.register(&identity, &bond_contract2, &true); // Should panic
}

#[test]
#[should_panic(expected = "Error(Contract, #401)")]
fn test_register_duplicate_bond_contract() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity1, &bond_contract, &true);
    client.register(&identity2, &bond_contract, &true); // Should panic
}

#[test]
fn test_get_bond_contract() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract, &true);

    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert_eq!(entry.identity, identity);
}

#[test]
#[should_panic(expected = "Error(Contract, #402)")]
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

    client.register(&identity, &bond_contract, &true);

    let found_identity = client.get_identity(&bond_contract);
    assert_eq!(found_identity, identity);
}

#[test]
#[should_panic(expected = "Error(Contract, #403)")]
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
    assert!(!client.is_registered(&identity));

    // Register
    client.register(&identity, &bond_contract, &true);

    // Now registered
    assert!(client.is_registered(&identity));
}

#[test]
fn test_deactivate() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract, &true);
    assert!(client.is_registered(&identity));

    client.deactivate(&identity);
    assert!(!client.is_registered(&identity));

    // Entry should still exist but be inactive
    let entry = client.get_bond_contract(&identity);
    assert!(!entry.active);
}

#[test]
#[should_panic(expected = "Error(Contract, #404)")]
fn test_deactivate_twice() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract, &true);
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

    client.register(&identity, &bond_contract, &true);
    client.deactivate(&identity);
    assert!(!client.is_registered(&identity));

    client.reactivate(&identity);
    assert!(client.is_registered(&identity));

    let entry = client.get_bond_contract(&identity);
    assert!(entry.active);
}

#[test]
#[should_panic(expected = "Error(Contract, #405)")]
fn test_reactivate_already_active() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract, &true);
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
    client.register(&identity1, &bond_contract1, &true);

    let identity2 = Address::generate(&env);
    let bond_contract2 = Address::generate(&env);
    client.register(&identity2, &bond_contract2, &true);

    let identity3 = Address::generate(&env);
    let bond_contract3 = Address::generate(&env);
    client.register(&identity3, &bond_contract3, &true);

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
    client.register(&identity, &bond_contract, &true);

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

    client.register(&identity, &bond_contract, &true);

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

        client.register(&identity, &bond_contract, &true);

        // Verify forward lookup
        let entry = client.get_bond_contract(&identity);
        assert_eq!(entry.bond_contract, bond_contract);

        // Verify reverse lookup
        let found_identity = client.get_identity(&bond_contract);
        assert_eq!(found_identity, identity);

        // Verify registration status
        assert!(client.is_registered(&identity));
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

    client.register(&identity, &bond_contract, &true);

    // Deactivate
    client.deactivate(&identity);

    // Mappings should still exist
    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert!(!entry.active);

    let found_identity = client.get_identity(&bond_contract);
    assert_eq!(found_identity, identity);

    // Reactivate
    client.reactivate(&identity);

    // Verify everything is back to active
    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert!(entry.active);
}

#[test]
fn test_timestamp_on_registration() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    let before_timestamp = env.ledger().timestamp();

    client.register(&identity, &bond_contract, &true);

    let entry = client.get_bond_contract(&identity);

    // Timestamp should be >= before registration
    assert!(entry.registered_at >= before_timestamp);
}

// -- Issue #139: duplicate asset listing prevention

#[test]
fn test_registered_identities_no_duplicates_after_deactivate_reregister() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract, &true);
    client.deactivate(&identity);

    // Attempt to re-register same identity must fail (error #400).
    let result = client.try_register(&identity, &bond_contract, &true);
    assert!(
        result.is_err(),
        "Re-registering an existing identity must fail"
    );

    // List must still have exactly one entry.
    let all = client.get_all_identities();
    assert_eq!(
        all.len(),
        1,
        "RegisteredIdentities must not contain duplicates"
    );
}

#[test]
fn test_registered_identities_list_length_matches_unique_registrations() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    env.mock_all_auths();

    let id1 = Address::generate(&env);
    let bond1 = Address::generate(&env);
    let id2 = Address::generate(&env);
    let bond2 = Address::generate(&env);
    let id3 = Address::generate(&env);
    let bond3 = Address::generate(&env);

    client.register(&id1, &bond1, &true);
    client.register(&id2, &bond2, &true);
    client.register(&id3, &bond3, &true);

    client.deactivate(&id2);

    let all = client.get_all_identities();
    assert_eq!(
        all.len(),
        3,
        "List length must reflect all registered identities including deactivated ones"
    );

    assert_eq!(client.get_identity(&bond1), id1);
    assert_eq!(client.get_identity(&bond2), id2);
    assert_eq!(client.get_identity(&bond3), id3);
}

// -- Issue #181: Check token contract code size before registration

// This test is disabled as zero-address check was removed from the contract
// due to lack of support in this SDK version.

#[test]
fn test_register_valid_contract_should_succeed() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);

    // Create a mock contract that has code
    let mock_contract_id = env.register(CredenceRegistry, ());

    env.mock_all_auths();

    // Should succeed because mock_contract_id is a deployed contract
    let entry = client.register(&identity, &mock_contract_id, &true);

    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, mock_contract_id);
    assert!(entry.active);
}

#[test]
fn test_register_eoa_address_should_succeed() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let eoa_address = Address::generate(&env); // This is an EOA, not a contract

    env.mock_all_auths();

    // Should succeed because we allow non-interface contracts
    // and only validate against zero address
    let entry = client.register(&identity, &eoa_address, &true);

    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, eoa_address);
    assert!(entry.active);
}

#[test]
fn test_register_inactive_identity_initially() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    // Register with active=false
    let entry = client.register(&identity, &bond_contract, &false);

    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert!(!entry.active);
    assert!(!client.is_registered(&identity));

    // Can still retrieve the entry
    let retrieved = client.get_bond_contract(&identity);
    assert!(!retrieved.active);
}

#[test]
fn test_multiple_deactivate_reactivate_cycles() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract, &true);

    // Cycle 1: deactivate -> reactivate
    client.deactivate(&identity);
    assert!(!client.is_registered(&identity));
    client.reactivate(&identity);
    assert!(client.is_registered(&identity));

    // Cycle 2: deactivate -> reactivate
    client.deactivate(&identity);
    assert!(!client.is_registered(&identity));
    client.reactivate(&identity);
    assert!(client.is_registered(&identity));

    // Cycle 3: deactivate -> reactivate
    client.deactivate(&identity);
    assert!(!client.is_registered(&identity));
    client.reactivate(&identity);
    assert!(client.is_registered(&identity));

    // Verify mappings are still intact
    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert!(entry.active);
}

#[test]
fn test_get_all_identities_includes_inactive() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    env.mock_all_auths();

    let identity1 = Address::generate(&env);
    let bond_contract1 = Address::generate(&env);
    client.register(&identity1, &bond_contract1, &true);

    let identity2 = Address::generate(&env);
    let bond_contract2 = Address::generate(&env);
    client.register(&identity2, &bond_contract2, &true);

    let identity3 = Address::generate(&env);
    let bond_contract3 = Address::generate(&env);
    client.register(&identity3, &bond_contract3, &true);

    // Deactivate one identity
    client.deactivate(&identity2);

    // get_all_identities should include all identities (active and inactive)
    let all_identities = client.get_all_identities();
    assert_eq!(all_identities.len(), 3);

    assert!(all_identities.iter().any(|addr| addr == identity1));
    assert!(all_identities.iter().any(|addr| addr == identity2));
    assert!(all_identities.iter().any(|addr| addr == identity3));
}

#[test]
fn test_admin_transfer_chain() {
    let (env, contract_id, admin1) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    env.mock_all_auths();

    // Verify initial admin
    assert_eq!(client.get_admin(), admin1);

    // Transfer to admin2
    client.transfer_admin(&admin2);
    assert_eq!(client.get_admin(), admin2);

    // Transfer to admin3
    client.transfer_admin(&admin3);
    assert_eq!(client.get_admin(), admin3);

    // Admin3 should be able to perform admin operations
    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);
    client.register(&identity, &bond_contract, &true);

    assert!(client.is_registered(&identity));
}

#[test]
fn test_reverse_lookup_after_deactivation() {
    let (env, contract_id, _admin) = setup_registry();
    let client = CredenceRegistryClient::new(&env, &contract_id);

    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    env.mock_all_auths();

    client.register(&identity, &bond_contract, &true);

    // Verify reverse lookup works when active
    let found_identity = client.get_identity(&bond_contract);
    assert_eq!(found_identity, identity);

    // Deactivate
    client.deactivate(&identity);

    // Reverse lookup should still work after deactivation
    let found_identity_after = client.get_identity(&bond_contract);
    assert_eq!(found_identity_after, identity);

    // Forward lookup should also still work
    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond_contract);
    assert!(!entry.active);
}
