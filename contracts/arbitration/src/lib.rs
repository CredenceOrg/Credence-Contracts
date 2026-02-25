#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, String, Symbol};

pub mod pausable;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub id: u64,
    pub creator: Address,
    pub description: String,
    pub voting_start: u64,
    pub voting_end: u64,
    pub resolved: bool,
    pub outcome: u32, // 0 for unresolved/tie, >0 for specific outcomes
}

#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u64),
    PauseApproval(u64, Address),
    PauseApprovalCount(u64),
    Arbitrator(Address),
    Dispute(u64),
    DisputeCounter,
    DisputeVotes(u64),         // Map<u32, i128> (outcome -> total_weight)
    VoterCasted(u64, Address), // (dispute_id, voter) -> bool
}

#[contract]
pub struct CredenceArbitration;

#[contractimpl]
impl CredenceArbitration {
    /// Initialize the contract with an admin address.
    pub fn initialize(e: Env, admin: Address) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Paused, &false);
        e.storage().instance().set(&DataKey::PauseSignerCount, &0_u32);
        e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
        e.storage().instance().set(&DataKey::PauseProposalCounter, &0_u64);
    }

    /// Register or update an arbitrator with a specific voting weight.
    pub fn register_arbitrator(e: Env, arbitrator: Address, weight: i128) {
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        admin.require_auth();

        if weight <= 0 {
            panic!("weight must be positive");
        }

        e.storage()
            .instance()
            .set(&DataKey::Arbitrator(arbitrator.clone()), &weight);

        e.events().publish(
            (Symbol::new(&e, "arbitrator_registered"), arbitrator),
            weight,
        );
    }

    /// Remove an arbitrator.
    pub fn unregister_arbitrator(e: Env, arbitrator: Address) {
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        admin.require_auth();

        e.storage()
            .instance()
            .remove(&DataKey::Arbitrator(arbitrator.clone()));

        e.events()
            .publish((Symbol::new(&e, "arbitrator_unregistered"), arbitrator), ());
    }

    /// Create a new dispute for arbitration.
    pub fn create_dispute(e: Env, creator: Address, description: String, duration: u64) -> u64 {
        pausable::require_not_paused(&e);
        creator.require_auth();

        let counter_key = DataKey::DisputeCounter;
        let id: u64 = e.storage().instance().get(&counter_key).unwrap_or(0);
        let next_id = id.checked_add(1).expect("dispute counter overflow");
        e.storage().instance().set(&counter_key, &next_id);

        let start = e.ledger().timestamp();
        let end = start.checked_add(duration).expect("duration overflow");

        let dispute = Dispute {
            id,
            creator: creator.clone(),
            description,
            voting_start: start,
            voting_end: end,
            resolved: false,
            outcome: 0,
        };

        e.storage().instance().set(&DataKey::Dispute(id), &dispute);

        e.events()
            .publish((Symbol::new(&e, "dispute_created"), id), creator);

        id
    }

    /// Cast a weighted vote for a dispute outcome.
    pub fn vote(e: Env, voter: Address, dispute_id: u64, outcome: u32) {
        pausable::require_not_paused(&e);
        voter.require_auth();

        if outcome == 0 {
            panic!("invalid outcome");
        }

        // Verify voter is a registered arbitrator
        let weight: i128 = e
            .storage()
            .instance()
            .get(&DataKey::Arbitrator(voter.clone()))
            .unwrap_or_else(|| panic!("voter is not an authorized arbitrator"));

        // Verify dispute exists and is within voting period
        let mut dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap_or_else(|| panic!("dispute not found"));

        let now = e.ledger().timestamp();
        if now < dispute.voting_start || now > dispute.voting_end {
            panic!("voting period is inactive");
        }

        if dispute.resolved {
            panic!("dispute already resolved");
        }

        // Prevent double voting
        let voter_casted_key = DataKey::VoterCasted(dispute_id, voter.clone());
        if e.storage().instance().has(&voter_casted_key) {
            panic!("arbitrator already voted on this dispute");
        }
        e.storage().instance().set(&voter_casted_key, &true);

        // Tally the vote
        let votes_key = DataKey::DisputeVotes(dispute_id);
        let mut votes: Map<u32, i128> = e
            .storage()
            .instance()
            .get(&votes_key)
            .unwrap_or(Map::new(&e));

        let current_tally = votes.get(outcome).unwrap_or(0);
        votes.set(
            outcome,
            current_tally.checked_add(weight).expect("weight overflow"),
        );

        e.storage().instance().set(&votes_key, &votes);

        e.events().publish(
            (Symbol::new(&e, "vote_cast"), dispute_id, voter),
            (outcome, weight),
        );
    }

    /// Resolve a dispute after the voting period has ended.
    pub fn resolve_dispute(e: Env, dispute_id: u64) -> u32 {
        pausable::require_not_paused(&e);
        let mut dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap_or_else(|| panic!("dispute not found"));

        if dispute.resolved {
            panic!("dispute already resolved");
        }

        let now = e.ledger().timestamp();
        if now <= dispute.voting_end {
            panic!("voting period has not ended");
        }

        let votes_key = DataKey::DisputeVotes(dispute_id);
        let votes: Map<u32, i128> = e
            .storage()
            .instance()
            .get(&votes_key)
            .unwrap_or(Map::new(&e));

        let mut winning_outcome = 0;
        let mut max_weight = -1;
        let mut is_tie = false;

        for (outcome, weight) in votes.iter() {
            if weight > max_weight {
                max_weight = weight;
                winning_outcome = outcome;
                is_tie = false;
            } else if weight == max_weight {
                is_tie = true;
            }
        }

        // If there's a tie, the outcome remains 0 (unresolved/tie)
        if is_tie {
            winning_outcome = 0;
        }

        dispute.resolved = true;
        dispute.outcome = winning_outcome;
        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        e.events().publish(
            (Symbol::new(&e, "dispute_resolved"), dispute_id),
            winning_outcome,
        );

        winning_outcome
    }

    /// Get dispute details.
    pub fn get_dispute(e: Env, dispute_id: u64) -> Dispute {
        e.storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .unwrap_or_else(|| panic!("dispute not found"))
    }

    /// Get current total weight for an outcome.
    pub fn get_tally(e: Env, dispute_id: u64, outcome: u32) -> i128 {
        let votes_key = DataKey::DisputeVotes(dispute_id);
        let votes: Map<u32, i128> = e
            .storage()
            .instance()
            .get(&votes_key)
            .unwrap_or(Map::new(&e));

        votes.get(outcome).unwrap_or(0)
    }

    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        pausable::pause(&e, &caller)
    }

    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        pausable::unpause(&e, &caller)
    }

    pub fn is_paused(e: Env) -> bool {
        pausable::is_paused(&e)
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
mod test;

#[cfg(test)]
mod test_pausable;
