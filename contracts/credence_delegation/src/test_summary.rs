//! Tests for `get_delegation_summary`, `revoked_at`, and `scheme` fields.
//!
//! Coverage:
//! - `revoked_at` is zero before revocation
//! - `revoked_at` shows the real ledger timestamp after `revoke_delegation`
//! - `revoked_at` shows the real ledger timestamp after `revoke_attestation`
//! - `revoked_at` shows the real ledger timestamp after `execute_delegated_revoke`
//! - `scheme` defaults to `0` (Ed25519) for direct-auth `delegate` calls
//! - `scheme` is stored from payload for `execute_delegated_delegate`
//! - double-revoke preserves the *first* `revoked_at` (second call panics before write)
//! - legacy-default: `revoked_at = 0`, `scheme = 0` when fields absent (v1 entry)

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Env;

fn setup() -> (Env, CredenceDelegationClient<'static>) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client)
}

fn make_payload(
    domain: DomainTag,
    owner: &Address,
    target: &Address,
    contract_id: &Address,
    nonce: u64,
    scheme: u32,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain,
        owner: owner.clone(),
        target: target.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme,
    }
}

// ---------------------------------------------------------------------------
// revoked_at field
// ---------------------------------------------------------------------------

/// Before revocation, `revoked_at` must be `0` in both the raw struct and the
/// summary view.
#[test]
fn test_revoked_at_is_zero_before_revocation() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64, &0_u64);

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(d.revoked_at, 0);

    let summary = client.get_delegation_summary(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(summary.revoked_at, 0);
}

/// After `revoke_delegation`, `revoked_at` in the summary must equal the ledger
/// timestamp at the time of the revoke call.
#[test]
fn test_revoked_at_set_on_revoke_delegation() {
    let (e, client) = setup();
    e.ledger().with_mut(|li| li.timestamp = 500);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64, &0_u64);

    // Advance ledger before revoking so we have a non-zero, distinct timestamp.
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);

    let summary = client.get_delegation_summary(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(summary.revoked_at, 1_000, "revoked_at must equal the ledger timestamp at revoke time");
    assert!(!summary.is_valid);
}

/// `revoke_attestation` path also sets `revoked_at`.
#[test]
fn test_revoked_at_set_on_revoke_attestation() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);
    client.delegate(&attester, &subject, &DelegationType::Attestation, &86400_u64, &0_u64);

    e.ledger().with_mut(|li| li.timestamp = 2_500);
    client.revoke_attestation(&attester, &subject, &1_u64);

    let d = client.get_delegation(&attester, &subject, &DelegationType::Attestation);
    assert_eq!(d.revoked_at, 2_500);
}

/// `execute_delegated_revoke` path also sets `revoked_at`.
#[test]
fn test_revoked_at_set_on_execute_delegated_revoke() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64, &0_u64);

    e.ledger().with_mut(|li| li.timestamp = 3_000);
    let payload = make_payload(
        DomainTag::RevokeDelegation,
        &owner,
        &delegate,
        &client.address,
        1,
        0,
    );
    client.execute_delegated_revoke(&owner, &delegate, &DelegationType::Management, &payload);

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Management);
    assert_eq!(d.revoked_at, 3_000);
}

/// `execute_delegated_revoke_attest` path also sets `revoked_at`.
#[test]
fn test_revoked_at_set_on_execute_delegated_revoke_attest() {
    let (e, client) = setup();
    let attester = Address::generate(&e);
    let subject = Address::generate(&e);
    client.delegate(&attester, &subject, &DelegationType::Attestation, &86400_u64, &0_u64);

    e.ledger().with_mut(|li| li.timestamp = 7_777);
    let payload = make_payload(
        DomainTag::RevokeAttestation,
        &attester,
        &subject,
        &client.address,
        1,
        0,
    );
    client.execute_delegated_revoke_attest(&attester, &subject, &payload);

    let d = client.get_delegation(&attester, &subject, &DelegationType::Attestation);
    assert_eq!(d.revoked_at, 7_777);
}

/// Double-revoke must panic (AlreadyRevoked = #502) and must NOT overwrite the
/// original `revoked_at`.
#[test]
fn test_double_revoke_preserves_first_revoked_at() {
    let (e, client) = setup();
    e.ledger().with_mut(|li| li.timestamp = 100);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64, &0_u64);

    // First revoke at t=200
    e.ledger().with_mut(|li| li.timestamp = 200);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);

    let d_after_first = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(d_after_first.revoked_at, 200, "first revoke_at must be 200");

    // Second revoke must panic; revoked_at must stay 200
    e.ledger().with_mut(|li| li.timestamp = 300);
    let result = client.try_revoke_delegation(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &2_u64,
    );
    assert!(result.is_err(), "second revoke must fail");

    // Record must be unchanged
    let d_after_second = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(d_after_second.revoked_at, 200, "revoked_at must not change after failed second revoke");
}

// ---------------------------------------------------------------------------
// scheme field
// ---------------------------------------------------------------------------

/// Direct `delegate()` (no payload) stores `scheme = 0` (Ed25519).
#[test]
fn test_scheme_defaults_to_zero_for_direct_delegate() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let d = client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64, &0_u64);
    assert_eq!(d.scheme, 0);

    let summary = client.get_delegation_summary(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(summary.scheme, 0);
}

/// `execute_delegated_delegate` stores the scheme from the payload.
#[test]
fn test_scheme_stored_from_payload_scheme_field() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Scheme 1 = Secp256r1
    let payload = make_payload(DomainTag::Delegate, &owner, &delegate, &client.address, 0, 1);
    let d = client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &86400_u64,
        &payload,
    );
    assert_eq!(d.scheme, 1);

    let summary = client.get_delegation_summary(&owner, &delegate, &DelegationType::Management);
    assert_eq!(summary.scheme, 1);
}

/// Ed25519 (scheme 0) stored via payload is also accessible.
#[test]
fn test_scheme_zero_via_payload() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let payload = make_payload(DomainTag::Delegate, &owner, &delegate, &client.address, 0, 0);
    let d = client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &86400_u64,
        &payload,
    );
    assert_eq!(d.scheme, 0);
}

// ---------------------------------------------------------------------------
// Legacy-entry defaults (documented behaviour)
// ---------------------------------------------------------------------------

/// A delegation created via the direct `delegate()` path (which always writes
/// `scheme = 0`) and never revoked must surface `revoked_at = 0` and
/// `scheme = 0` from the summary — matching the documented legacy defaults.
///
/// This is the observable invariant for pre-v2 entries once they have been
/// migrated: both sentinel values remain `0`.
#[test]
fn test_legacy_defaults_observable_through_summary() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Write a new entry with the direct path (scheme = 0 always)
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64, &0_u64);

    let summary = client.get_delegation_summary(&owner, &delegate, &DelegationType::Attestation);
    // Both sentinel values match the documented defaults for legacy (v1) entries
    assert_eq!(summary.revoked_at, 0, "revoked_at default must be 0 (not-revoked sentinel)");
    assert_eq!(summary.scheme, 0, "scheme default must be 0 (Ed25519)");
    assert!(summary.is_valid);
}

// ---------------------------------------------------------------------------
// Summary correctness
// ---------------------------------------------------------------------------

/// Full lifecycle: create → advance time → revoke → confirm summary fields.
#[test]
fn test_summary_lifecycle() {
    let (e, client) = setup();
    e.ledger().with_mut(|li| li.timestamp = 0);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Management, &1_000_u64, &0_u64);

    // At t=0: valid
    let s1 = client.get_delegation_summary(&owner, &delegate, &DelegationType::Management);
    assert!(s1.is_valid);
    assert_eq!(s1.time_to_expiry, 1_000);
    assert_eq!(s1.revoked_at, 0);
    assert_eq!(s1.scheme, 0);

    // Advance to t=400
    e.ledger().with_mut(|li| li.timestamp = 400);
    let s2 = client.get_delegation_summary(&owner, &delegate, &DelegationType::Management);
    assert!(s2.is_valid);
    assert_eq!(s2.time_to_expiry, 600);
    assert_eq!(s2.revoked_at, 0);

    // Revoke at t=400
    client.revoke_delegation(&owner, &delegate, &DelegationType::Management, &1_u64);
    let s3 = client.get_delegation_summary(&owner, &delegate, &DelegationType::Management);
    assert!(!s3.is_valid);
    assert_eq!(s3.revoked_at, 400, "revoked_at must equal revoke timestamp");
    assert_eq!(s3.time_to_expiry, 600); // expiry didn't change

    // Advance past expiry (t=1100)
    e.ledger().with_mut(|li| li.timestamp = 1_100);
    let s4 = client.get_delegation_summary(&owner, &delegate, &DelegationType::Management);
    assert!(!s4.is_valid);
    assert_eq!(s4.time_to_expiry, 0);
    assert_eq!(s4.revoked_at, 400, "revoked_at must still be 400");
}
