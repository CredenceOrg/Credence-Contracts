#![allow(
    deprecated,
    unused_imports,
    unused_variables,
    dead_code,
    unused_assignments,
    unused_mut,
    mismatched_lifetime_syntaxes,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::restriction
)]
//! # Credence Contract Template
//!
//! Canonical starting point for new Soroban contracts in this workspace.
//!
//! ## Patterns demonstrated
//! - `#![no_std]` + `#![deny(clippy::float_arithmetic)]` + `soroban_sdk` imports
//! - `DataKey` enum for typed storage
//! - `#[contracttype]` structs for on-chain data
//! - Authenticated initialisation (typed already-initialised guard)
//! - Caller `require_auth()` on mutating entry points
//! - `Symbol`-keyed event emission
//! - Ledger-timestamp-based expiry check
//!
//! Copy this crate, rename the package and struct, then extend.

#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use credence_errors::ContractError;
use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, Env, Symbol};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
pub enum DataKey {
    /// The contract administrator.
    Admin,
    /// A named record stored by the admin.
    Record(Address),
}

// ---------------------------------------------------------------------------
// On-chain types
// ---------------------------------------------------------------------------

/// A simple record stored per identity.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    /// Arbitrary numeric value set by the admin.
    pub value: i128,
    /// Ledger timestamp when the record was last updated.
    pub updated_at: u64,
    /// Ledger timestamp when the record expires (0 = never expires).
    pub expires_at: u64,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct TemplateContract;

fn read_admin(e: &Env) -> Address {
    match e.storage().instance().get(&DataKey::Admin) {
        Some(admin) => admin,
        None => panic_with_error!(e, ContractError::NotInitialized),
    }
}

#[contractimpl]
impl TemplateContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialise the contract. Requires the proposed admin's authorization
    /// and returns `AlreadyInitialized` if called more than once.
    pub fn initialize(e: Env, admin: Address) {
        credence_errors::require_contract_uninitialized(
            &e,
            e.storage().instance().has(&DataKey::Admin),
        );
        admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.events().publish((Symbol::new(&e, "initialized"),), admin);
    }

    // -----------------------------------------------------------------------
    // Mutating entry points
    // -----------------------------------------------------------------------

    /// Store or overwrite a record for `owner`. Only the admin may call this.
    pub fn set_record(e: Env, owner: Address, value: i128, expires_at: u64) {
        let admin = read_admin(&e);
        admin.require_auth();

        if expires_at != 0 && expires_at <= e.ledger().timestamp() {
            panic_with_error!(&e, ContractError::ExpiryInPast);
        }

        let record = Record {
            value,
            updated_at: e.ledger().timestamp(),
            expires_at,
        };
        e.storage()
            .instance()
            .set(&DataKey::Record(owner.clone()), &record);

        e.events()
            .publish((Symbol::new(&e, "record_set"), owner), value);
    }

    /// Remove the record for `owner`. Only the admin may call this.
    pub fn remove_record(e: Env, owner: Address) {
        let admin = read_admin(&e);
        admin.require_auth();

        e.storage()
            .instance()
            .remove(&DataKey::Record(owner.clone()));

        e.events()
            .publish((Symbol::new(&e, "record_removed"), owner), ());
    }

    // -----------------------------------------------------------------------
    // Read-only entry points
    // -----------------------------------------------------------------------

    /// Return the record for `owner`; reads never mutate storage.
    pub fn get_record(e: Env, owner: Address) -> Record {
        let record: Record = match e.storage().instance().get(&DataKey::Record(owner)) {
            Some(record) => record,
            None => panic_with_error!(&e, ContractError::BondNotFound),
        };

        if credence_errors::is_expired(&e, record.expires_at) {
            panic_with_error!(&e, ContractError::SignatureExpired);
        }

        record
    }

    /// Return `true` if a record exists for `owner` and is not expired.
    /// This read-only view does not purge expired records.
    pub fn has_record(e: Env, owner: Address) -> bool {
        if let Some(record) = e
            .storage()
            .instance()
            .get::<_, Record>(&DataKey::Record(owner))
        {
            return !credence_errors::is_expired(&e, record.expires_at);
        }
        false
    }

    /// Remove an expired record. This cleanup is permissionless because it can
    /// only delete data that is already expired; it must be called separately
    /// because storage writes are rolled back when an invocation panics.
    pub fn purge_expired(e: Env, owner: Address) {
        let record: Record = match e.storage().instance().get(&DataKey::Record(owner.clone())) {
            Some(record) => record,
            None => panic_with_error!(&e, ContractError::BondNotFound),
        };

        if !credence_errors::is_expired(&e, record.expires_at) {
            panic_with_error!(&e, ContractError::LockupNotExpired);
        }

        e.storage()
            .instance()
            .remove(&DataKey::Record(owner.clone()));
        e.events()
            .publish((Symbol::new(&e, "record_purged"), owner), ());
    }

    /// Return `true` if a record exists for `owner` and is currently expired.
    /// Read-only expiry view; it never removes the record.
    pub fn is_expired(e: Env, owner: Address) -> bool {
        if let Some(record) = e
            .storage()
            .instance()
            .get::<_, Record>(&DataKey::Record(owner))
        {
            return credence_errors::is_expired(&e, record.expires_at);
        }
        false
    }

    pub fn transfer_admin(e: Env, new_admin: Address) {
        let current_admin = read_admin(&e);
        current_admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &new_admin);
        e.events().publish(
            (Symbol::new(&e, "admin_transferred"),),
            (current_admin, new_admin),
        );
    }

    /// Return the current administrator through the contract's single export.
    pub fn get_admin(e: Env) -> Address {
        read_admin(&e)
    }

    /// Transfer administrative control through the Governable endpoint.
    pub fn set_admin(e: Env, new_admin: Address) {
        Self::transfer_admin(e, new_admin);
    }
}

impl interfaces::governable::Governable for TemplateContract {
    fn get_admin(e: Env) -> Address {
        Self::get_admin(e)
    }

    fn set_admin(e: Env, new_admin: Address) {
        Self::transfer_admin(e, new_admin);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
