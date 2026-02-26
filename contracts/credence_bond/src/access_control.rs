//! # Access Control Module
//!
//! Provides reusable access control modifiers for admin, verifier, and identity roles.
//! Supports role composition and emits access denial events for security auditing.
//!
//! ## Roles
//! - **Admin**: Full administrative privileges (contract initialization, slashing, config)
//! - **Verifier**: Can verify and validate identity claims
//! - **Identity Owner**: Can manage their own identity and bonds
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

use soroban_sdk::{Address, Env, Symbol};

/// Storage keys for access control roles
const ADMIN_KEY: &str = "admin";
const VERIFIER_PREFIX: &str = "verifier";

/// Event topics for access control
const ACCESS_DENIED_EVENT: &str = "access_denied";

/// Access control error types
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessError {
    NotAdmin,
    NotVerifier,
    NotIdentityOwner,
    NotInitialized,
}

/// @notice Require that the caller is the contract admin.
/// @param caller Address attempting to execute an admin-restricted path.
/// @dev Reads the `admin` value from instance storage.
///
/// # Panics
/// Panics with "not admin" if the caller is not the admin.
///
/// # Events
/// Emits `access_denied` event on failure with (caller, role, reason).
///
/// # Example
/// ```ignore
/// pub fn set_config(e: Env, caller: Address, new_value: u32) {
///     require_admin(&e, &caller);
///     // Admin-only logic
/// }
/// ```
pub fn require_admin(e: &Env, caller: &Address) {
    let admin_key = Symbol::new(e, ADMIN_KEY);

    match e.storage().instance().get::<Symbol, Address>(&admin_key) {
        Some(admin) => {
            if caller != &admin {
                emit_access_denied(e, caller, "admin", AccessError::NotAdmin);
                panic!("not admin");
            }
        }
        None => {
            emit_access_denied(e, caller, "admin", AccessError::NotInitialized);
            panic!("not initialized");
        }
    }
}

/// @notice Require that the caller is a registered verifier.
/// @param caller Address attempting to execute a verifier-restricted path.
/// @dev Verifier roles are stored under `(verifier, address)` tuple keys.
///
/// # Panics
/// Panics with "not verifier" if the caller is not a registered verifier.
///
/// # Events
/// Emits `access_denied` event on failure with (caller, role, reason).
///
/// # Example
/// ```ignore
/// pub fn verify_claim(e: Env, verifier: Address, identity: Address) {
///     require_verifier(&e, &verifier);
///     // Verifier-only logic
/// }
/// ```
pub fn require_verifier(e: &Env, caller: &Address) {
    let verifier_key = build_verifier_key(e, caller);

    match e
        .storage()
        .instance()
        .get::<(Symbol, Address), bool>(&verifier_key)
    {
        Some(true) => {
            // Caller is a registered verifier
        }
        _ => {
            emit_access_denied(e, caller, "verifier", AccessError::NotVerifier);
            panic!("not verifier");
        }
    }
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
/// Emits `access_denied` event on failure with (caller, role, reason).
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
}

/// @notice Require that the caller is either admin OR verifier (role composition).
/// @param caller Address attempting to execute the composed-role path.
/// @dev This allows shared workflows for admin and verifier roles.
///
/// # Panics
/// Panics with "not authorized" if the caller is neither admin nor verifier.
///
/// # Events
/// Emits `access_denied` event on failure.
///
/// # Example
/// ```ignore
/// pub fn review_claim(e: Env, caller: Address) {
///     require_admin_or_verifier(&e, &caller);
///     // Admin or verifier logic
/// }
/// ```
pub fn require_admin_or_verifier(e: &Env, caller: &Address) {
    let admin_key = Symbol::new(e, ADMIN_KEY);
    let is_admin = e
        .storage()
        .instance()
        .get::<Symbol, Address>(&admin_key)
        .map(|admin| caller == &admin)
        .unwrap_or(false);

    if is_admin {
        return;
    }

    let verifier_key = build_verifier_key(e, caller);
    let is_verifier = e
        .storage()
        .instance()
        .get::<(Symbol, Address), bool>(&verifier_key)
        .unwrap_or(false);

    if !is_verifier {
        emit_access_denied(e, caller, "admin_or_verifier", AccessError::NotVerifier);
        panic!("not authorized");
    }
}

/// @notice Add a verifier (admin only).
/// @param admin Address expected to match the configured admin.
/// @param verifier Address to grant verifier role.
///
/// # Panics
/// Panics if the caller is not admin.
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
/// # Panics
/// Panics if the caller is not admin.
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
/// `true` if the address is the admin, `false` otherwise.
pub fn is_admin(e: &Env, address: &Address) -> bool {
    let admin_key = Symbol::new(e, ADMIN_KEY);
    e.storage()
        .instance()
        .get::<Symbol, Address>(&admin_key)
        .map(|admin| address == &admin)
        .unwrap_or(false)
}

/// @notice Get the current admin address.
/// @dev Panics if admin has not been initialized.
///
/// # Returns
/// The admin address if set, or panics if not initialized.
pub fn get_admin(e: &Env) -> Address {
    let admin_key = Symbol::new(e, ADMIN_KEY);
    e.storage()
        .instance()
        .get(&admin_key)
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
