#![no_std]

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

use soroban_sdk::String;
use credence_errors::ContractError;
use soroban_sdk::panic_with_error;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};
pub mod idempotency;

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

/// Interface identifier expected from Credence bond contracts.
pub const IFACE_CREDENCE_BOND_V1: u32 = 0x4342_5631;

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
    Admin,
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u64),
    PauseApproval(u64, Address),
    PauseApprovalCount(u64),
    IdentityToBond(Address),
    BondToIdentity(Address),
    RegisteredIdentities,
    AllowNonInterface(Address),
}

/// Maximum number of identities that can be returned in a single page
/// This hard cap prevents unbounded ledger reads that could exceed Soroban's
/// per-transaction resource limits as the registry grows.
const MAX_IDENTITIES_PAGE_SIZE: u32 = 200;

pub mod pausable;

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
            .set(&DataKey::PauseProposalCounter, &0_u64);

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
    pub fn register(
        e: Env,
        identity: Address,
        bond_contract: Address,
        allow_non_interface: bool,
    ) -> RegistryEntry {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        // Verify admin authorization
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));

        admin.require_auth();

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
                panic!("bond contract does not support required interface");
            }
        }

        // Check if identity is already registered
        let identity_key = DataKey::IdentityToBond(identity.clone());
        if e.storage().instance().has(&identity_key) {
            panic_with_error!(&e, ContractError::IdentityAlreadyRegistered);
        }

        // Check if bond contract is already associated with another identity
        let bond_key = DataKey::BondToIdentity(bond_contract.clone());
        if e.storage().instance().has(&bond_key) {
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
        e.storage().instance().set(&identity_key, &entry);

        // Store reverse mapping (bond -> identity)
        e.storage().instance().set(&bond_key, &identity);

        // Add to registered identities list only if not already present.
        // Guards against duplicate entries when a deactivated identity slot
        // still exists in storage (fix for #139).
        let mut identities: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RegisteredIdentities)
            .unwrap_or_else(|| Vec::new(&e));

        if !identities.iter().any(|a| a == identity) {
            identities.push_back(identity.clone());
            e.storage()
                .instance()
                .set(&DataKey::RegisteredIdentities, &identities);
        }

        // Store opt-out flag for audit trail
        if allow_non_interface {
            e.storage()
                .instance()
                .set(&DataKey::AllowNonInterface(bond_contract.clone()), &true);
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
        e.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::IdentityNotRegistered))
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
        e.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::BondContractNotRegistered))
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
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        // Verify admin authorization
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));

        admin.require_auth();

        let key = DataKey::IdentityToBond(identity.clone());
        let mut entry: RegistryEntry = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::IdentityNotRegistered));

        if !entry.active {
            panic_with_error!(&e, ContractError::AlreadyDeactivated);
        }

        entry.active = false;
        e.storage().instance().set(&key, &entry);

        e.events()
            .publish((Symbol::new(&e, "identity_deactivated"),), entry);
    }

    /// Remove a registration permanently (hard delete).
    ///
    /// Clears both the forward mapping (identity → bond) and the reverse mapping
    /// (bond → identity) from storage, and removes the identity from the
    /// `RegisteredIdentities` list.  After removal the identity and bond contract
    /// are free to be re-registered with new counterparts.
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

        admin.require_auth();

        let identity_key = DataKey::IdentityToBond(identity.clone());
        let entry: RegistryEntry = e
            .storage()
            .instance()
            .get(&identity_key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::IdentityNotRegistered));

        // Remove forward mapping
        e.storage().instance().remove(&identity_key);

        // Remove reverse mapping so the bond contract can be re-registered
        let bond_key = DataKey::BondToIdentity(entry.bond_contract.clone());
        e.storage().instance().remove(&bond_key);

        // Remove from the identities list
        let mut identities: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RegisteredIdentities)
            .unwrap_or_else(|| Vec::new(&e));
        if let Some(pos) = identities.iter().position(|a| a == identity) {
            identities.remove(pos as u32);
            e.storage()
                .instance()
                .set(&DataKey::RegisteredIdentities, &identities);
        }

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

        admin.require_auth();

        let key = DataKey::IdentityToBond(identity.clone());
        let mut entry: RegistryEntry = e
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::IdentityNotRegistered));

        if entry.active {
            panic_with_error!(&e, ContractError::AlreadyActive);
        }

        entry.active = true;
        e.storage().instance().set(&key, &entry);

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
    /// Identities are returned in insertion order (the order they were registered).
    /// This ordering is stable and deterministic, allowing callers to paginate
    /// without gaps or duplicates.
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
        let all_identities: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RegisteredIdentities)
            .unwrap_or_else(|| Vec::new(&e));

        let actual_limit = limit.min(MAX_IDENTITIES_PAGE_SIZE);
        let total_count = all_identities.len() as u32;

        // Handle offset past end
        if offset >= total_count {
            return Vec::new(&e);
        }

        let start = offset;
        let end = (start + actual_limit).min(total_count);

        let mut result = Vec::new(&e);
        for i in start..end {
            result.push_back(all_identities.get(i as u32).unwrap());
        }

        result
    }

    /// Get all registered identities.
    ///
    /// # Deprecated
    /// This function is deprecated because it returns an unbounded list that will
    /// eventually exceed Soroban's per-transaction resource limits as the registry grows.
    ///
    /// Use `get_identities_page` instead for bounded, paginated access.
    /// For event-based discovery, listen to `identity_registered` events.
    ///
    /// # Returns
    /// A `Vec` of all registered identity addresses
    #[deprecated(note = "Use get_identities_page for bounded pagination")]
    pub fn get_all_identities(e: Env) -> Vec<Address> {
        bump_instance_ttl(&e);
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

        admin.require_auth();

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
    pub fn pause(e: Env, caller: Address) -> Option<u64> {
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
    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
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
    pub fn approve_pause_proposal(e: Env, signer: Address, proposal_id: u64) {
        bump_instance_ttl(&e);
        pausable::approve_pause_proposal(&e, &signer, proposal_id)
    }

    /// Execute a pause proposal.
    ///
    /// # Arguments
    /// * `proposal_id` - The proposal ID
    pub fn execute_pause_proposal(e: Env, proposal_id: u64) {
        bump_instance_ttl(&e);
        pausable::execute_pause_proposal(&e, proposal_id)
    }

}

