//! Comprehensive unit tests for access control modifiers.
//! Covers admin/verifier/identity-owner checks, role composition, unauthorized paths,
//! and access denial event emission.

#![cfg(test)]

extern crate std;

use crate::access_control::{
    add_verifier_role, get_admin, is_admin, is_verifier, remove_verifier_role, require_admin,
    require_admin_or_verifier, require_identity_owner, require_verifier,
};
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{
    contract, contractimpl, symbol_short, vec, Address, Env, IntoVal, Symbol, TryFromVal,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

#[contract]
pub struct AccessControlHarness;

#[contractimpl]
impl AccessControlHarness {
    pub fn initialize(e: Env, admin: Address) {
        e.storage().instance().set(&symbol_short!("admin"), &admin);
    }

    pub fn require_admin_only(e: Env, caller: Address) {
        require_admin(&e, &caller);
    }

    pub fn require_verifier_only(e: Env, caller: Address) {
        require_verifier(&e, &caller);
    }

    pub fn require_identity_owner_only(e: Env, caller: Address, expected: Address) {
        require_identity_owner(&e, &caller, &expected);
    }

    pub fn require_admin_or_verifier_only(e: Env, caller: Address) {
        require_admin_or_verifier(&e, &caller);
    }

    pub fn add_verifier(e: Env, admin: Address, verifier: Address) {
        add_verifier_role(&e, &admin, &verifier);
    }

    pub fn remove_verifier(e: Env, admin: Address, verifier: Address) {
        remove_verifier_role(&e, &admin, &verifier);
    }

    pub fn is_verifier_role(e: Env, address: Address) -> bool {
        is_verifier(&e, &address)
    }

    pub fn is_admin_role(e: Env, address: Address) -> bool {
        is_admin(&e, &address)
    }

    pub fn current_admin(e: Env) -> Address {
        get_admin(&e)
    }
}

fn setup(e: &Env) -> (AccessControlHarnessClient<'_>, Address) {
    let contract_id = e.register_contract(None, AccessControlHarness);
    let client = AccessControlHarnessClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin)
}

fn count_access_denied_events(e: &Env, contract_id: &Address, role: &str, code: u32) -> u32 {
    let expected_topics = vec![e, Symbol::new(e, "access_denied").into_val(e)];

    e.events()
        .all()
        .iter()
        .filter(|(contract, topics, data)| {
            if contract != contract_id || topics != &expected_topics {
                return false;
            }

            let parsed = <(Address, Symbol, u32)>::try_from_val(e, data);
            match parsed {
                Ok((_, role_symbol, error_code)) => {
                    role_symbol == Symbol::new(e, role) && error_code == code
                }
                Err(_) => false,
            }
        })
        .count() as u32
}

#[test]
fn test_require_admin_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.require_admin_only(&admin);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_require_admin_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let unauthorized = Address::generate(&e);
    client.require_admin_only(&unauthorized);
}

#[test]
#[should_panic(expected = "not initialized")]
fn test_require_admin_not_initialized() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AccessControlHarness);
    let client = AccessControlHarnessClient::new(&e, &contract_id);

    let caller = Address::generate(&e);
    client.require_admin_only(&caller);
}

#[test]
fn test_add_and_remove_verifier_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let verifier = Address::generate(&e);
    client.add_verifier(&admin, &verifier);
    assert!(client.is_verifier_role(&verifier));

    client.remove_verifier(&admin, &verifier);
    assert!(!client.is_verifier_role(&verifier));
}

#[test]
#[should_panic(expected = "not admin")]
fn test_add_verifier_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let unauthorized = Address::generate(&e);
    let verifier = Address::generate(&e);
    client.add_verifier(&unauthorized, &verifier);
}

#[test]
fn test_require_verifier_success() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let verifier = Address::generate(&e);
    client.add_verifier(&admin, &verifier);
    client.require_verifier_only(&verifier);
}

#[test]
#[should_panic(expected = "not verifier")]
fn test_require_verifier_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let unauthorized = Address::generate(&e);
    client.require_verifier_only(&unauthorized);
}

#[test]
fn test_require_identity_owner_success() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let identity = Address::generate(&e);
    client.require_identity_owner_only(&identity, &identity);
}

#[test]
#[should_panic(expected = "not identity owner")]
fn test_require_identity_owner_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let identity = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    client.require_identity_owner_only(&unauthorized, &identity);
}

#[test]
fn test_require_admin_or_verifier_success_for_admin() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.require_admin_or_verifier_only(&admin);
}

#[test]
fn test_require_admin_or_verifier_success_for_verifier() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let verifier = Address::generate(&e);
    client.add_verifier(&admin, &verifier);
    client.require_admin_or_verifier_only(&verifier);
}

#[test]
#[should_panic(expected = "not authorized")]
fn test_require_admin_or_verifier_unauthorized() {
    let e = Env::default();
    let (client, _) = setup(&e);

    let unauthorized = Address::generate(&e);
    client.require_admin_or_verifier_only(&unauthorized);
}

#[test]
fn test_admin_read_helpers() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let non_admin = Address::generate(&e);
    assert!(client.is_admin_role(&admin));
    assert!(!client.is_admin_role(&non_admin));
    assert_eq!(client.current_admin(), admin);
}

#[test]
fn test_multiple_verifiers() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let verifier_1 = Address::generate(&e);
    let verifier_2 = Address::generate(&e);
    let verifier_3 = Address::generate(&e);

    client.add_verifier(&admin, &verifier_1);
    client.add_verifier(&admin, &verifier_2);
    client.add_verifier(&admin, &verifier_3);

    assert!(client.is_verifier_role(&verifier_1));
    assert!(client.is_verifier_role(&verifier_2));
    assert!(client.is_verifier_role(&verifier_3));

    client.remove_verifier(&admin, &verifier_2);

    assert!(client.is_verifier_role(&verifier_1));
    assert!(!client.is_verifier_role(&verifier_2));
    assert!(client.is_verifier_role(&verifier_3));
}

#[test]
fn test_access_denied_event_for_not_admin() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AccessControlHarness);
    let client = AccessControlHarnessClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    client.initialize(&admin);

    let denied_before = count_access_denied_events(&e, &contract_id, "admin", 1);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        client.require_admin_only(&unauthorized);
    }));

    let denied_after = count_access_denied_events(&e, &contract_id, "admin", 1);
    assert_eq!(denied_before + 1, denied_after);
}

#[test]
fn test_access_denied_event_for_not_verifier() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AccessControlHarness);
    let client = AccessControlHarnessClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    client.initialize(&admin);

    let denied_before = count_access_denied_events(&e, &contract_id, "verifier", 2);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        client.require_verifier_only(&unauthorized);
    }));

    let denied_after = count_access_denied_events(&e, &contract_id, "verifier", 2);
    assert_eq!(denied_before + 1, denied_after);
}

#[test]
fn test_access_denied_event_for_not_identity_owner() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AccessControlHarness);
    let client = AccessControlHarnessClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    let owner = Address::generate(&e);
    client.initialize(&admin);

    let denied_before = count_access_denied_events(&e, &contract_id, "identity_owner", 3);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        client.require_identity_owner_only(&unauthorized, &owner);
    }));

    let denied_after = count_access_denied_events(&e, &contract_id, "identity_owner", 3);
    assert_eq!(denied_before + 1, denied_after);
}

#[test]
fn test_access_denied_event_for_admin_or_verifier() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AccessControlHarness);
    let client = AccessControlHarnessClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    client.initialize(&admin);

    let denied_before = count_access_denied_events(&e, &contract_id, "admin_or_verifier", 2);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        client.require_admin_or_verifier_only(&unauthorized);
    }));

    let denied_after = count_access_denied_events(&e, &contract_id, "admin_or_verifier", 2);
    assert_eq!(denied_before + 1, denied_after);
}
