//! Replay attack prevention using per-identity nonces.
//!
//! Each identity has a nonce that must be included in state-changing calls.
//! The contract rejects replayed transactions by requiring nonce to match
//! the stored value, then incrementing it. Handles nonce overflow by wrapping.

use soroban_sdk::Env;

use crate::DataKey;

/// Returns the current nonce for an identity. Caller must use this value in the next state-changing call.
///
/// # Returns
/// Current nonce (starts at 0). After a successful state-changing call, the nonce increments.
#[must_use]
pub fn get_nonce(e: &Env, identity: &soroban_sdk::Address) -> u64 {
    e.storage()
        .instance()
        .get(&DataKey::Nonce(identity.clone()))
        .unwrap_or(0)
}

/// Checks that the provided nonce matches the current nonce for the identity, then increments.
/// Call this at the start of state-changing functions.
///
/// # Errors
/// Panics if `expected_nonce` does not match the stored nonce (replay or out-of-order).
pub fn consume_nonce(e: &Env, identity: &soroban_sdk::Address, expected_nonce: u64) {
    let current = get_nonce(e, identity);
    if current != expected_nonce {
        panic!("invalid nonce: replay or out-of-order");
    }
    let next = current.checked_add(1).expect("nonce overflow");
    e.storage()
        .instance()
        .set(&DataKey::Nonce(identity.clone()), &next);
}
