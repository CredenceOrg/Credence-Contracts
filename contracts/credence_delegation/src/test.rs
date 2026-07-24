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
        scheme: 0, // Default to Ed25519 for backwards compatibility
        signature_domain: String::from_str(e, "CredenceDelegation"),
    }
}

fn delegate_payload_with_scheme(
    e: &Env,
    domain: DomainTag,
    owner: &Address,
    target: &Address,
    contract_id: &Address,
    nonce: u64,
    scheme: u8,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain,
        owner: owner.clone(),
        target: target.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme,
        signature_domain: String::from_str(e, "CredenceDelegation"),
    }
}

// ---------------------------------------------------------------------------
// Existing delegation tests
// ---------------------------------------------------------------------------

#[test]
fn test_delegate_attestation() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let d = client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &86400_u64,
        &0_u64,
    );

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
    let d = client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64, &0_u64);

    assert_eq!(d.owner, owner);
    assert_eq!(d.delegate, delegate);
    assert!(matches!(d.delegation_type, DelegationType::Management));
}

#[test]
fn test_get_delegation() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64, &0_u64);

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
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &86400_u64,
        &0_u64,
    );
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64, &1_u64);

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(d.revoked);
}

#[test]
fn test_is_valid_delegate() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64, &0_u64);

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
    client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64, &0_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Management, &1_u64);

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
fn test_is_valid_delegate_after_expiry() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &100_u64, &0_u64);

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
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64, &0_u64);
    client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64, &0_u64);

    // Revoke only attestation
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_double_initialize() {
    let (e, client) = setup();
    let admin2 = Address::generate(&e);
    client.initialize(&admin2);
}

#[test]
#[should_panic(expected = "Error(Contract, #500)")]
fn test_delegate_with_past_expiry() {
    let (e, client) = setup();
    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &500_u64, &0_u64);
}

#[test]
#[should_panic(expected = "Error(Contract, #500)")]
fn test_delegate_rejects_expiry_at_now() {
    let (e, client) = setup();
    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &1000_u64, &0_u64);
}

#[test]
fn test_delegate_accepts_exact_max_expiry() {
    let (e, client) = setup();
    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = e.ledger().timestamp() + MAX_DELEGATION_DURATION;

    let d = client.delegate(&owner, &delegate, &DelegationType::Management, &expires_at, &0_u64);

    assert_eq!(d.expires_at, expires_at);
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
#[should_panic(expected = "Error(Contract, #503)")]
fn test_delegate_rejects_expiry_over_max() {
    let (e, client) = setup();
    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = e.ledger().timestamp() + MAX_DELEGATION_DURATION + 1;

    client.delegate(&owner, &delegate, &DelegationType::Management, &expires_at, &0_u64);
}

#[test]
#[should_panic(expected = "Error(Contract, #503)")]
fn test_delegate_rejects_u64_max_expiry() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.delegate(&owner, &delegate, &DelegationType::Management, &u64::MAX, &0_u64);
}

#[test]
fn test_execute_delegated_delegate_accepts_exact_max_expiry() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = e.ledger().timestamp() + MAX_DELEGATION_DURATION;
    let payload = delegate_payload(&e, DomainTag::Delegate, &owner, &delegate, &client.address, 0);

    let d = client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &payload,
    );

    assert_eq!(d.expires_at, expires_at);
    assert_eq!(client.get_nonce(&owner), 1);
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
fn test_execute_delegated_delegate_rejects_over_max_without_consuming_nonce() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = e.ledger().timestamp() + MAX_DELEGATION_DURATION + 1;
    let payload = delegate_payload(&e, DomainTag::Delegate, &owner, &delegate, &client.address, 0);

    assert!(client
        .try_execute_delegated_delegate(
            &owner,
            &delegate,
            &DelegationType::Management,
            &expires_at,
            &payload,
        )
        .is_err());
    assert_eq!(client.get_nonce(&owner), 0);
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
fn test_is_valid_delegate_false_at_exact_expiry_boundary() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = e.ledger().timestamp() + 10;

    client.delegate(&owner, &delegate, &DelegationType::Attestation, &expires_at, &0_u64);
    e.ledger().with_mut(|li| {
        li.timestamp = expires_at;
    });

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
fn test_revoke_delegation_after_expiry_marks_revoked_and_stays_invalid() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = e.ledger().timestamp() + 10;

    client.delegate(&owner, &delegate, &DelegationType::Management, &expires_at, &0_u64);
    e.ledger().with_mut(|li| {
        li.timestamp = expires_at;
    });

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));

    client.revoke_delegation(&owner, &delegate, &DelegationType::Management, &1_u64);

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Management);
    assert!(d.revoked);
    assert_eq!(d.expires_at, expires_at);
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
#[should_panic(expected = "Error(Contract, #501)")]
fn test_get_nonexistent_delegation() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
}

#[test]
#[should_panic(expected = "Error(Contract, #502)")]
fn test_double_revoke() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64, &0_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);
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
        &0_u64,
    );

    // Status before revocation
    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::Active
    ));

    // Revoke
    client.revoke_attestation(&attester, &subject, &1_u64);

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
        &0_u64,
    );
    client.revoke_attestation(&attester, &subject, &1_u64);

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
        &0_u64,
    );
    assert!(client.is_valid_delegate(&attester, &subject, &DelegationType::Attestation));

    client.revoke_attestation(&attester, &subject, &1_u64);
    assert!(!client.is_valid_delegate(&attester, &subject, &DelegationType::Attestation));
}

/// Revoking an attestation that was never issued must panic with `"Error(Contract, #501)"`.
#[test]
#[should_panic(expected = "Error(Contract, #501)")]
fn test_revoke_attestation_not_found() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    client.revoke_attestation(&attester, &subject, &1_u64);
}

/// Double-revoking an attestation must panic with `"Error(Contract, #502)"`.
#[test]
#[should_panic(expected = "Error(Contract, #502)")]
fn test_revoke_attestation_double_revoke() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);

    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &86400_u64,
        &0_u64,
    );
    client.revoke_attestation(&attester, &subject, &1_u64);
    // Second revoke must panic
    client.revoke_attestation(&attester, &subject, &1_u64);
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
        &0_u64,
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
        &0_u64,
    );
    client.delegate(&attester, &subject, &DelegationType::Management, &86400_u64, &0_u64);

    client.revoke_attestation(&attester, &subject, &1_u64);

    // Attestation is revoked
    assert!(matches!(
        client.get_attestation_status(&attester, &subject),
        AttestationStatus::Revoked
    ));

    // Management delegation is unaffected
    assert!(client.is_valid_delegate(&attester, &subject, &DelegationType::Management));
}

#[test]
fn test_get_delegation_summary() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    
    // 1. Initial creation
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &1000_u64, &0_u64);
    
    let summary = client.get_delegation_summary(&owner, &delegate, &DelegationType::Attestation);
    assert!(summary.is_valid);
    assert_eq!(summary.time_to_expiry, 1000);
    assert!(matches!(summary.delegation_type, DelegationType::Attestation));
    assert_eq!(summary.revoked_at, 0);
    assert_eq!(summary.scheme, 0);

    // 2. Advance time
    e.ledger().with_mut(|li| {
        li.timestamp = 400;
    });
    let summary2 = client.get_delegation_summary(&owner, &delegate, &DelegationType::Attestation);
    assert!(summary2.is_valid);
    assert_eq!(summary2.time_to_expiry, 600);

    // 3. Revoke
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);
    let summary3 = client.get_delegation_summary(&owner, &delegate, &DelegationType::Attestation);
    assert!(!summary3.is_valid);
    assert_eq!(summary3.time_to_expiry, 600);

    // 4. Expire
    e.ledger().with_mut(|li| {
        li.timestamp = 1100;
    });
    let summary4 = client.get_delegation_summary(&owner, &delegate, &DelegationType::Attestation);
    assert!(!summary4.is_valid);
    assert_eq!(summary4.time_to_expiry, 0);
}

// ---------------------------------------------------------------------------
// cleanup_expired — tests
// ---------------------------------------------------------------------------

#[test]
fn test_cleanup_expired_happy_path() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = e.ledger().timestamp() + 10;

    client.delegate(&owner, &delegate, &DelegationType::Attestation, &expires_at, &0_u64);

    // Verify it exists and is valid
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));

    // Try cleanup before expiry (should fail)
    let res = client.try_cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
    assert!(res.is_err());
    
    // Advance ledger past expiry
    e.ledger().with_mut(|li| {
        li.timestamp = expires_at;
    });

    // Cleanup should succeed now
    let res_ok = client.try_cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
    assert!(res_ok.is_ok());

    // Verify entry is removed: is_valid_delegate returns false, get_delegation panics
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
    
    let res_get = client.try_get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(res_get.is_err());

    // Verify delegation_cleaned event is emitted
    let events = e.events().all();
    let cleaned_events: std::vec::Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == client.address
                && Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "delegation_cleaned")
        })
        .collect();

    assert_eq!(cleaned_events.len(), 1);
    let event = &cleaned_events[0];
    let topic_name = Symbol::from_val(&e, &event.1.get(0).unwrap());
    let topic_owner = Address::from_val(&e, &event.1.get(1).unwrap());
    let topic_delegate = Address::from_val(&e, &event.1.get(2).unwrap());
    let data_type = DelegationType::from_val(&e, &event.2);

    assert_eq!(topic_name, Symbol::new(&e, "delegation_cleaned"));
    assert_eq!(topic_owner, owner);
    assert_eq!(topic_delegate, delegate);
    assert_eq!(data_type, DelegationType::Attestation);
}

#[test]
#[should_panic(expected = "Error(Contract, #501)")]
fn test_cleanup_expired_nonexistent() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
}

#[test]
#[should_panic(expected = "Error(Contract, #509)")]
fn test_cleanup_expired_non_expired() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = e.ledger().timestamp() + 10;

    client.delegate(&owner, &delegate, &DelegationType::Attestation, &expires_at, &0_u64);

    client.cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
}

#[test]
fn test_cleanup_expired_revoked_but_not_expired() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = e.ledger().timestamp() + 10;

    client.delegate(&owner, &delegate, &DelegationType::Attestation, &expires_at, &0_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);

    // Verify it is revoked
    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(d.revoked);

    // Try to cleanup (should fail with DelegationNotExpired because now < expires_at)
    let res = client.try_cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
    assert!(res.is_err());
    
    // Advance time past expiry
    e.ledger().with_mut(|li| {
        li.timestamp = expires_at;
    });

    // Cleanup should now succeed
    let res_ok = client.try_cleanup_expired(&owner, &delegate, &DelegationType::Attestation);
    assert!(res_ok.is_ok());

    let res_get = client.try_get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(res_get.is_err());
}

