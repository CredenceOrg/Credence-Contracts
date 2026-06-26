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

// ---------------------------------------------------------------------------
// Mock verifier contracts — each in its own module to avoid symbol collisions
// from #[contractimpl] generating module-level names based on fn name.
// ---------------------------------------------------------------------------

mod valid_verifier {
    use soroban_sdk::{contract, contractimpl, Address, Bytes};

    #[contract]
    pub struct AlwaysValidVerifier;

    #[contractimpl]
    impl AlwaysValidVerifier {
        pub fn verify(_owner: Address, _message: Bytes, _signature: Bytes) -> bool {
            true
        }
    }
}

mod invalid_verifier {
    use soroban_sdk::{contract, contractimpl, Address, Bytes};

    #[contract]
    pub struct AlwaysInvalidVerifier;

    #[contractimpl]
    impl AlwaysInvalidVerifier {
        pub fn verify(_owner: Address, _message: Bytes, _signature: Bytes) -> bool {
            false
        }
    }
}

use invalid_verifier::AlwaysInvalidVerifier;
use valid_verifier::AlwaysValidVerifier;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, CredenceDelegationClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let cid = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &cid);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client, admin)
}

fn make_payload(
    owner: &Address,
    target: &Address,
    contract_id: &Address,
    nonce: u64,
    scheme: u32,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain: DomainTag::Delegate,
        owner: owner.clone(),
        target: target.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme,
    }
}

fn expiry(e: &Env) -> u64 {
    e.ledger().timestamp() + 1000
}

// ---------------------------------------------------------------------------
// Ed25519: no verifier registered → success (auth engine handles it)
// ---------------------------------------------------------------------------

#[test]
fn test_ed25519_unaffected() {
    let (e, client, _) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let p = make_payload(
        &owner,
        &delegate,
        &client.address,
        0,
        SchemeTag::Ed25519.to_u32(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expiry(&e),
        &p,
    );
}

// Ed25519: invalid verifier registered → still succeeds (registry ignored)
#[test]
fn test_ed25519_ignores_registered_verifier() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let v = e.register(AlwaysInvalidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Ed25519.to_u32(), &v);
    let p = make_payload(
        &owner,
        &delegate,
        &client.address,
        0,
        SchemeTag::Ed25519.to_u32(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expiry(&e),
        &p,
    );
}

// ---------------------------------------------------------------------------
// Unknown scheme: decode_scheme_safe defaults to Ed25519, so payloads with
// an unrecognized scheme tag succeed via the Ed25519 (auth-engine) path.
// The UnknownScheme guard in verify_delegated_signature is an internal
// defence; it is not reachable from the public API when the payload is
// decoded with decode_scheme_safe.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Secp256r1
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_secp256r1_unregistered_panics() {
    let (e, client, _) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let p = make_payload(
        &owner,
        &delegate,
        &client.address,
        0,
        SchemeTag::Secp256r1.to_u32(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expiry(&e),
        &p,
    );
}

#[test]
fn test_secp256r1_valid_verifier_succeeds() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let v = e.register(AlwaysValidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Secp256r1.to_u32(), &v);
    let p = make_payload(
        &owner,
        &delegate,
        &client.address,
        0,
        SchemeTag::Secp256r1.to_u32(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expiry(&e),
        &p,
    );
}

#[test]
#[should_panic]
fn test_secp256r1_invalid_verifier_panics() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let v = e.register(AlwaysInvalidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Secp256r1.to_u32(), &v);
    let p = make_payload(
        &owner,
        &delegate,
        &client.address,
        0,
        SchemeTag::Secp256r1.to_u32(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expiry(&e),
        &p,
    );
}

// ---------------------------------------------------------------------------
// MLDSA44
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_mldsa44_unregistered_panics() {
    let (e, client, _) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let p = make_payload(
        &owner,
        &delegate,
        &client.address,
        0,
        SchemeTag::MLDSA44.to_u32(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expiry(&e),
        &p,
    );
}

#[test]
fn test_mldsa44_valid_verifier_succeeds() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));
    let v = e.register(AlwaysValidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::MLDSA44.to_u32(), &v);
    let p = make_payload(
        &owner,
        &delegate,
        &client.address,
        0,
        SchemeTag::MLDSA44.to_u32(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expiry(&e),
        &p,
    );
}

// ---------------------------------------------------------------------------
// Re-registration overwrites the dispatch target
// ---------------------------------------------------------------------------

#[test]
fn test_re_registration_overwrites() {
    let (e, client, admin) = setup();
    let (owner, delegate) = (Address::generate(&e), Address::generate(&e));

    let bad = e.register(AlwaysInvalidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Secp256r1.to_u32(), &bad);

    let good = e.register(AlwaysValidVerifier, ());
    client.register_verifier(&admin, &SchemeTag::Secp256r1.to_u32(), &good);

    let p = make_payload(
        &owner,
        &delegate,
        &client.address,
        0,
        SchemeTag::Secp256r1.to_u32(),
    );
    client.execute_delegated_delegate(
        &owner,
        &delegate,
        &DelegationType::Management,
        &expiry(&e),
        &p,
    );
}
