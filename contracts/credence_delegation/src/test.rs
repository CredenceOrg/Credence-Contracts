#![cfg(test)]

use super::*;
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

// ---------------------------------------------------------------------------
// Existing delegation tests
// ---------------------------------------------------------------------------

#[test]
fn test_delegate_attestation() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let d = client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);

    assert_eq!(d.owner, owner);
    assert_eq!(d.delegate, delegate);
    assert_eq!(d.expires_at, 86400);
    assert!(!d.revoked);
    assert!(matches!(d.delegation_type, DelegationType::Attestation));
}

#[test]
fn test_delegate_management() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let d = client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64);

    assert_eq!(d.owner, owner);
    assert_eq!(d.delegate, delegate);
    assert!(matches!(d.delegation_type, DelegationType::Management));
}

#[test]
fn test_get_delegation() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(d.owner, owner);
    assert_eq!(d.delegate, delegate);
    assert_eq!(d.expires_at, 86400);
}

#[test]
fn test_revoke_delegation() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation);

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(d.revoked);
}

#[test]
fn test_is_valid_delegate() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);

    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
fn test_is_valid_delegate_not_found() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
fn test_is_valid_delegate_after_revoke() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Management);

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
fn test_is_valid_delegate_after_expiry() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &100_u64);

    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));

    // Advance ledger past expiry
    e.ledger().with_mut(|li| {
        li.timestamp = 200;
    });

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
fn test_independent_delegation_types() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);
    client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64);

    // Revoke only attestation
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation);

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_double_initialize() {
    let (e, client) = setup();
    let admin2 = Address::generate(&e);
    client.initialize(&admin2);
}

#[test]
#[should_panic(expected = "expiry must be in the future")]
fn test_delegate_with_past_expiry() {
    let (e, client) = setup();
    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &500_u64);
}

#[test]
#[should_panic(expected = "delegation not found")]
fn test_get_nonexistent_delegation() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
}

#[test]
#[should_panic(expected = "already revoked")]
fn test_double_revoke() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation);
}

// ---------------------------------------------------------------------------
// revoke_attestation — new tests
// ---------------------------------------------------------------------------

/// Happy path: attester issues an attestation and then revokes it.
/// The returned status should be `Revoked` afterwards.
#[test]
fn test_revoke_attestation_happy_path() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    // Issue attestation (modelled as an Attestation-type delegation)
    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &86400_u64,
    );

    // Status before revocation
    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::Active
    ));

    // Revoke
    client.revoke_attestation(&attester, &subject);

    // Status after revocation
    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::Revoked
    ));
}

/// After revocation the underlying `Delegation` record must still be readable
/// (audit history is preserved — the record is never deleted).
#[test]
fn test_revoke_attestation_history_preserved() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &86400_u64,
    );
    client.revoke_attestation(&attester, &subject);

    // Full record must still be reachable via get_delegation
    let d = client.get_delegation(&attester, &subject, &DelegationType::Attestation);
    assert_eq!(d.owner, attester);
    assert_eq!(d.delegate, subject);
    assert!(d.revoked);
    assert_eq!(d.expires_at, 86400);
}

/// After `revoke_attestation`, `is_valid_delegate` must return `false`.
#[test]
fn test_revoke_attestation_is_valid_false() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &86400_u64,
    );
    assert!(client.is_valid_delegate(&attester, &subject, &DelegationType::Attestation));

    client.revoke_attestation(&attester, &subject);
    assert!(!client.is_valid_delegate(&attester, &subject, &DelegationType::Attestation));
}

/// Revoking an attestation that was never issued must panic with `"attestation not found"`.
#[test]
#[should_panic(expected = "attestation not found")]
fn test_revoke_attestation_not_found() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    client.revoke_attestation(&attester, &subject);
}

/// Double-revoking an attestation must panic with `"attestation already revoked"`.
#[test]
#[should_panic(expected = "attestation already revoked")]
fn test_revoke_attestation_double_revoke() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &86400_u64,
    );
    client.revoke_attestation(&attester, &subject);
    // Second revoke must panic
    client.revoke_attestation(&attester, &subject);
}

/// `get_attestation_status` returns `Active` for a live attestation.
#[test]
fn test_get_attestation_status_active() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &86400_u64,
    );

    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::Active
    ));
}

/// `get_attestation_status` returns `NotFound` when no attestation was ever issued.
#[test]
fn test_get_attestation_status_not_found() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::NotFound
    ));
}

/// Revoking an attestation must not affect an unrelated Management delegation
/// between the same pair of addresses.
#[test]
fn test_revoke_attestation_does_not_affect_management() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &86400_u64,
    );
    client.delegate(&attester, &subject, &DelegationType::Management, &86400_u64);

    client.revoke_attestation(&attester, &subject);

    // Attestation is revoked
    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::Revoked
    ));

    // Management delegation is unaffected
    assert!(client.is_valid_delegate(&attester, &subject, &DelegationType::Management));
}
