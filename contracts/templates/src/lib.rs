//! # Credence Contract Template
//!
//! Canonical starting point for new Soroban contracts in this workspace.
//!
//! ## Patterns demonstrated
//! - `#![no_std]` + `soroban_sdk` imports
//! - `DataKey` enum for typed storage
//! - `#[contracttype]` structs for on-chain data
//! - Admin-gated initialisation (panic-on-reinit guard)
//! - Caller `require_auth()` on mutating entry points
//! - `Symbol`-keyed event emission
//! - Ledger-timestamp-based expiry check
//!
//! Copy this crate, rename the package and struct, then extend.

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

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
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct TemplateContract;

#[contractimpl]
impl TemplateContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialise the contract. Panics if already initialised.
    pub fn initialize(e: Env, admin: Address) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.events().publish((Symbol::new(&e, "initialized"),), admin);
    }

    // -----------------------------------------------------------------------
    // Mutating entry points
    // -----------------------------------------------------------------------

    /// Store or overwrite a record for `owner`. Only the admin may call this.
    pub fn set_record(e: Env, owner: Address, value: i128) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        admin.require_auth();

        let record = Record {
            value,
            updated_at: e.ledger().timestamp(),
        };
        e.storage()
            .instance()
            .set(&DataKey::Record(owner.clone()), &record);

        e.events()
            .publish((Symbol::new(&e, "record_set"), owner), value);
    }

    /// Remove the record for `owner`. Only the admin may call this.
    pub fn remove_record(e: Env, owner: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
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

    /// Return the record for `owner`, or panic if none exists.
    pub fn get_record(e: Env, owner: Address) -> Record {
        e.storage()
            .instance()
            .get(&DataKey::Record(owner))
            .expect("record not found")
    }

    /// Return `true` if a record exists for `owner`.
    pub fn has_record(e: Env, owner: Address) -> bool {
        e.storage().instance().has(&DataKey::Record(owner))
    }

    /// Return the current admin address.
    pub fn get_admin(e: Env) -> Address {
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
