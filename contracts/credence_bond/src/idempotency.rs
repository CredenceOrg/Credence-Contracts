//! Idempotency Key Module
//!
//! Provides idempotency protection for externally-triggered admin operations
//! (e.g., slash_bond, collect_fees, emergency withdrawals) that may arrive via
//! webhook retries. The system computes a hash from (actor, operation, salt) and
//! persists it to reject duplicate submissions.
//!
//! ## Design
//! - **Hash Computation**: SHA-256 hash of (actor_address, operation_name, salt_bytes)
//! - **Storage**: Persistent storage keyed by the computed hash
//! - **TTL**: Idempotency keys are stored indefinitely to prevent replay attacks
//! - **Scope**: Applied to admin-only operations that can be triggered externally
//!
//! ## Usage
//! Call `check_and_record_idempotency` at the start of any externally-triggered
//! admin operation. If the idempotency key has been seen before, the function
//! panics with `ContractError::DuplicateIdempotencyKey`.
//!
//! ## Example
//! ```no_run
//! use credence_bond::idempotency;
//! use soroban_sdk::{Address, Env, Symbol};
//!
//! pub fn slash_bond(e: Env, admin: Address, amount: i128, idempotency_salt: Bytes) {
//!     // Check idempotency before any state changes
//!     idempotency::check_and_record(&e, &admin, Symbol::new(&e, "slash_bond"), &idempotency_salt);
//!
//!     // ... rest of the operation
//! }
//! ```

use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Address, Bytes, Env, Symbol};
use soroban_sdk::xdr::ToXdr;

use soroban_sdk::xdr::ToXdr;

use crate::DataKey;

/// Computes an idempotency key hash from (actor, operation, salt).
///
/// The hash is computed as: SHA256(actor_address || operation_name || salt_bytes)
/// This ensures that:
/// - Different actors have separate idempotency namespaces
/// - Different operations cannot replay each other's keys
/// - The salt allows callers to generate unique keys per request
///
/// # Arguments
/// * `e` - Soroban environment
/// * `actor` - Address of the actor initiating the operation
/// * `operation` - Symbol identifying the operation (e.g., "slash_bond")
/// * `salt` - Unique bytes provided by the caller for this specific request
///
/// # Returns
/// A 32-byte hash representing the unique idempotency key
#[must_use]
pub fn compute_key(e: &Env, actor: &Address, operation: &Symbol, salt: &Bytes) -> Bytes {
    // Create a byte vector containing all components by converting to XDR
    let mut hash_input = Bytes::new(e);
    
    // Use XDR serialization for deterministic byte representation
    hash_input.append(&actor.to_xdr(e));
    hash_input.append(&operation.to_xdr(e));
    hash_input.append(salt);

    // Compute SHA-256 hash and convert to Bytes
    e.crypto().sha256(&hash_input).into()
}

/// Checks if an idempotency key has been used before, and if not, records it.
///
/// This function implements the idempotency check:
/// 1. Computes the hash from (actor, operation, salt)
/// 2. Checks if the hash exists in persistent storage
/// 3. If it exists, panics with `DuplicateIdempotencyKey`
/// 4. If it doesn't exist, stores it in persistent storage
///
/// # Panics
/// - `ContractError::DuplicateIdempotencyKey` if the idempotency key has already been used
///
/// # Arguments
/// * `e` - Soroban environment
/// * `actor` - Address of the actor initiating the operation
/// * `operation` - Symbol identifying the operation (e.g., "slash_bond")
/// * `salt` - Unique bytes provided by the caller for this specific request
pub fn check_and_record(e: &Env, actor: &Address, operation: &Symbol, salt: &Bytes) {
    let key = compute_key(e, actor, operation, salt);
    let storage_key = DataKey::IdempotencyKey(key.clone());

    // Check if this idempotency key has already been used
    if e.storage().persistent().has(&storage_key) {
        panic_with_error!(e, ContractError::DuplicateIdempotencyKey);
    }

    // Record the idempotency key to prevent future duplicates
    e.storage().persistent().set(&storage_key, &true);
}

/// Checks if an idempotency key has been used before (read-only).
///
/// This is a read-only variant that doesn't record the key. Useful for
/// pre-flight checks or monitoring.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `actor` - Address of the actor initiating the operation
/// * `operation` - Symbol identifying the operation (e.g., "slash_bond")
/// * `salt` - Unique bytes provided by the caller for this specific request
///
/// # Returns
/// `true` if the idempotency key has been used, `false` otherwise
#[must_use]
pub fn is_used(e: &Env, actor: &Address, operation: &Symbol, salt: &Bytes) -> bool {
    let key = compute_key(e, actor, operation, salt);
    let storage_key = DataKey::IdempotencyKey(key);
    e.storage().persistent().has(&storage_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_compute_key_deterministic() {
        let e = Env::default();
        let actor = Address::generate(&e);
        let operation = Symbol::new(&e, "test_op");
        let salt = Bytes::from_slice(&e, b"test_salt");

        let key1 = compute_key(&e, &actor, &operation, &salt);
        let key2 = compute_key(&e, &actor, &operation, &salt);

        assert_eq!(key1, key2, "compute_key should be deterministic");
    }

    #[test]
    fn test_compute_key_unique_per_actor() {
        let e = Env::default();
        let actor1 = Address::generate(&e);
        let actor2 = Address::generate(&e);
        let operation = Symbol::new(&e, "test_op");
        let salt = Bytes::from_slice(&e, b"test_salt");

        let key1 = compute_key(&e, &actor1, &operation, &salt);
        let key2 = compute_key(&e, &actor2, &operation, &salt);

        assert_ne!(key1, key2, "compute_key should differ per actor");
    }

    #[test]
    fn test_compute_key_unique_per_operation() {
        let e = Env::default();
        let actor = Address::generate(&e);
        let op1 = Symbol::new(&e, "op1");
        let op2 = Symbol::new(&e, "op2");
        let salt = Bytes::from_slice(&e, b"test_salt");

        let key1 = compute_key(&e, &actor, &op1, &salt);
        let key2 = compute_key(&e, &actor, &op2, &salt);

        assert_ne!(key1, key2, "compute_key should differ per operation");
    }

    #[test]
    fn test_compute_key_unique_per_salt() {
        let e = Env::default();
        let actor = Address::generate(&e);
        let operation = Symbol::new(&e, "test_op");
        let salt1 = Bytes::from_slice(&e, b"salt1");
        let salt2 = Bytes::from_slice(&e, b"salt2");

        let key1 = compute_key(&e, &actor, &operation, &salt1);
        let key2 = compute_key(&e, &actor, &operation, &salt2);

        assert_ne!(key1, key2, "compute_key should differ per salt");
    }

    #[test]
    fn test_is_used_initially_false() {
        let e = Env::default();
        let actor = Address::generate(&e);
        let operation = Symbol::new(&e, "test_op");
        let salt = Bytes::from_slice(&e, b"test_salt");

        assert!(!is_used(&e, &actor, &operation, &salt));
    }

    #[test]
    fn test_check_and_record_prevents_duplicate() {
        let e = Env::default();
        let actor = Address::generate(&e);
        let operation = Symbol::new(&e, "test_op");
        let salt = Bytes::from_slice(&e, b"test_salt");

        // First call should succeed
        check_and_record(&e, &actor, &operation, &salt);

        // Second call should panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            check_and_record(&e, &actor, &operation, &salt);
        }));
        assert!(result.is_err(), "Duplicate idempotency key should panic");
    }

    #[test]
    fn test_different_keys_dont_conflict() {
        let e = Env::default();
        let actor = Address::generate(&e);
        let operation = Symbol::new(&e, "test_op");
        let salt1 = Bytes::from_slice(&e, b"salt1");
        let salt2 = Bytes::from_slice(&e, b"salt2");

        // Both should succeed
        check_and_record(&e, &actor, &operation, &salt1);
        check_and_record(&e, &actor, &operation, &salt2);
    }
}
