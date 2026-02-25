//! # Credence Multi-Signature Contract
//!
//! Generic multi-signature contract for governance and administrative actions.
//! Supports configurable signer threshold, proposal submissions, signature counting,
//! and execution at threshold. Can be used for any administrative action requiring
//! multi-party approval.

use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env, String, Symbol, Vec};

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
    ///
    /// # Panics
    /// * If threshold is 0 or exceeds signer count
    /// * If signers list is empty
    /// * If threshold exceeds signer count
    ///
    /// # Events
    /// Emits `multisig_initialized` event
    pub fn initialize(e: Env, admin: Address, signers: Vec<Address>, threshold: u32) {
        admin.require_auth();

        if signers.is_empty() {
            panic!("signers list cannot be empty");
        }

        let signer_count = signers.len();
        if threshold == 0 || threshold > signer_count {
            panic!("invalid threshold: must be 1 <= threshold <= signer count");
        }

        e.storage().instance().set(&DataKey::Admin, &admin);
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
    ///
    /// @param e Contract environment
    /// @param admin Admin address (must authenticate)
    /// @param signer Address to add as signer
    ///
    /// # Panics
    /// * If caller is not admin
    /// * If signer already exists
    ///
    /// # Events
    /// Emits `signer_added` event
    pub fn add_signer(e: Env, admin: Address, signer: Address) {
        Self::require_admin(&e, &admin);

        let already = e
            .storage()
            .instance()
            .get(&DataKey::Signer(signer.clone()))
            .unwrap_or(false);

        if already {
            panic!("signer already exists");
        }

        e.storage()
            .instance()
            .set(&DataKey::Signer(signer.clone()), &true);

        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);
        let new_count = count.checked_add(1).expect("signer count overflow");
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
    /// Threshold is auto-capped to new signer count if needed.
    ///
    /// @param e Contract environment
    /// @param admin Admin address (must authenticate)
    /// @param signer Address to remove
    ///
    /// # Panics
    /// * If caller is not admin
    /// * If signer doesn't exist
    /// * If removing would leave zero signers
    ///
    /// # Events
    /// Emits `signer_removed` event
    pub fn remove_signer(e: Env, admin: Address, signer: Address) {
        Self::require_admin(&e, &admin);

        let exists = e
            .storage()
            .instance()
            .get(&DataKey::Signer(signer.clone()))
            .unwrap_or(false);

        if !exists {
            panic!("signer does not exist");
        }

        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(1);

        if count <= 1 {
            panic!("cannot remove last signer");
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

        e.events()
            .publish((Symbol::new(&e, "signer_removed"),), signer);
    }

    /// Set the signature threshold. Only admin can set threshold.
    ///
    /// @param e Contract environment
    /// @param admin Admin address (must authenticate)
    /// @param threshold New threshold value
    ///
    /// # Panics
    /// * If caller is not admin
    /// * If threshold is 0 or exceeds signer count
    ///
    /// # Events
    /// Emits `threshold_updated` event
    pub fn set_threshold(e: Env, admin: Address, threshold: u32) {
        Self::require_admin(&e, &admin);

        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);

        if threshold == 0 || threshold > count {
            panic!("invalid threshold: must be 1 <= threshold <= signer count");
        }

        e.storage().instance().set(&DataKey::Threshold, &threshold);

        e.events()
            .publish((Symbol::new(&e, "threshold_updated"),), threshold);
    }

    /// Submit a new proposal. Only signers can submit proposals.
    ///
    /// @param e Contract environment
    /// @param proposer Signer submitting the proposal (must authenticate)
    /// @param action_type Type of action
    /// @param target Target contract address (optional)
    /// @param function_name Function to call (optional)
    /// @param arguments Encoded arguments (optional)
    /// @param description Human-readable description
    /// @param expires_at Expiration timestamp (0 for no expiration)
    /// @param metadata Custom metadata (optional)
    /// @return Proposal ID
    ///
    /// # Panics
    /// * If caller is not a signer
    /// * If description is empty
    ///
    /// # Events
    /// Emits `proposal_submitted` event
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
    ) -> u64 {
        proposer.require_auth();

        Self::require_signer(&e, &proposer);

        if description.len() == 0 {
            panic!("description cannot be empty");
        }

        let id: u64 = e
            .storage()
            .instance()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        let next_id = id.checked_add(1).expect("proposal counter overflow");
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
        };

        e.storage()
            .instance()
            .set(&DataKey::Proposal(id), &proposal);
        e.storage()
            .instance()
            .set(&DataKey::SignatureCount(id), &0_u32);

        e.events().publish(
            (Symbol::new(&e, "proposal_submitted"), id),
            (proposer, action_type, description),
        );

        id
    }

    /// Sign a proposal. Only signers can sign.
    ///
    /// @param e Contract environment
    /// @param signer Signer address (must authenticate)
    /// @param proposal_id ID of proposal to sign
    ///
    /// # Panics
    /// * If caller is not a signer
    /// * If proposal doesn't exist
    /// * If proposal is not pending
    /// * If proposal has expired
    /// * If signer has already signed
    ///
    /// # Events
    /// Emits `proposal_signed` event
    pub fn sign_proposal(e: Env, signer: Address, proposal_id: u64) {
        signer.require_auth();

        Self::require_signer(&e, &signer);

        let proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));

        if proposal.status != ProposalStatus::Pending {
            panic!("proposal is not pending");
        }

        if proposal.expires_at > 0 && e.ledger().timestamp() >= proposal.expires_at {
            panic!("proposal has expired");
        }

        let already_signed = e
            .storage()
            .instance()
            .get(&DataKey::Signature(proposal_id, signer.clone()))
            .unwrap_or(false);

        if already_signed {
            panic!("already signed");
        }

        e.storage()
            .instance()
            .set(&DataKey::Signature(proposal_id, signer.clone()), &true);

        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignatureCount(proposal_id))
            .unwrap_or(0);
        let new_count = count.checked_add(1).expect("signature count overflow");
        e.storage()
            .instance()
            .set(&DataKey::SignatureCount(proposal_id), &new_count);

        e.events().publish(
            (Symbol::new(&e, "proposal_signed"), proposal_id),
            (signer, new_count),
        );
    }

    /// Execute a proposal. Anyone can execute once threshold is met.
    ///
    /// @param e Contract environment
    /// @param proposal_id ID of proposal to execute
    ///
    /// # Panics
    /// * If proposal doesn't exist
    /// * If proposal is not pending
    /// * If proposal has expired
    /// * If signature count < threshold
    ///
    /// # Events
    /// Emits `proposal_executed` event
    ///
    /// # Note
    /// This function marks the proposal as executed but does not perform
    /// the actual action. The caller should invoke the target contract
    /// or perform the action after this succeeds. For security, actual
    /// execution logic should be implemented by the calling contract.
    pub fn execute_proposal(e: Env, proposal_id: u64) {
        let mut proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));

        if proposal.status != ProposalStatus::Pending {
            panic!("proposal is not pending");
        }

        if proposal.expires_at > 0 && e.ledger().timestamp() >= proposal.expires_at {
            Self::expire_proposal(&e, proposal_id);
            panic!("proposal has expired");
        }

        let threshold: u32 = e.storage().instance().get(&DataKey::Threshold).unwrap_or(0);
        let signatures: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignatureCount(proposal_id))
            .unwrap_or(0);

        if signatures < threshold {
            panic!("insufficient signatures to execute");
        }

        proposal.status = ProposalStatus::Executed;
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        e.events().publish(
            (Symbol::new(&e, "proposal_executed"), proposal_id),
            (proposal.action_type, signatures),
        );
    }

    /// Reject a proposal. Only admin can reject.
    ///
    /// @param e Contract environment
    /// @param admin Admin address (must authenticate)
    /// @param proposal_id ID of proposal to reject
    ///
    /// # Panics
    /// * If caller is not admin
    /// * If proposal doesn't exist
    /// * If proposal is not pending
    ///
    /// # Events
    /// Emits `proposal_rejected` event
    pub fn reject_proposal(e: Env, admin: Address, proposal_id: u64) {
        Self::require_admin(&e, &admin);

        let mut proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));

        if proposal.status != ProposalStatus::Pending {
            panic!("proposal is not pending");
        }

        proposal.status = ProposalStatus::Rejected;
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        e.events()
            .publish((Symbol::new(&e, "proposal_rejected"), proposal_id), admin);
    }

    // ==================== Query Functions ====================

    /// Get proposal by ID.
    pub fn get_proposal(e: Env, proposal_id: u64) -> Proposal {
        e.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"))
    }

    /// Get current signature count for a proposal.
    pub fn get_signature_count(e: Env, proposal_id: u64) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::SignatureCount(proposal_id))
            .unwrap_or(0)
    }

    /// Check if a signer has signed a proposal.
    pub fn has_signed(e: Env, proposal_id: u64, signer: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Signature(proposal_id, signer))
            .unwrap_or(false)
    }

    /// Check if an address is a signer.
    pub fn is_signer(e: Env, address: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Signer(address))
            .unwrap_or(false)
    }

    /// Get current threshold.
    pub fn get_threshold(e: Env) -> u32 {
        e.storage().instance().get(&DataKey::Threshold).unwrap_or(0)
    }

    /// Get current signer count.
    pub fn get_signer_count(e: Env) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0)
    }

    /// Get list of all signers.
    pub fn get_signers(e: Env) -> Vec<Address> {
        e.storage()
            .instance()
            .get(&DataKey::SignerList)
            .unwrap_or(Vec::new(&e))
    }

    /// Get admin address.
    pub fn get_admin(e: Env) -> Address {
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"))
    }

    // ==================== Internal Helpers ====================

    fn require_admin(e: &Env, admin: &Address) {
        admin.require_auth();
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        if stored_admin != *admin {
            panic!("not admin");
        }
    }

    fn require_signer(e: &Env, signer: &Address) {
        let is_signer = e
            .storage()
            .instance()
            .get(&DataKey::Signer(signer.clone()))
            .unwrap_or(false);
        if !is_signer {
            panic!("not a signer");
        }
    }

    fn expire_proposal(e: &Env, proposal_id: u64) {
        let mut proposal: Proposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));

        proposal.status = ProposalStatus::Expired;
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);

        e.events()
            .publish((Symbol::new(&e, "proposal_expired"), proposal_id), ());
    }
}
