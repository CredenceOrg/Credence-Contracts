//! # Credence Multi-Signature Contract
//!
//! Generic multi-signature contract for governance and administrative actions.
//! Supports configurable signer threshold, proposal submissions, signature counting,
//! and execution at threshold. Can be used for any administrative action requiring
//! multi-party approval.

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, Address, Bytes, BytesN, Env, String,
    Symbol, Vec,
};

/// Type of action that can be proposed and executed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionType {
    /// Generic contract call to another contract.
    ContractCall = 0,
    /// Transfer tokens/assets.
    Transfer = 1,
    /// Configuration change.
    ConfigChange = 2,
    /// Add/remove signer.
    SignerManagement = 3,
    /// Custom action type.
    Custom = 99,
}

/// Status of a proposal.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    /// Proposal is pending approval.
    Pending = 0,
    /// Proposal has been approved and executed.
    Executed = 1,
    /// Proposal has been rejected.
    Rejected = 2,
    /// Proposal has expired.
    Expired = 3,
}

/// A multi-signature proposal.
/// Created by a signer; executable when signature count >= threshold.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    /// Unique proposal identifier.
    pub id: u64,
    /// Type of action.
    pub action_type: ActionType,
    /// Target contract address (if applicable).
    pub target: Option<Address>,
    /// Function name to call (if ContractCall).
    pub function_name: Option<String>,
    /// Encoded function arguments (if ContractCall).
    pub arguments: Option<Bytes>,
    /// Description of the proposal.
    pub description: String,
    /// Ledger timestamp when proposed.
    pub proposed_at: u64,
    /// Proposer (signer who created the proposal).
    pub proposer: Address,
    /// Current status.
    pub status: ProposalStatus,
    /// Expiration timestamp (0 = no expiration).
    pub expires_at: u64,
    /// Custom metadata (flexible storage).
    pub metadata: Option<String>,
    /// Deterministic hash of the operation payload to prevent replays.
    pub op_hash: BytesN<32>,
}

#[contracttype]
pub enum DataKey {
    /// Contract admin (can initialize, add/remove signers initially).
    Admin,
    /// Signers for multi-sig (can propose and sign proposals).
    Signer(Address),
    /// Number of active signers.
    SignerCount,
    /// Required number of signatures to execute a proposal.
    Threshold,
    /// Next proposal id counter.
    ProposalCounter,
    /// Proposal by id.
    Proposal(u64),
    /// Signature: (proposal_id, signer) -> true.
    Signature(u64, Address),
    /// Signature count per proposal.
    SignatureCount(u64),
    /// List of all signer addresses (for enumeration).
    SignerList,
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u64),
    PauseApproval(u64, Address),
    PauseApprovalCount(u64),
    /// Tracks executed operations by their deterministic hash to prevent replay across proposals.
    ExecutedOp(BytesN<32>),
}

/// Hard cap on max_iter for proposal pruning to prevent out-of-gas.
pub const MAX_ITER_HARD_CAP: u32 = 200;

/// Default page size for proposal pruning if the caller passes 0.
pub const DEFAULT_MAX_ITER: u32 = 50;

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

#[contract]
pub struct CredenceMultiSig;

#[contractimpl]
impl CredenceMultiSig {
    /// Initialize the multi-sig contract.
    ///
    /// @param e Contract environment
    /// @param admin Address that can manage initial configuration
    /// @param signers Initial list of authorized signers
    /// @param threshold Required number of signatures for execution
    pub fn initialize(e: Env, admin: Address, signers: Vec<Address>, threshold: u32) {
        bump_instance_ttl(&e);
        admin.require_auth();

        if signers.is_empty() {
            panic_with_error!(&e, ContractError::ThresholdExceedsSigners);
        }

        let signer_count = signers.len();
        if threshold == 0 || threshold > signer_count {
            panic_with_error!(&e, ContractError::ThresholdExceedsSigners);
        }

        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Paused, &false);
        e.storage()
            .instance()
            .set(&DataKey::PauseSignerCount, &0_u32);
        e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
        e.storage()
            .instance()
            .set(&DataKey::PauseProposalCounter, &0_u64);
        e.storage()
            .instance()
            .set(&DataKey::SignerCount, &signer_count);
        e.storage().instance().set(&DataKey::Threshold, &threshold);
        e.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &0_u64);
        e.storage().instance().set(&DataKey::SignerList, &signers);

        for signer in signers.iter() {
            e.storage()
                .instance()
                .set(&DataKey::Signer(signer.clone()), &true);
        }

        e.events().publish(
            (Symbol::new(&e, "multisig_initialized"),),
            (admin, signer_count, threshold),
        );
    }

    /// Add a new signer. Only admin can add signers.
    pub fn add_signer(e: Env, admin: Address, signer: Address) {
        bump_instance_ttl(&e);
        crate::pausable::require_not_paused(&e);
        Self::require_admin(&e, &admin);

        let already = e
            .storage()
            .instance()
            .get(&DataKey::Signer(signer.clone()))
            .unwrap_or(false);

        if already {
            panic_with_error!(&e, ContractError::AlreadyActive);
        }

        e.storage()
            .instance()
            .set(&DataKey::Signer(signer.clone()), &true);

        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);
        let new_count = count
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        e.storage()
            .instance()
            .set(&DataKey::SignerCount, &new_count);

        let mut signer_list: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::SignerList)
            .unwrap_or(Vec::new(&e));
        signer_list.push_back(signer.clone());
        e.storage()
            .instance()
            .set(&DataKey::SignerList, &signer_list);

        e.events()
            .publish((Symbol::new(&e, "signer_added"),), signer);
    }

    /// Remove a signer. Only admin can remove signers.
    pub fn remove_signer(e: Env, admin: Address, signer: Address) {
        bump_instance_ttl(&e);
        crate::pausable::require_not_paused(&e);
        Self::require_admin(&e, &admin);

        let exists = e
            .storage()
            .instance()
            .get(&DataKey::Signer(signer.clone()))
            .unwrap_or(false);

        if !exists {
            panic_with_error!(&e, ContractError::NotSigner);
        }

        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(1);

        if count <= 1 {
            panic_with_error!(&e, ContractError::InvalidPauseAction);
        }

        e.storage()
            .instance()
            .remove(&DataKey::Signer(signer.clone()));

        let new_count = count - 1;
        e.storage()
            .instance()
            .set(&DataKey::SignerCount, &new_count);

        let signer_list: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::SignerList)
            .unwrap_or(Vec::new(&e));

        let mut new_list = Vec::new(&e);
        for s in signer_list.iter() {
            if s != signer {
                new_list.push_back(s);
            }
        }
        e.storage().instance().set(&DataKey::SignerList, &new_list);

        let threshold: u32 = e.storage().instance().get(&DataKey::Threshold).unwrap_or(0);
        if threshold > new_count {
            e.storage().instance().set(&DataKey::Threshold, &new_count);
            e.events()
                .publish((Symbol::new(&e, "threshold_auto_adjusted"),), new_count);
        }

        // Keep stored signatures for removed signers so if the signer is later
        // re-added, their prior approvals can be restored. Execution only counts
        // signatures from the current signer set.
        e.events()
            .publish((Symbol::new(&e, "signer_removed"),), signer);
    }

    /// Set the signature threshold. Only admin can set threshold.
    pub fn set_threshold(e: Env, admin: Address, threshold: u32) {
        bump_instance_ttl(&e);
        crate::pausable::require_not_paused(&e);
        Self::require_admin(&e, &admin);

        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);

        if threshold == 0 || threshold > count {
            panic_with_error!(&e, ContractError::ThresholdExceedsSigners);
        }

        e.storage().instance().set(&DataKey::Threshold, &threshold);

        e.events()
            .publish((Symbol::new(&e, "threshold_updated"),), threshold);
    }

    /// Submit a new proposal. Only signers can submit proposals.
    pub fn submit_proposal(
        e: Env,
        proposer: Address,
        action_type: ActionType,
        target: Option<Address>,
        function_name: Option<String>,
        arguments: Option<Bytes>,
        description: String,
        expires_at: u64,
        metadata: Option<String>,
        op_hash: BytesN<32>,
    ) -> u64 {
        bump_instance_ttl(&e);
        crate::pausable::require_not_paused(&e);
        proposer.require_auth();

        Self::require_signer(&e, &proposer);

        if description.len() == 0 {
            panic_with_error!(&e, ContractError::InvalidPauseAction);
        }

        let id: u64 = e
            .storage()
            .instance()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        e.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &next_id);

        let proposal = Proposal {
            id,
            action_type: action_type.clone(),
            target: target.clone(),
            function_name: function_name.clone(),
            arguments: arguments.clone(),
            description: description.clone(),
            proposed_at: e.ledger().timestamp(),
            proposer: proposer.clone(),
            status: ProposalStatus::Pending,
            expires_at,
            metadata: metadata.clone(),
            op_hash: op_hash.clone(),
        };

        e.storage()
            .instance()
            .set(&DataKey::Proposal(id), &proposal);
        e.storage()
            .instance()
            .set(&DataKey::SignatureCount(id), &0_u32);

        e.events().publish(
            (Symbol::new(&e, "proposal_submitted"), id),
            (proposer, action_type, description, op_hash),
        );

        id
    }

    /// Sign a proposal. Only signers can sign.
    pub fn sign_proposal(e: Env, signer: Address, proposal_id: u64) {
        bump_instance_ttl(&e);
        crate::pausable::require_not_paused(&e);
        signer.require_auth();

        Self::require_signer(&e, &signer);

        let proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound));

        if proposal.status != ProposalStatus::Pending {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        if proposal.expires_at > 0 && e.ledger().timestamp() >= proposal.expires_at {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        let already_signed = e
            .storage()
            .instance()
            .get(&DataKey::Signature(proposal_id, signer.clone()))
            .unwrap_or(false);

        if already_signed {
            panic_with_error!(&e, ContractError::AlreadyActive);
        }

        e.storage()
            .instance()
            .set(&DataKey::Signature(proposal_id, signer.clone()), &true);

        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignatureCount(proposal_id))
            .unwrap_or(0);
        let new_count = count
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        e.storage()
            .instance()
            .set(&DataKey::SignatureCount(proposal_id), &new_count);

        e.events().publish(
            (Symbol::new(&e, "proposal_signed"), proposal_id),
            (signer, new_count),
        );
    }

    /// Execute a proposal. Anyone can execute once threshold is met.
    pub fn execute_proposal(e: Env, proposal_id: u64) {
        bump_instance_ttl(&e);
        crate::pausable::require_not_paused(&e);
        let mut proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound));

        if proposal.status != ProposalStatus::Pending {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        if proposal.expires_at > 0 && e.ledger().timestamp() >= proposal.expires_at {
            Self::expire_proposal(&e, proposal_id);
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        let threshold: u32 = e.storage().instance().get(&DataKey::Threshold).unwrap_or(0);
        let signatures: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignatureCount(proposal_id))
            .unwrap_or(0);
        let effective_signatures = Self::count_active_signatures(&e, proposal_id);

        if effective_signatures != signatures {
            e.storage()
                .instance()
                .set(&DataKey::SignatureCount(proposal_id), &effective_signatures);
            e.events().publish(
                (Symbol::new(&e, "proposal_signatures_revalidated"),),
                (proposal_id, signatures, effective_signatures),
            );
        }

        if effective_signatures < threshold {
            panic_with_error!(&e, ContractError::InsufficientApprovals);
        }

        // Execute-once invariant using deterministic op hash
        let op_hash = proposal.op_hash.clone();
        let already_executed = e
            .storage()
            .instance()
            .get(&DataKey::ExecutedOp(op_hash.clone()))
            .unwrap_or(false);

        if already_executed {
            panic!("operation already executed");
        }

        // Mark executed globally to prevent exact replay
        e.storage()
            .instance()
            .set(&DataKey::ExecutedOp(op_hash.clone()), &true);

        proposal.status = ProposalStatus::Executed;
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        e.events().publish(
            (Symbol::new(&e, "proposal_executed"), proposal_id, op_hash),
            (proposal.action_type, signatures),
        );
    }

    /// Reject a proposal. Only admin can reject.
    pub fn reject_proposal(e: Env, admin: Address, proposal_id: u64) {
        bump_instance_ttl(&e);
        crate::pausable::require_not_paused(&e);
        Self::require_admin(&e, &admin);

        let mut proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound));

        if proposal.status != ProposalStatus::Pending {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }

        proposal.status = ProposalStatus::Rejected;
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        e.events()
            .publish((Symbol::new(&e, "proposal_rejected"), proposal_id), admin);
    }

    /// Prune expired proposals from storage to reclaim space.
    /// Bounded sweep, permissionless (no require_auth).
    ///
    /// @param e Contract environment
    /// @param start_id Starting proposal ID to scan
    /// @param max_iter Maximum number of proposals to scan (bounded by MAX_ITER_HARD_CAP)
    /// @return The count of proposals pruned
    pub fn prune_expired_proposals(e: Env, start_id: u64, max_iter: u32) -> u32 {
        bump_instance_ttl(&e);
        crate::pausable::require_not_paused(&e);

        let effective_max = if max_iter == 0 {
            DEFAULT_MAX_ITER
        } else {
            max_iter.min(MAX_ITER_HARD_CAP)
        };

        let mut pruned_count = 0_u32;
        let now = e.ledger().timestamp();
        let signers = Self::get_signers(e.clone());

        for i in 0..effective_max {
            let proposal_id = match start_id.checked_add(i as u64) {
                Some(id) => id,
                None => break,
            };

            let proposal_key = DataKey::Proposal(proposal_id);
            if let Some(proposal) = e.storage().instance().get::<_, Proposal>(&proposal_key) {
                let is_expired = proposal.status == ProposalStatus::Expired
                    || (proposal.status != ProposalStatus::Executed
                        && proposal.expires_at > 0
                        && now >= proposal.expires_at);

                if is_expired {
                    // Remove proposal itself
                    e.storage().instance().remove(&proposal_key);

                    // Remove signatures/approvals for all current signers
                    for signer in signers.iter() {
                        e.storage()
                            .instance()
                            .remove(&DataKey::Signature(proposal_id, signer.clone()));
                    }

                    // Also remove proposer signature if it was not in active signers
                    e.storage()
                        .instance()
                        .remove(&DataKey::Signature(proposal_id, proposal.proposer.clone()));

                    // Remove signature count
                    e.storage()
                        .instance()
                        .remove(&DataKey::SignatureCount(proposal_id));

                    pruned_count += 1;
                }
            }
        }

        e.events().publish(
            (Symbol::new(&e, "proposals_pruned"),),
            (start_id, pruned_count),
        );

        pruned_count
    }

    // ==================== Query Functions ====================

    /// Get proposal by ID.
    pub fn get_proposal(e: Env, proposal_id: u64) -> Proposal {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound))
    }

    /// Check if a deterministic operation hash has already been executed.
    pub fn is_operation_executed(e: Env, op_hash: BytesN<32>) -> bool {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::ExecutedOp(op_hash))
            .unwrap_or(false)
    }

    /// Get current signature count for a proposal.
    pub fn get_signature_count(e: Env, proposal_id: u64) -> u32 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::SignatureCount(proposal_id))
            .unwrap_or(0)
    }

    /// Check if a signer has signed a proposal.
    pub fn has_signed(e: Env, proposal_id: u64, signer: Address) -> bool {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Signature(proposal_id, signer))
            .unwrap_or(false)
    }

    /// Check if an address is a signer.
    pub fn is_signer(e: Env, address: Address) -> bool {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Signer(address))
            .unwrap_or(false)
    }

    /// Get current threshold.
    pub fn get_threshold(e: Env) -> u32 {
        bump_instance_ttl(&e);
        e.storage().instance().get(&DataKey::Threshold).unwrap_or(0)
    }

    /// Get current signer count.
    pub fn get_signer_count(e: Env) -> u32 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0)
    }

    /// Get list of all signers.
    pub fn get_signers(e: Env) -> Vec<Address> {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::SignerList)
            .unwrap_or(Vec::new(&e))
    }

    /// Get admin address.
    pub fn get_admin(e: Env) -> Address {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized))
    }

    // ==================== Internal Helpers ====================

    fn require_admin(e: &Env, admin: &Address) {
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        if stored_admin != *admin {
            panic_with_error!(e, ContractError::NotAdmin);
        }
    }

    fn require_signer(e: &Env, signer: &Address) {
        let is_signer = e
            .storage()
            .instance()
            .get(&DataKey::Signer(signer.clone()))
            .unwrap_or(false);
        if !is_signer {
            panic_with_error!(e, ContractError::NotSigner);
        }
    }

    fn count_active_signatures(e: &Env, proposal_id: u64) -> u32 {
        let signers = Self::get_signers(e.clone());
        let mut count = 0_u32;

        for signer in signers.iter() {
            let signed = e
                .storage()
                .instance()
                .get(&DataKey::Signature(proposal_id, signer.clone()))
                .unwrap_or(false);
            if signed {
                count = count
                    .checked_add(1)
                    .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));
            }
        }

        count
    }

    fn expire_proposal(e: &Env, proposal_id: u64) {
        let mut proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));

        proposal.status = ProposalStatus::Expired;
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        e.events()
            .publish((Symbol::new(&e, "proposal_expired"), proposal_id), ());
    }

    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        bump_instance_ttl(&e);
        crate::pausable::pause(&e, &caller)
    }

    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        bump_instance_ttl(&e);
        crate::pausable::unpause(&e, &caller)
    }

    pub fn is_paused(e: Env) -> bool {
        bump_instance_ttl(&e);
        crate::pausable::is_paused(&e)
    }

    pub fn set_pause_signer(e: Env, admin: Address, signer: Address, enabled: bool) {
        bump_instance_ttl(&e);
        crate::pausable::set_pause_signer(&e, &admin, &signer, enabled)
    }

    pub fn set_pause_threshold(e: Env, admin: Address, threshold: u32) {
        bump_instance_ttl(&e);
        crate::pausable::set_pause_threshold(&e, &admin, threshold)
    }

    pub fn approve_pause_proposal(e: Env, signer: Address, proposal_id: u64) {
        bump_instance_ttl(&e);
        crate::pausable::approve_pause_proposal(&e, &signer, proposal_id)
    }

    pub fn execute_pause_proposal(e: Env, proposal_id: u64) {
        bump_instance_ttl(&e);
        crate::pausable::execute_pause_proposal(&e, proposal_id)
    }
}
