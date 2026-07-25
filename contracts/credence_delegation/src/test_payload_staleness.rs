//! Negative tests for the signed-operation staleness check.
//!
//! # Threat being mitigated
//!
//! Without a `ledger_number` bound, an attacker who captures a valid signed
//! `DelegatedActionPayload` (e.g. by monitoring the p2p network, bribing a
//! relayer, or exploiting a mempool information leak) can hold the payload and
//! submit it at any future ledger — silently creating or revoking a delegation
//! on behalf of the signer — as long as the nonce has not been consumed by
//! another operation.  The window is unbounded: a payload signed today could be
//! replayed months later.
//!
//! # Fix
//!
//! `DelegatedActionPayload.ledger_number` records the Stellar ledger sequence
//! at signing time.  Each `execute_delegated_*` entry point calls
//! `domain::check_payload_age` which rejects payloads where
//! `current_sequence - ledger_number > MAX_PAYLOAD_AGE_LEDGERS` (200 ledgers
//! ≈ 17 min).
//!
//! # Tests
//!
//! | Test | Payload age | Expected result |
//! |------|-------------|-----------------|
//! | `stale_delegate_is_rejected` | 201 ledgers old | `Err(PayloadTooOld)` |
//! | `exactly_at_limit_delegate_is_accepted` | 200 ledgers old | `Ok(…)` |
//! | `fresh_delegate_is_accepted` | 0 ledgers old | `Ok(…)` |
//! | `stale_revoke_is_rejected` | 201 ledgers old | `Err(PayloadTooOld)` |
//! | `stale_revoke_attest_is_rejected` | 201 ledgers old | `Err(PayloadTooOld)` |

use super::*;
use credence_errors::ContractError;
use domain::MAX_PAYLOAD_AGE_LEDGERS;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Env;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, CredenceDelegationClient<'static>) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client)
}

/// Advance the ledger sequence by `n` without changing the timestamp meaningfully.
fn advance_sequence(e: &Env, n: u32) {
    e.ledger().with_mut(|info| {
        info.sequence_number = info.sequence_number.saturating_add(n);
        // Advance timestamp proportionally (5 s / ledger) to keep things
        // consistent, but the staleness check only cares about sequence.
        info.timestamp = info.timestamp.saturating_add(u64::from(n) * 5);
    });
}

/// Build a delegate payload whose `ledger_number` is `signed_at_sequence`.
fn delegate_payload(
    e: &Env,
    owner: &Address,
    delegate: &Address,
    contract_id: &Address,
    nonce: u64,
    signed_at_sequence: u32,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain: DomainTag::Delegate,
        owner: owner.clone(),
        target: delegate.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme: 0,
        ledger_number: signed_at_sequence,
    }
}

/// Future expiry (1 day ahead) suitable for delegation tests.
fn expires_at(e: &Env) -> u64 {
    e.ledger().timestamp() + 86_400
}

// ---------------------------------------------------------------------------
// execute_delegated_delegate — staleness tests
// ---------------------------------------------------------------------------

/// A payload signed 201 ledgers ago MUST be rejected with `PayloadTooOld`.
///
/// This is the primary negative test required by the issue acceptance criteria.
/// It fails before the fix (no staleness check) and passes after (check present).
#[test]
fn stale_delegate_is_rejected() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Record the sequence at "signing time".
    let signed_at = e.ledger().sequence();

    // Advance past the staleness window by 1 ledger.
    advance_sequence(&e, MAX_PAYLOAD_AGE_LEDGERS + 1);

    let payload = delegate_payload(&e, &owner, &delegate, &client.address, 0, signed_at);

    let result = client.try_execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at(&e),
        &payload,
    );

    assert!(
        result.is_err(),
        "a payload older than MAX_PAYLOAD_AGE_LEDGERS must be rejected"
    );
    let err = result.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ContractError::PayloadTooOld as u32),
        "rejection must surface as PayloadTooOld (code 510)"
    );
}

/// A payload signed exactly `MAX_PAYLOAD_AGE_LEDGERS` ledgers ago MUST be accepted.
///
/// The boundary is inclusive: age == MAX_PAYLOAD_AGE_LEDGERS is still valid.
#[test]
fn exactly_at_limit_delegate_is_accepted() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let signed_at = e.ledger().sequence();
    advance_sequence(&e, MAX_PAYLOAD_AGE_LEDGERS); // age == limit, not over

    let payload = delegate_payload(&e, &owner, &delegate, &client.address, 0, signed_at);

    client
        .execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Attestation,
            &expires_at(&e),
            &payload,
        );
    // If we reach here the delegation was accepted.
}

/// A fresh payload (age == 0) MUST always be accepted.
#[test]
fn fresh_delegate_is_accepted() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let signed_at = e.ledger().sequence();
    let payload = delegate_payload(&e, &owner, &delegate, &client.address, 0, signed_at);

    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at(&e),
        &payload,
    );
}

// ---------------------------------------------------------------------------
// execute_delegated_revoke — staleness test
// ---------------------------------------------------------------------------

/// A revoke payload signed 201 ledgers ago MUST be rejected.
#[test]
fn stale_revoke_is_rejected() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Create the delegation while ledger is fresh.
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400 * 30), // 30-day expiry
        &0_u64,
    );

    let signed_at = e.ledger().sequence();
    advance_sequence(&e, MAX_PAYLOAD_AGE_LEDGERS + 1);

    let payload = DelegatedActionPayload {
        domain: DomainTag::RevokeDelegation,
        owner: owner.clone(),
        target: delegate.clone(),
        contract_id: client.address.clone(),
        nonce: 1, // nonce was consumed by delegate() above
        scheme: 0,
        ledger_number: signed_at,
    };

    let result = client.try_execute_delegated_revoke(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &payload,
    );

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ContractError::PayloadTooOld as u32),
    );
}

// ---------------------------------------------------------------------------
// execute_delegated_revoke_attest — staleness test
// ---------------------------------------------------------------------------

/// A revoke-attestation payload signed 201 ledgers ago MUST be rejected.
#[test]
fn stale_revoke_attest_is_rejected() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    // Create an attestation-type delegation.
    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400 * 30),
        &0_u64,
    );

    let signed_at = e.ledger().sequence();
    advance_sequence(&e, MAX_PAYLOAD_AGE_LEDGERS + 1);

    let payload = DelegatedActionPayload {
        domain: DomainTag::RevokeAttestation,
        owner: attester.clone(),
        target: subject.clone(),
        contract_id: client.address.clone(),
        nonce: 1,
        scheme: 0,
        ledger_number: signed_at,
    };

    let result = client.try_execute_delegated_revoke_attest(&attester, &subject, &payload);

    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ContractError::PayloadTooOld as u32),
    );
}
