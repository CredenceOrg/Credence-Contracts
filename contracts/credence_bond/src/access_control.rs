//! # Access Control Module
//!
//! Provides reusable access control guards for admin, verifier, and identity roles.
//! Supports role composition and emits access denial events for security auditing.
//!
//! ## Roles
//! - **Admin**: Full administrative privileges (contract initialization, slashing, config)
//! - **Verifier**: Can verify and validate identity claims
//! - **Identity Owner**: Can manage their own identity and bonds
//!
//! ## Storage layout
//! The admin is read from [`DataKey::Admin`], the same key [`CredenceBond::initialize`]
//! writes. Earlier revisions of this module read a bare `Symbol("admin")` key,
//! which nothing in `credence_bond` ever wrote, so every guard here failed with
//! `ContractError::NotInitialized` against a correctly initialized contract. The
//! module was unreachable dead code until it was wired into the module tree.
//!
//! Verifier grants are module-local and live under a
//! `(Symbol("verifier"), Address)` tuple key in instance storage. They are not
//! part of [`DataKey`], so verifier grants do not survive a protocol migration
//! that re-keys the contract, and the grant set is unbounded within the
//! instance-storage entry.
//!
//! ## Authentication vs. authorization
//! Every `require_*` guard in this module performs **both** checks: it
//! authenticates the address via `Address::require_auth` and then authorizes it
//! against storage. `require_admin` has always done this; `require_verifier`,
//! `require_identity_owner`, and `require_admin_or_verifier` previously only
//! authorized, which meant a contract that forwarded a caller-supplied address
//! into them (exactly what the doc examples did) would accept an unverified
//! claim. Signatures do not change, but these three now require a signature.
//!
//! ## `access_denied` events are not observable on chain
//!
//! Every failure path here publishes `access_denied` and then panics. A failing
//! Soroban transaction reverts the entire frame, so the event it just published
//! is discarded along with the state change that triggered it. Off-chain
//! alerting on `access_denied` will therefore **not** fire for any of these
//! guards: the only durable signal is the transaction failure and its error
//! code. The events are still published, because the payload (caller, role,
//! numeric reason) is the right shape for a future non-reverting audit path,
//! and `access_control_boundaries` asserts the payload so it stays correct if
//! that path is ever added.
//!
//! Note that the in-memory test host behaves differently from the ledger here:
//! under `catch_unwind` the frame is not reverted and the event stays visible.
//! Tests can therefore assert the payload, but they cannot assert the on-chain
//! rollback, and this module does not claim the event is useful for monitoring
//! today.
//!
//! ## Usage
//! ```ignore
//! use access_control::{require_admin, require_verifier, require_identity_owner};
//!
//! pub fn admin_function(e: Env, caller: Address) {
//!     require_admin(&e, &caller);
//!     // Admin-only logic here
//! }
//! ```

use crate::DataKey;
use credence_errors::{ContractError, Role};
use soroban_sdk::{panic_with_error, Address, Env, Symbol};

/// Storage keys for access control roles
const VERIFIER_PREFIX: &str = "verifier";

/// Event topics for access control
const ACCESS_DENIED_EVENT: &str = "access_denied";

/// Access control error types
///
/// Encoded into the `access_denied` event as a stable `u32` reason code.
/// `NotInitialized` is currently unconstructed: the uninitialized path is
/// handled by the canonical `require_admin!` guard, which does not publish
/// `access_denied`. The variant is retained so the code-to-reason mapping stays
/// wire-stable and so a future non-reverting audit path can use it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessError {
    NotAdmin,
    NotVerifier,
    NotIdentityOwner,
    NotInitialized,
}

/// @notice Require that the caller is the contract admin.
/// @param caller Address attempting to execute an admin-restricted path.
/// @dev Reads the admin from [`DataKey::Admin`], the key `initialize` writes, and
///      requires a signature from `caller`.
///
/// # Panics
/// Panics with `ContractError::NotInitialized` when no admin is configured and
/// `ContractError::NotAdmin` when `caller` is not the configured admin.
///
/// # Events
/// None. The canonical `require_admin!` guard panics directly and does not
/// publish `access_denied`; the other three guards in this module do.
///
/// # Example
/// ```ignore
/// pub fn set_config(e: Env, caller: Address, new_value: u32) {
///     require_admin(&e, &caller);
///     // Admin-only logic
/// }
/// ```
pub fn require_admin(e: &Env, caller: &Address) {
    credence_errors::require_admin!(e, caller, DataKey::Admin);
}

/// @notice Require that the caller is a registered verifier.
/// @param caller Address attempting to execute a verifier-restricted path.
/// @dev Verifier roles are stored under `(verifier, address)` tuple keys. Requires
///      a signature from `caller` in addition to the storage check.
///
/// # Panics
/// Panics with `ContractError::RoleRequired` if the caller is not a registered verifier.
///
/// # Events
/// Publishes `access_denied` before panicking; a failing transaction reverts it
/// away. See the module-level "access_denied events are not observable on chain".
///
/// # Example
/// ```ignore
/// pub fn verify_claim(e: Env, verifier: Address, identity: Address) {
///     require_verifier(&e, &verifier);
///     // Verifier-only logic
/// }
/// ```
pub fn require_verifier(e: &Env, caller: &Address) {
    if !is_verifier(e, caller) {
        emit_access_denied(e, caller, "verifier", AccessError::NotVerifier);
        panic_with_error!(e, ContractError::RoleRequired);
    }

    caller.require_auth();
}

/// @notice Require that the caller is the identity owner.
/// @param caller Address attempting to access identity-owned state.
/// @param expected_identity Address that owns the state being accessed.
/// @dev This check is a direct address equality comparison.
///
/// # Panics
/// Panics with "not identity owner" if the caller does not match the expected identity.
///
/// # Events
/// Publishes `access_denied` before panicking; a failing transaction reverts it
/// away. See the module-level "access_denied events are not observable on chain".
///
/// # Example
/// ```ignore
/// pub fn withdraw(e: Env, caller: Address, bond: &IdentityBond) {
///     require_identity_owner(&e, &caller, &bond.identity);
///     // Identity owner logic
/// }
/// ```
pub fn require_identity_owner(e: &Env, caller: &Address, expected_identity: &Address) {
    if caller != expected_identity {
        emit_access_denied(e, caller, "identity_owner", AccessError::NotIdentityOwner);
        panic!("not identity owner");
    }

    caller.require_auth();
}

/// @notice Require that the caller is either admin OR verifier (role composition).
/// @param caller Address attempting to execute the composed-role path.
/// @dev This allows shared workflows for admin and verifier roles.
///
/// # Panics
/// Panics with `ContractError::NotAdmin` if the caller is neither admin nor verifier.
///
/// # Events
/// Publishes `access_denied` before panicking; a failing transaction reverts it
/// away. See the module-level "access_denied events are not observable on chain".
///
/// # Example
/// ```ignore
/// pub fn review_claim(e: Env, caller: Address) {
///     require_admin_or_verifier(&e, &caller);
///     // Admin or verifier logic
/// }
/// ```
pub fn require_admin_or_verifier(e: &Env, caller: &Address) {
    if is_admin(e, caller) == Role::Admin {
        caller.require_auth();
        return;
    }

    if is_verifier(e, caller) {
        caller.require_auth();
        return;
    }

    emit_access_denied(e, caller, "admin_or_verifier", AccessError::NotAdmin);
    panic_with_error!(e, ContractError::NotAdmin);
}

/// @notice Add a verifier (admin only).
/// @param admin Address expected to match the configured admin.
/// @param verifier Address to grant verifier role.
///
/// # Idempotency
/// Granting a role that is already held succeeds and republishes
/// `verifier_added`. Storage is idempotent; the event stream is not, so an
/// indexer must treat repeated `verifier_added` for the same address as a
/// no-op rather than a second grant.
///
/// # Panics
/// Panics with `ContractError::NotInitialized` / `ContractError::NotAdmin` when
/// `admin` is not the configured admin.
///
/// # Example
/// ```ignore
/// pub fn add_verifier(e: Env, admin: Address, verifier: Address) {
///     add_verifier_role(&e, &admin, &verifier);
/// }
/// ```
pub fn add_verifier_role(e: &Env, admin: &Address, verifier: &Address) {
    require_admin(e, admin);

    let verifier_key = build_verifier_key(e, verifier);
    e.storage().instance().set(&verifier_key, &true);

    e.events()
        .publish((Symbol::new(e, "verifier_added"),), (verifier.clone(),));
}

/// @notice Remove a verifier (admin only).
/// @param admin Address expected to match the configured admin.
/// @param verifier Address to revoke verifier role.
///
/// # Idempotency
/// Revoking a role that is not held succeeds and publishes `verifier_removed`
/// anyway; there is no existence check. Storage already holds `false`, so the
/// call is a no-op, but an indexer cannot distinguish "revoked a live grant"
/// from "revoked a role that was never granted".
///
/// # Panics
/// Panics with `ContractError::NotInitialized` / `ContractError::NotAdmin` when
/// `admin` is not the configured admin.
///
/// # Example
/// ```ignore
/// pub fn remove_verifier(e: Env, admin: Address, verifier: Address) {
///     remove_verifier_role(&e, &admin, &verifier);
/// }
/// ```
pub fn remove_verifier_role(e: &Env, admin: &Address, verifier: &Address) {
    require_admin(e, admin);

    let verifier_key = build_verifier_key(e, verifier);
    e.storage().instance().set(&verifier_key, &false);

    e.events()
        .publish((Symbol::new(e, "verifier_removed"),), (verifier.clone(),));
}

/// @notice Check if an address is a verifier (read-only, no panic).
/// @param address Address to check.
///
/// # Returns
/// `true` if the address is a registered verifier, `false` otherwise.
pub fn is_verifier(e: &Env, address: &Address) -> bool {
    let verifier_key = build_verifier_key(e, address);
    e.storage()
        .instance()
        .get::<(Symbol, Address), bool>(&verifier_key)
        .unwrap_or(false)
}

/// @notice Check if an address is the admin (read-only, no panic).
/// @param address Address to check.
///
/// # Returns
/// `Role::Admin` if the address is the admin, `Role::User` otherwise.
pub fn is_admin(e: &Env, address: &Address) -> Role {
    e.storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::Admin)
        .map(|admin| {
            if address == &admin {
                Role::Admin
            } else {
                Role::User
            }
        })
        .unwrap_or(Role::User)
}

/// @notice Get the current admin address.
/// @dev Reads [`DataKey::Admin`], the key `initialize` writes.
///
/// # Panics
/// Panics with "not initialized" if no admin has been configured.
///
/// # Returns
/// The admin address.
pub fn get_admin(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic!("not initialized"))
}

// Internal helper functions

/// Build a storage key for a verifier address.
fn build_verifier_key(e: &Env, verifier: &Address) -> (Symbol, Address) {
    // Use a tuple key with prefix and address for unique verifier storage
    (Symbol::new(e, VERIFIER_PREFIX), verifier.clone())
}

/// Emit an access denied event for audit logging.
fn emit_access_denied(e: &Env, caller: &Address, role: &str, error: AccessError) {
    let error_code = match error {
        AccessError::NotAdmin => 1u32,
        AccessError::NotVerifier => 2u32,
        AccessError::NotIdentityOwner => 3u32,
        AccessError::NotInitialized => 4u32,
    };

    e.events().publish(
        (Symbol::new(e, ACCESS_DENIED_EVENT),),
        (caller.clone(), Symbol::new(e, role), error_code),
    );
}
