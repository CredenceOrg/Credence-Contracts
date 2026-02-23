#![no_std]

//! # Credence Registry Contract
//!
//! Maps identity addresses to their bond contract addresses, enabling efficient
//! lookup and reverse lookup operations for the Credence trust protocol.
//!
//! ## Features
//! - Register identity-to-bond mappings
//! - Lookup bond contract by identity
//! - Reverse lookup identity by bond contract
//! - Track registration status
//! - Emit events for all registry operations
//!
//! ## Security
//! - Admin-controlled registration
//! - Prevents duplicate registrations
//! - Validates addresses before registration
//! - Emits events for audit trail

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

/// Represents a registry entry mapping an identity to their bond contract
#[contracttype]
#[derive(Clone, Debug)]
pub struct RegistryEntry {
    /// The identity address
    pub identity: Address,
    /// The bond contract address for this identity
    pub bond_contract: Address,
    /// Timestamp when this entry was registered
    pub registered_at: u64,
    /// Whether this registration is currently active
    pub active: bool,
}

/// Storage keys for the registry contract
#[contracttype]
#[derive(Clone)]
enum DataKey {
    /// Admin address
    Admin,
    /// Identity to bond contract mapping: Identity -> RegistryEntry
    IdentityToBond(Address),
    /// Reverse mapping: BondContract -> Identity
    BondToIdentity(Address),
    /// List of all registered identities
    RegisteredIdentities,
}

#[contract]
pub struct CredenceRegistry;

#[contractimpl]
impl CredenceRegistry {
    /// Initialize the registry contract with an admin address.
    ///
    /// # Arguments
    /// * `admin` - Address that will have admin privileges
    ///
    /// # Panics
    /// * If contract is already initialized
    pub fn initialize(e: Env, admin: Address) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }

        admin.require_auth();

        e.storage().instance().set(&DataKey::Admin, &admin);

        // Initialize empty registered identities list
        let identities: Vec<Address> = Vec::new(&e);
        e.storage()
            .instance()
            .set(&DataKey::RegisteredIdentities, &identities);

        e.events()
            .publish((Symbol::new(&e, "registry_initialized"),), admin.clone());
    }

    /// Register a new identity-to-bond mapping.
    ///
    /// # Arguments
    /// * `identity` - The identity address to register
    /// * `bond_contract` - The bond contract address for this identity
    ///
    /// # Returns
    /// The created `RegistryEntry`
    ///
    /// # Panics
    /// * If caller is not admin
    /// * If identity is already registered
    /// * If bond contract is already associated with another identity
    ///
    /// # Events
    /// Emits `identity_registered` with the `RegistryEntry`
    pub fn register(e: Env, identity: Address, bond_contract: Address) -> RegistryEntry {
        // Verify admin authorization
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));

        admin.require_auth();

        // Check if identity is already registered
        let identity_key = DataKey::IdentityToBond(identity.clone());
        if e.storage().instance().has(&identity_key) {
            panic!("identity already registered");
        }

        // Check if bond contract is already associated with another identity
        let bond_key = DataKey::BondToIdentity(bond_contract.clone());
        if e.storage().instance().has(&bond_key) {
            panic!("bond contract already registered");
        }

        // Create registry entry
        let entry = RegistryEntry {
            identity: identity.clone(),
            bond_contract: bond_contract.clone(),
            registered_at: e.ledger().timestamp(),
            active: true,
        };

        // Store forward mapping (identity -> bond)
        e.storage().instance().set(&identity_key, &entry);

        // Store reverse mapping (bond -> identity)
        e.storage().instance().set(&bond_key, &identity);

        // Add to registered identities list
        let mut identities: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RegisteredIdentities)
            .unwrap_or_else(|| Vec::new(&e));

        identities.push_back(identity.clone());
        e.storage()
            .instance()
            .set(&DataKey::RegisteredIdentities, &identities);

        // Emit event
        e.events()
            .publish((Symbol::new(&e, "identity_registered"),), entry.clone());

        entry
    }

    /// Lookup the bond contract address for a given identity.
    ///
    /// # Arguments
    /// * `identity` - The identity address to lookup
    ///
    /// # Returns
    /// The `RegistryEntry` for this identity
    ///
    /// # Panics
    /// * If identity is not registered
    pub fn get_bond_contract(e: Env, identity: Address) -> RegistryEntry {
        let key = DataKey::IdentityToBond(identity.clone());
        e.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("identity not registered"))
    }

    /// Reverse lookup: get the identity for a given bond contract.
    ///
    /// # Arguments
    /// * `bond_contract` - The bond contract address to lookup
    ///
    /// # Returns
    /// The identity `Address` associated with this bond contract
    ///
    /// # Panics
    /// * If bond contract is not registered
    pub fn get_identity(e: Env, bond_contract: Address) -> Address {
        let key = DataKey::BondToIdentity(bond_contract.clone());
        e.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("bond contract not registered"))
    }

    /// Check if an identity is registered.
    ///
    /// # Arguments
    /// * `identity` - The identity address to check
    ///
    /// # Returns
    /// `true` if the identity is registered and active, `false` otherwise
    pub fn is_registered(e: Env, identity: Address) -> bool {
        let key = DataKey::IdentityToBond(identity);
        match e.storage().instance().get::<_, RegistryEntry>(&key) {
            Some(entry) => entry.active,
            None => false,
        }
    }

    /// Deactivate a registration (soft delete).
    ///
    /// # Arguments
    /// * `identity` - The identity address to deactivate
    ///
    /// # Panics
    /// * If caller is not admin
    /// * If identity is not registered
    /// * If identity is already deactivated
    ///
    /// # Events
    /// Emits `identity_deactivated` with the updated `RegistryEntry`
    pub fn deactivate(e: Env, identity: Address) {
        // Verify admin authorization
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));

        admin.require_auth();

        let key = DataKey::IdentityToBond(identity.clone());
        let mut entry: RegistryEntry = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("identity not registered"));

        if !entry.active {
            panic!("already deactivated");
        }

        entry.active = false;
        e.storage().instance().set(&key, &entry);

        e.events()
            .publish((Symbol::new(&e, "identity_deactivated"),), entry);
    }

    /// Reactivate a previously deactivated registration.
    ///
    /// # Arguments
    /// * `identity` - The identity address to reactivate
    ///
    /// # Panics
    /// * If caller is not admin
    /// * If identity is not registered
    /// * If identity is already active
    ///
    /// # Events
    /// Emits `identity_reactivated` with the updated `RegistryEntry`
    pub fn reactivate(e: Env, identity: Address) {
        // Verify admin authorization
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));

        admin.require_auth();

        let key = DataKey::IdentityToBond(identity.clone());
        let mut entry: RegistryEntry = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic!("identity not registered"));

        if entry.active {
            panic!("already active");
        }

        entry.active = true;
        e.storage().instance().set(&key, &entry);

        e.events()
            .publish((Symbol::new(&e, "identity_reactivated"),), entry);
    }

    /// Get all registered identities.
    ///
    /// # Returns
    /// A `Vec` of all registered identity addresses
    pub fn get_all_identities(e: Env) -> Vec<Address> {
        e.storage()
            .instance()
            .get(&DataKey::RegisteredIdentities)
            .unwrap_or_else(|| Vec::new(&e))
    }

    /// Get the admin address.
    ///
    /// # Returns
    /// The admin `Address`
    ///
    /// # Panics
    /// * If contract is not initialized
    pub fn get_admin(e: Env) -> Address {
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"))
    }

    /// Transfer admin rights to a new address.
    ///
    /// # Arguments
    /// * `new_admin` - The new admin address
    ///
    /// # Panics
    /// * If caller is not current admin
    ///
    /// # Events
    /// Emits `admin_transferred` with the new admin address
    pub fn transfer_admin(e: Env, new_admin: Address) {
        // Verify current admin authorization
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));

        admin.require_auth();

        e.storage().instance().set(&DataKey::Admin, &new_admin);

        e.events()
            .publish((Symbol::new(&e, "admin_transferred"),), new_admin);
    }
}

#[cfg(test)]
mod test;
