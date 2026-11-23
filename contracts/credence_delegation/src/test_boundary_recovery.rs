//! Boundary and recovery unit tests for CredenceDelegation contract.
//!
//! Validates:
//! 1. `cleanup_expired` boundaries (now < expires_at, now == expires_at, revoked, non-existent, paused).
//! 2. `invalidate_nonce_range` boundaries (equal, lower, span bounds, replay recovery invariant, paused).
//! 3. Delegated payload domain separation & replay invariants (re-execution, domain mismatch, replayed revoke).
//! 4. Verifier registration authorization & scheme bounds (non-admin, unknown scheme, paused).
//! 5. Re-delegation state overwriting (updating active delegation, re-activating revoked delegation).

#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Env, String};

fn setup() -> (Env, CredenceDelegationClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client, admin)
}

fn delegate_payload(
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
        ledger_number: 0,
        signature_domain: String::from_str(e, "CredenceDelegation"),
    }
}

// ---------------------------------------------------------------------------
// 1. `cleanup_expired` Boundaries & Recovery Tests
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #505)")]
fn test_cleanup_expired_rejects_before_expiry() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 1000_u64;

    e.ledger().with_mut(|li| li.timestamp = 500);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    // Call cleanup while now < expires_at
    e.ledger().with_mut(|li| li.timestamp = 999);
    client.cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
}

#[test]
fn test_cleanup_expired_succeeds_at_exact_expiry() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 1000_u64;

    e.ledger().with_mut(|li| li.timestamp = 500);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    // Advance to exact expiry
    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.cleanup_expired(&owner, &delegate, &DelegationType::Attestation);

    // Verify record is removed
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
fn test_cleanup_expired_succeeds_on_revoked_delegation_after_expiry() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 1000_u64;

    e.ledger().with_mut(|li| li.timestamp = 500);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);

    // Advance past expiry
    e.ledger().with_mut(|li| li.timestamp = 1001);
    client.cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
}

#[test]
#[should_panic(expected = "Error(Contract, #504)")]
fn test_cleanup_expired_panics_on_non_existent() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
}

#[test]
#[should_panic(expected = "Error(Contract, #106)")]
fn test_cleanup_expired_rejects_when_paused() {
    let (e, client, admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 1000_u64;

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
    e.ledger().with_mut(|li| li.timestamp = 1001);

    // Pause contract
    client.pause(&admin);

    client.cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
}

// ---------------------------------------------------------------------------
// 2. `invalidate_nonce_range` Boundaries & Recovery Tests
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #501)")]
fn test_invalidate_nonce_range_equal_nonce_rejects() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);
    assert_eq!(client.get_nonce(&identity), 0);

    client.invalidate_nonce_range(&identity, &0_u64);
}

#[test]
#[should_panic(expected = "Error(Contract, #501)")]
fn test_invalidate_nonce_range_lower_nonce_rejects() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);

    client.invalidate_nonce_range(&identity, &5_u64);
    client.invalidate_nonce_range(&identity, &3_u64);
}

#[test]
fn test_invalidate_nonce_range_max_span_succeeds() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);

    // Invalidate up to max allowed span (10,000)
    client.invalidate_nonce_range(&identity, &10_000_u64);
    assert_eq!(client.get_nonce(&identity), 10_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #509)")]
fn test_invalidate_nonce_range_exceeds_max_span_rejects() {
    let (e, client, _admin) = setup();
    let identity = Address::generate(&e);

    client.invalidate_nonce_range(&identity, &10_001_u64);
}

#[test]
#[should_panic(expected = "Error(Contract, #501)")]
fn test_invalidate_nonce_range_recovery_old_payload_rejected() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let contract_id = client.address.clone();
    let expires_at = 2000_u64;

    e.ledger().with_mut(|li| li.timestamp = 1000);

    // Invalidate nonces 0..50
    client.invalidate_nonce_range(&owner, &50_u64);
    assert_eq!(client.get_nonce(&owner), 50);

    // Payload with old nonce 0 must fail with InvalidNonce (#501)
    let old_payload = delegate_payload(&e, DomainTag::Delegate, &owner, &delegate, &contract_id, 0);
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &old_payload,
    );
}

#[test]
fn test_invalidate_nonce_range_recovery_new_payload_succeeds() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let contract_id = client.address.clone();
    let expires_at = 2000_u64;

    e.ledger().with_mut(|li| li.timestamp = 1000);

    // Invalidate nonces 0..50
    client.invalidate_nonce_range(&owner, &50_u64);
    assert_eq!(client.get_nonce(&owner), 50);

    // Payload with valid updated nonce 50 must succeed
    let valid_payload =
        delegate_payload(&e, DomainTag::Delegate, &owner, &delegate, &contract_id, 50);
    let d = client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &valid_payload,
    );
    assert_eq!(d.expires_at, expires_at);
    assert_eq!(client.get_nonce(&owner), 51);
}

#[test]
#[should_panic(expected = "Error(Contract, #106)")]
fn test_invalidate_nonce_range_rejects_when_paused() {
    let (e, client, admin) = setup();
    let identity = Address::generate(&e);

    client.pause(&admin);
    client.invalidate_nonce_range(&identity, &10_u64);
}

// ---------------------------------------------------------------------------
// 3. Delegated Payload Domain & Replay Invariants
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #501)")]
fn test_delegated_payload_replay_rejects() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let contract_id = client.address.clone();
    let expires_at = 2000_u64;

    e.ledger().with_mut(|li| li.timestamp = 1000);

    let payload = delegate_payload(&e, DomainTag::Delegate, &owner, &delegate, &contract_id, 0);

    // First execution succeeds
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &payload,
    );

    // Replay of same payload must fail with InvalidNonce (#501)
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &payload,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #225)")]
fn test_delegated_revoke_domain_mismatch_rejects() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let contract_id = client.address.clone();
    let expires_at = 2000_u64;

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    // Create payload with wrong DomainTag (Delegate instead of RevokeDelegation)
    let payload = delegate_payload(&e, DomainTag::Delegate, &owner, &delegate, &contract_id, 1);

    client.execute_delegated_revoke(&owner, &delegate, &DelegationType::Attestation, &payload);
}

#[test]
#[should_panic(expected = "Error(Contract, #225)")]
fn test_delegated_revoke_attest_domain_mismatch_rejects() {
    let (e, client, _admin) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);
    let contract_id = client.address.clone();
    let expires_at = 2000_u64;

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    // Wrong DomainTag (Delegate instead of RevokeAttestation)
    let payload = delegate_payload(
        &e,
        DomainTag::Delegate,
        &attester,
        &subject,
        &contract_id,
        1,
    );

    client.execute_delegated_revoke_attest(&attester, &subject, &payload);
}

#[test]
#[should_panic(expected = "Error(Contract, #501)")]
fn test_delegated_revoke_replay_fails_at_nonce_consumption() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let contract_id = client.address.clone();
    let expires_at = 2000_u64;

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    let payload = delegate_payload(
        &e,
        DomainTag::RevokeDelegation,
        &owner,
        &delegate,
        &contract_id,
        1,
    );

    // First revoke succeeds
    client.execute_delegated_revoke(&owner, &delegate, &DelegationType::Attestation, &payload);

    // Replay of revoke payload fails at nonce check (#501), before state check
    client.execute_delegated_revoke(&owner, &delegate, &DelegationType::Attestation, &payload);
}

// ---------------------------------------------------------------------------
// 4. Verifier Registration Authorization & Bounds
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_register_verifier_non_admin_rejects() {
    let (e, client, _admin) = setup();
    let non_admin = Address::generate(&e);
    let verifier = Address::generate(&e);

    client.register_verifier(&non_admin, &1_u32, &verifier);
}

#[test]
#[should_panic(expected = "Error(Contract, #513)")]
fn test_register_verifier_unknown_scheme_rejects() {
    let (e, client, admin) = setup();
    let verifier = Address::generate(&e);

    // Scheme tag 99 is invalid/unsupported
    client.register_verifier(&admin, &99_u32, &verifier);
}

#[test]
#[should_panic(expected = "Error(Contract, #106)")]
fn test_register_verifier_rejects_when_paused() {
    let (e, client, admin) = setup();
    let verifier = Address::generate(&e);

    client.pause(&admin);
    client.register_verifier(&admin, &1_u32, &verifier);
}

// ---------------------------------------------------------------------------
// 5. Re-delegation State Overwriting & Invariants
// ---------------------------------------------------------------------------

#[test]
fn test_redelegation_updates_active_expiry() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &1500_u64,
        &0_u64,
    );

    // Re-delegate with longer expiry
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &2500_u64,
        &1_u64,
    );

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(d.expires_at, 2500);
    assert!(!d.revoked);
}

#[test]
fn test_redelegation_reactivates_revoked_delegation() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    e.ledger().with_mut(|li| li.timestamp = 1000);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &1500_u64,
        &0_u64,
    );
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);

    let revoked_d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(revoked_d.revoked);

    // Re-delegate reactivates record
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &2500_u64,
        &2_u64,
    );

    let active_d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(!active_d.revoked);
    assert_eq!(active_d.revoked_at, 0);
    assert_eq!(active_d.expires_at, 2500);
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}
