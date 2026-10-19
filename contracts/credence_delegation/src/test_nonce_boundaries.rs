//! Nonce boundaries and recovery, including real contract-frame rollback.
//!
//! A test-only contract exposes the module helpers to SDK `try_*` clients.
//! Rejections are tested through contract frames, rather than catching native
//! Rust panics, so storage rollback follows Soroban invocation semantics.
//! Public-entrypoint tests separately verify owner authorization and failures
//! that occur after nonce consumption. Competing submissions are serialized,
//! as on ledger; exactly one may spend a given nonce.

extern crate std;

use super::*;
use crate::{
    CredenceDelegation, CredenceDelegationClient, DelegationType, MAX_NONCE_INVALIDATION_SPAN,
};
use soroban_sdk::testutils::{storage::Persistent as _, Address as _, Events as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, Error};
use std::string::ToString;

#[contract]
struct NonceHarness;

#[contractimpl]
impl NonceHarness {
    pub fn read(e: Env, owner: Address) -> u64 {
        get_nonce(&e, &owner)
    }

    pub fn consume(e: Env, owner: Address, expected: u64) {
        consume_nonce(&e, &owner, expected);
    }

    pub fn invalidate(e: Env, owner: Address, target: u64, span: u64) -> (u64, u64) {
        invalidate_nonce_range(&e, &owner, target, span)
    }
}

fn setup() -> (Env, Address, Address) {
    let e = Env::default();
    let contract = e.register(NonceHarness, ());
    let owner = Address::generate(&e);
    (e, contract, owner)
}

fn seed(e: &Env, contract: &Address, owner: &Address, value: u64) {
    e.as_contract(contract, || {
        let key = DataKey::Nonce(owner.clone());
        e.storage().persistent().set(&key, &value);
        bump_nonce_ttl(e, &key, 0);
    });
}

fn stored(e: &Env, contract: &Address, owner: &Address) -> Option<u64> {
    e.as_contract(contract, || {
        e.storage().persistent().get(&DataKey::Nonce(owner.clone()))
    })
}

fn error(code: ContractError) -> Error {
    Error::from_contract_error(code as u32)
}

fn public_setup() -> (Env, Address, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let contract = e.register(CredenceDelegation, ());
    CredenceDelegationClient::new(&e, &contract).initialize(&Address::generate(&e));
    let owner = Address::generate(&e);
    (e, contract, owner)
}

#[test]
fn fresh_reads_do_not_create_nonce_entries() {
    let (e, contract, owner) = setup();
    let client = NonceHarnessClient::new(&e, &contract);
    assert_eq!(client.read(&owner), 0);
    assert_eq!(client.read(&owner), 0);
    assert_eq!(stored(&e, &contract, &owner), None);
}

#[test]
fn consecutive_consumption_is_isolated_per_identity() {
    let (e, contract, owner) = setup();
    let client = NonceHarnessClient::new(&e, &contract);
    let other = Address::generate(&e);
    for expected in 0..3 {
        client.consume(&owner, &expected);
        assert_eq!(client.read(&owner), expected + 1);
        assert_eq!(client.read(&other), 0);
    }
    client.consume(&other, &0);
    assert_eq!(client.read(&owner), 3);
    assert_eq!(client.read(&other), 1);
}

#[test]
fn stale_future_and_window_boundary_rejections_preserve_state_and_allow_retry() {
    let (e, contract, owner) = setup();
    let client = NonceHarnessClient::new(&e, &contract);
    seed(&e, &contract, &owner, 7);
    for expected in [
        0,
        6,
        8,
        7 + MAX_NONCE_FUTURE_WINDOW,
        8 + MAX_NONCE_FUTURE_WINDOW,
        u64::MAX,
    ] {
        assert_eq!(
            client.try_consume(&owner, &expected),
            Err(Ok(error(ContractError::InvalidNonce)))
        );
        assert_eq!(stored(&e, &contract, &owner), Some(7));
    }
    client.consume(&owner, &7);
    assert_eq!(client.read(&owner), 8);
    assert_eq!(
        client.try_consume(&owner, &7),
        Err(Ok(error(ContractError::InvalidNonce)))
    );
    assert_eq!(client.read(&owner), 8);
}

#[test]
fn saturated_future_window_and_terminal_overflow_never_wrap_nonce() {
    let (e, contract, owner) = setup();
    let client = NonceHarnessClient::new(&e, &contract);
    seed(&e, &contract, &owner, u64::MAX - 1);
    assert_eq!(
        client.try_consume(&owner, &u64::MAX),
        Err(Ok(error(ContractError::InvalidNonce)))
    );
    assert_eq!(client.read(&owner), u64::MAX - 1);
    client.consume(&owner, &(u64::MAX - 1));
    for _ in 0..2 {
        assert_eq!(
            client.try_consume(&owner, &u64::MAX),
            Err(Ok(error(ContractError::Overflow)))
        );
        assert_eq!(client.read(&owner), u64::MAX);
    }
    assert_eq!(
        client.try_consume(&owner, &0),
        Err(Ok(error(ContractError::InvalidNonce)))
    );
}

#[test]
fn recovery_range_is_half_open_and_duplicate_invalidations_are_rejected() {
    let (e, contract, owner) = setup();
    let client = NonceHarnessClient::new(&e, &contract);
    for target in [0, MAX_NONCE_INVALIDATION_SPAN + 1] {
        assert_eq!(
            client.try_invalidate(&owner, &target, &MAX_NONCE_INVALIDATION_SPAN),
            Err(Ok(error(ContractError::InvalidNonce)))
        );
        assert_eq!(stored(&e, &contract, &owner), None);
    }
    assert_eq!(
        client.invalidate(&owner, &1, &MAX_NONCE_INVALIDATION_SPAN),
        (0, 1)
    );
    assert_eq!(
        client.invalidate(
            &owner,
            &(1 + MAX_NONCE_INVALIDATION_SPAN),
            &MAX_NONCE_INVALIDATION_SPAN
        ),
        (1, 1 + MAX_NONCE_INVALIDATION_SPAN)
    );
    let target = 1 + MAX_NONCE_INVALIDATION_SPAN;
    for invalid in [0, target - 1, target] {
        assert_eq!(
            client.try_invalidate(&owner, &invalid, &MAX_NONCE_INVALIDATION_SPAN),
            Err(Ok(error(ContractError::InvalidNonce)))
        );
        assert_eq!(client.read(&owner), target);
    }
    assert_eq!(
        client.try_consume(&owner, &(target - 1)),
        Err(Ok(error(ContractError::InvalidNonce)))
    );
    client.consume(&owner, &target);
    assert_eq!(client.read(&owner), target + 1);
}

#[test]
fn zero_span_and_upper_integer_invalidation_boundaries() {
    let (e, contract, owner) = setup();
    let client = NonceHarnessClient::new(&e, &contract);
    assert_eq!(
        client.try_invalidate(&owner, &1, &0),
        Err(Ok(error(ContractError::InvalidNonce)))
    );
    assert_eq!(stored(&e, &contract, &owner), None);
    seed(
        &e,
        &contract,
        &owner,
        u64::MAX - MAX_NONCE_INVALIDATION_SPAN,
    );
    assert_eq!(
        client.invalidate(&owner, &u64::MAX, &MAX_NONCE_INVALIDATION_SPAN),
        (u64::MAX - MAX_NONCE_INVALIDATION_SPAN, u64::MAX)
    );
    assert_eq!(
        client.try_invalidate(&owner, &u64::MAX, &MAX_NONCE_INVALIDATION_SPAN),
        Err(Ok(error(ContractError::InvalidNonce)))
    );
    assert_eq!(client.read(&owner), u64::MAX);
}

#[test]
fn snapshot_reload_preserves_replay_protection_and_allows_next_nonce() {
    let (e, contract, owner) = setup();
    let client = NonceHarnessClient::new(&e, &contract);
    client.consume(&owner, &0);
    client.invalidate(&owner, &50, &MAX_NONCE_INVALIDATION_SPAN);
    // SDK ledger reload models persisted-state recovery, not a network
    // RestoreFootprint transaction. Native function registration is not saved.
    let restored = Env::from_snapshot(e.to_snapshot());
    let contract = Address::from_str(&restored, &contract.to_string().to_string());
    let owner = Address::from_str(&restored, &owner.to_string().to_string());
    restored.register_at(&contract, NonceHarness, ());
    let client = NonceHarnessClient::new(&restored, &contract);
    assert_eq!(client.read(&owner), 50);
    assert_eq!(
        client.try_consume(&owner, &0),
        Err(Ok(error(ContractError::InvalidNonce)))
    );
    assert_eq!(
        client.try_consume(&owner, &49),
        Err(Ok(error(ContractError::InvalidNonce)))
    );
    client.consume(&owner, &50);
    assert_eq!(client.read(&owner), 51);
}

#[test]
fn expiry_ttl_floor_buffer_cap_and_extreme_timestamps() {
    let (e, _, _) = setup();
    e.ledger().with_mut(|info| info.timestamp = 100);
    for (expiry, expected) in [
        (0, LEDGER_BUMP_BUFFER),
        (99, LEDGER_BUMP_BUFFER),
        (100, LEDGER_BUMP_BUFFER),
        (104, LEDGER_BUMP_BUFFER),
        (105, LEDGER_BUMP_BUFFER + 1),
        (100 + u64::from(MAX_TTL - LEDGER_BUMP_BUFFER) * 5, MAX_TTL),
        (100 + (u64::from(u32::MAX) + 1) * 5, MAX_TTL),
        (u64::MAX, MAX_TTL),
    ] {
        assert_eq!(ttl_for_expiry(&e, expiry), expected);
    }
}

#[test]
fn nonce_ttl_refreshes_at_threshold_and_never_shortens_existing_lifetime() {
    let (e, contract, owner) = setup();
    let key = DataKey::Nonce(owner.clone());
    seed(&e, &contract, &owner, 42);
    e.as_contract(&contract, || {
        // Keep the harness callable while advancing ledgers to the nonce's
        // renewal threshold; instance archival is a separate lifecycle.
        e.storage().instance().extend_ttl(MAX_TTL / 2, MAX_TTL);
        assert_eq!(e.storage().persistent().get_ttl(&key), MIN_NONCE_TTL)
    });
    let delta = MIN_NONCE_TTL / 2 + 1;
    e.ledger().with_mut(|info| {
        info.sequence_number += delta;
    });
    e.as_contract(&contract, || {
        assert_eq!(get_nonce(&e, &owner), 42);
        assert_eq!(e.storage().persistent().get_ttl(&key), MIN_NONCE_TTL);
        // Force renewal before requesting the maximum horizon.
        e.ledger().with_mut(|info| {
            info.sequence_number += delta;
        });
        bump_nonce_ttl(&e, &key, u64::MAX);
        assert_eq!(e.storage().persistent().get_ttl(&key), MAX_TTL);
        bump_nonce_ttl(&e, &key, 0);
        assert_eq!(e.storage().persistent().get_ttl(&key), MAX_TTL);
        assert_eq!(e.storage().persistent().get::<_, u64>(&key), Some(42));
    });
}

#[test]
fn existing_entry_expiry_bump_preserves_value_and_never_shortens_ttl() {
    let (e, contract, owner) = setup();
    let key = DataKey::Nonce(owner.clone());
    seed(&e, &contract, &owner, 42);
    e.as_contract(&contract, || {
        // The expiry helper accepts any persistent DataKey. Exercise its
        // existing-entry path without changing the stored nonce value.
        bump_delegation_ttl(&e, &key, u64::MAX);
        assert_eq!(e.storage().persistent().get_ttl(&key), MAX_TTL);
        bump_delegation_ttl(&e, &key, 0);
        assert_eq!(e.storage().persistent().get_ttl(&key), MAX_TTL);
        assert_eq!(e.storage().persistent().get::<_, u64>(&key), Some(42));
    });
}

#[test]
fn absent_ttl_bumps_do_not_create_storage() {
    let (e, contract, owner) = setup();
    let key = DataKey::Nonce(owner.clone());
    e.as_contract(&contract, || {
        bump_nonce_ttl(&e, &key, u64::MAX);
        bump_delegation_ttl(&e, &key, u64::MAX);
        assert!(!e.storage().persistent().has(&key));
    });
}

#[test]
fn downstream_failure_rolls_back_consumed_nonce_and_retry_succeeds() {
    let (e, contract, owner) = public_setup();
    let client = CredenceDelegationClient::new(&e, &contract);
    let delegate = Address::generate(&e);
    for _ in 0..2 {
        assert_eq!(
            client.try_revoke_delegation(&owner, &delegate, &DelegationType::Management, &0),
            Err(Ok(error(ContractError::DelegationNotFound)))
        );
        assert!(e.events().all().is_empty());
        assert_eq!(stored(&e, &contract, &owner), None);
    }
    client.delegate(&owner, &delegate, &DelegationType::Management, &3600, &0);
    assert_eq!(client.get_nonce(&owner), 1);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Management, &1);
    assert_eq!(client.get_nonce(&owner), 2);
    assert!(
        client
            .get_delegation(&owner, &delegate, &DelegationType::Management)
            .revoked
    );
}

#[test]
fn competing_submissions_spend_nonce_exactly_once_and_can_resynchronize() {
    let (e, contract, owner) = public_setup();
    let first = CredenceDelegationClient::new(&e, &contract);
    let second = CredenceDelegationClient::new(&e, &contract);
    let a = Address::generate(&e);
    let b = Address::generate(&e);
    first.delegate(&owner, &a, &DelegationType::Attestation, &3600, &0);
    assert_eq!(
        second
            .try_delegate(&owner, &b, &DelegationType::Attestation, &3600, &0)
            .err(),
        Some(Ok(error(ContractError::InvalidNonce)))
    );
    assert_eq!(first.get_nonce(&owner), 1);
    e.as_contract(&contract, || {
        assert!(!e.storage().persistent().has(&DataKey::Delegation(
            owner.clone(),
            b.clone(),
            DelegationType::Attestation
        )))
    });
    second.delegate(&owner, &b, &DelegationType::Attestation, &3600, &1);
    assert_eq!(first.get_nonce(&owner), 2);
}

#[test]
fn unauthenticated_invalidation_preserves_state_then_authorized_recovery_succeeds() {
    let (e, contract, owner) = public_setup();
    let client = CredenceDelegationClient::new(&e, &contract);
    seed(&e, &contract, &owner, 3);
    e.mock_auths(&[]);
    assert!(matches!(
        client.try_invalidate_nonce_range(&owner, &4),
        Err(Err(_))
    ));
    assert_eq!(stored(&e, &contract, &owner), Some(3));
    e.mock_all_auths();
    client.invalidate_nonce_range(&owner, &4);
    assert_eq!(client.get_nonce(&owner), 4);
}
