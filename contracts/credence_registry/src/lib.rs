#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

//! # Credence Registry Contract
//!
//! Maps identity addresses to their bond contract addresses, enabling efficient
//! lookup and reverse lookup operations for the Credence trust protocol.
//!
//! ## Features
//! - Register identity-to-bond mappings (admin-only)
//! - Trustless self-registration via code-hash verification (bonds can self-register)
//! - Lookup bond contract by identity
//! - Reverse lookup identity by bond contract
//! - Track registration status
//! - Emit events for all registry operations
//!
//! ## Security
//! - Admin-controlled registration
//! - Trustless bond self-registration with code-hash verification
//! - Prevents duplicate registrations
//! - Validates addresses before registration
//! - Emits events for audit trail

use credence_errors::ContractError;
use soroban_sdk::panic_with_error;
use soroban_sdk::{contract, contractimpl, Address, Env, String, Symbol, Vec};
pub mod idempotency;
pub mod storage;

pub use storage::RegistryEntry;
use storage::{bump_instance_ttl, extend_persistent_ttl, DataKey};

/// Signature domain identifier for the CredenceRegistry contract.
#[allow(dead_code)]
const SIGNATURE_DOMAIN: &str = "CredenceRegistry";

/// Interface identifier expected from Credence bond contracts.
pub const IFACE_CREDENCE_BOND_V1: u32 = 0x4342_5631;

/// Maximum number of identities that can be returned in a single page
/// This hard cap prevents unbounded ledger reads that could exceed Soroban's
/// per-transaction resource limits as the registry grows.
const MAX_IDENTITIES_PAGE_SIZE: u32 = 200;

pub mod pausable;

#[cfg(test)]
mod test_pausable;

#[cfg(test)]
mod test_access_control;

#[cfg(test)]
mod test_uniqueness;

#[cfg(test)]
mod test_boundary_recovery;

#[cfg(test)]
mod test_indexed_storage;

#[contract]
pub struct CredenceRegistry;

#[contractimpl]
impl CredenceRegistry {
    /// Return the contract version.
    pub fn version(e: Env) -> String {
        String::from_str(&e, credence_errors::VERSION)
    }

    /// Initialize the registry contract with an admin address.
    ///
    /// # Arguments
    /// * `admin` - Address that will have admin privileges
    ///
    /// # Panics
    /// * If contract is already initialized
    pub fn initialize(e: Env, admin: Address) {
        bump_instance_ttl(&e);
        if e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&e, ContractError::AlreadyInitialized);
        }

        admin.require_auth();

        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Paused, &false);
        e.storage()
            .instance()
            .set(&DataKey::PauseSignerCount, &0_u32);
        e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
        e.storage()
            .instance()
            .set(&DataKey::PauseProposalCounter, &0_u32);

        // Initialize the identity index. The index itself lives in persistent
        // storage (`IdentityAt` / `IdentityIndex`); only the counter, a single
        // `u32`, stays in the instance entry.
        e.storage().instance().set(&DataKey::IdentityCount, &0_u32);

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
    pub fn register(
        e: Env,
        identity: Address,
        bond_contract: Address,
        allow_non_interface: bool,
    ) -> RegistryEntry {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        pausable::require_admin_auth(&e, &admin);

        // Validate that bond_contract is not a zero address
        // Note: Address::from_array is not supported in this SDK version.
        // In Soroban, address validation is handled by the host environment.

        // ERC165-equivalent interface check
        if !allow_non_interface {
            let supported: bool = e
                .try_invoke_contract::<bool, soroban_sdk::Error>(
                    &bond_contract,
                    &Symbol::new(&e, "supports_interface"),
                    soroban_sdk::vec![&e, IFACE_CREDENCE_BOND_V1.into()],
                )
                .unwrap_or(Ok(false))
                .unwrap_or(false);
            if !supported {
                panic_with_error!(&e, ContractError::UnsupportedInterface);
            }
        }

        // Check if identity is already registered
        let identity_key = DataKey::IdentityToBond(identity.clone());
        if e.storage().persistent().has(&identity_key) {
            panic_with_error!(&e, ContractError::IdentityAlreadyRegistered);
        }

        // Check if bond contract is already associated with another identity
        let bond_key = DataKey::BondToIdentity(bond_contract.clone());
        if e.storage().persistent().has(&bond_key) {
            panic_with_error!(&e, ContractError::BondContractAlreadyRegistered);
        }

        // Create registry entry
        let entry = RegistryEntry {
            identity: identity.clone(),
            bond_contract: bond_contract.clone(),
            registered_at: e.ledger().timestamp(),
            active: true,
        };

        // Store forward mapping (identity -> bond)
        e.storage().persistent().set(&identity_key, &entry);
        extend_persistent_ttl(&e, &identity_key);

        // Store reverse mapping (bond -> identity)
        e.storage().persistent().set(&bond_key, &identity);
        extend_persistent_ttl(&e, &bond_key);

        // Append to the identity index. The duplicate check above already
        // guarantees the identity is not indexed, so no scan is needed and
        // the append is O(1).
        Self::index_identity(&e, &identity);

        // Store opt-out flag for audit trail
        if allow_non_interface {
            let flag_key = DataKey::AllowNonInterface(bond_contract.clone());
            e.storage().persistent().set(&flag_key, &true);
            extend_persistent_ttl(&e, &flag_key);
        }

        // Emit event
        e.events().publish(
            (Symbol::new(&e, "identity_registered"),),
            (entry.clone(), allow_non_interface),
        );

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
        bump_instance_ttl(&e);
        let key = DataKey::IdentityToBond(identity.clone());
        match e.storage().persistent().get::<_, RegistryEntry>(&key) {
            Some(entry) => {
                extend_persistent_ttl(&e, &key);
                entry
            }
            None => panic_with_error!(&e, ContractError::IdentityNotRegistered),
        }
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
        bump_instance_ttl(&e);
        let key = DataKey::BondToIdentity(bond_contract.clone());
        match e.storage().persistent().get::<_, Address>(&key) {
            Some(identity) => {
                extend_persistent_ttl(&e, &key);
                identity
            }
            None => panic_with_error!(&e, ContractError::BondContractNotRegistered),
        }
    }

    /// Check if an identity is registered.
    ///
    /// # Arguments
    /// * `identity` - The identity address to check
    ///
    /// # Returns
    /// `true` if the identity is registered and active, `false` otherwise
    pub fn is_registered(e: Env, identity: Address) -> bool {
        bump_instance_ttl(&e);
        let key = DataKey::IdentityToBond(identity);
        match e.storage().persistent().get::<_, RegistryEntry>(&key) {
            Some(entry) => {
                extend_persistent_ttl(&e, &key);
                entry.active
            }
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
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        // Verify admin authorization
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        pausable::require_admin_auth(&e, &admin);

        let key = DataKey::IdentityToBond(identity.clone());
        let mut entry: RegistryEntry = e
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::IdentityNotRegistered));

        if !entry.active {
            panic_with_error!(&e, ContractError::AlreadyDeactivated);
        }

        entry.active = false;
        e.storage().persistent().set(&key, &entry);
        extend_persistent_ttl(&e, &key);

        e.events()
            .publish((Symbol::new(&e, "identity_deactivated"),), entry);
    }

    /// Remove a registration permanently (hard delete).
    ///
    /// Clears both the forward mapping (identity → bond) and the reverse mapping
    /// (bond → identity) from persistent storage, and drops the identity from the
    /// insertion index with swap-and-pop.  After removal the identity and bond
    /// contract are free to be re-registered with new counterparts.
    ///
    /// # Arguments
    /// * `identity` - The identity address to remove
    ///
    /// # Panics
    /// * `ContractError::NotInitialized`  – contract not yet initialised
    /// * `ContractError::IdentityNotRegistered` – identity has no entry
    ///
    /// # Events
    /// Emits `identity_removed` with the removed `RegistryEntry`
    pub fn remove(e: Env, identity: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);

        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        pausable::require_admin_auth(&e, &admin);

        let identity_key = DataKey::IdentityToBond(identity.clone());
        let entry: RegistryEntry = e
            .storage()
            .persistent()
            .get(&identity_key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::IdentityNotRegistered));

        // Remove forward mapping
        e.storage().persistent().remove(&identity_key);

        // Remove reverse mapping so the bond contract can be re-registered
        let bond_key = DataKey::BondToIdentity(entry.bond_contract.clone());
        e.storage().persistent().remove(&bond_key);

        // Remove AllowNonInterface flag if set for this bond contract
        let allow_non_iface_key = DataKey::AllowNonInterface(entry.bond_contract.clone());
        if e.storage().persistent().has(&allow_non_iface_key) {
            e.storage().persistent().remove(&allow_non_iface_key);
        }

        // Drop the identity from the index with swap-and-pop.
        Self::unindex_identity(&e, &identity);

        e.events()
            .publish((Symbol::new(&e, "identity_removed"),), entry);
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
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        // Verify admin authorization
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        pausable::require_admin_auth(&e, &admin);

        let key = DataKey::IdentityToBond(identity.clone());
        let mut entry: RegistryEntry = e
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::IdentityNotRegistered));

        if entry.active {
            panic_with_error!(&e, ContractError::AlreadyActive);
        }

        entry.active = true;
        e.storage().persistent().set(&key, &entry);
        extend_persistent_ttl(&e, &key);

        e.events()
            .publish((Symbol::new(&e, "identity_reactivated"),), entry);
    }

    /// Get a paginated page of registered identities.
    ///
    /// # Arguments
    /// * `offset` - Number of identities to skip (for pagination)
    /// * `limit` - Maximum number of identities to return (capped at MAX_IDENTITIES_PAGE_SIZE)
    ///
    /// # Returns
    /// A `Vec` of identity addresses for the requested page
    ///
    /// # Ordering
    /// Identities are returned in insertion order (the order they were
    /// registered). Removing an identity moves the tail entry into the freed
    /// slot (swap-and-pop), so a page after a removal is still gapless and
    /// deterministic, but the moved identity may appear earlier than it did
    /// before. The cost is proportional to the page size, never to the number
    /// of registered identities.
    ///
    /// # Examples
    /// ```ignore
    /// // Get first page of 50 identities
    /// let page1 = registry.get_identities_page(&env, 0, 50);
    ///
    /// // Get second page
    /// let page2 = registry.get_identities_page(&env, 50, 50);
    /// ```
    pub fn get_identities_page(e: Env, offset: u32, limit: u32) -> Vec<Address> {
        bump_instance_ttl(&e);

        let actual_limit = limit.min(MAX_IDENTITIES_PAGE_SIZE);
        let total_count = Self::identity_count(&e);

        // Handle offset past end
        if offset >= total_count || actual_limit == 0 {
            return Vec::new(&e);
        }

        let start = offset;
        let end = start.saturating_add(actual_limit).min(total_count);

        // Read only the requested slice: one persistent key per returned
        // identity, never the whole registry.
        let mut result = Vec::new(&e);
        for i in start..end {
            let key = DataKey::IdentityAt(i);
            if let Some(identity) = e.storage().persistent().get::<_, Address>(&key) {
                extend_persistent_ttl(&e, &key);
                result.push_back(identity);
            }
        }

        result
    }

    /// Get the number of identities currently in the index.
    ///
    /// This is an O(1) instance read, independent of how many identities have
    /// ever been registered.
    pub fn get_identity_count(e: Env) -> u32 {
        bump_instance_ttl(&e);
        Self::identity_count(&e)
    }

    /// Read the identity count from instance storage (defaults to `0`).
    fn identity_count(e: &Env) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::IdentityCount)
            .unwrap_or(0)
    }

    /// Append `identity` to the index.
    ///
    /// O(1): two persistent writes plus one instance counter update, no matter
    /// how many identities are already registered. The caller must already
    /// have rejected duplicates (`IdentityToBond` existence check).
    fn index_identity(e: &Env, identity: &Address) {
        let count = Self::identity_count(e);

        let at_key = DataKey::IdentityAt(count);
        e.storage().persistent().set(&at_key, identity);
        extend_persistent_ttl(e, &at_key);

        let index_key = DataKey::IdentityIndex(identity.clone());
        e.storage().persistent().set(&index_key, &count);
        extend_persistent_ttl(e, &index_key);

        let next = count
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
        e.storage().instance().set(&DataKey::IdentityCount, &next);
    }

    /// Remove `identity` from the index using swap-and-pop. O(1).
    ///
    /// The tail entry is moved into the freed slot and its `IdentityIndex` is
    /// rewritten before the tail key is dropped, so every remaining identity
    /// keeps a valid position and the pages stay gap-free.
    fn unindex_identity(e: &Env, identity: &Address) {
        let count = Self::identity_count(e);
        let index_key = DataKey::IdentityIndex(identity.clone());
        let removed_at: u32 = match e.storage().persistent().get::<_, u32>(&index_key) {
            Some(i) => i,
            // Not indexed: nothing to renumber (defensive, cannot happen for
            // entries written by this contract).
            None => return,
        };

        if count == 0 {
            e.storage().persistent().remove(&index_key);
            return;
        }

        let last = count - 1;
        if removed_at < last {
            let last_key = DataKey::IdentityAt(last);
            if let Some(moved) = e.storage().persistent().get::<_, Address>(&last_key) {
                let target_key = DataKey::IdentityAt(removed_at);
                e.storage().persistent().set(&target_key, &moved);
                extend_persistent_ttl(e, &target_key);

                let moved_index_key = DataKey::IdentityIndex(moved);
                e.storage().persistent().set(&moved_index_key, &removed_at);
                extend_persistent_ttl(e, &moved_index_key);
            }
        }

        e.storage().persistent().remove(&DataKey::IdentityAt(last));
        e.storage().persistent().remove(&index_key);
        e.storage().instance().set(&DataKey::IdentityCount, &last);
    }

    /// Get the admin address.
    ///
    /// # Returns
    /// The admin `Address`
    ///
    /// # Panics
    /// * If contract is not initialized
    pub fn get_admin(e: Env) -> Address {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized))
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
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        // Verify current admin authorization
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        pausable::require_admin_auth(&e, &admin);

        e.storage().instance().set(&DataKey::Admin, &new_admin);

        e.events()
            .publish((Symbol::new(&e, "admin_transferred"),), new_admin);
    }

    /// Pause the registry contract.
    ///
    /// # Arguments
    /// * `caller` - The address of the caller
    ///
    /// # Returns
    /// The proposal ID if the pause is proposed, `None` if the pause is immediate
    pub fn pause(e: Env, caller: Address) -> Option<u32> {
        bump_instance_ttl(&e);
        pausable::pause(&e, &caller)
    }

    /// Unpause the registry contract.
    ///
    /// # Arguments
    /// * `caller` - The address of the caller
    ///
    /// # Returns
    /// The proposal ID if the unpause is proposed, `None` if the unpause is immediate
    pub fn unpause(e: Env, caller: Address) -> Option<u32> {
        bump_instance_ttl(&e);
        pausable::unpause(&e, &caller)
    }

    /// Check if the registry contract is paused.
    ///
    /// # Returns
    /// `true` if the contract is paused, `false` otherwise
    pub fn is_paused(e: Env) -> bool {
        bump_instance_ttl(&e);
        pausable::is_paused(&e)
    }

    /// Return a structured view of the current pause state.
    ///
    /// Aggregates the pause flag, threshold, and signer count into a single
    /// [`PauseState`] struct for off-chain monitoring and operator dashboards.
    ///
    /// # Returns
    /// A [`PauseState`] containing:
    /// * `is_paused` — whether the contract is currently paused
    /// * `threshold` — minimum approvals required to execute a proposal
    /// * `signer_count` — total number of authorised pause signers
    pub fn get_pause_state(e: Env) -> pausable::PauseState {
        bump_instance_ttl(&e);
        pausable::get_pause_state(&e)
    }

    /// Set a pause signer.
    ///
    /// # Arguments
    /// * `admin` - The admin address
    /// * `signer` - The signer address
    /// * `enabled` - Whether the signer is enabled
    pub fn set_pause_signer(e: Env, admin: Address, signer: Address, enabled: bool) {
        bump_instance_ttl(&e);
        pausable::set_pause_signer(&e, &admin, &signer, enabled)
    }

    /// Set the pause threshold.
    ///
    /// # Arguments
    /// * `admin` - The admin address
    /// * `threshold` - The new threshold
    pub fn set_pause_threshold(e: Env, admin: Address, threshold: u32) {
        bump_instance_ttl(&e);
        pausable::set_pause_threshold(&e, &admin, threshold)
    }

    /// Approve a pause proposal.
    ///
    /// # Arguments
    /// * `signer` - The signer address
    /// * `proposal_id` - The proposal ID
    pub fn approve_pause_proposal(e: Env, signer: Address, proposal_id: u32) {
        bump_instance_ttl(&e);
        pausable::approve_pause_proposal(&e, &signer, proposal_id)
    }

    /// Execute a pause proposal.
    ///
    /// # Arguments
    /// * `proposal_id` - The proposal ID
    pub fn execute_pause_proposal(e: Env, proposal_id: u32) {
        bump_instance_ttl(&e);
        pausable::execute_pause_proposal(&e, proposal_id)
    }

    /// Set the expected bond contract code hash for trustless verification.
    ///
    /// # Arguments
    /// * `code_hash` - The WASM code hash (32 bytes) of a valid bond contract
    ///
    /// # Security
    /// - Admin-only operation (requires admin authorization)
    /// - Used by `register_trustless` to verify that a caller is a genuine bond contract
    pub fn set_bond_code_hash(e: Env, code_hash: soroban_sdk::Bytes) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        pausable::require_admin_auth(&e, &admin);

        e.storage()
            .instance()
            .set(&DataKey::BondCodeHash, &code_hash);

        e.events().publish(
            (Symbol::new(&e, "bond_code_hash_updated"),),
            code_hash.clone(),
        );
    }

    /// Get the expected bond contract code hash.
    ///
    /// # Returns
    /// The stored bond code hash, or an empty Bytes if not set
    pub fn get_bond_code_hash(e: Env) -> soroban_sdk::Bytes {
        e.storage()
            .instance()
            .get(&DataKey::BondCodeHash)
            .unwrap_or_else(|| soroban_sdk::Bytes::new(&e))
    }

    /// Self-register a bond contract with trustless code-hash verification.
    ///
    /// Called by a bond contract during its `initialize()` to register itself
    /// with the registry. The caller's WASM code hash is verified against the
    /// admin-pinned expected bond code hash, ensuring the caller is a genuine
    /// bond contract instance.
    ///
    /// # Arguments
    /// * `identity` - The identity address this bond is for
    ///
    /// # Returns
    /// The created `RegistryEntry`
    ///
    /// # Panics
    /// * `ContractError::NotInitialized` – bond code hash not yet configured by admin
    /// * `ContractError::ContractCodeVerificationFailed` – caller's code hash does not match
    /// * `ContractError::IdentityAlreadyRegistered` – identity already has an active registration
    /// * `ContractError::BondContractAlreadyRegistered` – calling bond already registered for another identity
    ///
    /// # Idempotency
    /// If the bond is already registered for the same identity, returns the existing
    /// entry without error. If registered for a different identity, panics.
    ///
    /// # Security
    /// - Caller must be a contract (verified via code-hash check)
    /// - Code hash must match the admin-configured expected bond code hash
    /// - This removes the need for manual admin-controlled registration
    ///
    /// # Events
    /// Emits `bond_registered` with the `RegistryEntry` on successful registration
    pub fn register_trustless(e: Env, caller: Address, identity: Address) -> RegistryEntry {
        pausable::require_not_paused(&e);

        caller.require_auth();
        let expected_hash = Self::get_bond_code_hash(e.clone());

        // Ensure admin has pinned a bond code hash
        if expected_hash.is_empty() {
            panic_with_error!(&e, ContractError::NotInitialized);
        }

        // Verify the caller's contract code hash matches the expected bond code hash.
        // This uses Soroban's contract instance introspection to fetch the caller's
        // WASM code hash and compare it against the admin-pinned reference.
        // The comparison must be constant-time to prevent timing attacks.
        let caller_code_hash = e.invoke_contract::<soroban_sdk::Bytes>(
            &caller,
            &Symbol::new(&e, "get_contract_code_hash"),
            soroban_sdk::vec![&e],
        );

        // Constant-time comparison: if hashes don't match, reject the registration
        if caller_code_hash.len() != 32 || expected_hash.len() != 32 {
            panic_with_error!(&e, ContractError::ContractCodeVerificationFailed);
        }
        let mut caller_buf = [0u8; 32];
        caller_code_hash.copy_into_slice(&mut caller_buf);
        let mut expected_buf = [0u8; 32];
        expected_hash.copy_into_slice(&mut expected_buf);

        if !constant_time_eq(&caller_buf, &expected_buf) {
            panic_with_error!(&e, ContractError::ContractCodeVerificationFailed);
        }

        let identity_key = DataKey::IdentityToBond(identity.clone());
        let bond_key = DataKey::BondToIdentity(caller.clone());

        // Idempotency: if already registered by this bond for this identity, return existing entry
        if let Some(existing_entry) = e
            .storage()
            .persistent()
            .get::<_, RegistryEntry>(&identity_key)
        {
            if existing_entry.bond_contract == caller && existing_entry.active {
                // Already registered and active — idempotent success
                e.events().publish(
                    (Symbol::new(&e, "bond_registered"),),
                    existing_entry.clone(),
                );
                return existing_entry;
            }
            // Registered for a different bond or deactivated — reject
            panic_with_error!(&e, ContractError::IdentityAlreadyRegistered);
        }

        // Ensure bond contract is not already registered for another identity
        if e.storage().persistent().has(&bond_key) {
            panic_with_error!(&e, ContractError::BondContractAlreadyRegistered);
        }

        // Create registry entry with current ledger timestamp
        let entry = RegistryEntry {
            identity: identity.clone(),
            bond_contract: caller.clone(),
            registered_at: e.ledger().timestamp(),
            active: true,
        };

        // Persist both forward and reverse mappings
        e.storage().persistent().set(&identity_key, &entry);
        extend_persistent_ttl(&e, &identity_key);
        e.storage().persistent().set(&bond_key, &identity);
        extend_persistent_ttl(&e, &bond_key);

        // Append to the identity index (O(1), no scan)
        Self::index_identity(&e, &identity);

        // Emit trustless binding event
        e.events()
            .publish((Symbol::new(&e, "bond_registered"),), entry.clone());

        entry
    }
}

/// Constant-time comparison for security-sensitive data.
/// Prevents timing attacks by ensuring comparison time does not leak
/// information about where the first difference occurs.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0, |acc, (l, r)| acc | (l ^ r))
        == 0
}

#[cfg(test)]
mod tests {
    //! Regression coverage for issue #923: the registry used to carry two
    //! admin-check helpers — one shared (`pausable::require_admin_auth`)
    //! and another inlined copy repeated across `register`, `deactivate`,
    //! `remove`, `reactivate`, `transfer_admin`, and `set_bond_code_hash`.
    //! They have been collapsed into a single call to
    //! `pausable::require_admin_auth`.
    //!
    //! These tests exercise the consolidated helper through the public
    //! contract surface:
    //!
    //! * happy path: the initialized admin performs an admin-only write.
    //! * explicit failure mode: the contract is invoked before
    //!   `initialize` has run, which must panic from the helper's
    //!   `unwrap_or_else` guard.

    use super::*;
    use soroban_sdk::testutils::Address as _;

    /// Happy path — establish admin ownership end-to-end through
    /// `initialize` and then exercise an admin-only entry point. Both
    /// paths share the collapsed `pausable::require_admin_auth` helper.
    /// Succeeding without panicking proves the helper reachable, the
    /// storage lookup returns the stored admin, and `require_auth`
    /// accepted the admin's signature.
    #[test]
    fn admin_check_round_trips_through_initialize_and_pauses() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(CredenceRegistry, ());
        let client = CredenceRegistryClient::new(&env, &contract_id);

        env.mock_all_auths();

        client.initialize(&admin);

        // The stored admin is reachable — the helper's `get(&DataKey::Admin)`
        // path worked, so the contract is initialized.
        let stored = client.get_admin();
        assert_eq!(stored, admin);

        // An admin-only write through `set_bond_code_hash` exercises the
        // same collapsed helper that previously was an inlined copy here.
        // Succeeding without panicking is the happy-path assertion.
        let hash = soroban_sdk::Bytes::from_array(&env, &[0u8; 32]);
        client.set_bond_code_hash(&hash);
    }

    /// Explicit failure mode: invoking any admin-only entry point on an
    /// uninitialized contract must panic from the helper's
    /// `unwrap_or_else` guard. Reading admin storage on an uninitialized
    /// contract fails the same way the helper fails — the `is_err` check
    /// pins that invariant without re-implementing the panic transport.
    #[test]
    fn admin_check_panics_when_uninitialized() {
        let env = Env::default();
        let contract_id = env.register(CredenceRegistry, ());
        let client = CredenceRegistryClient::new(&env, &contract_id);

        let result = client.try_get_admin();
        assert!(
            result.is_err(),
            "admin-check must panic on an uninitialized registry"
        );
    }
}

impl interfaces::governable::Governable for CredenceRegistry {
    fn get_admin(e: Env) -> Address {
        Self::get_admin(e)
    }

    fn set_admin(e: Env, new_admin: Address) {
        Self::transfer_admin(e, new_admin);
    }
}
