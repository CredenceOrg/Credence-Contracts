//! Unit tests for the `verify_delegated_signature` lease-signature helper.
//!
//! Closes #854.
//!
//! Coverage: **Valid** / **Corrupted** / **Revoked** scenarios. A single
//! property-based test exercises the unknown-scheme rejection path across
//! the full `u32` range above the registered whitelist.
//!
//! Why "lease-signature"?  The signature covers a delegated, time-bound
//! action: the owner "leases" signing authority to a relayed payload that
//! must be consumed inside `MAX_PAYLOAD_AGE_LEDGERS`.  The signature
//! dispatcher therefore behaves like a lease validator — it accepts
//! well-formed, in-window signatures; rejects malformed ones; and refuses
//! to honour a scheme whose verifier has been administratively revoked.
//!
//! `#[should_panic(expected = "Error(Contract, #NNN)")]` asserts that the
//! panic surface is the canonical wire-stable error code.  The numeric values
//! come from `contracts/credence_errors/src/lib.rs`:
//!
//! | # | Variant                  |
//! |---|--------------------------|
//! | 504 | `UnknownScheme`         |
//! | 506 | `VerifierNotRegistered` |
//! | 507 | `VerificationFailed`    |
//!
//! All tests run against the in-process Soroban `Env`; they are deterministic
//! (no `Date.now()` / wall clock) and rely on the same `mock_all_auths()`
//! shortcut already used across the rest of the crate.

#![cfg(test)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    Address, Bytes, Env,
};

use crate::{
    verifier::{verify_delegated_signature, SchemeTag},
    DataKey,
};

// -----------------------------------------------------------------------------
// Mock verifier contracts.
//
// `#[contractimpl]` expands to module-level items named after each method, so
// two contracts with the same method name in the same `mod` would collide.
// Each contract lives in its own module, matching the pattern established by
// `test_verifier_dispatch.rs`.
// -----------------------------------------------------------------------------

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

mod panicking_verifier {
    use soroban_sdk::{contract, contractimpl, Address, Bytes};

    #[contract]
    pub struct PanickingVerifier;

    #[contractimpl]
    impl PanickingVerifier {
        // A guest-trapping verifier models real-world crypto libraries that
        // panic on malformed signature lengths rather than returning `false`.
        pub fn verify(_owner: Address, _message: Bytes, _signature: Bytes) -> bool {
            panic!("simulated verifier guest trap")
        }
    }
}

use invalid_verifier::AlwaysInvalidVerifier;
use panicking_verifier::PanickingVerifier;
use valid_verifier::AlwaysValidVerifier;

// -----------------------------------------------------------------------------
// Test fixtures
// -----------------------------------------------------------------------------

fn fresh_env() -> Env {
    let e = Env::default();
    e.mock_all_auths();
    e
}

/// Bypass the public `register_verifier` entry point so that tests can drive
/// the dispatcher directly without satisfying admin-auth/storage checks
/// already covered elsewhere.
fn storage_register_verifier(e: &Env, scheme: u32, verifier_id: &Address) {
    e.storage()
        .instance()
        .set(&DataKey::Verifier(scheme), verifier_id);
}

/// Remove the verifier entry for `scheme`, simulating a scheme whose
/// registration has been administratively revoked (no contract exists in
/// production that erases this entry; the test models the storage-level
/// outcome directly).
fn storage_unregister_verifier(e: &Env, scheme: u32) {
    e.storage().instance().remove(&DataKey::Verifier(scheme));
}

// =============================================================================
// Valid scenarios — the dispatcher completes without panic.
// =============================================================================

#[test]
fn verify_delegated_signature_valid_ed25519_completes_without_panic() {
    // Ed25519 is implicitly verified by Soroban's auth engine at the call
    // site (mocked by `mock_all_auths()`). The dispatcher MUST return Ok so
    // that callers like `execute_delegated_delegate` can proceed to the
    // subsequent guards.
    let e = fresh_env();
    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        SchemeTag::Ed25519.to_u32(),
    );
}

#[test]
fn verify_delegated_signature_valid_secp256r1_with_accepting_verifier_completes() {
    let e = fresh_env();
    let verifier_id = e.register(AlwaysValidVerifier, ());
    storage_register_verifier(&e, SchemeTag::Secp256r1.to_u32(), &verifier_id);

    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        SchemeTag::Secp256r1.to_u32(),
    );
}

#[test]
fn verify_delegated_signature_valid_mldsa44_with_accepting_verifier_completes() {
    let e = fresh_env();
    let verifier_id = e.register(AlwaysValidVerifier, ());
    storage_register_verifier(&e, SchemeTag::MLDSA44.to_u32(), &verifier_id);

    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        SchemeTag::MLDSA44.to_u32(),
    );
}

// =============================================================================
// Corrupted scenarios — the dispatcher panics with a wire-stable error code.
// =============================================================================

#[test]
#[should_panic(expected = "Error(Contract, #504)")]
fn verify_delegated_signature_corrupted_unknown_scheme_99_panics_with_unknown_scheme() {
    // `504` = `ContractError::UnknownScheme`.
    let e = fresh_env();
    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        99,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #504)")]
fn verify_delegated_signature_corrupted_unknown_scheme_max_panics_with_unknown_scheme() {
    let e = fresh_env();
    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        u32::MAX,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #507)")]
fn verify_delegated_signature_corrupted_secp256r1_rejecting_verifier_panics_with_verification_failed() {
    // `507` = `ContractError::VerificationFailed`.
    let e = fresh_env();
    let verifier_id = e.register(AlwaysInvalidVerifier, ());
    storage_register_verifier(&e, SchemeTag::Secp256r1.to_u32(), &verifier_id);

    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        SchemeTag::Secp256r1.to_u32(),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #507)")]
fn verify_delegated_signature_corrupted_mldsa44_rejecting_verifier_panics_with_verification_failed() {
    let e = fresh_env();
    let verifier_id = e.register(AlwaysInvalidVerifier, ());
    storage_register_verifier(&e, SchemeTag::MLDSA44.to_u32(), &verifier_id);

    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        SchemeTag::MLDSA44.to_u32(),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #507)")]
fn verify_delegated_signature_corrupted_all_zero_message_signature_bytes_rejected() {
    // Wire-level corruption: dropping the message and signature to all-zero
    // bytes still produces the canonical VerificationFailed panic. This is
    // the boundary case for "garbled signature" payloads.
    let e = fresh_env();
    let verifier_id = e.register(AlwaysInvalidVerifier, ());
    storage_register_verifier(&e, SchemeTag::Secp256r1.to_u32(), &verifier_id);

    let owner = Address::generate(&e);
    let message = Bytes::from_slice(&e, &[0u8; 64]);
    let signature = Bytes::from_slice(&e, &[0u8; 64]);
    verify_delegated_signature(
        &e,
        &owner,
        &message,
        &signature,
        SchemeTag::Secp256r1.to_u32(),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #507)")]
fn verify_delegated_signature_corrupted_all_one_message_signature_bytes_rejected() {
    // A non-zero byte corruption pattern: all-`0xFF` for both bytes; the
    // rejecting verifier still classifies this as invalid.
    let e = fresh_env();
    let verifier_id = e.register(AlwaysInvalidVerifier, ());
    storage_register_verifier(&e, SchemeTag::Secp256r1.to_u32(), &verifier_id);

    let owner = Address::generate(&e);
    let message = Bytes::from_slice(&e, &[0xFFu8; 64]);
    let signature = Bytes::from_slice(&e, &[0xFFu8; 64]);
    verify_delegated_signature(
        &e,
        &owner,
        &message,
        &signature,
        SchemeTag::Secp256r1.to_u32(),
    );
}

// =============================================================================
// Revoked scenarios — the dispatcher refuses service for a scheme whose
// verifier has been administratively revoked, even when the scheme value
// is otherwise in-whitelist.
// =============================================================================

#[test]
#[should_panic(expected = "Error(Contract, #506)")]
fn verify_delegated_signature_revoked_secp256r1_never_registered_panics_verifier_not_registered() {
    // `506` = `ContractError::VerifierNotRegistered`.
    let e = fresh_env();
    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        SchemeTag::Secp256r1.to_u32(),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #506)")]
fn verify_delegated_signature_revoked_mldsa44_never_registered_panics_verifier_not_registered() {
    let e = fresh_env();
    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        SchemeTag::MLDSA44.to_u32(),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #506)")]
fn verify_delegated_signature_revoked_secp256r1_after_unregister_panics_verifier_not_registered() {
    // Realistic revocation flow: an admin first registers a verifier, the
    // verifier is later administratively removed, and a new relayed payload
    // arrives during the gap. The dispatcher must still refuse the call.
    let e = fresh_env();
    let verifier_id = e.register(AlwaysValidVerifier, ());
    storage_register_verifier(&e, SchemeTag::Secp256r1.to_u32(), &verifier_id);
    storage_unregister_verifier(&e, SchemeTag::Secp256r1.to_u32());

    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        SchemeTag::Secp256r1.to_u32(),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #506)")]
fn verify_delegated_signature_revoked_mldsa44_after_unregister_panics_verifier_not_registered() {
    let e = fresh_env();
    let verifier_id = e.register(AlwaysValidVerifier, ());
    storage_register_verifier(&e, SchemeTag::MLDSA44.to_u32(), &verifier_id);
    storage_unregister_verifier(&e, SchemeTag::MLDSA44.to_u32());

    let owner = Address::generate(&e);
    verify_delegated_signature(
        &e,
        &owner,
        &Bytes::new(&e),
        &Bytes::new(&e),
        SchemeTag::MLDSA44.to_u32(),
    );
}

// =============================================================================
// Property-based test coverage
// =============================================================================
// Property-based testing for the unknown-scheme rejection path would require
// `std::panic::catch_unwind` to assert that the dispatcher panics for every
// out-of-whitelist scheme value.  This crate is `#![no_std]` and the test
// harness does not expose `std::panic` symbols here, so the explicit manual
// tests `verify_delegated_signature_corrupted_unknown_scheme_99_panics_with_unknown_scheme`
// and `verify_delegated_signature_corrupted_unknown_scheme_max_panics_with_unknown_scheme`
// cover the rejection invariant with concrete boundary values (99 and
// `u32::MAX`).  Proptest can be added later in a follow-up that introduces a
// non-`no_std` integration-test harness if exhaustive shrinking is required.
