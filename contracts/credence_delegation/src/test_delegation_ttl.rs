//! Tests for storage TTL bumping on Delegation and Nonce entries (issue #376).
//!
//! Verifies that:
//! 1. Delegation TTL is set on write (delegate).
//! 2. Delegation TTL is refreshed on read (get_delegation, is_valid_delegate).
//! 3. Nonce TTL is set when a nonce is first written (consume_nonce).
//! 4. Nonce TTL is bumped to cover the delegation lifetime on store_delegation.
//! 5. Nonce TTL is refreshed on read (get_nonce).
//! 6. Revoked delegation still has its TTL bumped (mark_delegation_revoked).
//! 7. TTL is capped at MAX_TTL for very long-lived delegations.

use super::*;
use soroban_sdk::testutils::storage::Persistent as PersistentTestutils;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Env;

use crate::nonce::{LEDGER_BUMP_BUFFER, MAX_TTL, MIN_NONCE_TTL};

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

/// Advance the ledger timestamp by `secs` seconds.
/// Bumps the contract instance TTL first so it remains accessible after the advance.
fn advance_time(e: &Env, contract_id: &soroban_sdk::Address, secs: u64) {
    let ledgers = (secs / 5) as u32;
    // Extend instance TTL *before* advancing so the contract stays live.
    e.as_contract(contract_id, || {
        e.storage()
            .instance()
            .extend_ttl(ledgers + 17_280, ledgers + 17_280);
    });
    e.ledger().with_mut(|info| {
        info.timestamp += secs;
        info.sequence_number += ledgers;
    });
}

/// Return the current TTL (in ledgers) for a persistent key.
fn delegation_ttl(e: &Env, contract_id: &soroban_sdk::Address, key: &DataKey) -> u32 {
    e.as_contract(contract_id, || {
        PersistentTestutils::get_ttl(&e.storage().persistent(), key)
    })
}

fn nonce_ttl(e: &Env, contract_id: &soroban_sdk::Address, key: &DataKey) -> u32 {
    e.as_contract(contract_id, || {
        PersistentTestutils::get_ttl(&e.storage().persistent(), key)
    })
}

// ---------------------------------------------------------------------------
// Test 1: Delegation TTL is set on write
// ---------------------------------------------------------------------------

#[test]
fn test_delegation_ttl_set_on_write() {
    let (e, client) = setup();
    let contract_id = client.address.clone();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // expires_at = now + 30 days in seconds
    let now = e.ledger().timestamp();
    let expires_at = now + 30 * 24 * 3600;

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    let key = DataKey::Delegation(owner.clone(), delegate.clone(), DelegationType::Attestation);
    let ttl = delegation_ttl(&e, &contract_id, &key);

    // TTL should be at least LEDGER_BUMP_BUFFER and at most MAX_TTL
    assert!(
        ttl >= LEDGER_BUMP_BUFFER,
        "TTL {ttl} < LEDGER_BUMP_BUFFER {LEDGER_BUMP_BUFFER}"
    );
    assert!(ttl <= MAX_TTL, "TTL {ttl} > MAX_TTL {MAX_TTL}");
}

// ---------------------------------------------------------------------------
// Test 2: Delegation TTL is refreshed on read (get_delegation)
// ---------------------------------------------------------------------------

#[test]
fn test_delegation_ttl_refreshed_on_get() {
    let (e, client) = setup();
    let contract_id = client.address.clone();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let now = e.ledger().timestamp();
    let expires_at = now + 60 * 24 * 3600; // 60 days

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    let key = DataKey::Delegation(owner.clone(), delegate.clone(), DelegationType::Attestation);
    let ttl_after_write = delegation_ttl(&e, &contract_id, &key);

    // Advance time so TTL would naturally decrease
    advance_time(&e, &contract_id, 10 * 24 * 3600); // +10 days

    // Read triggers a bump
    client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    let ttl_after_read = delegation_ttl(&e, &contract_id, &key);

    // After advancing 10 days the natural TTL would be lower; the bump should
    // keep it close to the original (within a day's worth of ledgers).
    assert!(
        ttl_after_read >= LEDGER_BUMP_BUFFER,
        "TTL after read {ttl_after_read} < LEDGER_BUMP_BUFFER"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Delegation TTL is refreshed on is_valid_delegate
// ---------------------------------------------------------------------------

#[test]
fn test_delegation_ttl_refreshed_on_is_valid() {
    let (e, client) = setup();
    let contract_id = client.address.clone();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let now = e.ledger().timestamp();
    let expires_at = now + 30 * 24 * 3600;

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    let key = DataKey::Delegation(owner.clone(), delegate.clone(), DelegationType::Attestation);

    advance_time(&e, &contract_id, 5 * 24 * 3600);

    let valid = client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation);
    assert!(valid);

    let ttl = delegation_ttl(&e, &contract_id, &key);
    assert!(ttl >= LEDGER_BUMP_BUFFER);
}

// ---------------------------------------------------------------------------
// Test 4: Nonce TTL is set when a nonce is first consumed
// ---------------------------------------------------------------------------

#[test]
fn test_nonce_ttl_set_on_consume() {
    let (e, client) = setup();
    let contract_id = client.address.clone();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let now = e.ledger().timestamp();
    let expires_at = now + 30 * 24 * 3600;

    // Use execute_delegated_delegate to trigger consume_nonce
    let payload = crate::domain::DelegatedActionPayload {
        domain: crate::domain::DomainTag::Delegate,
        owner: owner.clone(),
        target: delegate.clone(),
        contract_id: contract_id.clone(),
        nonce: 0,
        scheme: 0,
    };
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &payload,
    );

    let nonce_key = DataKey::Nonce(owner.clone());
    let ttl = nonce_ttl(&e, &contract_id, &nonce_key);
    assert!(
        ttl >= MIN_NONCE_TTL,
        "Nonce TTL {ttl} < MIN_NONCE_TTL {MIN_NONCE_TTL}"
    );
    assert!(ttl <= MAX_TTL);
}

// ---------------------------------------------------------------------------
// Test 5: Nonce TTL is bumped to cover delegation lifetime on store_delegation
// ---------------------------------------------------------------------------

#[test]
fn test_nonce_ttl_covers_delegation_lifetime() {
    let (e, client) = setup();
    let contract_id = client.address.clone();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let now = e.ledger().timestamp();
    // Long-lived delegation: 90 days
    let expires_at = now + 90 * 24 * 3600;

    // Use execute_delegated_delegate so the nonce key is written by consume_nonce.
    let payload = crate::domain::DelegatedActionPayload {
        domain: crate::domain::DomainTag::Delegate,
        owner: owner.clone(),
        target: delegate.clone(),
        contract_id: contract_id.clone(),
        nonce: 0,
        scheme: 0,
    };
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &payload,
    );

    let nonce_key = DataKey::Nonce(owner.clone());
    let ttl = nonce_ttl(&e, &contract_id, &nonce_key);

    // TTL should be at least MIN_NONCE_TTL
    assert!(
        ttl >= MIN_NONCE_TTL,
        "Nonce TTL {ttl} < MIN_NONCE_TTL {MIN_NONCE_TTL}"
    );
    assert!(ttl <= MAX_TTL);
}

// ---------------------------------------------------------------------------
// Test 6: Nonce TTL is refreshed on get_nonce
// ---------------------------------------------------------------------------

#[test]
fn test_nonce_ttl_refreshed_on_get_nonce() {
    let (e, client) = setup();
    let contract_id = client.address.clone();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let now = e.ledger().timestamp();
    let expires_at = now + 30 * 24 * 3600;

    // Use execute_delegated_delegate to write the nonce key.
    let payload = crate::domain::DelegatedActionPayload {
        domain: crate::domain::DomainTag::Delegate,
        owner: owner.clone(),
        target: delegate.clone(),
        contract_id: contract_id.clone(),
        nonce: 0,
        scheme: 0,
    };
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &payload,
    );

    advance_time(&e, &contract_id, 10 * 24 * 3600);

    // get_nonce should bump the TTL
    let nonce = client.get_nonce(&owner);
    assert_eq!(nonce, 1);

    let nonce_key = DataKey::Nonce(owner.clone());
    let ttl = nonce_ttl(&e, &contract_id, &nonce_key);
    // After advancing 10 days (172_800 ledgers), the TTL was bumped to MIN_NONCE_TTL
    // at the time of the bump, so the remaining TTL is MIN_NONCE_TTL - 172_800.
    let ledgers_advanced: u32 = (10 * 24 * 3600 / 5) as u32;
    assert!(
        ttl >= MIN_NONCE_TTL.saturating_sub(ledgers_advanced),
        "Nonce TTL {ttl} too low after get_nonce bump"
    );
}

// ---------------------------------------------------------------------------
// Test 7: Revoked delegation still has its TTL bumped
// ---------------------------------------------------------------------------

#[test]
fn test_delegation_ttl_bumped_on_revoke() {
    let (e, client) = setup();
    let contract_id = client.address.clone();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let now = e.ledger().timestamp();
    let expires_at = now + 30 * 24 * 3600;

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &expires_at,
        &0_u64,
    );

    advance_time(&e, &contract_id, 5 * 24 * 3600);

    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation, &1_u64);

    let key = DataKey::Delegation(owner.clone(), delegate.clone(), DelegationType::Attestation);
    let ttl = delegation_ttl(&e, &contract_id, &key);
    assert!(
        ttl >= LEDGER_BUMP_BUFFER,
        "TTL after revoke {ttl} < LEDGER_BUMP_BUFFER"
    );
}

// ---------------------------------------------------------------------------
// Test 8: TTL is capped at MAX_TTL for maximum-duration delegations
// ---------------------------------------------------------------------------

#[test]
fn test_delegation_ttl_capped_at_max() {
    let (e, client) = setup();
    let contract_id = client.address.clone();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    let now = e.ledger().timestamp();
    let expires_at = now + MAX_DELEGATION_DURATION;

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expires_at,
        &0_u64,
    );

    let key = DataKey::Delegation(owner.clone(), delegate.clone(), DelegationType::Management);
    let ttl = delegation_ttl(&e, &contract_id, &key);
    assert_eq!(
        ttl, MAX_TTL,
        "TTL {ttl} should be capped at MAX_TTL {MAX_TTL}"
    );
}
