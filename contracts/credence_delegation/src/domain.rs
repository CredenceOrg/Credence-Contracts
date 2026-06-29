//! Domain-separated payload for delegated action signatures.
//!
//! Without explicit domain separation a signature created for one function
//! (e.g. `delegate`) could be replayed against a different function
//! (e.g. `revoke_delegation`) because both consume the same nonce namespace.
//!
//! This module introduces a [`DomainTag`] enum that labels *which* function
//! domain owns a given signature, and a [`DelegatedActionPayload`] struct that
//! binds together:
//!
//! * `domain`       — the specific action type / function domain
//! * `owner`        — the principal whose authority is being invoked
//! * `target`       — the address being acted upon (delegate or subject)
//! * `contract_id`  — the current contract's address (chain / deployment context)
//! * `nonce`        — monotonically increasing per-owner counter
//! * `scheme`       — the signature scheme (Ed25519, Secp256r1, MLDSA44)
//!
//! Signature verification must hash *all* of these fields together.  A
//! signature produced for `domain = Delegate` will be structurally incompatible
//! with a `revoke_delegation` call even if the nonce happens to match.
//!
//! ## Multi-Scheme Support
//!
//! The `scheme` field enables support for post-quantum cryptographic algorithms.
//! Legacy payloads created before multi-scheme support default to Ed25519 when
//! the scheme field is absent, preserving backwards compatibility. Clients
//! transmitting payloads should always set the scheme explicitly.

use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env};

pub use crate::verifier::SchemeTag;

/// Labels each function domain that accepts a delegated (off-chain) signature.
///
/// Adding a new domain here forces a compile-time decision at every match site,
/// making it impossible to silently forget a function.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum DomainTag {
    /// Matches `delegate(…)` — creates or replaces a delegation entry.
    Delegate,
    /// Matches `revoke_delegation(…)` — revokes an existing delegation.
    RevokeDelegation,
    /// Matches `revoke_attestation(…)` — revokes an attestation-type delegation.
    RevokeAttestation,
}

/// Typed payload that must be hashed and signed by `owner` before a relayer
/// can submit a delegated action on their behalf.
///
/// The Soroban runtime does not expose EIP-712 natively, but the same
/// guarantees are achieved by requiring callers to pass this struct explicitly
/// and by verifying `owner.require_auth()` with the env's built-in
/// authorisation mechanism — which itself binds the call to the contract
/// address and ledger network.
///
/// ## Backwards Compatibility
///
/// Payloads created before multi-scheme support may lack an explicit `scheme`
/// field. During decoding, if the scheme is absent or invalid, [`decode_scheme_safe`]
/// defaults to Ed25519, ensuring that existing delegated payloads continue to verify
/// without modification.
///
/// Wire-stability note: The `scheme` field value must never be reinterpreted
/// or renumbered after deployment. Old signatures must remain verifiable even
/// after the contract supports new schemes.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DelegatedActionPayload {
    /// Which function this payload authorises.
    pub domain: DomainTag,
    /// The account whose authority is being delegated / invoked.
    pub owner: Address,
    /// The address the action targets (the delegate or subject address).
    pub target: Address,
    /// The contract address (deployment / chain context).
    pub contract_id: Address,
    /// Owner's current nonce — consumed on success.
    pub nonce: u64,
    /// The signature scheme: Ed25519, Secp256r1, or MLDSA44.
    /// Defaults to Ed25519 for backwards compatibility with legacy payloads.
    pub scheme: u32,
}

/// Validates that the fields in `payload` match the parameters supplied at the
/// call site, and that the `domain` tag is exactly `expected_domain`.
///
/// Panics with a descriptive, wire-stable delegation error code on any mismatch.
/// This preserves audit trail clarity by making every distinguishable payload
/// failure mode observable as a distinct `ContractError`.
///
/// - Domain mismatch => `DomainMismatch` (504)
/// - Owner mismatch  => `OwnerMismatch` (505)
/// - Target mismatch => `TargetMismatch` (506)
/// - Contract ID mismatch => `ContractIdMismatch` (507)
pub fn verify_payload(
    e: &Env,
    payload: &DelegatedActionPayload,
    expected_domain: DomainTag,
    caller_owner: &Address,
    caller_target: &Address,
) {
    if payload.domain != expected_domain {
        panic_with_error!(e, ContractError::DomainMismatch);
    }
    if &payload.owner != caller_owner {
        panic_with_error!(e, ContractError::OwnerMismatch);
    }
    if &payload.target != caller_target {
        panic_with_error!(e, ContractError::TargetMismatch);
    }
    if payload.contract_id != e.current_contract_address() {
        panic_with_error!(e, ContractError::ContractIdMismatch);
    }
}

/// Safely decode a scheme tag from the payload, defaulting to Ed25519 for
/// backwards compatibility with legacy payloads.
///
/// This function is used during deserialization to handle payloads created
/// before multi-scheme support. If the scheme field is absent or contains
/// an unrecognized value, it defaults to Ed25519.
///
/// For strict validation (rejecting unknown schemes), use
/// [`verify_scheme_supported`] after calling this function.
pub fn decode_scheme_safe(payload: &DelegatedActionPayload) -> SchemeTag {
    match SchemeTag::try_from_u32(payload.scheme) {
        Some(scheme) => scheme,
        None => SchemeTag::default_scheme(),
    }
}

/// Verify that the payload's scheme is a known and supported value.
///
/// Panics with `UnknownScheme` if the scheme is not recognized.
/// Call this after decoding to enforce strict validation.
pub fn verify_scheme_supported(e: &Env, scheme: u32) {
    if !SchemeTag::is_known(scheme) {
        panic_with_error!(e, ContractError::UnknownScheme);
    }
}
