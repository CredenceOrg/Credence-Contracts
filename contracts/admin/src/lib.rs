#![no_std]

pub mod pausable;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

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
}

#[contract]
pub struct AdminContract;

#[contractimpl]
impl AdminContract {
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
        if e.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }

        if min_admins == 0 {
            panic!("min_admins cannot be zero");
        }

        if min_admins > max_admins {
            panic!("min_admins cannot be greater than max_admins");
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
        pausable::require_not_paused(&e);
        caller.require_auth();

        // Verify caller authorization
        Self::require_role_at_least(&e, &caller, Self::get_required_role_to_assign(role))
            .unwrap_or_else(|_| panic!("insufficient privileges"));

        // Check if new admin already exists
        if e.storage()
            .instance()
            .has(&DataKey::AdminInfo(new_admin.clone()))
        {
            panic!("address is already an admin");
        }

        // Prevent self-assignment of equal or higher role
        if caller == new_admin && Self::get_role(e.clone(), caller.clone()) >= role {
            panic!("cannot assign equal or higher role to self");
        }

        // Check admin limit
        let current_count = Self::get_admin_count(e.clone());
        let max_admins: u32 = e
            .storage()
            .instance()
            .get(&DataKey::MaxAdmins)
            .unwrap_or(100);
        if current_count >= max_admins {
            panic!("maximum admin limit reached");
        }

        // Create admin info
        let admin_info = AdminInfo {
            address: new_admin.clone(),
            role,
            assigned_at: e.ledger().timestamp(),
            assigned_by: caller.clone(),
            active: true,
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
        pausable::require_not_paused(&e);
        caller.require_auth();

        // Get admin info
        let admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_to_remove.clone()))
            .unwrap_or_else(|| panic!("admin not found"));

        // Verify caller authorization
        let caller_role = Self::get_role(e.clone(), caller.clone());
        if caller_role <= admin_info.role {
            panic!("insufficient privileges to remove admin");
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
            panic!("cannot remove last super admin");
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
            admin_list.remove(index.try_into().unwrap());
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
            role_admins.remove(index.try_into().unwrap());
            e.storage()
                .instance()
                .set(&DataKey::RoleAdmins(admin_info.role), &role_admins);
        }

        e.events()
            .publish((Symbol::new(&e, "admin_removed"),), admin_info);
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
        pausable::require_not_paused(&e);
        caller.require_auth();

        // Get current admin info
        let mut admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_address.clone()))
            .unwrap_or_else(|| panic!("admin not found"));

        // Verify caller authorization
        Self::require_role_at_least(&e, &caller, Self::get_required_role_to_assign(new_role))
            .unwrap_or_else(|_| panic!("insufficient privileges"));

        // Prevent self-assignment of equal or higher role
        if caller == admin_address && Self::get_role(e.clone(), caller.clone()) >= new_role {
            panic!("cannot assign equal or higher role to self");
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
            old_role_admins.remove(index.try_into().unwrap());
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
            (admin_address, old_role, new_role),
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
        pausable::require_not_paused(&e);
        caller.require_auth();

        let mut admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_address.clone()))
            .unwrap_or_else(|| panic!("admin not found"));

        // Verify caller authorization
        let caller_role = Self::get_role(e.clone(), caller.clone());
        if caller_role <= admin_info.role {
            panic!("insufficient privileges to deactivate admin");
        }

        if !admin_info.active {
            panic!("admin already deactivated");
        }

        admin_info.active = false;
        e.storage().instance().set(
            &DataKey::AdminInfo(admin_address.clone()),
            &admin_info.clone(),
        );

        e.events()
            .publish((Symbol::new(&e, "admin_deactivated"),), admin_info);
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
        pausable::require_not_paused(&e);
        caller.require_auth();

        let mut admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_address.clone()))
            .unwrap_or_else(|| panic!("admin not found"));

        // Verify caller authorization
        let caller_role = Self::get_role(e.clone(), caller.clone());
        if caller_role <= admin_info.role {
            panic!("insufficient privileges to reactivate admin");
        }

        if admin_info.active {
            panic!("admin already active");
        }

        admin_info.active = true;
        e.storage().instance().set(
            &DataKey::AdminInfo(admin_address.clone()),
            &admin_info.clone(),
        );

        e.events()
            .publish((Symbol::new(&e, "admin_reactivated"),), admin_info);
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
        e.storage()
            .instance()
            .get(&DataKey::AdminInfo(admin_address))
            .unwrap_or_else(|| panic!("admin not found"))
    }

    /// Check if an address is an admin and return their role.
    ///
    /// # Arguments
    /// * `address` - Address to check
    ///
    /// # Returns
    /// The admin role if the address is an admin, panics otherwise
    pub fn get_admin_role(e: Env, address: Address) -> AdminRole {
        let admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(address))
            .unwrap_or_else(|| panic!("address is not an admin"));
        admin_info.role
    }

    /// Check if an address is an active admin.
    ///
    /// # Arguments
    /// * `address` - Address to check
    ///
    /// # Returns
    /// `true` if the address is an active admin, `false` otherwise
    pub fn is_admin(e: Env, address: Address) -> bool {
        match e
            .storage()
            .instance()
            .get::<_, AdminInfo>(&DataKey::AdminInfo(address))
        {
            Some(admin_info) => admin_info.active,
            None => false,
        }
    }

    /// Check if an address has at least the specified role level.
    ///
    /// # Arguments
    /// * `address` - Address to check
    /// * `required_role` - Minimum required role
    ///
    /// # Returns
    /// `true` if the address has at least the required role, `false` otherwise
    pub fn has_role_at_least(e: Env, address: Address, required_role: AdminRole) -> bool {
        match e
            .storage()
            .instance()
            .get::<_, AdminInfo>(&DataKey::AdminInfo(address))
        {
            Some(admin_info) => admin_info.active && admin_info.role >= required_role,
            None => false,
        }
    }

    /// Get all admin addresses.
    ///
    /// # Returns
    /// A `Vec` of all admin addresses
    pub fn get_all_admins(e: Env) -> Vec<Address> {
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
        Self::get_all_admins(e).len()
    }

    /// Get the number of active admins.
    ///
    /// # Returns
    /// The count of active admins
    pub fn get_active_admin_count(e: Env) -> u32 {
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
        let min_admins: u32 = e.storage().instance().get(&DataKey::MinAdmins).unwrap_or(1);
        let max_admins: u32 = e
            .storage()
            .instance()
            .get(&DataKey::MaxAdmins)
            .unwrap_or(100);
        (min_admins, max_admins)
    }

    // Helper functions

    /// Get the role of an address (panics if not admin).
    pub fn get_role(e: Env, address: Address) -> AdminRole {
        let admin_info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(address))
            .unwrap_or_else(|| panic!("address is not an admin"));
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

    /// Require that the caller has at least the specified role.
    fn require_role_at_least(
        e: &Env,
        caller: &Address,
        required_role: AdminRole,
    ) -> Result<(), ()> {
        let caller_role = Self::get_role(e.clone(), caller.clone());
        if caller_role >= required_role {
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
        pausable::is_paused(&e)
    }

    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        pausable::pause(&e, &caller)
    }

    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        pausable::unpause(&e, &caller)
    }

    pub fn set_pause_signer(e: Env, admin: Address, signer: Address, enabled: bool) {
        pausable::set_pause_signer(&e, &admin, &signer, enabled)
    }

    pub fn set_pause_threshold(e: Env, admin: Address, threshold: u32) {
        pausable::set_pause_threshold(&e, &admin, threshold)
    }

    pub fn approve_pause_proposal(e: Env, signer: Address, proposal_id: u64) {
        pausable::approve_pause_proposal(&e, &signer, proposal_id)
    }

    pub fn execute_pause_proposal(e: Env, proposal_id: u64) {
        pausable::execute_pause_proposal(&e, proposal_id)
    }
}

#[cfg(test)]
mod test_pausable;

#[cfg(test)]
mod test_basic;
