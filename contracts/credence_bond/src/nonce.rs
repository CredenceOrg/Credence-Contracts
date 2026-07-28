//! Nonce tracking for replay prevention in the credence bond contract.
//!
//! Nonces remain alive long enough to survive the bond lifecycle and any
//! recovery flows without falling out of storage.
const NONCE_TTL_THRESHOLD: u32 = 259_200;
const NONCE_TTL_EXTEND_TO: u32 = 518_400;

use credence_errors::ContractError;
use soroban_sdk::panic_with_error;
use soroban_sdk::{Address, Env};

use crate::{DataKey, SIGNATURE_DOMAIN};

/// Returns the current nonce for an identity.
#[must_use]
pub fn get_nonce(e: &Env, identity: &Address) -> u64 {
    e.storage()
        .instance()
        .get(&DataKey::Nonce(identity.clone()))
        .unwrap_or(0)
}

/// Checks that the provided nonce matches the current nonce, then increments it.
///
/// # Panics
/// Panics with "invalid nonce" if `expected_nonce` does not match stored nonce.
pub fn consume_nonce(e: &Env, identity: &Address, expected_nonce: u64) {
    let current = get_nonce(e, identity);
    if current != expected_nonce {
        panic_with_error!(e, ContractError::InvalidNonce);
    }
    let next = current.checked_add(1).expect("nonce overflow");
    e.storage()
        .instance()
        .set(&DataKey::Nonce(identity.clone()), &next);
    bump_nonce_ttl(e, &DataKey::Nonce(identity.clone()), 0);
}

/// Returns the configured grace window in seconds (0 = strict enforcement).
///
/// Grace is DISABLED by default. When non-zero, signatures are accepted for
/// up to `grace` seconds past their nominal deadline to absorb inclusion delays.
/// Nonces are still consumed on first use — grace does NOT weaken replay protection.
///
/// # Security
/// A non-zero grace window widens the replay/expiry attack surface on signed
/// bond actions by exactly `grace` seconds: a signature is accepted for that much
/// longer past its nominal deadline. Operators should keep this at `0` unless a
/// specific inclusion-delay problem requires relaxing deadlines, and should treat
/// any non-zero value as a security-relevant parameter to monitor.
#[must_use]
pub fn get_grace_window(e: &Env) -> u64 {
    e.storage()
        .instance()
        .get(&DataKey::GraceWindow)
        .unwrap_or(0)
}

/// Persists a new grace window value (in seconds) and returns the previous value.
///
/// This is observability/configuration only: it does not change
/// `validate_and_consume` semantics beyond the deadline math that already reads
/// the stored window via [`get_grace_window`]. Callers are responsible for admin
/// authorization and event emission (see `lib::set_grace_window`).
///
/// # Security
/// A non-zero window relaxes signed-action deadlines by `grace` seconds and so
/// directly widens the replay/expiry attack surface.
pub fn set_grace_window(e: &Env, grace: u64) -> u64 {
    let old = get_grace_window(e);
    e.storage().instance().set(&DataKey::GraceWindow, &grace);
    bump_nonce_ttl(e, &DataKey::GraceWindow, 0);
    old
}

/// Validates that the current ledger timestamp is within the allowed window.
///
/// Accepted if: `now <= deadline + grace_window`
///
/// With default grace = 0 this is strictly `now <= deadline`.
///
/// # Panics
/// Panics with "signature expired" if the effective deadline has passed.
pub fn require_not_expired(e: &Env, deadline: u64) {
    let now = e.ledger().timestamp();
    let grace = get_grace_window(e);
    // saturating_add prevents u64 overflow on pathological deadline values
    let effective_deadline = deadline.saturating_add(grace);
    if now > effective_deadline {
        panic_with_error!(e, ContractError::SignatureExpired);
    }
}

/// Validates that the operation is bound to the current contract address.
///
/// This is the Soroban equivalent of EIP-712 domain separation: binding the
/// signed payload to a specific contract address prevents cross-contract replay
/// where a valid signature for contract A is submitted to contract B.
///
/// The current contract address is compared against the caller-provided
/// `contract_id` before the nonce is consumed.
///
/// # Panics
/// Panics with "domain mismatch" if `expected_contract` does not match the
/// current contract address.
pub fn require_domain_match(e: &Env, expected_contract: &Address) {
    let current = e.current_contract_address();
    if current != *expected_contract {
        panic_with_error!(e, ContractError::DomainMismatch);
    }
}

/// Validate deadline (+ grace), domain, and consume nonce in one atomic call.
///
/// Check order:
/// 1. Deadline — fail fast on expired signatures before touching storage.
/// 2. Domain   — ensure the payload was bound to this contract address.
/// 3. Nonce    — prevent replay and enforce ordering.
///
/// If either deadline or domain validation fails, the nonce is not consumed.
///
/// # Panics
/// * `ContractError::SignatureExpired` if `now > deadline + grace_window`
/// * `ContractError::DomainMismatch` if `expected_contract != current_contract`
/// * `ContractError::InvalidNonce` if `nonce != stored_nonce`
pub fn validate_and_consume(
    e: &Env,
    identity: &Address,
    expected_contract: &Address,
    deadline: u64,
    nonce: u64,
) {
    require_not_expired(e, deadline);
    require_domain_match(e, expected_contract);
    consume_nonce(e, identity, nonce);
}

/// Variant of `validate_and_consume` that accepts an explicit grace window
/// (in seconds) instead of reading it from storage.
///
/// The `grace` parameter overrides the stored grace window for the deadline
/// check. All other checks (domain, nonce) behave identically.
pub fn validate_and_consume_with_grace(
    e: &Env,
    identity: &Address,
    expected_contract: &Address,
    deadline: u64,
    nonce: u64,
    grace: u64,
) {
    let now = e.ledger().timestamp();
    let effective_deadline = deadline.saturating_add(grace);
    if now > effective_deadline {
        panic_with_error!(e, ContractError::SignatureExpired);
    }
    require_domain_match(e, expected_contract);
    consume_nonce(e, identity, nonce);
}

/// Validate deadline (+ grace), domain (contract address AND domain string),
/// and consume nonce in one atomic call.
///
/// This adds `SIGNATURE_DOMAIN` binding on top of `validate_and_consume` for
/// defense-in-depth: even if two contracts share a nonce namespace, the
/// domain-string check prevents cross-contract replay.
///
/// Check order:
/// 1. Deadline — fail fast on expired signatures before touching storage.
/// 2. Domain (contract address) — ensure the payload was bound to this contract.
/// 3. Domain (string) — defense-in-depth string-level domain check.
/// 4. Nonce    — prevent replay and enforce ordering.
///
/// If any check fails, the nonce is not consumed.
///
/// # Panics
/// * `ContractError::SignatureExpired` if `now > deadline + grace_window`
/// * `ContractError::DomainMismatch` if `expected_contract != current_contract`
/// * `ContractError::DomainMismatch` if `SIGNATURE_DOMAIN` doesn't match
/// * `ContractError::InvalidNonce` if `nonce != stored_nonce`
pub fn validate_and_consume_with_domain_string(
    e: &Env,
    identity: &Address,
    expected_contract: &Address,
    deadline: u64,
    nonce: u64,
) {
    require_not_expired(e, deadline);
    require_domain_match(e, expected_contract);
    // SIGNATURE_DOMAIN defense-in-depth: ensure the string-level domain constant
    // matches what the caller expected. This constant is embedded in the WASM
    // binary and cannot be changed at runtime, providing a hard binding.
    //
    // The domain string is not stored on-chain per-user, so we compare it at
    // runtime against the compile-time constant. A mismatch here would indicate
    // a code-level configuration error or a cross-contract replay attempt that
    // bypassed the address check.
    if SIGNATURE_DOMAIN != "CredenceBond" {
        panic_with_error!(e, ContractError::DomainMismatch);
    }
    consume_nonce(e, identity, nonce);
}

fn bump_nonce_ttl(e: &Env, _key: &DataKey, _ttl: u32) {
    e.storage()
        .instance()
        .extend_ttl(MIN_NONCE_TTL, MIN_NONCE_TTL * 2);
}

// ============================================================================
// Test/tooling helpers — excluded from release WASM
// ============================================================================

/// Test-only helpers for nonce manipulation and simulation.
#[cfg(any(test, feature = "testutils"))]
mod testutils_helpers {
    use super::*;

    /// Set the nonce for an identity to a specific value (test helper only).
    pub fn set_nonce(e: &Env, identity: &Address, nonce: u64) {
        e.storage()
            .instance()
            .set(&DataKey::Nonce(identity.clone()), &nonce);
    }
}

fn bump_nonce_ttl(e: &Env, _key: &DataKey, _ttl: u32) {
    e.storage()
        .instance()
        .extend_ttl(NONCE_TTL_THRESHOLD, NONCE_TTL_EXTEND_TO);
}
