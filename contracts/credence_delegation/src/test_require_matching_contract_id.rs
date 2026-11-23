#![cfg(test)]

//! `contract_id` boundary tests for `domain::verify_payload`.
//!
//! Every delegated (relayer) entry point requires that
//! `payload.contract_id` equal `e.current_contract_address()`, guarding
//! against cross-contract / cross-deployment signature replay. A mismatch
//! panics with `ContractError::ContractIdMismatch` (#221).
//!
//! Three boundary cases are locked in here for every entry point that calls
//! `verify_payload`:
//!
//! - **Right ID** – `payload.contract_id` equals the contract currently
//!   handling the call; the call must succeed.
//! - **Wrong ID** – `payload.contract_id` is the address of a different,
//!   actually-deployed `CredenceDelegation` contract; the call must panic
//!   with `ContractIdMismatch`.
//! - **Unset ID** – `payload.contract_id` is a bare generated address that
//!   was never registered as any contract; the call must panic with
//!   `ContractIdMismatch`.
//!
//! Covered entry points:
//!   `execute_delegated_delegate`, `execute_delegated_revoke`,
//!   `execute_delegated_revoke_attest`

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Env;

fn setup() -> (Env, CredenceDelegationClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client, contract_id)
}

fn make_payload(
    e: &Env,
    domain: DomainTag,
    owner: &Address,
    target: &Address,
    contract_id: &Address,
    nonce: u64,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain,
        owner: owner.clone(),
        target: target.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme: 0,
        ledger_number: e.ledger().sequence(),
        signature_domain: soroban_sdk::String::from_str(e, "CredenceDelegation"),
    }
}

// ── execute_delegated_delegate ──────────────────────────────────────────────

/// Right ID: a payload carrying this contract's own address succeeds.
#[test]
fn delegate_succeeds_with_right_contract_id() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expiry = e.ledger().timestamp() + 86_400;

    let payload = make_payload(&e, DomainTag::Delegate, &owner, &delegate, &contract_id, 0);
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry,
        &payload,
    );
    assert_eq!(client.get_nonce(&owner), 1);
}

/// Wrong ID: a payload carrying a different, real deployed contract's
/// address is rejected before any state change.
#[test]
fn delegate_returns_mismatch_for_wrong_contract_id() {
    let (e, client, _contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expiry = e.ledger().timestamp() + 86_400;

    let other_contract_id = e.register(CredenceDelegation, ());
    let payload = make_payload(
        &e,
        DomainTag::Delegate,
        &owner,
        &delegate,
        &other_contract_id,
        0,
    );

    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry,
            &payload,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::ContractIdMismatch.into());
    assert_eq!(client.get_nonce(&owner), 0);
}

/// Unset ID: a payload carrying an address that was never registered as
/// any contract is rejected.
#[test]
fn delegate_returns_mismatch_for_unset_contract_id() {
    let (e, client, _contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expiry = e.ledger().timestamp() + 86_400;

    let unset_contract_id = Address::generate(&e);
    let payload = make_payload(
        &e,
        DomainTag::Delegate,
        &owner,
        &delegate,
        &unset_contract_id,
        0,
    );

    let err = client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expiry,
            &payload,
        )
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::ContractIdMismatch.into());
    assert_eq!(client.get_nonce(&owner), 0);
}

// ── execute_delegated_revoke ────────────────────────────────────────────────

/// Right ID: revoking with this contract's own address succeeds.
#[test]
fn revoke_succeeds_with_right_contract_id() {
    let (e, client, contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expiry = e.ledger().timestamp() + 86_400;

    let create_payload = make_payload(&e, DomainTag::Delegate, &owner, &delegate, &contract_id, 0);
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expiry,
        &create_payload,
    );

    let revoke_payload = make_payload(
        &e,
        DomainTag::RevokeDelegation,
        &owner,
        &delegate,
        &contract_id,
        1,
    );
    client.execute_delegated_revoke(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &revoke_payload,
    );
    assert_eq!(client.get_nonce(&owner), 2);
}

/// Wrong ID: revoking with a different, real deployed contract's address
/// is rejected before the nonce is consumed.
#[test]
fn revoke_returns_mismatch_for_wrong_contract_id() {
    let (e, client, _contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let other_contract_id = e.register(CredenceDelegation, ());
    let payload = make_payload(
        &e,
        DomainTag::RevokeDelegation,
        &owner,
        &delegate,
        &other_contract_id,
        0,
    );

    let err = client
        .try_execute_delegated_revoke(&owner, &delegate, &DelegationType::Attestation, &payload)
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::ContractIdMismatch.into());
    assert_eq!(client.get_nonce(&owner), 0);
}

/// Unset ID: revoking with an address that was never registered as any
/// contract is rejected.
#[test]
fn revoke_returns_mismatch_for_unset_contract_id() {
    let (e, client, _contract_id) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let unset_contract_id = Address::generate(&e);
    let payload = make_payload(
        &e,
        DomainTag::RevokeDelegation,
        &owner,
        &delegate,
        &unset_contract_id,
        0,
    );

    let err = client
        .try_execute_delegated_revoke(&owner, &delegate, &DelegationType::Attestation, &payload)
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::ContractIdMismatch.into());
    assert_eq!(client.get_nonce(&owner), 0);
}

// ── execute_delegated_revoke_attest ─────────────────────────────────────────

/// Right ID: revoking an attestation with this contract's own address
/// succeeds.
#[test]
fn revoke_attest_succeeds_with_right_contract_id() {
    let (e, client, contract_id) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    // Create the attestation delegation entry first (consumes nonce 0 via direct path)
    let expiry = e.ledger().timestamp() + 86_400;
    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &expiry,
        &0_u64,
    );

    // Revoke via delegated path (nonce 1)
    let payload = make_payload(
        &e,
        DomainTag::RevokeAttestation,
        &attester,
        &subject,
        &contract_id,
        1,
    );
    client.execute_delegated_revoke_attest(&attester, &subject, &payload);
    assert_eq!(client.get_nonce(&attester), 2);
}

/// Wrong ID: revoking an attestation with a different, real deployed
/// contract's address is rejected before the nonce is consumed.
#[test]
fn revoke_attest_returns_mismatch_for_wrong_contract_id() {
    let (e, client, _contract_id) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    let other_contract_id = e.register(CredenceDelegation, ());
    let payload = make_payload(
        &e,
        DomainTag::RevokeAttestation,
        &attester,
        &subject,
        &other_contract_id,
        0,
    );

    let err = client
        .try_execute_delegated_revoke_attest(&attester, &subject, &payload)
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::ContractIdMismatch.into());
    assert_eq!(client.get_nonce(&attester), 0);
}

/// Unset ID: revoking an attestation with an address that was never
/// registered as any contract is rejected.
#[test]
fn revoke_attest_returns_mismatch_for_unset_contract_id() {
    let (e, client, _contract_id) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    let unset_contract_id = Address::generate(&e);
    let payload = make_payload(
        &e,
        DomainTag::RevokeAttestation,
        &attester,
        &subject,
        &unset_contract_id,
        0,
    );

    let err = client
        .try_execute_delegated_revoke_attest(&attester, &subject, &payload)
        .unwrap_err()
        .unwrap();
    assert!(err == ContractError::ContractIdMismatch.into());
    assert_eq!(client.get_nonce(&attester), 0);
}
