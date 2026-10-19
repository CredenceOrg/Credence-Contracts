//! Regression tests for registry uniqueness invariants, remove/reinsert
//! semantics, and deterministic error codes (#400–#405).
//!
//! Covers issue #1003.

use crate::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, CredenceRegistryClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&admin);
    (e, client, admin)
}

// ── #400 IdentityAlreadyRegistered ──────────────────────────────────────

#[test]
fn duplicate_identity_while_active_panics_400() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);
    let bond_1 = Address::generate(&e);
    let bond_2 = Address::generate(&e);

    let entry = client.register(&identity, &bond_1, &true);
    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, bond_1);
    assert!(entry.active);

    // Duplicate identity while active → #400
    let res = client.try_register(&identity, &bond_2, &true);
    assert!(res.is_err(), "duplicate identity must fail");
}

#[test]
fn duplicate_identity_while_deactivated_panics_400() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);
    let bond_1 = Address::generate(&e);
    let bond_2 = Address::generate(&e);

    client.register(&identity, &bond_1, &true);
    client.deactivate(&identity);
    assert!(!client.is_registered(&identity));

    // Deactivated entry still blocks re-registration → #400
    let res = client.try_register(&identity, &bond_2, &true);
    assert!(
        res.is_err(),
        "deactivated identity must still block re-registration"
    );
}

// ── #401 BondContractAlreadyRegistered ──────────────────────────────────

#[test]
fn duplicate_bond_while_active_panics_401() {
    let (e, client, _admin) = setup();
    let id_1 = Address::generate(&e);
    let id_2 = Address::generate(&e);
    let bond = Address::generate(&e);

    client.register(&id_1, &bond, &true);

    // Another identity with same bond → #401
    let res = client.try_register(&id_2, &bond, &true);
    assert!(res.is_err(), "duplicate bond contract must fail");
}

#[test]
fn duplicate_bond_while_deactivated_panics_401() {
    let (e, client, _admin) = setup();
    let id_1 = Address::generate(&e);
    let id_2 = Address::generate(&e);
    let bond = Address::generate(&e);

    client.register(&id_1, &bond, &true);
    client.deactivate(&id_1);

    // Deactivated entry still blocks bond re-use → #401
    let res = client.try_register(&id_2, &bond, &true);
    assert!(
        res.is_err(),
        "deactivated bond must still block re-registration"
    );
}

// ── #402 IdentityNotRegistered ──────────────────────────────────────────

#[test]
fn get_bond_contract_unknown_identity_panics_402() {
    let (e, client, _admin) = setup();
    let unknown = Address::generate(&e);

    let res = client.try_get_bond_contract(&unknown);
    assert!(
        res.is_err(),
        "get_bond_contract on unknown identity must fail"
    );
}

#[test]
fn deactivate_unknown_identity_panics_402() {
    let (e, client, _admin) = setup();
    let unknown = Address::generate(&e);

    let res = client.try_deactivate(&unknown);
    assert!(res.is_err(), "deactivate on unknown identity must fail");
}

#[test]
fn reactivate_unknown_identity_panics_402() {
    let (e, client, _admin) = setup();
    let unknown = Address::generate(&e);

    let res = client.try_reactivate(&unknown);
    assert!(res.is_err(), "reactivate on unknown identity must fail");
}

#[test]
fn remove_unknown_identity_panics_402() {
    let (e, client, _admin) = setup();
    let unknown = Address::generate(&e);

    let res = client.try_remove(&unknown);
    assert!(res.is_err(), "remove on unknown identity must fail");
}

// ── #403 BondContractNotRegistered ──────────────────────────────────────

#[test]
fn get_identity_unknown_bond_panics_403() {
    let (e, client, _admin) = setup();
    let unknown_bond = Address::generate(&e);

    let res = client.try_get_identity(&unknown_bond);
    assert!(res.is_err(), "get_identity on unknown bond must fail");
}

// ── #404 AlreadyDeactivated ─────────────────────────────────────────────

#[test]
fn double_deactivate_panics_404() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);
    let bond = Address::generate(&e);

    client.register(&identity, &bond, &true);
    client.deactivate(&identity);

    let res = client.try_deactivate(&identity);
    assert!(res.is_err(), "double deactivate must fail");
}

// ── #405 AlreadyActive ──────────────────────────────────────────────────

#[test]
fn reactivate_active_entry_panics_405() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);
    let bond = Address::generate(&e);

    client.register(&identity, &bond, &true);

    let res = client.try_reactivate(&identity);
    assert!(res.is_err(), "reactivate on already-active entry must fail");
}

#[test]
fn double_reactivate_panics_405() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);
    let bond = Address::generate(&e);

    client.register(&identity, &bond, &true);
    client.deactivate(&identity);
    client.reactivate(&identity);
    assert!(client.is_registered(&identity));

    let res = client.try_reactivate(&identity);
    assert!(res.is_err(), "double reactivate must fail");
}

// ── Remove / Reinsert Semantics ─────────────────────────────────────────

#[test]
fn remove_frees_identity_and_bond_for_reregistration() {
    let (e, client, _admin) = setup();
    let id_1 = Address::generate(&e);
    let id_2 = Address::generate(&e);
    let bond_a = Address::generate(&e);
    let bond_b = Address::generate(&e);

    // 1. register(id_1, bond_a) → ok
    client.register(&id_1, &bond_a, &true);
    assert!(client.is_registered(&id_1));
    assert_eq!(client.get_identity(&bond_a), id_1);

    // 2. deactivate → soft-delete; id_1 still blocks
    client.deactivate(&id_1);
    assert!(!client.is_registered(&id_1));
    assert!(
        client.try_register(&id_1, &bond_b, &true).is_err(),
        "deactivated identity must block re-registration"
    );
    assert!(
        client.try_register(&id_2, &bond_a, &true).is_err(),
        "deactivated bond must block re-registration"
    );

    // 3. remove(id_1) → hard-delete; both freed
    client.remove(&id_1);
    assert!(
        client.try_get_bond_contract(&id_1).is_err(),
        "identity must be gone after remove"
    );
    assert!(
        client.try_get_identity(&bond_a).is_err(),
        "bond must be gone after remove"
    );

    // 4. register(id_1, bond_b) → ok (fresh entry)
    let entry_new = client.register(&id_1, &bond_b, &true);
    assert_eq!(entry_new.bond_contract, bond_b);
    assert!(entry_new.active);

    // 5. register(id_2, bond_a) → ok (bond_a freed by remove)
    let entry_bond_a = client.register(&id_2, &bond_a, &true);
    assert_eq!(entry_bond_a.identity, id_2);
    assert_eq!(client.get_identity(&bond_a), id_2);

    // Pagination reflects both active identities
    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 2);
}

#[test]
fn remove_works_on_active_entry() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);
    let bond = Address::generate(&e);

    client.register(&identity, &bond, &true);
    assert!(client.is_registered(&identity));

    // Remove while active
    client.remove(&identity);
    assert!(client.try_get_bond_contract(&identity).is_err());
    assert!(client.try_get_identity(&bond).is_err());
}

#[test]
fn remove_cleans_up_allow_non_interface_flag() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);
    let bond = Address::generate(&e);

    // Register with allow_non_interface = true
    client.register(&identity, &bond, &true);

    // Remove entry
    client.remove(&identity);

    // After remove, both id and bond are freed — re-registration succeeds
    let entry = client.register(&identity, &bond, &true);
    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, bond);
}

#[test]
fn remove_after_remove_panics_402() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);
    let bond = Address::generate(&e);

    client.register(&identity, &bond, &true);
    client.remove(&identity);

    // Second remove → #402
    let res = client.try_remove(&identity);
    assert!(res.is_err(), "remove on already-removed identity must fail");
}
