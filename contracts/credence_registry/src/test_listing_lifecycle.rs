//! # Listing lifecycle tests — `contracts/credence_registry/src/lib.rs`
//!
//! Tests the full listing state after each lifecycle step (deactivate,
//! reactivate, and remove) to ensure the four listing functions behave
//! correctly:
//!
//! - `get_identities_page`: paginated identity list from the insertion index
//! - `get_identity_count`: total number of identities in the index
//! - `is_registered`: checks if identity is registered AND active
//! - `get_identity`: reverse lookup from bond contract to identity
//!
//! ## Expected state matrix
//!
//! | Operation   | get_identities_page | get_identity_count | is_registered | get_identity |
//! |-------------|---------------------|-------------------|---------------|--------------|
//! | register    | includes identity   | count includes    | true          | returns id   |
//! | deactivate  | includes identity   | unchanged         | false         | returns id   |
//! | reactivate  | includes identity   | unchanged         | true          | returns id   |
//! | remove      | excludes identity   | decremented       | false         | panics       |
//!
//! The matrix is covered for all three transitions (deactivate, reactivate,
//! remove) with multiple identities to verify pagination behavior.

#![allow(clippy::disallowed_macros)]

use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

/// Fresh, initialized registry with admin auth mocked.
fn setup(env: &Env) -> (CredenceRegistryClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Register an identity and return the identity and bond contract addresses.
fn register_identity(
    env: &Env,
    client: &CredenceRegistryClient<'_>,
) -> (Address, Address) {
    let identity = Address::generate(env);
    let bond = Address::generate(env);
    client.register(&identity, &bond, &true);
    (identity, bond)
}

// ─────────────────────────────────────────────────────────────────────────────
// Deactivate transition
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn deactivate_preserves_index_and_reverse_mapping() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Register two identities to test pagination
    let (id1, bond1) = register_identity(&env, &client);
    let (id2, _bond2) = register_identity(&env, &client);

    // Initial state after registration
    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), id1);
    assert_eq!(page.get(1).unwrap(), id2);
    assert_eq!(client.get_identity_count(), 2);
    assert!(client.is_registered(&id1));
    assert_eq!(client.get_identity(&bond1), id1);

    // Deactivate first identity
    client.deactivate(&id1);

    // After deactivate: index unchanged, count unchanged, is_registered false,
    // reverse mapping still works
    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 2, "page should still include deactivated identity");
    assert_eq!(page.get(0).unwrap(), id1, "deactivated identity still in index");
    assert_eq!(page.get(1).unwrap(), id2);
    assert_eq!(
        client.get_identity_count(),
        2,
        "count unchanged after deactivate"
    );
    assert!(
        !client.is_registered(&id1),
        "is_registered false for deactivated identity"
    );
    assert_eq!(
        client.get_identity(&bond1),
        id1,
        "reverse mapping still works after deactivate"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Reactivate transition
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn reactivate_restores_active_flag_without_changing_index() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Register and deactivate an identity
    let (id1, bond1) = register_identity(&env, &client);
    let (id2, _bond2) = register_identity(&env, &client);
    client.deactivate(&id1);

    // Reactivate the identity
    client.reactivate(&id1);

    // After reactivate: index unchanged, count unchanged, is_registered true,
    // reverse mapping still works
    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), id1);
    assert_eq!(page.get(1).unwrap(), id2);
    assert_eq!(
        client.get_identity_count(),
        2,
        "count unchanged after reactivate"
    );
    assert!(
        client.is_registered(&id1),
        "is_registered true after reactivate"
    );
    assert_eq!(
        client.get_identity(&bond1),
        id1,
        "reverse mapping works after reactivate"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Remove transition
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn remove_drops_identity_from_index_and_clears_reverse_mapping() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Register two identities
    let (id1, bond1) = register_identity(&env, &client);
    let (id2, _bond2) = register_identity(&env, &client);

    // Remove first identity
    client.remove(&id1);

    // After remove: identity excluded from page, count decremented,
    // is_registered false, get_identity panics
    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 1, "page should not include removed identity");
    assert_eq!(
        page.get(0).unwrap(),
        id2,
        "remaining identity moved to position 0"
    );
    assert_eq!(
        client.get_identity_count(),
        1,
        "count decremented after remove"
    );
    assert!(
        !client.is_registered(&id1),
        "is_registered false for removed identity"
    );

    // get_identity should panic for removed bond
    let result = client.try_get_identity(&bond1);
    assert!(result.is_err(), "get_identity must panic after remove");
}

// ─────────────────────────────────────────────────────────────────────────────
// Full lifecycle: register → deactivate → reactivate → remove
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn full_lifecycle_register_deactivate_reactivate_remove() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Register
    let (id1, bond1) = register_identity(&env, &client);
    let (id2, _bond2) = register_identity(&env, &client);

    // State after register
    assert_eq!(client.get_identities_page(&0, &10).len(), 2);
    assert_eq!(client.get_identity_count(), 2);
    assert!(client.is_registered(&id1));
    assert_eq!(client.get_identity(&bond1), id1);

    // Deactivate
    client.deactivate(&id1);
    assert_eq!(client.get_identities_page(&0, &10).len(), 2);
    assert_eq!(client.get_identity_count(), 2);
    assert!(!client.is_registered(&id1));
    assert_eq!(client.get_identity(&bond1), id1);

    // Reactivate
    client.reactivate(&id1);
    assert_eq!(client.get_identities_page(&0, &10).len(), 2);
    assert_eq!(client.get_identity_count(), 2);
    assert!(client.is_registered(&id1));
    assert_eq!(client.get_identity(&bond1), id1);

    // Remove
    client.remove(&id1);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
    assert_eq!(client.get_identity_count(), 1);
    assert!(!client.is_registered(&id1));
    assert!(client.try_get_identity(&bond1).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Pagination behavior with lifecycle operations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn pagination_with_deactivate_and_reactivate() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Register 5 identities
    let mut identities = Vec::new(&env);
    let mut bonds = Vec::new(&env);
    for _ in 0..5 {
        let (id, bond) = register_identity(&env, &client);
        identities.push_back(id.clone());
        bonds.push_back(bond);
    }

    // Get first page (3 items)
    let page1 = client.get_identities_page(&0, &3);
    assert_eq!(page1.len(), 3);
    assert_eq!(page1.get(0).unwrap(), identities.get(0).unwrap());
    assert_eq!(page1.get(1).unwrap(), identities.get(1).unwrap());
    assert_eq!(page1.get(2).unwrap(), identities.get(2).unwrap());

    // Deactivate identity at index 1
    client.deactivate(&identities.get(1).unwrap());

    // Page should still include deactivated identity
    let page1_after = client.get_identities_page(&0, &3);
    assert_eq!(page1_after.len(), 3);
    assert_eq!(page1_after.get(0).unwrap(), identities.get(0).unwrap());
    assert_eq!(page1_after.get(1).unwrap(), identities.get(1).unwrap());
    assert_eq!(page1_after.get(2).unwrap(), identities.get(2).unwrap());

    // Reactivate identity at index 1
    client.reactivate(&identities.get(1).unwrap());

    // Page unchanged
    let page1_reactivate = client.get_identities_page(&0, &3);
    assert_eq!(page1_reactivate.len(), 3);
    assert_eq!(page1_reactivate.get(0).unwrap(), identities.get(0).unwrap());
    assert_eq!(page1_reactivate.get(1).unwrap(), identities.get(1).unwrap());
    assert_eq!(page1_reactivate.get(2).unwrap(), identities.get(2).unwrap());
}

#[test]
fn pagination_with_remove_swap_and_pop() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Register 5 identities
    let mut identities = Vec::new(&env);
    let mut bonds = Vec::new(&env);
    for _ in 0..5 {
        let (id, bond) = register_identity(&env, &client);
        identities.push_back(id.clone());
        bonds.push_back(bond);
    }

    // Remove identity at index 1 (middle of list)
    client.remove(&identities.get(1).unwrap());

    // Count decremented
    assert_eq!(client.get_identity_count(), 4);

    // Page should have 4 items, with tail identity moved to position 1
    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 4);
    assert_eq!(page.get(0).unwrap(), identities.get(0).unwrap());
    // Position 1 now has the tail identity (index 4)
    assert_eq!(page.get(1).unwrap(), identities.get(4).unwrap());
    assert_eq!(page.get(2).unwrap(), identities.get(2).unwrap());
    assert_eq!(page.get(3).unwrap(), identities.get(3).unwrap());

    // Reverse mapping for removed bond should panic
    let removed_bond = bonds.get(1).unwrap();
    assert!(client.try_get_identity(&removed_bond).is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge cases
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn deactivate_reactivate_single_identity() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let (id, bond) = register_identity(&env, &client);

    // Deactivate single identity
    client.deactivate(&id);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
    assert_eq!(client.get_identity_count(), 1);
    assert!(!client.is_registered(&id));
    assert_eq!(client.get_identity(&bond), id);

    // Reactivate single identity
    client.reactivate(&id);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
    assert_eq!(client.get_identity_count(), 1);
    assert!(client.is_registered(&id));
    assert_eq!(client.get_identity(&bond), id);
}

#[test]
fn remove_single_identity() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let (id, bond) = register_identity(&env, &client);

    // Remove single identity
    client.remove(&id);
    assert_eq!(client.get_identities_page(&0, &10).len(), 0);
    assert_eq!(client.get_identity_count(), 0);
    assert!(!client.is_registered(&id));
    assert!(client.try_get_identity(&bond).is_err());
}
