//! Boundary tests for the configurable post-expiry revocation grace window.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Env;

fn setup() -> (Env, CredenceDelegationClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client, admin)
}

#[test]
fn grace_zero_preserves_hard_cliff_status_and_unlimited_late_revoke() {
    let (e, client, _admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 100_u64;

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
    assert_eq!(client.get_revocation_grace_period(), 0);

    e.ledger().with_mut(|li| {
        li.timestamp = expires_at;
    });
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
    assert_eq!(
        client
            .get_delegation_summary(&owner, &delegate, &DelegationType::Attestation)
            .status,
        DelegationStatus::Expired
    );
    assert_eq!(
        client.get_attestation_status(&owner, &delegate),
        AttestationStatus::Expired
    );

    e.ledger().with_mut(|li| {
        li.timestamp = expires_at + 1_000;
    });
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(d.revoked);
    assert_eq!(d.revoked_at, expires_at + 1_000);
    assert_eq!(
        client
            .get_delegation_summary(&owner, &delegate, &DelegationType::Attestation)
            .status,
        DelegationStatus::Revoked
    );
}

#[test]
fn in_grace_status_without_authority() {
    let (e, client, admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 1_000_u64;
    let grace = 60_u64;

    client.set_revocation_grace_period(&admin, &grace);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &0_u64,
    );

    e.ledger().with_mut(|li| {
        li.timestamp = expires_at;
    });
    let summary = client.get_delegation_summary(&owner, &delegate, &DelegationType::Management);
    assert_eq!(summary.status, DelegationStatus::InGrace);
    assert!(!summary.is_valid);
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));

    e.ledger().with_mut(|li| {
        li.timestamp = expires_at + grace;
    });
    let summary_at_end =
        client.get_delegation_summary(&owner, &delegate, &DelegationType::Management);
    assert_eq!(summary_at_end.status, DelegationStatus::InGrace);
    assert!(!summary_at_end.is_valid);
}

#[test]
fn revoke_succeeds_within_grace_and_records_revoked_at() {
    let (e, client, admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 500_u64;
    let grace = 30_u64;

    client.set_revocation_grace_period(&admin, &grace);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    let revoke_at = expires_at + grace;
    e.ledger().with_mut(|li| {
        li.timestamp = revoke_at;
    });

    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);
    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(d.revoked);
    assert_eq!(d.revoked_at, revoke_at);

    let summary = client.get_delegation_summary(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(summary.status, DelegationStatus::Revoked);
    assert_eq!(summary.revoked_at, revoke_at);
    assert_eq!(
        client.get_attestation_status(&owner, &delegate),
        AttestationStatus::Revoked
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #508)")]
fn revoke_after_grace_window_rejects() {
    let (e, client, admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 200_u64;
    let grace = 10_u64;

    client.set_revocation_grace_period(&admin, &grace);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &0_u64,
    );

    e.ledger().with_mut(|li| {
        li.timestamp = expires_at + grace + 1;
    });
    client.revoke_delegation(&owner, &delegate, &DelegationType::Management, &1_u64);
}

#[test]
fn active_revocation_always_allowed() {
    let (e, client, admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 10_000_u64;

    client.set_revocation_grace_period(&admin, &30_u64);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);
    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(d.revoked);
    assert_eq!(d.revoked_at, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn non_admin_cannot_set_grace_period() {
    let (e, client, _admin) = setup();
    let stranger = Address::generate(&e);
    client.set_revocation_grace_period(&stranger, &60_u64);
}

#[test]
fn admin_can_update_and_read_grace_period() {
    let (e, client, admin) = setup();
    let _ = e;
    assert_eq!(client.get_revocation_grace_period(), 0);
    client.set_revocation_grace_period(&admin, &120_u64);
    assert_eq!(client.get_revocation_grace_period(), 120_u64);
    client.set_revocation_grace_period(&admin, &0_u64);
    assert_eq!(client.get_revocation_grace_period(), 0);
}

#[test]
fn status_transitions_at_each_boundary() {
    let (e, client, admin) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let expires_at = 1_000_u64;
    let grace = 100_u64;

    client.set_revocation_grace_period(&admin, &grace);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    e.ledger().with_mut(|li| {
        li.timestamp = expires_at - 1;
    });
    assert_eq!(
        client
            .get_delegation_summary(&owner, &delegate, &DelegationType::Attestation)
            .status,
        DelegationStatus::Active
    );

    e.ledger().with_mut(|li| {
        li.timestamp = expires_at;
    });
    assert_eq!(
        client.get_attestation_status(&owner, &delegate),
        AttestationStatus::InGrace
    );

    e.ledger().with_mut(|li| {
        li.timestamp = expires_at + grace + 1;
    });
    assert_eq!(
        client.get_attestation_status(&owner, &delegate),
        AttestationStatus::Expired
    );
}
