//! Governance Approval for Slashing
//!
//! Multi-signature verification for slash requests: proposals are created, governors vote
//! (with optional delegation), and slashing is executed only when quorum and approval
//! requirements are met. Emits governance events for audit.

use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

/// Status of a slash proposal.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    /// Open for voting.
    Open,
    /// Executed (slash applied).
    Executed,
    /// Rejected (quorum not met or majority against).
    Rejected,
}

/// A slash proposal: amount to slash, proposer, and execution state.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SlashProposal {
    pub id: u64,
    pub amount: i128,
    pub proposed_by: Address,
    pub proposed_at: u64,
    pub status: ProposalStatus,
}

fn key_next_id() -> crate::DataKey {
    crate::DataKey::GovernanceNextProposalId
}

fn key_proposal(id: u64) -> crate::DataKey {
    crate::DataKey::GovernanceProposal(id)
}

fn key_vote(proposal_id: u64, voter: Address) -> crate::DataKey {
    crate::DataKey::GovernanceVote(proposal_id, voter)
}

fn key_delegate(from: Address) -> crate::DataKey {
    crate::DataKey::GovernanceDelegate(from)
}

fn key_governors() -> crate::DataKey {
    crate::DataKey::GovernanceGovernors
}

fn key_quorum_bps() -> crate::DataKey {
    crate::DataKey::GovernanceQuorumBps
}

fn key_min_governors() -> crate::DataKey {
    crate::DataKey::GovernanceMinGovernors
}

fn is_governor(governors: &Vec<Address>, addr: &Address) -> bool {
    for g in governors.iter() {
        if g == addr.clone() {
            return true;
        }
    }
    false
}

/// Initialize governance: set governors and quorum. Admin only (enforced by caller).
pub fn initialize_governance(
    e: &Env,
    governors: Vec<Address>,
    quorum_bps: u32,
    min_governors: u32,
) {
    if quorum_bps > 10_000 {
        panic!("quorum_bps must be <= 10000");
    }
    e.storage().instance().set(&key_governors(), &governors);
    e.storage().instance().set(&key_quorum_bps(), &quorum_bps);
    e.storage()
        .instance()
        .set(&key_min_governors(), &min_governors);
    e.storage().instance().set(&key_next_id(), &0_u64);
}

/// Create a new slash proposal. Caller must be admin or governor. Returns proposal id.
pub fn propose_slash(e: &Env, proposer: &Address, amount: i128) -> u64 {
    if amount <= 0 {
        panic!("slash amount must be positive");
    }
    let id: u64 = e.storage().instance().get(&key_next_id()).unwrap_or(0);
    let next_id = id.checked_add(1).expect("proposal id overflow");
    e.storage().instance().set(&key_next_id(), &next_id);

    let proposal = SlashProposal {
        id,
        amount,
        proposed_by: proposer.clone(),
        proposed_at: e.ledger().timestamp(),
        status: ProposalStatus::Open,
    };
    e.storage().instance().set(&key_proposal(id), &proposal);
    emit_governance_event(e, "slash_proposed", id, proposer, amount);
    id
}

/// Record a vote (approve = true, reject = false). Caller must be a governor or delegate.
pub fn vote(e: &Env, voter: &Address, proposal_id: u64, approve: bool) {
    let proposal: SlashProposal = e
        .storage()
        .instance()
        .get(&key_proposal(proposal_id))
        .unwrap_or_else(|| panic!("proposal not found"));
    if proposal.status != ProposalStatus::Open {
        panic!("proposal not open for voting");
    }
    let governors: Vec<Address> = e
        .storage()
        .instance()
        .get(&key_governors())
        .unwrap_or_else(|| panic!("governance not initialized"));
    let is_gov = is_governor(&governors, voter);
    let is_delegate_of_some = governors.iter().any(|g| {
        let d: Option<Address> = e.storage().instance().get(&key_delegate(g.clone()));
        d.as_ref() == Some(voter)
    });
    let can_vote = is_gov || is_delegate_of_some;
    if !can_vote {
        panic!("not a governor or delegate");
    }
    let vote_key = key_vote(proposal_id, voter.clone());
    if e.storage().instance().has(&vote_key) {
        panic!("already voted");
    }
    e.storage().instance().set(&vote_key, &approve);
    emit_governance_event(
        e,
        "governance_vote",
        proposal_id,
        voter,
        if approve { 1_i128 } else { 0_i128 },
    );
}

/// Delegate voting power to another address. Caller must be a governor.
pub fn delegate(e: &Env, governor: &Address, to: &Address) {
    governor.require_auth();
    let governors: Vec<Address> = e
        .storage()
        .instance()
        .get(&key_governors())
        .unwrap_or_else(|| panic!("governance not initialized"));
    if !is_governor(&governors, governor) {
        panic!("not a governor");
    }
    e.storage()
        .instance()
        .set(&key_delegate(governor.clone()), to);
    emit_governance_event(e, "governance_delegate", 0, governor, 0_i128);
}

/// Resolve effective voter for a governor (follow delegation chain, one level).
fn effective_voter(e: &Env, governor: &Address) -> Address {
    let delegated: Option<Address> = e.storage().instance().get(&key_delegate(governor.clone()));
    delegated.unwrap_or_else(|| governor.clone())
}

/// Count votes for a proposal: (approve_count, reject_count, total_voted).
fn count_votes(e: &Env, proposal_id: u64) -> (u32, u32, u32) {
    let governors: Vec<Address> = e
        .storage()
        .instance()
        .get(&key_governors())
        .unwrap_or(Vec::new(e));
    let mut approve = 0u32;
    let mut reject = 0u32;
    let mut voted = 0u32;
    for g in governors.iter() {
        let effective = effective_voter(e, &g);
        let vote_key = key_vote(proposal_id, effective);
        if e.storage().instance().has(&vote_key) {
            voted += 1;
            let v: bool = e.storage().instance().get(&vote_key).unwrap();
            if v {
                approve += 1;
            } else {
                reject += 1;
            }
        }
    }
    (approve, reject, voted)
}

/// Check if quorum is met and majority approve.
pub fn is_approved(e: &Env, proposal_id: u64) -> bool {
    let governors: Vec<Address> = e
        .storage()
        .instance()
        .get(&key_governors())
        .unwrap_or(Vec::new(e));
    let total = governors.len() as u32;
    if total == 0 {
        return false;
    }
    let quorum_bps: u32 = e
        .storage()
        .instance()
        .get(&key_quorum_bps())
        .unwrap_or(5100);
    let min_governors: u32 = e
        .storage()
        .instance()
        .get(&key_min_governors())
        .unwrap_or(1);
    let (approve, _reject, voted) = count_votes(e, proposal_id);
    let quorum_ok = voted >= (total * quorum_bps / 10_000).max(min_governors);
    let majority_approve = voted > 0 && approve > voted / 2;
    quorum_ok && majority_approve
}

/// Execute slash for an approved proposal. Returns true if executed.
pub fn execute_slash_if_approved(e: &Env, proposal_id: u64) -> bool {
    let mut proposal: SlashProposal = e
        .storage()
        .instance()
        .get(&key_proposal(proposal_id))
        .unwrap_or_else(|| panic!("proposal not found"));
    if proposal.status != ProposalStatus::Open {
        panic!("proposal already closed");
    }
    if !is_approved(e, proposal_id) {
        proposal.status = ProposalStatus::Rejected;
        e.storage()
            .instance()
            .set(&key_proposal(proposal_id), &proposal);
        emit_governance_event(
            e,
            "slash_proposal_rejected",
            proposal_id,
            &proposal.proposed_by,
            proposal.amount,
        );
        return false;
    }
    proposal.status = ProposalStatus::Executed;
    e.storage()
        .instance()
        .set(&key_proposal(proposal_id), &proposal);
    emit_governance_event(
        e,
        "slash_proposal_executed",
        proposal_id,
        &proposal.proposed_by,
        proposal.amount,
    );
    true
}

/// Get proposal by id.
pub fn get_proposal(e: &Env, proposal_id: u64) -> Option<SlashProposal> {
    e.storage().instance().get(&key_proposal(proposal_id))
}

/// Get vote for (proposal_id, voter). Returns None if not voted.
pub fn get_vote(e: &Env, proposal_id: u64, voter: &Address) -> Option<bool> {
    let key = key_vote(proposal_id, voter.clone());
    if e.storage().instance().has(&key) {
        e.storage().instance().get(&key)
    } else {
        None
    }
}

/// Get governors list.
pub fn get_governors(e: &Env) -> Vec<Address> {
    e.storage()
        .instance()
        .get(&key_governors())
        .unwrap_or(Vec::new(e))
}

/// Get delegate for a governor.
pub fn get_delegate(e: &Env, governor: &Address) -> Option<Address> {
    e.storage().instance().get(&key_delegate(governor.clone()))
}

/// Get quorum config (quorum_bps, min_governors).
pub fn get_quorum_config(e: &Env) -> (u32, u32) {
    let quorum_bps: u32 = e
        .storage()
        .instance()
        .get(&key_quorum_bps())
        .unwrap_or(5100);
    let min_governors: u32 = e
        .storage()
        .instance()
        .get(&key_min_governors())
        .unwrap_or(1);
    (quorum_bps, min_governors)
}

fn emit_governance_event(e: &Env, topic: &str, proposal_id: u64, addr: &Address, amount: i128) {
    e.events().publish(
        (Symbol::new(e, topic),),
        (proposal_id, addr.clone(), amount),
    );
}
