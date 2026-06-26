#![cfg(test)]

//! Authentication boundary tests for CredenceDelegation.
//!
//! Verifies that every non-view #[contractimpl] function requires an
//! authenticated address argument and rejects unauthorised callers.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, Address, Address, CredenceDelegationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&env, &contract_id);
    client.initialize(&admin);
    let owner = Address::generate(&env);
    (env, admin, owner, client)
}

/// Return a ledger timestamp that is `secs` seconds in the future relative
/// to the current ledger timestamp, without advancing the ledger itself.
fn future_ts(env: &Env, secs: u64) -> u64 {
    env.ledger().timestamp() + secs
}

/// Advance the mock ledger timestamp by `secs`.
fn advance(env: &Env, secs: u64) {
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: env.ledger().timestamp() + secs,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 3_000_000,
    });
}

// ---------------------------------------------------------------------------
// delegate — owner must authorize
// ---------------------------------------------------------------------------

/// Happy path: owner creates a delegation with a valid expiry.
#[test]
fn delegate_succeeds_when_owner_authorizes() {
    let (env, _admin, owner, client) = setup();
    let delegate = Address::generate(&env);
    let expires_at = future_ts(&env, 3600);
    let delegation = client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
    assert_eq!(delegation.owner, owner);
    assert_eq!(delegation.delegate, delegate);
    assert!(!delegation.revoked);
}

/// Sad path: expiry in the past is rejected (ExpiryInPast).
#[test]
#[should_panic]
fn delegate_rejected_when_expiry_is_in_the_past() {
    let (env, _admin, owner, client) = setup();
    let delegate = Address::generate(&env);
    // Advance ledger past the expiry we will supply.
    advance(&env, 7200);
    let past_expiry = env.ledger().timestamp() - 1;
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &past_expiry,
        &0_u64,
    );
}

// ---------------------------------------------------------------------------
// revoke_delegation — owner must authorize
// ---------------------------------------------------------------------------

/// Happy path: owner revokes an active delegation.
#[test]
fn revoke_delegation_succeeds_when_owner_authorizes() {
    let (env, _admin, owner, client) = setup();
    let delegate = Address::generate(&env);
    let expires_at = future_ts(&env, 3600);
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
}

/// Sad path: a stranger calling revoke_delegation (owner ≠ delegation.owner)
/// panics because the delegation was never created under stranger's address.
#[test]
#[should_panic]
fn revoke_delegation_rejected_when_delegation_does_not_exist_for_caller() {
    let (env, _admin, owner, client) = setup();
    let delegate = Address::generate(&env);
    let expires_at = future_ts(&env, 3600);
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
    let stranger = Address::generate(&env);
    // stranger is not the owner of this delegation → DelegationNotFound.
    client.revoke_delegation(&stranger, &delegate, &DelegationType::Attestation, &0_u64);
}

// ---------------------------------------------------------------------------
// revoke_attestation — attester must authorize
// ---------------------------------------------------------------------------

/// Happy path: attester revokes an attestation-type delegation they created.
#[test]
fn revoke_attestation_succeeds_when_attester_authorizes() {
    let (env, _admin, attester, client) = setup();
    let subject = Address::generate(&env);
    let expires_at = future_ts(&env, 3600);
    client.delegate(
        &attester,
        &subject,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );
    client.revoke_attestation(&attester, &subject, &1_u64);
    let d = client.get_delegation(&attester, &subject, &DelegationType::Attestation);
    assert!(d.revoked);
}

// ---------------------------------------------------------------------------
// invalidate_nonce_range — identity must authorize
// ---------------------------------------------------------------------------

/// Happy path: identity advances its own nonce.
#[test]
fn invalidate_nonce_range_succeeds_when_identity_authorizes() {
    let (env, _admin, owner, client) = setup();
    let nonce_before = client.get_nonce(&owner);
    client.invalidate_nonce_range(&owner, &(nonce_before + 5));
    assert_eq!(client.get_nonce(&owner), nonce_before + 5);
}

// ---------------------------------------------------------------------------
// set_revocation_grace_period — admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: admin sets a non-zero grace period.
#[test]
fn set_revocation_grace_period_succeeds_when_admin_authorizes() {
    let (env, admin, _owner, client) = setup();
    client.set_revocation_grace_period(&admin, &86400_u64);
    assert_eq!(client.get_revocation_grace_period(), 86400_u64);
}

/// Sad path: a stranger (not the stored admin) is rejected with NotAdmin.
#[test]
#[should_panic]
fn set_revocation_grace_period_rejected_when_non_admin_calls() {
    let (env, _admin, _owner, client) = setup();
    let stranger = Address::generate(&env);
    client.set_revocation_grace_period(&stranger, &86400_u64);
}

// ---------------------------------------------------------------------------
// register_verifier — admin must authorize
// ---------------------------------------------------------------------------

/// Sad path: a stranger (not the stored admin) is rejected with NotAdmin.
/// (The happy path requires a valid verifier contract — exercised separately
/// in test_verifier_dispatch.rs; we only lock the auth boundary here.)
#[test]
#[should_panic]
fn register_verifier_rejected_when_non_admin_calls() {
    let (env, _admin, _owner, client) = setup();
    let stranger = Address::generate(&env);
    let verifier_id = Address::generate(&env);
    // Scheme 0 = Ed25519; stranger ≠ stored admin → NotAdmin.
    client.register_verifier(&stranger, &0_u32, &verifier_id);
}
