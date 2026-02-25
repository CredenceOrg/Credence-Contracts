//! Tests for replay attack prevention: nonce validation and rejection of replayed transactions.

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Env, String};

fn setup(e: &Env) -> (CredenceBondClient<'_>, soroban_sdk::Address) {
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = soroban_sdk::Address::generate(e);
    client.initialize(&admin);
    let attester = soroban_sdk::Address::generate(e);
    client.register_attester(&attester);
    (client, attester)
}

#[test]
fn nonce_starts_at_zero() {
    let e = Env::default();
    let (client, attester) = setup(&e);
    assert_eq!(client.get_nonce(&attester), 0);
}

#[test]
fn nonce_increments_after_add_attestation() {
    let e = Env::default();
    let (client, attester) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    assert_eq!(client.get_nonce(&attester), 0);
    client.add_attestation(&attester, &subject, &String::from_str(&e, "d"), &0u64);
    assert_eq!(client.get_nonce(&attester), 1);
    client.add_attestation(&attester, &subject, &String::from_str(&e, "d2"), &1u64);
    assert_eq!(client.get_nonce(&attester), 2);
}

#[test]
#[should_panic(expected = "invalid nonce")]
fn replay_add_attestation_rejected() {
    let e = Env::default();
    let (client, attester) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let data = String::from_str(&e, "once");
    client.add_attestation(&attester, &subject, &data, &0u64);
    client.add_attestation(&attester, &subject, &data, &0u64);
}

#[test]
#[should_panic(expected = "invalid nonce")]
fn wrong_nonce_rejected() {
    let e = Env::default();
    let (client, attester) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    client.add_attestation(&attester, &subject, &String::from_str(&e, "x"), &1u64);
}

#[test]
fn nonce_increments_after_revoke() {
    let e = Env::default();
    let (client, attester) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "rev"),
        &client.get_nonce(&attester),
    );
    let nonce_before = client.get_nonce(&attester);
    client.revoke_attestation(&attester, &att.id, &nonce_before);
    assert_eq!(client.get_nonce(&attester), nonce_before + 1);
}

#[test]
#[should_panic(expected = "invalid nonce")]
fn replay_revoke_rejected() {
    let e = Env::default();
    let (client, attester) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "r"),
        &client.get_nonce(&attester),
    );
    let used_nonce = client.get_nonce(&attester) - 1;
    client.revoke_attestation(&attester, &att.id, &used_nonce);
    client.revoke_attestation(&attester, &att.id, &used_nonce);
}
