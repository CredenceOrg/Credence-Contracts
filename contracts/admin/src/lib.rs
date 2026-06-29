#![no_std]
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

pub mod pausable;

#[cfg(test)]
mod test_ownership_transfer;

use credence_errors::{ContractError, Role};
use soroban_sdk::panic_with_error;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Symbol, Vec};

/// Admin role hierarchy levels
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Copy)]
pub enum AdminRole {
    /// Can perform all operations including managing other admins
    SuperAdmin = 3,
    /// Can manage operators and perform most administrative tasks
    Admin = 2,
    /// Can perform limited operational tasks
    Operator = 1,
}

/// Admin role information
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminInfo {
    /// The admin address
    pub address: Address,
    /// The assigned role
    pub role: AdminRole,
    /// Timestamp when this role was assigned
    pub assigned_at: u64,
    /// Address of the admin who assigned this role
    pub assigned_by: Address,
    /// Whether this admin is currently active
    pub active: bool,
    /// Unix timestamp until which this admin is suspended (0 = not suspended).
    /// While `e.ledger().timestamp() < suspended_until` the admin is treated
    /// as inactive; the suspension expires automatically — no second transaction
    /// is required.
    pub suspended_until: u64,
}

/// Storage keys for the admin contract
#[contracttype]
#[derive(Clone)]
enum DataKey {
    /// List of all admin addresses
    AdminList,
    /// Admin information by address: Address -> AdminInfo
    AdminInfo(Address),
    /// Role-based admin lists: AdminRole -> Vec<Address>
    RoleAdmins(AdminRole),
    /// Contract initialization flag
    Initialized,
    /// Minimum number of admins required
    MinAdmins,
    /// Maximum number of admins allowed
    MaxAdmins,
    // Pause mechanism
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u64),
    PauseApproval(u64, Address),
    PauseApprovalCount(u64),
    /// Current contract owner
    Owner,
    /// Pending owner for two-step ownership transfer
    PendingOwner,
}

/// The zero/invalid address sentinel.
///
/// In Soroban the all-zero Ed25519 public key encodes to this strkey.
/// Assigning a governance role to (or transferring ownership to) this
/// address can permanently strand administration, so every privileged
/// entrypoint that accepts a target `Address` MUST reject it.
const INVALID_ADDRESS_SENTINEL: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

#[contract]
pub struct AdminContract;

#[contractimpl]
impl AdminContract {
    /// Return the contract version.
    pub fn version(e: Env) -> String {
        String::from_str(&e, credence_errors::VERSION)
    }

    /// Initialize the admin contract with a super admin.
    ///
    /// # Arguments
    /// * `super_admin` - Address that will have super admin privileges
    /// * `min_admins` - Minimum number of admins required (default: 1)
    /// * `max_admins` - Maximum number of admins allowed (default: 100)
    ///
    /// # Panics
    /// * If contract is already initialized
    /// * If min_admins is 0 or greater than max_admins
    ///
    /// # Events
    /// Emits `admin_initialized` with the super admin address
    pub fn initialize(e: Env, super_admin: Address, min_admins: u32, max_admins: u32) {
        bump_instance_ttl(&e);
        if e.storage().instance().has(&DataKey::Initialized) {
            panic_with_error!(&e, ContractError::AlreadyInitialized);
        }

        if min_admins == 0 {
            panic_with_error!(&e, ContractError::InvalidPauseAction);
        }

        if min_admins > max_admins {
            panic_with_error!(&e, ContractError::InvalidPauseAction);
        }

        super_admin.require_auth();

        // Set configuration
        e.storage().instance().set(&DataKey::Initialized, &true);
        e.storage().instance().set(&DataKey::MinAdmins, &min_admins);
        e.storage().instance().set(&DataKey::MaxAdmins, &max_admins);

        // Initialize pause state
        e.storage().instance().set(&DataKey::Paused, &false);
        e.storage()
            .instance()
            .set(&DataKey::PauseSignerCount, &0_u32);
        e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
        e.storage()
            .instance()
            .set(&DataKey::PauseProposalCounter, &0_u64);

        // Create initial super admin
        let admin_info = AdminInfo {
            address: super_admin.clone(),
            role: AdminRole::SuperAdmin,
            assigned_at: e.ledger().timestamp(),
            assigned_by: super_admin.clone(), // Self-assigned for initialization
            active: true,
            suspended_until: 0,
        };

        // Store admin info
        e.storage()
            .instance()
            .set(&DataKey::AdminInfo(super_admin.clone()), &admin_info);

        // Initialize admin list
        let mut admin_list: Vec<Address> = Vec::new(&e);
        admin_list.push_back(super_admin.clone());
        e.storage().instance().set(&DataKey::AdminList, &admin_list);

        // Initialize role-based admin list
        let super_admins = Vec::from_array(&e, [super_admin.clone()]);
        e.storage()
            .instance()
            .set(&DataKey::RoleAdmins(AdminRole::SuperAdmin), &super_admins);

        // Initialize empty lists for other roles
        e.storage().instance().set(
            &DataKey::RoleAdmins(AdminRole::Admin),
            &Vec::<Address>::new(&e),
        );
        e.storage().instance().set(
            &DataKey::RoleAdmins(AdminRole::Operator),
            &Vec::<Address>::new(&e),
        );

        // Set the initial owner as the super admin
        e.storage().instance().set(&DataKey::Owner, &super_admin);

        e.events()
            .publish((Symbol::new(&e, "admin_initialized"),), super_admin);
    }

    /// Add a new admin with the specified role.
    ///
    /// # Arguments
    /// * `caller` - Address of the caller making the assignment
    /// * `new_admin` - Address of the new admin to add
    /// * `role` - Role to assign to the new admin
    ///
    /// # Returns
    /// The created `AdminInfo`
    ///
    /// # Panics
    /// * If caller is not authorized to assign this role
    /// * If new_admin is already an admin
    /// * If maximum admin limit would be exceeded
    /// * If caller is trying to assign equal or higher role to themselves
    ///
    /// # Events
    /// Emits `admin_added` with the new admin information
    pub fn add_admin(e: Env, caller: Address, new_admin: Address, role: AdminRole) -> AdminInfo {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        caller.require_auth();

        Self::require_valid_admin_address(&e, &new_admin);

        // Verify caller authorization
        Self::require_role_at_least(&e, &caller, Self::get_required_role_to_assign(role))
            .unwrap_or_else(|_| panic_with_error!(&e, ContractError::NotAdmin));

        // Check if new admin already exists
        if e.storage()
            .instance()
            .has(&DataKey::AdminInfo(new_admin.clone()))
        {
            panic_with_error!(&e, ContractError::AlreadyActive);
        }

        // Prevent self-assignment of equal or higher role
        if caller == new_admin && Self::get_role(e.clone(), caller.clone()) >= role {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        // Check admin limit
        let current_count = Self::get_admin_count(e.clone());
        let max_admins: u32 = e
            .storage()
            .instance()
            .get(&DataKey::MaxAdmins)
            .unwrap_or(100);
        if current_count >= max_admins {
            panic_with_error!(&e, ContractError::ThresholdExceedsSigners);
        }

        // Create admin info
        let admin_info = AdminInfo {
            address: new_admin.clone(),
            role,
            assigned_at: e.ledger().timestamp(),
            assigned_by: caller.clone(),
            active: true,
            suspended_until: 0,
        };

        // Store admin info
        e.storage()
            .instance()
            .set(&DataKey::AdminInfo(new_admin.clone()), &admin_info.clone());

        // Update admin list
        let mut admin_list: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or(Vec::new(&e));
        admin_list.push_back(new_admin.clone());
        e.storage().instance().set(&DataKey::AdminList, &admin_list);

        // Update role-based admin list
        let mut role_admins: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(role))
            .unwrap_or(Vec::new(&e));
        role_admins.push_back(new_admin.clone());
        e.storage()
            .instance()
            .set(&DataKey::RoleAdmins(role), &role_admins);

        e.events()
            .publish((Symbol::new(&e, "admin_added"),), admin_info.clone());

        e.events().publish(
            (Symbol::new(&e, "ROLE_ASSIGNED"), new_admin),
            (role, caller),
        );

        admin_info
    }

    /// Remove an admin from the system.
    ///
    /// # Arguments
    /// * `caller` - Address of the caller making the removal
    /// * `admin_to_remove` - Address of the admin to remove
    ///
    /// # Panics
    /// * If caller is not authorized to remove this admin
    /// * If admin_to_remove is not an admin
    /// * If removing would violate minimum admin requirements
    /// * If admin is trying to remove themselves and they're the last admin of their role
    ///
    /// # Events
    /// Emits `admin_removed` with the removed admin information
    pub fn remove_admin(e: Env, caller: Address, admin_to_remove: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        caller.require_auth();

        Self::require_valid_admin_address(&e, &admin_to_remove);

        // Get admin info
        let admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_to_remove.clone()))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin));

        // Verify caller authorization
        let caller_role = Self::get_role(e.clone(), caller.clone());
        if caller_role <= admin_info.role {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        // Check minimum admin requirements
        let role_admins: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(admin_info.role))
            .unwrap_or(Vec::new(&e));

        let min_admins: u32 = e.storage().instance().get(&DataKey::MinAdmins).unwrap_or(1);

        // Special protection for super admins
        if admin_info.role == AdminRole::SuperAdmin && role_admins.len() <= min_admins {
            panic_with_error!(&e, ContractError::InvalidPauseAction);
        }

        // Remove from admin info storage
        e.storage()
            .instance()
            .remove(&DataKey::AdminInfo(admin_to_remove.clone()));

        // Remove from admin list
        let mut admin_list: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or(Vec::new(&e));
        let admin_index = admin_list.iter().position(|x| x == admin_to_remove);
        if let Some(index) = admin_index {
            admin_list.remove(
                index
                    .try_into()
                    .unwrap_or_else(|_| panic_with_error!(&e, ContractError::Overflow)),
            );
            e.storage().instance().set(&DataKey::AdminList, &admin_list);
        }

        // Remove from role-based admin list
        let mut role_admins: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(admin_info.role))
            .unwrap_or(Vec::new(&e));
        let role_index = role_admins.iter().position(|x| x == admin_to_remove);
        if let Some(index) = role_index {
            role_admins.remove(
                index
                    .try_into()
                    .unwrap_or_else(|_| panic_with_error!(&e, ContractError::Overflow)),
            );
            e.storage()
                .instance()
                .set(&DataKey::RoleAdmins(admin_info.role), &role_admins);
        }

        e.events()
            .publish((Symbol::new(&e, "admin_removed"),), admin_info);

        e.events().publish(
            (Symbol::new(&e, "ROLE_REVOKED"), admin_to_remove),
            (caller,),
        );
    }

    /// Update an admin's role.
    ///
    /// # Arguments
    /// * `caller` - Address of the caller making the change
    /// * `admin_address` - Address of the admin to update
    /// * `new_role` - New role to assign
    ///
    /// # Returns
    /// The updated `AdminInfo`
    ///
    /// # Panics
    /// * If caller is not authorized to change to this role
    /// * If admin_address is not an admin
    /// * If caller is trying to assign equal or higher role to themselves
    ///
    /// # Events
    /// Emits `admin_role_updated` with the updated admin information
    pub fn update_admin_role(
        e: Env,
        caller: Address,
        admin_address: Address,
        new_role: AdminRole,
    ) -> AdminInfo {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        caller.require_auth();

        Self::require_valid_admin_address(&e, &admin_address);

        // Get current admin info
        let mut admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_address.clone()))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin));

        // Verify caller authorization
        Self::require_role_at_least(&e, &caller, Self::get_required_role_to_assign(new_role))
            .unwrap_or_else(|_| panic_with_error!(&e, ContractError::NotAdmin));

        // Prevent self-assignment of equal or higher role
        if caller == admin_address && Self::get_role(e.clone(), caller.clone()) >= new_role {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        let old_role = admin_info.role;

        // Remove from old role list
        let mut old_role_admins: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(old_role))
            .unwrap_or(Vec::new(&e));
        let old_index = old_role_admins.iter().position(|x| x == admin_address);
        if let Some(index) = old_index {
            old_role_admins.remove(
                index
                    .try_into()
                    .unwrap_or_else(|_| panic_with_error!(&e, ContractError::Overflow)),
            );
            e.storage()
                .instance()
                .set(&DataKey::RoleAdmins(old_role), &old_role_admins);
        }

        // Add to new role list
        let mut new_role_admins: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(new_role))
            .unwrap_or(Vec::new(&e));
        new_role_admins.push_back(admin_address.clone());
        e.storage()
            .instance()
            .set(&DataKey::RoleAdmins(new_role), &new_role_admins);

        // Update admin info
        admin_info.role = new_role;
        admin_info.assigned_at = e.ledger().timestamp();
        admin_info.assigned_by = caller.clone();

        // Store updated admin info
        e.storage().instance().set(
            &DataKey::AdminInfo(admin_address.clone()),
            &admin_info.clone(),
        );

        e.events().publish(
            (Symbol::new(&e, "admin_role_updated"),),
            (admin_address.clone(), old_role, new_role),
        );

        e.events().publish(
            (Symbol::new(&e, "ROLE_ASSIGNED"), admin_address),
            (new_role, caller),
        );

        admin_info
    }

    /// Deactivate an admin (can be reactivated later).
    ///
    /// # Arguments
    /// * `caller` - Address of the caller making the change
    /// * `admin_address` - Address of the admin to deactivate
    ///
    /// # Panics
    /// * If caller is not authorized to deactivate this admin
    /// * If admin_address is not an admin
    /// * If admin is already deactivated
    ///
    /// # Events
    /// Emits `admin_deactivated` with the deactivated admin information
    pub fn deactivate_admin(e: Env, caller: Address, admin_address: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        caller.require_auth();

        Self::require_valid_admin_address(&e, &admin_address);

        let mut admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_address.clone()))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin));

        // Verify caller authorization: caller must strictly outrank the target.
        let caller_role = Self::get_role(e.clone(), caller.clone());
        if caller_role <= admin_info.role {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        if !admin_info.active {
            panic_with_error!(&e, ContractError::AlreadyDeactivated);
        }

        admin_info.active = false;
        e.storage().instance().set(
            &DataKey::AdminInfo(admin_address.clone()),
            &admin_info.clone(),
        );

        e.events()
            .publish((Symbol::new(&e, "admin_deactivated"),), admin_info);

        e.events()
            .publish((Symbol::new(&e, "ROLE_REVOKED"), admin_address), (caller,));
    }

    /// Reactivate a previously deactivated admin.
    ///
    /// # Arguments
    /// * `caller` - Address of the caller making the change
    /// * `admin_address` - Address of the admin to reactivate
    ///
    /// # Panics
    /// * If caller is not authorized to reactivate this admin
    /// * If admin_address is not an admin
    /// * If admin is already active
    ///
    /// # Events
    /// Emits `admin_reactivated` with the reactivated admin information
    pub fn reactivate_admin(e: Env, caller: Address, admin_address: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        caller.require_auth();

        Self::require_valid_admin_address(&e, &admin_address);

        let mut admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_address.clone()))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin));

        // Verify caller authorization
        let caller_role = Self::get_role(e.clone(), caller.clone());
        // Allow reactivation when caller has the same role as the target.
        if caller_role < admin_info.role {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        if admin_info.active {
            panic_with_error!(&e, ContractError::AlreadyActive);
        }

        admin_info.active = true;
        e.storage().instance().set(
            &DataKey::AdminInfo(admin_address.clone()),
            &admin_info.clone(),
        );

        e.events()
            .publish((Symbol::new(&e, "admin_reactivated"),), admin_info.clone());

        e.events().publish(
            (Symbol::new(&e, "ROLE_ASSIGNED"), admin_address),
            (admin_info.role, caller),
        );
    }

    /// Suspend an admin until a future ledger timestamp.
    ///
    /// While `e.ledger().timestamp() < until_ts` the admin is treated as
    /// inactive by `is_admin` and `has_role_at_least`.  Once the timestamp
    /// passes the admin is **automatically** effective again — no second
    /// transaction is needed.
    ///
    /// Suspension is distinct from `deactivate_admin`: deactivation is
    /// indefinite and requires an explicit `reactivate_admin` call, whereas
    /// suspension is self-expiring.
    ///
    /// # Arguments
    /// * `caller`    - Address authorising the suspension (must have higher or
    ///                 equal role to the target, same rules as `deactivate_admin`)
    /// * `admin`     - Address of the admin to suspend
    /// * `until_ts`  - Unix timestamp (seconds) after which the suspension
    ///                 expires; must be strictly greater than the current ledger
    ///                 timestamp
    ///
    /// # Panics
    /// * `NotAdmin`          — caller or target is not a known admin
    /// * `NotAdmin`          — caller role is strictly lower than target role
    /// * `AdminSuspended`    — `until_ts` is not in the future
    /// * `InvalidPauseAction` — suspending would drop active admins below `MinAdmins`
    /// * `AlreadyDeactivated` — target admin is permanently deactivated
    ///
    /// # Events
    /// Emits `admin_suspended` with `(admin_address, until_ts)`
    pub fn suspend_admin(e: Env, caller: Address, admin: Address, until_ts: u64) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        caller.require_auth();

        // until_ts must be in the future
        if until_ts <= e.ledger().timestamp() {
            panic_with_error!(&e, ContractError::AdminSuspended);
        }

        let mut admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin.clone()))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin));

        // Cannot suspend a permanently deactivated admin
        if !admin_info.active {
            panic_with_error!(&e, ContractError::AlreadyDeactivated);
        }

        // Caller must have a role >= target's role (same rule as deactivate_admin)
        let caller_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(caller.clone()))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin));
        if caller_info.role < admin_info.role {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        // MinAdmins guard: count currently-effective active admins
        let min_admins: u32 = e.storage().instance().get(&DataKey::MinAdmins).unwrap_or(1);
        let now = e.ledger().timestamp();
        let all_admins: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or(Vec::new(&e));
        let mut effective_active: u32 = 0;
        for addr in all_admins.iter() {
            if addr == admin {
                continue; // exclude the target — they'll be suspended
            }
            if let Some(info) = e
                .storage()
                .instance()
                .get::<_, AdminInfo>(&DataKey::AdminInfo(addr))
            {
                if info.active && now >= info.suspended_until {
                    effective_active += 1;
                }
            }
        }
        if effective_active < min_admins {
            panic_with_error!(&e, ContractError::InvalidPauseAction);
        }

        admin_info.suspended_until = until_ts;
        e.storage()
            .instance()
            .set(&DataKey::AdminInfo(admin.clone()), &admin_info);

        e.events()
            .publish((Symbol::new(&e, "admin_suspended"),), (admin, until_ts));
    }

    /// Propose a new owner for the contract (two-step ownership transfer).
    /// * `new_owner` - Address of the proposed new owner
    ///
    /// # Panics
    /// * If caller is not the current owner
    /// * If new_owner is the same as current owner
    /// * If new_owner is not a SuperAdmin
    ///
    /// # Events
    /// Emits `ownership_transfer_initiated` with current owner and pending owner
    ///
    /// # Notes
    /// The ownership remains with the current owner until the new owner calls `accept_ownership`.
    pub fn transfer_ownership(e: Env, caller: Address, new_owner: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        caller.require_auth();

        Self::require_valid_admin_address(&e, &new_owner);

        // Get current owner
        let current_owner: Address = e
            .storage()
            .instance()
            .get(&DataKey::Owner)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));

        // Verify caller is the current owner
        if caller != current_owner {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        // Verify new owner is different from current owner
        if new_owner == current_owner {
            panic_with_error!(&e, ContractError::InvalidPauseAction);
        }

        // Verify new owner is a SuperAdmin
        let new_owner_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(new_owner.clone()))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin));

        if new_owner_info.role != AdminRole::SuperAdmin {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        if !new_owner_info.active {
            panic_with_error!(&e, ContractError::AlreadyDeactivated);
        }

        // Store pending owner
        e.storage()
            .instance()
            .set(&DataKey::PendingOwner, &new_owner.clone());

        e.events().publish(
            (Symbol::new(&e, "ownership_transfer_initiated"),),
            (current_owner, new_owner),
        );
    }

    /// Accept ownership transfer (two-step acceptance).
    ///
    /// # Arguments
    /// * `caller` - Address of the pending owner accepting the transfer
    ///
    /// # Panics
    /// * If there is no pending owner
    /// * If caller is not the pending owner
    ///
    /// # Events
    /// Emits `ownership_transfer_accepted` with previous owner and new owner
    ///
    /// # Notes
    /// This function completes the two-step ownership transfer process.
    /// The caller must be the address that was previously set as pending owner.
    pub fn accept_ownership(e: Env, caller: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        caller.require_auth();

        // Get pending owner
        let pending_owner: Address = e
            .storage()
            .instance()
            .get(&DataKey::PendingOwner)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));

        // Verify caller is the pending owner
        if caller != pending_owner {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        // Get current owner for event emission
        let previous_owner: Address = e
            .storage()
            .instance()
            .get(&DataKey::Owner)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));

        // Transfer ownership
        e.storage()
            .instance()
            .set(&DataKey::Owner, &pending_owner.clone());

        // Clear pending owner
        e.storage().instance().remove(&DataKey::PendingOwner);

        // Emit admin rotated event with ledger sequence
        let ledger_seq: u32 = e.ledger().sequence();
        e.events().publish(
            (
                Symbol::new(&e, "admin_rotated"),
                previous_owner.clone(),
                pending_owner.clone(),
            ),
            ledger_seq,
        );

        // Emit original ownership transfer accepted event
        e.events().publish(
            (Symbol::new(&e, "ownership_transfer_accepted"),),
            (previous_owner.clone(), pending_owner.clone()),
        );
    }

    /// Get the current owner of the contract.
    ///
    /// # Returns
    /// The address of the current owner
    ///
    /// # Panics
    /// * If owner has not been set (contract not initialized)
    pub fn get_owner(e: Env) -> Address {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Owner)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized))
    }

    /// Get the pending owner (if any) for the current ownership transfer.
    ///
    /// # Returns
    /// `Some(address)` if there is a pending owner, `None` otherwise
    pub fn get_pending_owner(e: Env) -> Option<Address> {
        bump_instance_ttl(&e);
        e.storage().instance().get(&DataKey::PendingOwner)
    }

    /// Get information about a specific admin.
    ///
    /// # Arguments
    /// * `admin_address` - Address of the admin to query
    ///
    /// # Returns
    /// The `AdminInfo` for the specified admin
    ///
    /// # Panics
    /// * If admin_address is not an admin
    pub fn get_admin_info(e: Env, admin_address: Address) -> AdminInfo {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_address))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin))
    }

    /// Check if an address is an admin and return their role.
    ///
    /// # Arguments
    /// * `address` - Address to check
    ///
    /// # Returns
    /// The admin role if the address is an admin, panics otherwise
    pub fn get_admin_role(e: Env, address: Address) -> AdminRole {
        bump_instance_ttl(&e);
        let admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(address))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin));
        admin_info.role
    }

    /// Check if an address is an active admin.
    ///
    /// # Arguments
    /// * `address` - Address to check
    ///
    /// # Returns
    /// `Role::Admin` if the address is an active admin, `Role::User` otherwise.
    pub fn is_admin(e: Env, address: Address) -> Role {
        match e
            .storage()
            .instance()
            .get::<_, AdminInfo>(&DataKey::AdminInfo(address))
        {
            Some(admin_info) => {
                if admin_info.active && e.ledger().timestamp() >= admin_info.suspended_until {
                    Role::Admin
                } else {
                    Role::User
                }
            }
            None => Role::User,
        }
    }

    /// Check if an address has at least the specified role level.
    ///
    /// # Arguments
    /// * `address` - Address to check
    /// * `required_role` - Minimum required role
    ///
    /// # Returns
    /// `true` if the address has at least the required role, `false` otherwise.
    /// A suspended admin fails this check until `suspended_until` has passed.
    pub fn has_role_at_least(e: Env, address: Address, required_role: AdminRole) -> bool {
        bump_instance_ttl(&e);
        match e
            .storage()
            .instance()
            .get::<_, AdminInfo>(&DataKey::AdminInfo(address))
        {
            Some(admin_info) => {
                admin_info.active
                    && e.ledger().timestamp() >= admin_info.suspended_until
                    && admin_info.role >= required_role
            }
            None => false,
        }
    }

    /// Get all admin addresses.
    ///
    /// # Returns
    /// A `Vec` of all admin addresses
    pub fn get_all_admins(e: Env) -> Vec<Address> {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or(Vec::new(&e))
    }

    /// Get all admins with a specific role.
    ///
    /// # Arguments
    /// * `role` - Role to filter by
    ///
    /// # Returns
    /// A `Vec` of admin addresses with the specified role
    pub fn get_admins_by_role(e: Env, role: AdminRole) -> Vec<Address> {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::RoleAdmins(role))
            .unwrap_or(Vec::new(&e))
    }

    /// Get the total number of admins.
    ///
    /// # Returns
    /// The total count of admins
    pub fn get_admin_count(e: Env) -> u32 {
        bump_instance_ttl(&e);
        Self::get_all_admins(e).len()
    }

    /// Get the number of active admins.
    ///
    /// # Returns
    /// The count of active admins
    pub fn get_active_admin_count(e: Env) -> u32 {
        bump_instance_ttl(&e);
        let all_admins = Self::get_all_admins(e.clone());
        let mut active_count = 0;
        for admin in all_admins.iter() {
            if let Some(admin_info) = e
                .storage()
                .instance()
                .get::<_, AdminInfo>(&DataKey::AdminInfo(admin.clone()))
            {
                if admin_info.active {
                    active_count += 1;
                }
            }
        }
        active_count
    }

    /// Get contract configuration.
    ///
    /// # Returns
    /// A tuple of (min_admins, max_admins)
    pub fn get_config(e: Env) -> (u32, u32) {
        bump_instance_ttl(&e);
        let min_admins: u32 = e
            .storage()
            .instance()
            .get(&DataKey::MinAdmins)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        let max_admins: u32 = e
            .storage()
            .instance()
            .get(&DataKey::MaxAdmins)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        (min_admins, max_admins)
    }

    // Helper functions

    /// Get the role of an address (panics if not admin).
    pub fn get_role(e: Env, address: Address) -> AdminRole {
        bump_instance_ttl(&e);
        let admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(address))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotAdmin));
        admin_info.role
    }

    /// Get the minimum role required to assign a specific role.
    pub fn get_required_role_to_assign(role: AdminRole) -> AdminRole {
        match role {
            AdminRole::SuperAdmin => AdminRole::SuperAdmin,
            AdminRole::Admin => AdminRole::SuperAdmin,
            AdminRole::Operator => AdminRole::Admin,
        }
    }

    /// Require that the target address is not the zero/invalid sentinel
    /// and is not the contract's own address.
    ///
    /// # Policy
    /// An address is considered invalid when:
    ///
    /// 1. Its strkey encoding matches the all-zero Ed25519 public key
    ///    (`GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABZKY`).
    ///    This catches uninitialised/garbage strkeys.
    /// 2. It equals the contract's own address.  Assigning a governance
    ///    role to the contract itself can cause invariants to break.
    ///
    /// # Panics
    /// * `ContractError::InvalidAdminAddress` if the address fails either check.
    fn require_valid_admin_address(e: &Env, address: &Address) {
        if address.to_string() == String::from_str(e, INVALID_ADDRESS_SENTINEL) {
            panic_with_error!(e, ContractError::InvalidAdminAddress);
        }
        if address == &e.current_contract_address() {
            panic_with_error!(e, ContractError::InvalidAdminAddress);
        }
    }

    /// Require that the caller has at least the specified role.
    fn require_role_at_least(
        e: &Env,
        caller: &Address,
        required_role: AdminRole,
    ) -> Result<(), ()> {
        let admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(caller.clone()))
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotAdmin));
        if admin_info.active
            && e.ledger().timestamp() >= admin_info.suspended_until
            && admin_info.role >= required_role
        {
            Ok(())
        } else {
            Err(())
        }
    }
}

#[cfg(test)]
mod test;

// Pause mechanism entrypoints
#[contractimpl]
impl AdminContract {
    pub fn is_paused(e: Env) -> bool {
        bump_instance_ttl(&e);
        pausable::is_paused(&e)
    }

    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        bump_instance_ttl(&e);
        pausable::pause(&e, &caller)
    }

    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        bump_instance_ttl(&e);
        pausable::unpause(&e, &caller)
    }

    pub fn set_pause_signer(e: Env, admin: Address, signer: Address, enabled: bool) {
        bump_instance_ttl(&e);
        pausable::set_pause_signer(&e, &admin, &signer, enabled)
    }

    pub fn set_pause_threshold(e: Env, admin: Address, threshold: u32) {
        bump_instance_ttl(&e);
        pausable::set_pause_threshold(&e, &admin, threshold)
    }

    pub fn approve_pause_proposal(e: Env, signer: Address, proposal_id: u64) {
        bump_instance_ttl(&e);
        pausable::approve_pause_proposal(&e, &signer, proposal_id)
    }

    pub fn execute_pause_proposal(e: Env, proposal_id: u64) {
        bump_instance_ttl(&e);
        pausable::execute_pause_proposal(&e, proposal_id)
    }
}

#[cfg(test)]
mod test_pausable;

#[cfg(test)]
mod test_basic;

#[cfg(test)]
mod test_zero_address;

#[cfg(test)]
mod test_immutable_config_simple;

#[cfg(test)]
mod test_authorization;

#[cfg(test)]
mod test_suspension;

#[cfg(test)]
mod test_auth_entrypoints;
