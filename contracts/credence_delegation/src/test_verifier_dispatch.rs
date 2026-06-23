//! Integration tests for the multi-scheme verifier dispatch.
//!
//! Covers: registered valid/invalid verifier, unregistered scheme,
//! unknown scheme, Ed25519 unaffected, re-registration overwrites.
#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, Bytes, Env,
};

use crate::{
    domain::{DelegatedActionPayload, DomainTag},
    verifier::SchemeTag,
    CredenceDelegation, CredenceDelegationClient, DelegationType,
};

#[contract]
pub struct AlwaysValidVerifier;
#[contractimpl]
impl AlwaysValidVerifier {
    pub fn verify(_owner: Address, _message: Bytes, _signature: Bytes) -> bool {
        true
    }
}

#[contract]
pub struct AlwaysInvalidVerifier;
#[contractimpl]
impl AlwaysInvalidVerifier {
    pub fn verify(_owner: Address, _message: Bytes, _signature: Bytes) -> bool {
        false
    }
}

fn setup() -> (Env, CredenceDelegationClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let cid = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &cid);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client, admin)
}

fn payload(owner: &Address, target: &Address, contract_id: &Address, nonce: u64, scheme: u32) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain: DomainTag::Delegate,
        owner: owner.clone(),
        target: target.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme,
    }
}

fn expiry(e: &Env) -> u64 { e.ledger().timestamp() + 1000 }

// Ed25519: no verifier registered, must succeed (auth engine handles it)
#[test]
fn test_ed25519_unaffected() {
    let (e, client, _) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    client.execute_delegated_delegate(&owner, &delegate, &DelegationType::Management, &expiry(&e),
        &payload(&owner, &delegate, &client.address, 0, SchemeTag::Ed25519.to_u32()));
}

// Ed25519: even with an invalid verifier registered, auth engine path wins
#[test]
fn test_ed25519_ignores_registered_verifier() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let v = e.register(AlwaysInvalidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Ed25519.to_u32(), &v);
    client.execute_delegated_delegate(&owner, &delegate, &DelegationType::Management, &expiry(&e),
        &payload(&owner, &delegate, &client.address, 0, SchemeTag::Ed25519.to_u32()));
}

// Unknown scheme → panic (UnknownScheme)
#[test]
#[should_panic]
fn test_unknown_scheme_panics() {
    let (e, client, _) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    client.execute_delegated_delegate(&owner, &delegate, &DelegationType::Management, &expiry(&e),
        &payload(&owner, &delegate, &client.address, 0, 99));
}

// Secp256r1 unregistered → panic (VerifierNotRegistered)
#[test]
#[should_panic]
fn test_secp256r1_unregistered_panics() {
    let (e, client, _) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    client.execute_delegated_delegate(&owner, &delegate, &DelegationType::Management, &expiry(&e),
        &payload(&owner, &delegate, &client.address, 0, SchemeTag::Secp256r1.to_u32()));
}

// Secp256r1 + valid verifier → success
#[test]
fn test_secp256r1_valid_verifier_succeeds() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let v = e.register(AlwaysValidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Secp256r1.to_u32(), &v);
    client.execute_delegated_delegate(&owner, &delegate, &DelegationType::Management, &expiry(&e),
        &payload(&owner, &delegate, &client.address, 0, SchemeTag::Secp256r1.to_u32()));
}

// Secp256r1 + invalid verifier → panic (VerificationFailed)
#[test]
#[should_panic]
fn test_secp256r1_invalid_verifier_panics() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let v = e.register(AlwaysInvalidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Secp256r1.to_u32(), &v);
    client.execute_delegated_delegate(&owner, &delegate, &DelegationType::Management, &expiry(&e),
        &payload(&owner, &delegate, &client.address, 0, SchemeTag::Secp256r1.to_u32()));
}

// MLDSA44 unregistered → panic (VerifierNotRegistered)
#[test]
#[should_panic]
fn test_mldsa44_unregistered_panics() {
    let (e, client, _) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    client.execute_delegated_delegate(&owner, &delegate, &DelegationType::Management, &expiry(&e),
        &payload(&owner, &delegate, &client.address, 0, SchemeTag::MLDSA44.to_u32()));
}

// MLDSA44 + valid verifier → success
#[test]
fn test_mldsa44_valid_verifier_succeeds() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let v = e.register(AlwaysValidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::MLDSA44.to_u32(), &v);
    client.execute_delegated_delegate(&owner, &delegate, &DelegationType::Management, &expiry(&e),
        &payload(&owner, &delegate, &client.address, 0, SchemeTag::MLDSA44.to_u32()));
}

// Re-registration overwrites the dispatch target
#[test]
fn test_re_registration_overwrites() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));

    let bad = e.register(AlwaysInvalidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Secp256r1.to_u32(), &bad);

    let good = e.register(AlwaysValidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Secp256r1.to_u32(), &good);

    client.execute_delegated_delegate(&owner, &delegate, &DelegationType::Management, &expiry(&e),
        &payload(&owner, &delegate, &client.address, 0, SchemeTag::Secp256r1.to_u32()));
}
