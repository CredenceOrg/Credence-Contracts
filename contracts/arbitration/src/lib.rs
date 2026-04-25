#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, String, Symbol};

pub mod pausable;
pub mod status;

use status::{require_transition, ArbitrationError, DisputeStatus};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub id: u64,
    pub creator: Address,
    pub description: String,
    pub voting_start: u64,
    pub voting_end: u64,
    /// Canonical status — replaces the old `resolved: bool`.
    pub status: DisputeStatus,
    /// Winning outcome (0 = unresolved/tie, >0 = specific outcome).
    pub outcome: u32,
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
    DisputeVotes(u64),
    VoterCasted(u64, Address),
}

#[contract]
pub struct CredenceArbitration;

#[contractimpl]
impl CredenceArbitration {
    /// Initialize the contract with an admin address.
    pub fn initialize(e: Env, admin: Address) -> Result<(), ArbitrationError> {
        if e.storage().instance().has(&DataKey::Admin) {
            return Err(ArbitrationError::AlreadyInitialized);
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Paused, &false);
        e.storage().instance().set(&DataKey::PauseSignerCount, &0_u32);
        e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
        e.storage().instance().set(&DataKey::PauseProposalCounter, &0_u64);
        Ok(())
    }

    /// Register or update an arbitrator with a specific voting weight.
    pub fn register_arbitrator(
        e: Env,
        arbitrator: Address,
        weight: i128,
    ) -> Result<(), ArbitrationError> {
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ArbitrationError::NotInitialized)?;
        admin.require_auth();

        if weight <= 0 {
            return Err(ArbitrationError::WeightNotPositive);
        }

        e.storage()
            .instance()
            .set(&DataKey::Arbitrator(arbitrator.clone()), &weight);

        e.events().publish(
            (Symbol::new(&e, "arbitrator_registered"), arbitrator),
            weight,
        );
        Ok(())
    }

    /// Remove an arbitrator.
    pub fn unregister_arbitrator(
        e: Env,
        arbitrator: Address,
    ) -> Result<(), ArbitrationError> {
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ArbitrationError::NotInitialized)?;
        admin.require_auth();

        e.storage()
            .instance()
            .remove(&DataKey::Arbitrator(arbitrator.clone()));

        e.events()
            .publish((Symbol::new(&e, "arbitrator_unregistered"), arbitrator), ());
        Ok(())
    }

    /// Create a new dispute. Status starts as Open, then immediately transitions
    /// to Voting (voting period begins at creation).
    pub fn create_dispute(
        e: Env,
        creator: Address,
        description: String,
        duration: u64,
    ) -> Result<u64, ArbitrationError> {
        pausable::require_not_paused(&e);
        creator.require_auth();

        let counter_key = DataKey::DisputeCounter;
        let id: u64 = e.storage().instance().get(&counter_key).unwrap_or(0);
        let next_id = id.checked_add(1).expect("dispute counter overflow");
        e.storage().instance().set(&counter_key, &next_id);

        let start = e.ledger().timestamp();
        let end = start.checked_add(duration).expect("duration overflow");

        // Open → Voting is the initial transition on creation
        require_transition(DisputeStatus::Open, DisputeStatus::Voting)?;

        let dispute = Dispute {
            id,
            creator: creator.clone(),
            description,
            voting_start: start,
            voting_end: end,
            status: DisputeStatus::Voting,
            outcome: 0,
        };

        e.storage().instance().set(&DataKey::Dispute(id), &dispute);

        // Lifecycle events: created + status transition
        e.events()
            .publish((Symbol::new(&e, "dispute_created"), id), creator);
        e.events().publish(
            (Symbol::new(&e, "status_transition"), id),
            (DisputeStatus::Open as u32, DisputeStatus::Voting as u32),
        );

        Ok(id)
    }

    /// Cancel a dispute. Allowed from Open or Voting by creator or admin.
    pub fn cancel_dispute(
        e: Env,
        caller: Address,
        dispute_id: u64,
    ) -> Result<(), ArbitrationError> {
        pausable::require_not_paused(&e);
        caller.require_auth();

        let mut dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;

        // Only creator or admin may cancel
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ArbitrationError::NotInitialized)?;
        if caller != dispute.creator && caller != admin {
            return Err(ArbitrationError::NotAuthorized);
        }

        let from = dispute.status.clone();
        require_transition(from, DisputeStatus::Cancelled)?;

        dispute.status = DisputeStatus::Cancelled;
        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        e.events().publish(
            (Symbol::new(&e, "dispute_cancelled"), dispute_id),
            caller,
        );
        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (from as u32, DisputeStatus::Cancelled as u32),
        );

        Ok(())
    }

    /// Cast a weighted vote for a dispute outcome.
    pub fn vote(
        e: Env,
        voter: Address,
        dispute_id: u64,
        outcome: u32,
    ) -> Result<(), ArbitrationError> {
        pausable::require_not_paused(&e);
        voter.require_auth();

        if outcome == 0 {
            return Err(ArbitrationError::InvalidOutcome);
        }

        let weight: i128 = e
            .storage()
            .instance()
            .get(&DataKey::Arbitrator(voter.clone()))
            .ok_or(ArbitrationError::NotArbitrator)?;

        let dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;

        // Must be in Voting status
        if dispute.status != DisputeStatus::Voting {
            return Err(ArbitrationError::VotingInactive);
        }

        let now = e.ledger().timestamp();
        if now < dispute.voting_start || now > dispute.voting_end {
            return Err(ArbitrationError::VotingInactive);
        }

        let voter_casted_key = DataKey::VoterCasted(dispute_id, voter.clone());
        if e.storage().instance().has(&voter_casted_key) {
            return Err(ArbitrationError::AlreadyVoted);
        }
        e.storage().instance().set(&voter_casted_key, &true);

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

        Ok(())
    }

    /// Transition Voting → Resolving → Resolved after the voting period ends.
    pub fn resolve_dispute(
        e: Env,
        dispute_id: u64,
    ) -> Result<u32, ArbitrationError> {
        pausable::require_not_paused(&e);

        let mut dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;

        // Must be Voting to start resolution
        require_transition(dispute.status.clone(), DisputeStatus::Resolving)?;

        let now = e.ledger().timestamp();
        if now <= dispute.voting_end {
            return Err(ArbitrationError::VotingNotEnded);
        }

        // Voting → Resolving
        dispute.status = DisputeStatus::Resolving;
        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (DisputeStatus::Voting as u32, DisputeStatus::Resolving as u32),
        );

        // Tally
        let votes_key = DataKey::DisputeVotes(dispute_id);
        let votes: Map<u32, i128> = e
            .storage()
            .instance()
            .get(&votes_key)
            .unwrap_or(Map::new(&e));

        let mut winning_outcome = 0u32;
        let mut max_weight: i128 = -1;
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

        if is_tie {
            winning_outcome = 0;
        }

        // Resolving → Resolved
        require_transition(DisputeStatus::Resolving, DisputeStatus::Resolved)?;
        dispute.status = DisputeStatus::Resolved;
        dispute.outcome = winning_outcome;
        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (DisputeStatus::Resolving as u32, DisputeStatus::Resolved as u32),
        );
        e.events().publish(
            (Symbol::new(&e, "dispute_resolved"), dispute_id),
            winning_outcome,
        );

        Ok(winning_outcome)
    }

    /// Get dispute details.
    pub fn get_dispute(e: Env, dispute_id: u64) -> Result<Dispute, ArbitrationError> {
        e.storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)
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

#[cfg(test)]
mod test_lifecycle;
