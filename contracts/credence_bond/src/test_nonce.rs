use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Env};

use crate::{nonce, CredenceBond, DataKey};

fn register_contract(env: &Env) -> Address {
    env.register(CredenceBond, ())
}

#[test]
fn test_nonce_lifecycle() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        assert_eq!(nonce::get_nonce(&e, &user), 0);
        nonce::consume_nonce(&e, &user, 0);
        assert_eq!(nonce::get_nonce(&e, &user), 1);
    });
}

#[test]
#[should_panic(expected = "invalid nonce")]
fn test_nonce_rejects_replay() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        nonce::consume_nonce(&e, &user, 0);
        nonce::consume_nonce(&e, &user, 0);
    });
}

#[test]
#[should_panic(expected = "signature expired")]
fn test_require_not_expired_panics() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);

    e.as_contract(&contract_id, || {
        e.ledger().with_mut(|l| l.timestamp = 100);
        nonce::require_not_expired(&e, 50);
    });
}

#[test]
fn test_require_not_expired_with_grace() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);

    e.as_contract(&contract_id, || {
        e.storage().instance().set(&DataKey::GraceWindow, &10u64);
        e.ledger().with_mut(|l| l.timestamp = 105);
        nonce::require_not_expired(&e, 100);
    });
}

#[test]
#[should_panic(expected = "domain mismatch")]
fn test_require_domain_match_panics() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let other_contract = Address::generate(&e);

    e.as_contract(&contract_id, || {
        nonce::require_domain_match(&e, &other_contract);
    });
}

#[test]
fn test_validate_and_consume() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        e.ledger().with_mut(|l| l.timestamp = 100);
        nonce::validate_and_consume(&e, &user, &contract_id, 150, 0);
        assert_eq!(nonce::get_nonce(&e, &user), 1);
    });
}

#[test]
fn test_validate_and_consume_with_grace() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = register_contract(&e);
    let user = Address::generate(&e);

    e.as_contract(&contract_id, || {
        e.ledger().with_mut(|l| l.timestamp = 120);
        nonce::validate_and_consume_with_grace(&e, &user, &contract_id, 110, 0, 15);
        assert_eq!(nonce::get_nonce(&e, &user), 1);
    });
}
