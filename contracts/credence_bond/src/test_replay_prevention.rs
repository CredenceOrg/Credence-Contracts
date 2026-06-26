//! Tests for replay attack prevention: nonce validation, deadline enforcement,
//! domain (contract address) binding, and rejection of replayed transactions.

use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Env, String};
use std::panic::AssertUnwindSafe;

fn setup(
    e: &Env,
) -> (
    CredenceBondClient<'_>,
    soroban_sdk::Address,
    soroban_sdk::Address,
) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = soroban_sdk::Address::generate(e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(e);
    client.register_attester(&attester);
    (client, attester, contract_id)
}

// ── Nonce tests ──────────────────────────────────────────────────────────────

#[test]
fn nonce_starts_at_zero() {
    let e = Env::default();
    let (client, attester, _) = setup(&e);
    assert_eq!(client.get_nonce(&attester), 0);
}

#[test]
fn nonce_increments_after_add_attestation() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 1000;

    assert_eq!(client.get_nonce(&attester), 0);
    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "d"),
        &contract_id,
        &deadline,
        &0u64,
    );
    assert_eq!(client.get_nonce(&attester), 1);
    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "d2"),
        &contract_id,
        &deadline,
        &1u64,
    );
    assert_eq!(client.get_nonce(&attester), 2);
}

#[test]
#[should_panic(expected = "invalid nonce")]
fn replay_add_attestation_rejected() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let data = String::from_str(&e, "once");
    let deadline = e.ledger().timestamp() + 1000;

    client.add_attestation(&attester, &subject, &data, &contract_id, &deadline, &0u64);
    // Reuse nonce 0 — must be rejected.
    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "other"),
        &contract_id,
        &deadline,
        &0u64,
    );
}

#[test]
#[should_panic(expected = "invalid nonce")]
fn wrong_nonce_rejected() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 1000;
    // Nonce 0 is correct; submitting 1 must fail.
    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "x"),
        &contract_id,
        &deadline,
        &1u64,
    );
}

// ── Deadline tests ────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "signature expired")]
fn expired_deadline_rejected_on_add_attestation() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    // Advance ledger past the deadline.
    e.ledger().with_mut(|l| l.timestamp = 2000);
    let expired_deadline = 1000u64; // already in the past

    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "late"),
        &contract_id,
        &expired_deadline,
        &0u64,
    );
}

#[test]
fn expired_deadline_does_not_consume_nonce_on_add_attestation() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = 1000u64;
    e.ledger().with_mut(|l| l.timestamp = 2000);

    let initial_nonce = client.get_nonce(&attester);
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.add_attestation(
            &attester,
            &subject,
            &String::from_str(&e, "late"),
            &contract_id,
            &deadline,
            &initial_nonce,
        );
    }));

    assert!(result.is_err());
    assert_eq!(client.get_nonce(&attester), initial_nonce);
}

#[test]
fn wrong_contract_does_not_consume_nonce_on_revoke() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 5000;

    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "r"),
        &contract_id,
        &deadline,
        &0u64,
    );
    let nonce_before = client.get_nonce(&attester);
    let wrong_contract = soroban_sdk::Address::generate(&e);

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.revoke_attestation(
            &attester,
            &att.id,
            &wrong_contract,
            &deadline,
            &nonce_before,
        );
    }));

    assert!(result.is_err());
    assert_eq!(client.get_nonce(&attester), nonce_before);
}

#[test]
#[should_panic(expected = "signature expired")]
fn expired_deadline_rejected_on_revoke() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 5000;

    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "rev"),
        &contract_id,
        &deadline,
        &0u64,
    );

    // Advance ledger so the revoke deadline is expired.
    e.ledger().with_mut(|l| l.timestamp = 9999);
    let expired_deadline = 5000u64;

    client.revoke_attestation(&attester, &att.id, &contract_id, &expired_deadline, &1u64);
}

#[test]
fn deadline_at_exact_timestamp_accepted() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);

    // deadline == now is valid (not yet expired).
    let now = e.ledger().timestamp();
    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "exact"),
        &contract_id,
        &now,
        &0u64,
    );
}

// ── Domain (contract address) tests ──────────────────────────────────────────

#[test]
#[should_panic(expected = "domain mismatch")]
fn wrong_contract_address_rejected_on_add_attestation() {
    let e = Env::default();
    let (client, attester, _contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 1000;

    // Pass a random address instead of the real contract address.
    let wrong_contract = soroban_sdk::Address::generate(&e);
    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "cross"),
        &wrong_contract,
        &deadline,
        &0u64,
    );
}

#[test]
fn wrong_contract_does_not_consume_nonce_on_add_attestation() {
    let e = Env::default();
    let (client, attester, _contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 1000;
    let initial_nonce = client.get_nonce(&attester);
    let wrong_contract = soroban_sdk::Address::generate(&e);

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.add_attestation(
            &attester,
            &subject,
            &String::from_str(&e, "cross"),
            &wrong_contract,
            &deadline,
            &initial_nonce,
        );
    }));

    assert!(result.is_err());
    assert_eq!(client.get_nonce(&attester), initial_nonce);
}

#[test]
#[should_panic(expected = "domain mismatch")]
fn wrong_contract_address_rejected_on_revoke() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 5000;

    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "r"),
        &contract_id,
        &deadline,
        &0u64,
    );

    let wrong_contract = soroban_sdk::Address::generate(&e);
    client.revoke_attestation(&attester, &att.id, &wrong_contract, &deadline, &1u64);
}

// ── Revoke nonce tests ────────────────────────────────────────────────────────

#[test]
fn nonce_increments_after_revoke() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 5000;

    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "rev"),
        &contract_id,
        &deadline,
        &client.get_nonce(&attester),
    );
    let nonce_before = client.get_nonce(&attester);
    client.revoke_attestation(&attester, &att.id, &contract_id, &deadline, &nonce_before);
    assert_eq!(client.get_nonce(&attester), nonce_before + 1);
}

#[test]
#[should_panic(expected = "invalid nonce")]
fn replay_revoke_rejected() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 5000;

    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "r"),
        &contract_id,
        &deadline,
        &client.get_nonce(&attester),
    );
    let revoke_nonce = client.get_nonce(&attester);
    client.revoke_attestation(&attester, &att.id, &contract_id, &deadline, &revoke_nonce);
    // Replay with the same (now stale) nonce — must be rejected.
    client.revoke_attestation(&attester, &att.id, &contract_id, &deadline, &revoke_nonce);
}

// ── Cross-namespace replay tests: delegation ↔ bond ─────────────────────────

/// Replay attempt: a payload bound to the delegation contract namespace cannot
/// be submitted through the bond signed add-attestation path, and the rejected
/// attempt must not consume the bond nonce.
#[test]
fn delegation_namespace_replay_rejected_on_add_attestation_without_consuming_bond_nonce() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let delegation_contract_id = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 1000;

    let result = client.try_add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "delegation-replay"),
        &delegation_contract_id,
        &deadline,
        &0_u64,
    );
    assert!(
        result.is_err(),
        "delegation namespace must not execute in bond"
    );
    assert_eq!(client.get_nonce(&attester), 0);

    client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "bond-valid"),
        &contract_id,
        &deadline,
        &0_u64,
    );
    assert_eq!(client.get_nonce(&attester), 1);
}

/// Replay attempt: a payload bound to the delegation contract namespace cannot
/// be submitted through the bond signed revoke path, and the rejected attempt
/// leaves the current revoke nonce available for the valid bond namespace.
#[test]
fn delegation_namespace_replay_rejected_on_revoke_without_consuming_bond_nonce() {
    let e = Env::default();
    let (client, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let delegation_contract_id = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 1000;

    let attestation = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "bond-valid"),
        &contract_id,
        &deadline,
        &0_u64,
    );
    assert_eq!(client.get_nonce(&attester), 1);

    let result = client.try_revoke_attestation(
        &attester,
        &attestation.id,
        &delegation_contract_id,
        &deadline,
        &1_u64,
    );
    assert!(
        result.is_err(),
        "delegation namespace must not revoke in bond"
    );
    assert_eq!(client.get_nonce(&attester), 1);

    client.revoke_attestation(&attester, &attestation.id, &contract_id, &deadline, &1_u64);
    assert_eq!(client.get_nonce(&attester), 2);
}
