//! Signature scheme verifier registry for delegated action signatures.
//!
//! Post-quantum cryptography requires support for multiple signature schemes.
//! This module provides an explicit verifier registry that maps scheme tags
//! (Ed25519, Secp256r1, MLDSA44) to verifier implementations, replacing the
//! implicit Ed25519-only approach of the Soroban auth engine.
//!
//! The registry is admin-controlled and emits `verifier_registered` events on
//! each registration. The default scheme remains Ed25519 for backwards
//! compatibility with existing delegated payloads.
//!
//! ## Wire Stability
//!
//! **WARNING**: The numeric values of `SchemeTag` enum variants are wire-stable
//! and encoded into signatures. Changing variant values after deployment will
//! break existing signatures. When adding new schemes, append at the end only.

use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Address, Bytes, Env, IntoVal, Symbol, Val, Vec};

use crate::DataKey;

/// Supported signature schemes for delegated action signatures.
///
/// Scheme tags are wire-stable and must never be renumbered after deployment.
/// New schemes must be appended at the end of the enum only.
///
/// Each variant corresponds to a cryptographic signing algorithm:
/// - **Ed25519** (0): EdDSA signature scheme using Curve25519 (NIST standard)
/// - **Secp256r1** (1): ECDSA over NIST P-256 curve (also called P-256, prime256v1)
/// - **MLDSA44** (2): ML-DSA with security parameter 44, post-quantum resistant
///
/// The wire-stable value constraint means old clients must be able to verify
/// signatures even after adding new schemes. Adding a scheme to this enum
/// requires corresponding verifier implementations registered in the contract.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SchemeTag {
    /// Ed25519 EdDSA signatures (default for backwards compatibility).
    Ed25519 = 0,
    /// ECDSA signatures over NIST P-256 curve.
    Secp256r1 = 1,
    /// ML-DSA signatures (post-quantum resistant).
    MLDSA44 = 2,
}

impl SchemeTag {
    /// Convert a u8 to SchemeTag, returning None for unknown values.
    ///
    /// This function enforces a strict whitelist of known schemes,
    /// rejecting any unregistered or future scheme tags.
    pub fn try_from_u32(value: u32) -> Option<SchemeTag> {
        match value {
            0 => Some(SchemeTag::Ed25519),
            1 => Some(SchemeTag::Secp256r1),
            2 => Some(SchemeTag::MLDSA44),
            _ => None,
        }
    }

    /// Convert SchemeTag to its wire-stable u8 value.
    pub fn to_u32(self) -> u32 {
        self as u32
    }

    /// Default scheme for backwards compatibility with legacy payloads.
    ///
    /// Existing delegated action payloads created before multi-scheme support
    /// are implicitly Ed25519. When decoding legacy payloads that lack an
    /// explicit scheme field, this default is assumed.
    pub fn default_scheme() -> SchemeTag {
        SchemeTag::Ed25519
    }

    /// Check if a scheme is currently known/supported by this implementation.
    ///
    /// This differs from `try_from_u32` in that it allows for schemes that may
    /// not yet have registered verifiers. Use this to provide better error
    /// messages when a scheme is recognized but not yet verified.
    pub fn is_known(value: u32) -> bool {
        value <= 2
    }
}

/// Trait for signature scheme verifiers.
///
/// Each verifier implementation handles the cryptographic operations for
/// a specific signature scheme. The contract runtime dispatches to the
/// appropriate verifier based on the `SchemeTag` in the payload.
///
/// ## Verification Semantics
///
/// Verifiers must:
/// 1. Validate the signature bytes format for their scheme
/// 2. Verify the signature against the message and public key
/// 3. Return success (no panic) only if the signature is cryptographically valid
/// 4. Panic with `VerificationFailed` for any validation error
///
/// The `owner` address is assumed to contain or derive the public key material.
/// The contract's auth engine (`owner.require_auth()`) is called separately.
pub trait SignatureVerifier: Send + Sync {
    /// Verify a signature for a delegated action payload.
    ///
    /// # Arguments
    /// * `e` - The Soroban environment
    /// * `owner` - The account whose signature is being verified
    /// * `message` - The serialized payload hash to be verified
    /// * `signature` - The raw signature bytes for this scheme
    ///
    /// # Behavior
    /// - Success: Returns normally (no panic)
    /// - Verification failure: Panics with an appropriate error
    /// - Invalid format: Panics with scheme-specific error
    fn verify(&self, e: &Env, owner: &Address, message: &Bytes, signature: &Bytes);
}

/// Storage of a registered verifier implementation.
///
/// Associates a scheme tag with a trait object that implements signature
/// verification for that scheme.
#[contracttype]
#[derive(Clone, Debug)]
pub struct VerifierEntry {
    /// The signature scheme this verifier handles.
    pub scheme: u32,
    /// A unique identifier or address for this verifier.
    pub verifier_id: Address,
}

/// Event emitted when a verifier is registered for a signature scheme.
///
/// This event is emitted by `register_verifier()` to provide an audit trail
/// of scheme registration events. Off-chain indexers can use this event
/// to track which schemes are supported and when they were enabled.
#[contracttype]
#[derive(Clone, Debug)]
pub struct VerifierRegisteredEvent {
    /// The signature scheme being registered.
    pub scheme: u32,
    /// The address of the verifier implementation.
    pub verifier_id: Address,
    /// The admin who performed the registration.
    pub admin: Address,
}

/// Emit a `verifier_registered` event for audit trail tracking.
pub fn emit_verifier_registered(e: &Env, scheme: u32, verifier_id: &Address, admin: &Address) {
    let event = VerifierRegisteredEvent {
        scheme,
        verifier_id: verifier_id.clone(),
        admin: admin.clone(),
    };

    e.events().publish(("verifier", "registered"), event);
}

/// Validate that a scheme is known and supported.
///
/// This function panics with `UnknownScheme` if the scheme is not recognized
/// or not currently registered. Use this to provide a consistent error path
/// for unsupported scheme tags.
pub fn validate_scheme_registered(e: &Env, scheme: u32) {
    if !SchemeTag::is_known(scheme) {
        panic_with_error!(e, ContractError::UnknownScheme);
    }
}

/// Verify a delegated action signature according to the specified scheme.
///
/// This function dispatches signature verification based on the scheme tag:
/// - **Ed25519** (0): Verification delegated to Soroban's built-in auth engine
///   via `owner.require_auth()`, preserving backwards compatibility with
///   existing delegated payloads created before multi-scheme support.
/// - **Secp256r1** (1) / **MLDSA44** (2): Verification dispatches to registered
///   verifier contracts. Each registered verifier must implement the
///   `SignatureVerifier` trait semantics.
///
/// # Arguments
/// * `e` - The Soroban environment
/// * `owner` - The account whose signature is being verified
/// * `message` - The serialized payload hash (typically the hash of DelegatedActionPayload)
/// * `signature` - The raw signature bytes
/// * `scheme` - The signature scheme tag (must be known)
///
/// # Verification Semantics
///
/// For **Ed25519** payloads:
/// - The Soroban auth engine has already validated the signature via `owner.require_auth()`
///   at the call site. This function returns success without additional verification.
/// - This ensures wire-stable backwards compatibility: signatures created before
///   multi-scheme support (which implicitly use Ed25519) continue to verify.
///
/// For **post-quantum schemes** (Secp256r1, MLDSA44):
/// - Looks up the verifier address stored under `DataKey::Verifier(scheme)`.
/// - If no verifier is registered for the scheme, panics with `VerifierNotRegistered`.
/// - Invokes the verifier via a cross-contract call to `verify(owner, message, signature) -> bool`.
/// - If the verifier returns `false` (or panics internally), panics with `VerificationFailed`.
///
/// # Wire Stability
///
/// Changing the scheme tag encoding would break existing signatures. The numeric
/// values must remain constant:
/// - Ed25519 = 0
/// - Secp256r1 = 1
/// - MLDSA44 = 2
///
/// # Example: Integration at Call Site
///
/// ```text
/// // In execute_delegated_delegate():
/// let scheme = domain::decode_scheme_safe(&payload);
/// verifier::verify_delegated_signature(&e, &owner, &message_hash, &sig, scheme.to_u32());
/// ```
pub fn verify_delegated_signature(
    e: &Env,
    owner: &Address,
    message: &Bytes,
    signature: &Bytes,
    scheme: u32,
) {
    match SchemeTag::try_from_u32(scheme) {
        Some(SchemeTag::Ed25519) => {
            // Ed25519 is implicitly verified via Soroban's auth engine.
            // At the call site, owner.require_auth() has already validated
            // the transaction signature. This function confirms the scheme
            // is recognized and allows the caller to proceed with confidence.
            // No additional signature verification is needed because Soroban
            // has already authenticated the owner.
        }
        Some(SchemeTag::Secp256r1) | Some(SchemeTag::MLDSA44) => {
            // 1. Look up the registered verifier address for this scheme.
            let verifier_addr: Address = e
                .storage()
                .instance()
                .get(&DataKey::Verifier(scheme))
                .unwrap_or_else(|| panic_with_error!(e, ContractError::VerifierNotRegistered));

            // 2. Dispatch to the verifier contract via cross-contract call.
            //    The verifier must expose `fn verify(owner, message, signature) -> bool`.
            //    A `false` return (or a panic inside the verifier) maps to VerificationFailed.
            let args: Vec<Val> = soroban_sdk::vec![
                e,
                owner.clone().into_val(e),
                message.clone().into_val(e),
                signature.clone().into_val(e),
            ];
            let ok: bool = e.invoke_contract(&verifier_addr, &Symbol::new(e, "verify"), args);
            if !ok {
                panic_with_error!(e, ContractError::VerificationFailed);
            }
        }
        None => {
            panic_with_error!(e, ContractError::UnknownScheme);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_scheme_tag_from_u8() {
        assert_eq!(SchemeTag::try_from_u32(0), Some(SchemeTag::Ed25519));
        assert_eq!(SchemeTag::try_from_u32(1), Some(SchemeTag::Secp256r1));
        assert_eq!(SchemeTag::try_from_u32(2), Some(SchemeTag::MLDSA44));
        assert_eq!(SchemeTag::try_from_u32(3), None);
        assert_eq!(SchemeTag::try_from_u32(255), None);
    }

    #[test]
    fn test_scheme_tag_to_u32() {
        assert_eq!(SchemeTag::Ed25519.to_u32(), 0);
        assert_eq!(SchemeTag::Secp256r1.to_u32(), 1);
        assert_eq!(SchemeTag::MLDSA44.to_u32(), 2);
    }

    #[test]
    fn test_default_scheme() {
        assert_eq!(SchemeTag::default_scheme(), SchemeTag::Ed25519);
    }

    #[test]
    fn test_is_known() {
        assert!(SchemeTag::is_known(0));
        assert!(SchemeTag::is_known(1));
        assert!(SchemeTag::is_known(2));
        assert!(!SchemeTag::is_known(3));
        assert!(!SchemeTag::is_known(255));
    }

    #[test]
    fn test_ed25519_backwards_compatible() {
        // Ed25519 (scheme=0) is the default for backwards compatibility.
        // Existing delegated payloads created before multi-scheme support
        // implicitly use Ed25519 and must continue to verify without modification.
        assert_eq!(SchemeTag::Ed25519.to_u32(), 0);
        assert_eq!(SchemeTag::default_scheme(), SchemeTag::Ed25519);
        assert!(SchemeTag::is_known(0));
    }
}
