//! Tests for `verify_delegation_active` / `check_delegation_active` (issue #773).
//!
//! # Threat being mitigated
//!
//! Without an explicit active-status check, expired or revoked delegations can
//! still be presented as valid authority.  An attacker can:
//!
//! 1. Hold onto an expired delegation record and replay it after the owner's
//!    intended authority window has closed.
//! 2. Continue using a delegation that the owner explicitly revoked (e.g. after
//!    a key compromise or change of intent).
//!
//! `verify_delegation_active` closes this gap by asserting at every
//! authorisation point that the delegation is both non-revoked AND
//! non-expired. The check is centralised so no caller can accidentally skip it.
//!
//! # Test structure
//!
//! - `active_delegation_passes`                             — happy path
//! - `expired_delegation_rejects_with_inactive`             — negative: now >= expires_at → 510
//! - `revoked_delegation_rejects_with_inactive`             — negative: revoked flag set → 510
//! - `missing_delegation_rejects_with_not_found`            — negative: no record → 501
//! - `delegation_active_at_one_second_before_expiry_passes` — boundary: now == expires_at - 1 passes
//! - `delegation_inactive_at_exact_expiry_ledger_rejects`   — boundary: now == expires_at rejects

#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, Address, CredenceDelegationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, contract_id, client)
}

fn set_ts(env: &Env, ts: u64) {
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: ts,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0u8; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 3_000_000,
    });
}

// ---------------------------------------------------------------------------
// Happy-path
// ---------------------------------------------------------------------------

/// Active delegation (not revoked, not expired) passes with no panic.
#[test]
fn active_delegation_passes() {
    let (env, _cid, client) = setup();
    set_ts(&env, 1000);
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &2000u64,
        &client.get_nonce(&owner),
    );

    // now (1000) < expires_at (2000): must not panic.
    client.check_delegation_active(&owner, &delegate, &DelegationType::Attestation);
}

// ---------------------------------------------------------------------------
// Expiry boundary tests
// ---------------------------------------------------------------------------

/// now == expires_at - 1: delegation is still active — must pass.
#[test]
fn delegation_active_at_one_second_before_expiry_passes() {
    let (env, _cid, client) = setup();
    set_ts(&env, 1000);
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &1100u64,
        &client.get_nonce(&owner),
    );

    set_ts(&env, 1099); // one second before expiry
    client.check_delegation_active(&owner, &delegate, &DelegationType::Attestation);
}

/// now == expires_at: delegation has just expired — must reject with DelegationInactive (510).
#[test]
#[should_panic(expected = "Error(Contract, #510)")]
fn delegation_inactive_at_exact_expiry_ledger_rejects() {
    let (env, _cid, client) = setup();
    set_ts(&env, 1000);
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &1100u64,
        &client.get_nonce(&owner),
    );

    set_ts(&env, 1100); // exactly expires_at
    client.check_delegation_active(&owner, &delegate, &DelegationType::Attestation);
}

// ---------------------------------------------------------------------------
// Negative tests
// ---------------------------------------------------------------------------

/// Expired delegation (now >> expires_at) → DelegationInactive (code 510).
#[test]
#[should_panic(expected = "Error(Contract, #510)")]
fn expired_delegation_rejects_with_inactive() {
    let (env, _cid, client) = setup();
    set_ts(&env, 1000);
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &1100u64,
        &client.get_nonce(&owner),
    );

    set_ts(&env, 99999); // far past expiry
    client.check_delegation_active(&owner, &delegate, &DelegationType::Attestation);
}

/// Revoked delegation (revoked before expiry) → DelegationInactive (code 510).
#[test]
#[should_panic(expected = "Error(Contract, #510)")]
fn revoked_delegation_rejects_with_inactive() {
    let (env, _cid, client) = setup();
    set_ts(&env, 1000);
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);

    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &5000u64,
        &client.get_nonce(&owner),
    );

    // Revoke while well within the validity window (now=1000 << expires_at=5000).
    client.revoke_delegation(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &client.get_nonce(&owner),
    );

    // Must reject even though the clock hasn't reached expires_at.
    client.check_delegation_active(&owner, &delegate, &DelegationType::Attestation);
}

/// No delegation record → DelegationNotFound (code 501).
#[test]
#[should_panic(expected = "Error(Contract, #501)")]
fn missing_delegation_rejects_with_not_found() {
    let (env, _cid, client) = setup();
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);

    // No delegation was ever created for this key.
    client.check_delegation_active(&owner, &delegate, &DelegationType::Attestation);
}
