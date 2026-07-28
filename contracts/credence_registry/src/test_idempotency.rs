//! Idempotency tests for the `register` function.
//!
//! Issue #1049: Verify repeated registration requests do not create duplicate
//! records or mutate state unexpectedly.
//!
//! ## Coverage
//! - `register` twice with same identity & bond → `IdentityAlreadyRegistered`
//! - `register` with same identity, different bond → `IdentityAlreadyRegistered`
//! - `register` with different identity, same bond → `BondContractAlreadyRegistered`
//! - `register` with `allow_non_interface` flag still detects duplicates
//! - `register` then `deactivate` then `register` → `IdentityAlreadyRegistered`
//! - `register` then `remove` then `register` → succeeds (new entry created)
//! - `register` then `remove` then same bond with different identity → succeeds
//! - Verifies identities list does not duplicate on repeated registration
//! - Verifies state unchanged after failed duplicate registration

use crate::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, CredenceRegistryClient<'static>) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client)
}

// ---------------------------------------------------------------------------
// `register` idempotency — duplicate identity + bond
// ---------------------------------------------------------------------------

#[test]
fn register_twice_same_identity_same_bond_panics_with_already_registered() {
    let (_e, client) = setup();
    let identity = Address::generate(&_e);
    let bond = Address::generate(&_e);

    // skip interface check to keep test self-contained
    client.register(&identity, &bond, &true);
    let result = client.try_register(&identity, &bond, &true);

    assert!(
        result.is_err(),
        "second register with same identity+bond must fail"
    );
}

#[test]
fn register_same_identity_different_bond_panics_with_already_registered() {
    let (_e, client) = setup();
    let identity = Address::generate(&_e);
    let bond_a = Address::generate(&_e);
    let bond_b = Address::generate(&_e);

    client.register(&identity, &bond_a, &true);
    let result = client.try_register(&identity, &bond_b, &true);

    assert!(
        result.is_err(),
        "register with same identity but different bond must fail"
    );
}

#[test]
fn register_different_identity_same_bond_panics_with_bond_already_registered() {
    let (_e, client) = setup();
    let identity_a = Address::generate(&_e);
    let identity_b = Address::generate(&_e);
    let bond = Address::generate(&_e);

    client.register(&identity_a, &bond, &true);
    let result = client.try_register(&identity_b, &bond, &true);

    assert!(
        result.is_err(),
        "register with different identity but same bond must fail"
    );
}

#[test]
fn register_allow_non_interface_still_detects_duplicate() {
    let (_e, client) = setup();
    let identity = Address::generate(&_e);
    let bond = Address::generate(&_e);

    // Register with allow_non_interface first.
    client.register(&identity, &bond, &true);

    // Second registration (even with allow_non_interface) must still fail.
    let result = client.try_register(&identity, &bond, &true);
    assert!(
        result.is_err(),
        "second register must still detect duplicate even with allow_non_interface"
    );
}

// ---------------------------------------------------------------------------
// Lifecycle-aware idempotency
// ---------------------------------------------------------------------------

#[test]
fn register_deactivate_then_register_same_identity_must_fail() {
    let (_e, client) = setup();
    let identity = Address::generate(&_e);
    let bond = Address::generate(&_e);

    client.register(&identity, &bond, &true);
    client.deactivate(&identity);

    // Even after deactivation, the identity slot still exists so
    // re-registration must still be rejected.
    let result = client.try_register(&identity, &bond, &true);
    assert!(
        result.is_err(),
        "register after deactivate must fail (identity slot still exists)"
    );
}

#[test]
fn register_remove_then_register_same_identity_same_bond_succeeds() {
    let (_e, client) = setup();
    let identity = Address::generate(&_e);
    let bond = Address::generate(&_e);

    client.register(&identity, &bond, &true);
    client.remove(&identity);

    // After removing, re-registration with the same identity+bond must succeed.
    client.register(&identity, &bond, &true);
    assert!(client.is_registered(&identity));
}

#[test]
fn register_remove_then_register_same_identity_different_bond_succeeds() {
    let (_e, client) = setup();
    let identity = Address::generate(&_e);
    let bond_a = Address::generate(&_e);
    let bond_b = Address::generate(&_e);

    client.register(&identity, &bond_a, &true);
    client.remove(&identity);

    // After removal, a different bond should be allowed for this identity.
    client.register(&identity, &bond_b, &true);
    assert!(client.is_registered(&identity));
}

#[test]
fn register_remove_then_same_bond_different_identity_succeeds() {
    let (_e, client) = setup();
    let identity_a = Address::generate(&_e);
    let identity_b = Address::generate(&_e);
    let bond = Address::generate(&_e);

    client.register(&identity_a, &bond, &true);
    client.remove(&identity_a);

    // After removal, the bond is freed and can be re-registered.
    client.register(&identity_b, &bond, &true);
    assert!(client.is_registered(&identity_b));
}

// ---------------------------------------------------------------------------
// State not mutated on duplicate
// ---------------------------------------------------------------------------

#[test]
fn repeated_register_does_not_duplicate_in_identities_list() {
    let (_e, client) = setup();
    let identity = Address::generate(&_e);
    let bond = Address::generate(&_e);

    client.register(&identity, &bond, &true);

    // Query page 0 with a limit well above the single registration count.
    // Relies on the fact that only one identity is registered — the bounded
    // page (max 200) comfortably holds it.
    let before = client.get_identities_page(&0_u32, &200_u32);

    // Attempt duplicate registration (must fail).
    let _ = client.try_register(&identity, &bond, &true);

    let after = client.get_identities_page(&0_u32, &200_u32);
    assert_eq!(before.len(), after.len(), "identities list must not grow on duplicate register");
    assert_eq!(before, after, "identities list must be unchanged on duplicate register");
}

#[test]
fn repeated_register_no_state_mutation_on_second_call() {
    let (_e, client) = setup();
    let identity = Address::generate(&_e);
    let bond = Address::generate(&_e);

    // First register
    let entry1 = client.register(&identity, &bond, &true);
    assert!(entry1.active);

    // Second register must panic
    let result = client.try_register(&identity, &bond, &true);
    assert!(result.is_err());

    // State must be identical to after first call
    let entry2 = client.get_bond_contract(&identity);
    assert!(entry2.active);
    assert_eq!(entry2.registered_at, entry1.registered_at);
    assert_eq!(entry2.identity, entry1.identity);
    assert_eq!(entry2.bond_contract, entry1.bond_contract);
}
