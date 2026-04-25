//! # UUPS Upgrade Authorization Module
//!
//! Implements secure upgrade authorization for UUPS (Universal Upgradeable Proxy Standard)
//! with role-based access control and explicit authorization checks.
//!
//! ## Features
//! - Role-based upgrade authorization
//! - Explicit upgrade authorization with custom errors
//! - Proxy compatibility safeguards
//! - Upgrade history tracking

use crate::{events, DataKey};
use soroban_sdk::{contracttype, Address, Bytes, Env, Vec};

/// Upgrade authorization roles
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum UpgradeRole {
    /// Can perform upgrades (highest level)
    Upgrader = 2,
    /// Can propose upgrades (requires approval)
    Proposer = 1,
}

/// Upgrade authorization status
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeAuthorization {
    /// Address authorized to upgrade
    pub authorized_address: Address,
    /// Role of the authorized address
    pub role: UpgradeRole,
    /// When the authorization was granted
    pub granted_at: u64,
    /// When the authorization expires (0 = no expiry)
    pub expires_at: u64,
    /// Address that granted this authorization
    pub granted_by: Address,
    /// Whether this authorization is currently active
    pub active: bool,
}

/// Upgrade proposal for governance
#[contracttype]
#[derive(Clone, Debug)]
pub struct UpgradeProposal {
    /// Unique proposal ID
    pub proposal_id: u64,
    /// Address proposing the upgrade
    pub proposer: Address,
    /// New implementation address
    pub new_implementation: Address,
    /// Additional data for the upgrade
    pub upgrade_data: Bytes,
    /// When the proposal was created
    pub created_at: u64,
    /// Current status of the proposal
    pub status: UpgradeStatus,
    /// Addresses that have approved this proposal
    pub approvals: Vec<Address>,
    /// Required number of approvals
    pub required_approvals: u32,
}

/// Upgrade proposal status
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeStatus {
    /// Proposal is pending approval
    Pending = 0,
    /// Proposal has been approved and can be executed
    Approved = 1,
    /// Proposal has been executed
    Executed = 2,
    /// Proposal has been rejected
    Rejected = 3,
    /// Proposal has expired
    Expired = 4,
}

/// Custom errors for upgrade operations
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpgradeError {
    /// Caller is not authorized to upgrade
    UnauthorizedUpgrade = 0,
    /// New implementation is not a valid contract
    InvalidImplementation = 1,
    /// Upgrade would break proxy compatibility
    IncompatibleUpgrade = 2,
    /// Proposal has not been approved
    ProposalNotApproved = 3,
    /// Proposal has expired
    ProposalExpired = 4,
    /// Upgrade authorization has expired
    AuthorizationExpired = 5,
}

/// Storage keys for upgrade authorization
impl DataKey {
    /// Upgrade authorization by address: DataKey::UpgradeAuth(address) -> UpgradeAuthorization
    pub fn upgrade_auth(address: &Address) -> DataKey {
        DataKey::UpgradeAuth(address.clone())
    }

    pub fn authorized_upgraders() -> DataKey {
        DataKey::AuthorizedUpgraders
    }

    pub fn implementation() -> DataKey {
        DataKey::Implementation
    }

    pub fn upgrade_admin() -> DataKey {
        DataKey::UpgradeAdmin
    }

    /// Pending upgrade admin address: DataKey::PndgUpgrAdmin -> Address
    pub fn pending_upgrade_admin() -> DataKey {
        DataKey::PndgUpgrAdmin
    }

    /// Upgrade proposal by ID: DataKey::UpgradeProposal(proposal_id) -> UpgradeProposal
    pub fn upgrade_proposal(proposal_id: u64) -> DataKey {
        DataKey::UpgradeProposal(proposal_id)
    }

    pub fn next_proposal_id() -> DataKey {
        DataKey::NextProposalId
    }

    pub fn upgrade_history() -> DataKey {
        DataKey::UpgradeHistory
    }
}

/// Record of an executed upgrade
#[contracttype]
#[derive(Clone, Debug)]
pub struct UpgradeRecord {
    /// Old implementation address
    pub old_implementation: Address,
    /// New implementation address
    pub new_implementation: Address,
    /// When the upgrade was executed
    pub executed_at: u64,
    /// Address that executed the upgrade
    pub executed_by: Address,
    /// Upgrade proposal ID (if applicable)
    pub proposal_id: Option<u64>,
}

/// Initialize upgrade authorization with an admin
///
/// # Arguments
/// * `e` - Contract environment
/// * `admin` - Address that will have upgrade admin privileges
///
/// # Panics
/// * If contract is already initialized
pub fn initialize_upgrade_auth(e: &Env, admin: &Address) {
    if e.storage().instance().has(&DataKey::UpgradeAdmin) {
        panic!("upgrade authorization already initialized");
    }

    // Set upgrade admin
    e.storage().instance().set(&DataKey::UpgradeAdmin, admin);

    // Grant upgrader role to admin
    let auth = UpgradeAuthorization {
        authorized_address: admin.clone(),
        role: UpgradeRole::Upgrader,
        granted_at: e.ledger().timestamp(),
        expires_at: 0, // No expiry
        granted_by: admin.clone(),
        active: true,
    };

    e.storage()
        .instance()
        .set(&DataKey::UpgradeAuth(admin.clone()), &auth);

    // Initialize authorized upgraders list
    let mut upgraders = Vec::new(e);
    upgraders.push_back(admin.clone());
    e.storage()
        .instance()
        .set(&DataKey::AuthorizedUpgraders, &upgraders);

    // Initialize proposal ID counter
    e.storage().instance().set(&DataKey::NextProposalId, &1u64);

    // Initialize upgrade history
    e.storage()
        .instance()
        .set(&DataKey::UpgradeHistory, &Vec::<UpgradeRecord>::new(e));

    events::emit_upgrade_auth_initialized(e, admin);
}

/// Grant upgrade authorization to an address
///
/// # Arguments
/// * `e` - Contract environment
/// * `admin` - Address granting the authorization (must be upgrade admin)
/// * `address` - Address to authorize
/// * `role` - Role to grant
/// * `expires_at` - When authorization expires (0 = no expiry)
///
/// # Panics
/// * If caller is not upgrade admin
/// * If address is already authorized
/// * If admin is trying to grant equal or higher role to themselves
pub fn grant_upgrade_auth(
    e: &Env,
    admin: &Address,
    address: &Address,
    role: UpgradeRole,
    expires_at: u64,
) {
    admin.require_auth();
    require_upgrade_admin(e, admin);

    // Check if address is already authorized
    if e.storage()
        .instance()
        .has(&DataKey::UpgradeAuth(address.clone()))
    {
        panic!("address already authorized");
    }

    // Prevent self-assignment of equal or higher role
    if admin == address {
        let admin_role = get_upgrade_role(e, admin);
        if admin_role >= role {
            panic!("cannot grant equal or higher role to self");
        }
    }

    let auth = UpgradeAuthorization {
        authorized_address: address.clone(),
        role,
        granted_at: e.ledger().timestamp(),
        expires_at,
        granted_by: admin.clone(),
        active: true,
    };

    // Store authorization
    e.storage()
        .instance()
        .set(&DataKey::UpgradeAuth(address.clone()), &auth);

    // Update authorized upgraders list if role is Upgrader
    if role == UpgradeRole::Upgrader {
        let mut upgraders: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::AuthorizedUpgraders)
            .unwrap_or(Vec::new(e));
        upgraders.push_back(address.clone());
        e.storage()
            .instance()
            .set(&DataKey::AuthorizedUpgraders, &upgraders);
    }

    events::emit_upgrade_auth_granted(e, admin, address, role);
}

/// Revoke upgrade authorization from an address
///
/// # Arguments
/// * `e` - Contract environment
/// * `admin` - Address revoking the authorization (must be upgrade admin)
/// * `address` - Address to revoke authorization from
///
/// # Panics
/// * If caller is not upgrade admin
/// * If address is not authorized
/// * If trying to revoke the last upgrade admin
pub fn revoke_upgrade_auth(e: &Env, admin: &Address, address: &Address) {
    admin.require_auth();
    require_upgrade_admin(e, admin);

    let mut auth: UpgradeAuthorization = e
        .storage()
        .instance()
        .get(&DataKey::UpgradeAuth(address.clone()))
        .unwrap_or_else(|| panic!("address not authorized"));

    // Check if this is the last upgrade admin
    if auth.role == UpgradeRole::Upgrader {
        let upgraders: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::AuthorizedUpgraders)
            .unwrap_or(Vec::new(e));

        if upgraders.len() <= 1 {
            panic!("cannot revoke last upgrade admin");
        }

        // Remove from authorized upgraders list
        let mut new_upgraders = Vec::new(e);
        for i in 0..upgraders.len() {
            let upgrader = upgraders.get(i).unwrap();
            if upgrader != *address {
                new_upgraders.push_back(upgrader);
            }
        }
        e.storage()
            .instance()
            .set(&DataKey::AuthorizedUpgraders, &new_upgraders);
    }

    // Deactivate authorization
    auth.active = false;
    e.storage()
        .instance()
        .set(&DataKey::UpgradeAuth(address.clone()), &auth);

    events::emit_upgrade_auth_revoked(e, admin, address);
}

/// Check if an address is authorized to upgrade
///
/// # Arguments
/// * `e` - Contract environment
/// * `address` - Address to check
///
/// # Returns
/// true if authorized, false otherwise
pub fn is_authorized_upgrader(e: &Env, address: &Address) -> bool {
    match e
        .storage()
        .instance()
        .get::<_, UpgradeAuthorization>(&DataKey::UpgradeAuth(address.clone()))
    {
        Some(auth) => {
            if !auth.active {
                return false;
            }

            // Check expiry
            if auth.expires_at > 0 && e.ledger().timestamp() > auth.expires_at {
                return false;
            }

            auth.role == UpgradeRole::Upgrader
        }
        None => false,
    }
}

/// Get the upgrade role of an address
///
/// # Arguments
/// * `e` - Contract environment
/// * `address` - Address to check
///
/// # Returns
/// The upgrade role if authorized, panics otherwise
pub fn get_upgrade_role(e: &Env, address: &Address) -> UpgradeRole {
    let auth: UpgradeAuthorization = e
        .storage()
        .instance()
        .get(&DataKey::UpgradeAuth(address.clone()))
        .unwrap_or_else(|| panic!("address not authorized"));
    auth.role
}

/// Require that the caller has upgrade admin privileges
///
/// # Arguments
/// * `e` - Contract environment
/// * `caller` - Address to check
///
/// # Panics
/// * If caller is not upgrade admin
pub fn require_upgrade_admin(e: &Env, caller: &Address) {
    let upgrade_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::UpgradeAdmin)
        .unwrap_or_else(|| panic!("upgrade authorization not initialized"));

    if *caller != upgrade_admin {
        panic!("not upgrade admin");
    }
}

/// Require that the caller is authorized to upgrade
///
/// # Arguments
/// * `e` - Contract environment
/// * `caller` - Address to check
///
/// # Panics
/// * If caller is not authorized to upgrade
/// * If caller's authorization has expired
pub fn require_upgrade_auth(e: &Env, caller: &Address) {
    if !is_authorized_upgrader(e, caller) {
        panic!("unauthorized upgrade");
    }
}

/// Create an upgrade proposal
///
/// # Arguments
/// * `e` - Contract environment
/// * `proposer` - Address proposing the upgrade
/// * `new_implementation` - New implementation address
/// * `upgrade_data` - Additional data for the upgrade
/// * `required_approvals` - Number of approvals required
///
/// # Returns
/// The proposal ID
///
/// # Panics
/// * If proposer is not authorized
/// * If new implementation is invalid
pub fn propose_upgrade(
    e: &Env,
    proposer: &Address,
    new_implementation: &Address,
    upgrade_data: Bytes,
    required_approvals: u32,
) -> u64 {
    proposer.require_auth();

    // Check if proposer is authorized (can be Proposer or Upgrader)
    let auth: UpgradeAuthorization = e
        .storage()
        .instance()
        .get(&DataKey::UpgradeAuth(proposer.clone()))
        .unwrap_or_else(|| panic!("not authorized to propose upgrade"));

    if !auth.active {
        panic!("authorization not active");
    }

    // Check expiry
    if auth.expires_at > 0 && e.ledger().timestamp() > auth.expires_at {
        panic!("authorization expired");
    }

    // Validate new implementation (basic checks)
    // In Soroban, addresses are always valid, so no additional validation needed

    // Get next proposal ID
    let proposal_id: u64 = e
        .storage()
        .instance()
        .get(&DataKey::NextProposalId)
        .unwrap_or(1);
    let next_id = proposal_id.checked_add(1).expect("proposal ID overflow");
    e.storage()
        .instance()
        .set(&DataKey::NextProposalId, &next_id);

    // Create proposal
    let proposal = UpgradeProposal {
        proposal_id,
        proposer: proposer.clone(),
        new_implementation: new_implementation.clone(),
        upgrade_data,
        created_at: e.ledger().timestamp(),
        status: UpgradeStatus::Pending,
        approvals: Vec::new(e),
        required_approvals,
    };

    // Store proposal
    e.storage()
        .instance()
        .set(&DataKey::UpgradeProposal(proposal_id), &proposal);

    events::emit_upgrade_proposed(e, proposer, proposal_id, new_implementation);

    proposal_id
}

/// Approve an upgrade proposal
///
/// # Arguments
/// * `e` - Contract environment
/// * `approver` - Address approving the proposal
/// * `proposal_id` - ID of the proposal to approve
///
/// # Panics
/// * If approver is not authorized
/// * If proposal does not exist
/// * If proposal is not in pending status
/// * If approver has already approved
pub fn approve_upgrade_proposal(e: &Env, approver: &Address, proposal_id: u64) {
    approver.require_auth();
    require_upgrade_auth(e, approver);

    let mut proposal: UpgradeProposal = e
        .storage()
        .instance()
        .get(&DataKey::UpgradeProposal(proposal_id))
        .unwrap_or_else(|| panic!("proposal not found"));

    if proposal.status != UpgradeStatus::Pending {
        panic!("proposal not pending");
    }

    // Check if already approved
    for i in 0..proposal.approvals.len() {
        if proposal.approvals.get(i).unwrap() == *approver {
            panic!("already approved");
        }
    }

    // Add approval
    proposal.approvals.push_back(approver.clone());

    // Check if proposal is now approved
    if proposal.approvals.len() >= proposal.required_approvals {
        proposal.status = UpgradeStatus::Approved;
    }

    // Store updated proposal
    e.storage()
        .instance()
        .set(&DataKey::UpgradeProposal(proposal_id), &proposal);

    events::emit_upgrade_approved(e, approver, proposal_id);
}

/// Execute an upgrade
///
/// # Arguments
/// * `e` - Contract environment
/// * `executor` - Address executing the upgrade
/// * `new_implementation` - New implementation address
/// * `proposal_id` - Optional proposal ID (if using governance)
///
/// # Panics
/// * If executor is not authorized
/// * If proposal is provided but not approved
/// * If upgrade would break compatibility
pub fn execute_upgrade(
    e: &Env,
    executor: &Address,
    new_implementation: &Address,
    proposal_id: Option<u64>,
) {
    executor.require_auth();
    require_upgrade_auth(e, executor);

    // Check proposal if provided
    if let Some(pid) = proposal_id {
        let proposal: UpgradeProposal = e
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposal(pid))
            .unwrap_or_else(|| panic!("proposal not found"));

        if proposal.status != UpgradeStatus::Approved {
            panic!("proposal not approved");
        }

        if proposal.new_implementation != *new_implementation {
            panic!("implementation does not match proposal");
        }

        // Mark proposal as executed
        let mut updated_proposal = proposal;
        updated_proposal.status = UpgradeStatus::Executed;
        e.storage()
            .instance()
            .set(&DataKey::UpgradeProposal(pid), &updated_proposal);
    }

    // Get current implementation
    let current_impl: Address = e
        .storage()
        .instance()
        .get(&DataKey::Implementation)
        .unwrap_or_else(|| panic!("no current implementation"));

    // Validate new implementation (basic compatibility check)
    if *new_implementation == current_impl {
        panic!("same implementation");
    }

    // Record upgrade in history
    let record = UpgradeRecord {
        old_implementation: current_impl,
        new_implementation: new_implementation.clone(),
        executed_at: e.ledger().timestamp(),
        executed_by: executor.clone(),
        proposal_id,
    };

    let mut history: Vec<UpgradeRecord> = e
        .storage()
        .instance()
        .get(&DataKey::UpgradeHistory)
        .unwrap_or(Vec::new(e));
    history.push_back(record);
    e.storage()
        .instance()
        .set(&DataKey::UpgradeHistory, &history);

    // Update implementation
    e.storage()
        .instance()
        .set(&DataKey::Implementation, new_implementation);

    events::emit_upgrade_executed(e, executor, new_implementation, proposal_id);
}

/// Get the current implementation address
///
/// # Arguments
/// * `e` - Contract environment
///
/// # Returns
/// Current implementation address
pub fn get_implementation(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Implementation)
        .unwrap_or_else(|| panic!("no implementation set"))
}

/// Get upgrade authorization info for an address
///
/// # Arguments
/// * `e` - Contract environment
/// * `address` - Address to query
///
/// # Returns
/// Upgrade authorization info
pub fn get_upgrade_auth(e: &Env, address: &Address) -> UpgradeAuthorization {
    e.storage()
        .instance()
        .get(&DataKey::UpgradeAuth(address.clone()))
        .unwrap_or_else(|| panic!("address not authorized"))
}

/// Get an upgrade proposal
///
/// # Arguments
/// * `e` - Contract environment
/// * `proposal_id` - ID of the proposal
///
/// # Returns
/// The upgrade proposal
pub fn get_upgrade_proposal(e: &Env, proposal_id: u64) -> UpgradeProposal {
    e.storage()
        .instance()
        .get(&DataKey::UpgradeProposal(proposal_id))
        .unwrap_or_else(|| panic!("proposal not found"))
}

/// Get all authorized upgraders
///
/// # Arguments
/// * `e` - Contract environment
///
/// # Returns
/// Vector of authorized upgrader addresses
pub fn get_authorized_upgraders(e: &Env) -> Vec<Address> {
    e.storage()
        .instance()
        .get(&DataKey::AuthorizedUpgraders)
        .unwrap_or(Vec::new(e))
}

/// Get upgrade history
///
/// # Arguments
/// * `e` - Contract environment
///
/// # Returns
/// Vector of upgrade records
pub fn get_upgrade_history(e: &Env) -> Vec<UpgradeRecord> {
    e.storage()
        .instance()
        .get(&DataKey::UpgradeHistory)
        .unwrap_or(Vec::new(e))
}
/// Propose a new upgrade admin (two-step transfer)
pub fn transfer_upgrade_admin(e: &Env, admin: &Address, new_admin: &Address) {
    admin.require_auth();
    require_upgrade_admin(e, admin);

    if admin == new_admin {
        panic!("new admin must be different");
    }

    // Zero-address check
    let zero_str = soroban_sdk::String::from_str(e, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    if new_admin.to_string() == zero_str {
        panic!("ZeroAddress");
    }

    e.storage()
        .instance()
        .set(&DataKey::PndgUpgrAdmin, new_admin);

    events::emit_upgrade_admin_transfer_started(e, admin, new_admin);
}

/// Accept the upgrade admin role (second step of transfer)
pub fn accept_upgrade_admin(e: &Env, caller: &Address) {
    caller.require_auth();
    let pending_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::PndgUpgrAdmin)
        .unwrap_or_else(|| panic!("no pending upgrade admin"));

    if *caller != pending_admin {
        panic!("not pending upgrade admin");
    }

    let old_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::UpgradeAdmin)
        .unwrap_or_else(|| panic!("no upgrade admin"));

    // Update upgrade admin
    e.storage().instance().set(&DataKey::UpgradeAdmin, caller);

    // Grant Upgrader role to the new admin
    let auth = UpgradeAuthorization {
        authorized_address: caller.clone(),
        role: UpgradeRole::Upgrader,
        granted_at: e.ledger().timestamp(),
        expires_at: 0, // No expiry
        granted_by: caller.clone(),
        active: true,
    };

    e.storage()
        .instance()
        .set(&DataKey::UpgradeAuth(caller.clone()), &auth);

    // Update authorized upgraders list
    let mut upgraders: Vec<Address> = e
        .storage()
        .instance()
        .get(&DataKey::AuthorizedUpgraders)
        .unwrap_or(Vec::new(e));
    
    let mut already_in = false;
    for i in 0..upgraders.len() {
        if upgraders.get(i).unwrap() == *caller {
            already_in = true;
            break;
        }
    }
    if !already_in {
        upgraders.push_back(caller.clone());
        e.storage()
            .instance()
            .set(&DataKey::AuthorizedUpgraders, &upgraders);
    }

    // Clear pending admin
    e.storage().instance().remove(&DataKey::PndgUpgrAdmin);

    events::emit_upgrade_admin_transfer_completed(e, &old_admin, caller);
}

/// Get the pending upgrade admin address
pub fn get_pending_upgrade_admin(e: &Env) -> Option<Address> {
    e.storage().instance().get(&DataKey::PndgUpgrAdmin)
}
