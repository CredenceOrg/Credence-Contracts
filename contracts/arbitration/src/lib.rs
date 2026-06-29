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

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, Address, Env, Map, String, Symbol, Vec,
};

pub mod pausable;
pub mod status;

use status::ArbitrationError as Error;
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
    pub cancellation_reason: Option<String>,
    pub cancelled_by_role: Option<Symbol>,
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
    VoterCounter(u64),
    ArbitratorRegistry,
    MinTotalWeight,
    MinVoters,
}

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

#[contract]
pub struct CredenceArbitration;

#[contractimpl]
impl CredenceArbitration {
    /// Initialize the contract with an admin address.
    pub fn initialize(e: Env, admin: Address) -> Result<(), ArbitrationError> {
        bump_instance_ttl(&e);
        if e.storage().instance().has(&DataKey::Admin) {
            return Err(ArbitrationError::AlreadyInitialized);
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
        Ok(())
    }

    /// Register or update an arbitrator with a specific voting weight.
    pub fn register_arbitrator(
        e: Env,
        arbitrator: Address,
        weight: i128,
    ) -> Result<(), ArbitrationError> {
        bump_instance_ttl(&e);
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

        // Update the arbitrator registry list
        let mut registry: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::ArbitratorRegistry)
            .unwrap_or_else(|| Vec::new(&e));

        let mut exists = false;
        for addr in registry.iter() {
            if addr == arbitrator {
                exists = true;
                break;
            }
        }
        if !exists {
            registry.push_back(arbitrator.clone());
            e.storage()
                .instance()
                .set(&DataKey::ArbitratorRegistry, &registry);
        }

        e.events().publish(
            (Symbol::new(&e, "arbitrator_registered"), arbitrator),
            weight,
        );
        Ok(())
    }

    /// Remove an arbitrator.
    pub fn unregister_arbitrator(e: Env, arbitrator: Address) -> Result<(), ArbitrationError> {
        bump_instance_ttl(&e);
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

        // Update the arbitrator registry list with removal compaction
        let registry: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::ArbitratorRegistry)
            .unwrap_or_else(|| Vec::new(&e));

        let mut new_registry = Vec::new(&e);
        for addr in registry.iter() {
            if addr != arbitrator {
                new_registry.push_back(addr);
            }
        }
        e.storage()
            .instance()
            .set(&DataKey::ArbitratorRegistry, &new_registry);

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
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        creator.require_auth();

        let counter_key = DataKey::DisputeCounter;
        let id: u64 = e.storage().instance().get(&counter_key).unwrap_or(0);
        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        e.storage().instance().set(&counter_key, &next_id);

        let start = e.ledger().timestamp();
        let end = start
            .checked_add(duration)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));

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
            cancellation_reason: None,
            cancelled_by_role: None,
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
        reason: Option<String>,
    ) -> Result<(), ArbitrationError> {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        caller.require_auth();

        if let Some(r) = &reason {
            if r.len() > 256 {
                return Err(ArbitrationError::ReasonTooLong);
            }
        }

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

        let role = if caller == dispute.creator {
            Symbol::short("creator")
        } else if caller == admin {
            Symbol::short("admin")
        } else {
            return Err(ArbitrationError::NotAuthorized);
        };

        let from = dispute.status.clone();
        require_transition(from, DisputeStatus::Cancelled)?;

        dispute.status = DisputeStatus::Cancelled;
        dispute.cancellation_reason = reason.clone();
        dispute.cancelled_by_role = Some(role.clone());
        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        e.events().publish(
            (Symbol::new(&e, "dispute_cancelled"), dispute_id),
            (caller.clone(), role, reason),
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
        bump_instance_ttl(&e);
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

        // Track distinct voter count for quorum
        let voter_counter_key = DataKey::VoterCounter(dispute_id);
        let voter_count: u32 = e.storage().instance().get(&voter_counter_key).unwrap_or(0);
        e.storage().instance().set(
            &voter_counter_key,
            &voter_count
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow)),
        );

        let votes_key = DataKey::DisputeVotes(dispute_id);
        let mut votes: Map<u32, i128> = e
            .storage()
            .instance()
            .get(&votes_key)
            .unwrap_or(Map::new(&e));

        let current_tally = votes.get(outcome).unwrap_or(0);
        votes.set(
            outcome,
            current_tally
                .checked_add(weight)
                .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow)),
        );
        e.storage().instance().set(&votes_key, &votes);

        e.events().publish(
            (Symbol::new(&e, "vote_cast"), dispute_id, voter),
            (outcome, weight),
        );

        Ok(())
    }

    /// Transition Voting → Resolving → Resolved after the voting period ends.
    pub fn resolve_dispute(e: Env, dispute_id: u64) -> Result<u32, ArbitrationError> {
        bump_instance_ttl(&e);
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

        // --- Quorum check (before Resolving transition) ---
        let min_total_weight: i128 = e
            .storage()
            .instance()
            .get(&DataKey::MinTotalWeight)
            .unwrap_or(0);
        let min_voters: u32 = e.storage().instance().get(&DataKey::MinVoters).unwrap_or(0);

        if min_total_weight > 0 || min_voters > 0 {
            let votes: Map<u32, i128> = e
                .storage()
                .instance()
                .get(&DataKey::DisputeVotes(dispute_id))
                .unwrap_or(Map::new(&e));

            let mut total_weight: i128 = 0;
            for (_, w) in votes.iter() {
                total_weight = total_weight
                    .checked_add(w)
                    .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
            }

            let voter_count: u32 = e
                .storage()
                .instance()
                .get(&DataKey::VoterCounter(dispute_id))
                .unwrap_or(0);

            let weight_met = total_weight >= min_total_weight;
            let voters_met = voter_count >= min_voters;

            if !weight_met || !voters_met {
                e.events().publish(
                    (Symbol::new(&e, "quorum_not_met"), dispute_id),
                    (total_weight, min_total_weight, voter_count, min_voters),
                );
                return Err(ArbitrationError::QuorumNotMet);
            }
        }
        // --- End quorum check ---

        // Voting → Resolving
        dispute.status = DisputeStatus::Resolving;
        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (
                DisputeStatus::Voting as u32,
                DisputeStatus::Resolving as u32,
            ),
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
            (
                DisputeStatus::Resolving as u32,
                DisputeStatus::Resolved as u32,
            ),
        );
        e.events().publish(
            (Symbol::new(&e, "dispute_resolved"), dispute_id),
            winning_outcome,
        );

        Ok(winning_outcome)
    }

    /// Set quorum requirements for dispute resolution.
    ///
    /// Once set, `resolve_dispute` will reject with `QuorumNotMet` unless:
    /// - The sum of all vote weights cast ≥ `min_total_weight`
    /// - The number of distinct voters ≥ `min_voters`
    ///
    /// Default (0, 0) preserves the legacy behaviour with no quorum gate.
    pub fn set_quorum(
        e: Env,
        admin: Address,
        min_total_weight: i128,
        min_voters: u32,
    ) -> Result<(), ArbitrationError> {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ArbitrationError::NotInitialized)?;
        admin.require_auth();
        if admin != stored_admin {
            return Err(ArbitrationError::NotAdmin);
        }
        e.storage()
            .instance()
            .set(&DataKey::MinTotalWeight, &min_total_weight);
        e.storage().instance().set(&DataKey::MinVoters, &min_voters);
        e.events().publish(
            (Symbol::new(&e, "quorum_set"),),
            (min_total_weight, min_voters),
        );
        Ok(())
    }

    /// Get the current quorum configuration.
    pub fn get_quorum(e: Env) -> (i128, u32) {
        bump_instance_ttl(&e);
        let min_total_weight: i128 = e
            .storage()
            .instance()
            .get(&DataKey::MinTotalWeight)
            .unwrap_or(0);
        let min_voters: u32 = e.storage().instance().get(&DataKey::MinVoters).unwrap_or(0);
        (min_total_weight, min_voters)
    }

    /// Get dispute details.
    pub fn get_dispute(e: Env, dispute_id: u64) -> Result<Dispute, ArbitrationError> {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)
    }

    /// Get current total weight for an outcome.
    pub fn get_tally(e: Env, dispute_id: u64, outcome: u32) -> i128 {
        bump_instance_ttl(&e);
        let votes_key = DataKey::DisputeVotes(dispute_id);
        let votes: Map<u32, i128> = e
            .storage()
            .instance()
            .get(&votes_key)
            .unwrap_or(Map::new(&e));
        votes.get(outcome).unwrap_or(0)
    }

    /// Get the voting weight of a registered arbitrator.
    ///
    /// # Arguments
    /// * `e` - The Soroban environment.
    /// * `arbitrator` - The address of the arbitrator.
    ///
    /// # Returns
    /// The arbitrator's weight as `u32` if registered, or `Error::NotArbitrator` if not.
    pub fn get_arbitrator_weight(e: Env, arbitrator: Address) -> Result<u32, Error> {
        bump_instance_ttl(&e);
        let weight: i128 = e
            .storage()
            .instance()
            .get(&DataKey::Arbitrator(arbitrator))
            .ok_or(Error::NotArbitrator)?;
        Ok(weight as u32)
    }

    /// Check if a voter has already casted a vote for a specific dispute.
    ///
    /// # Arguments
    /// * `e` - The Soroban environment.
    /// * `dispute_id` - The ID of the dispute.
    /// * `voter` - The address of the voter.
    ///
    /// # Returns
    /// `true` if the voter has already voted, `false` otherwise.
    pub fn has_voted(e: Env, dispute_id: u64, voter: Address) -> bool {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .has(&DataKey::VoterCasted(dispute_id, voter))
    }

    /// Get a paginated list of registered arbitrator addresses.
    ///
    /// # Arguments
    /// * `e` - The Soroban environment.
    /// * `cursor` - The index to start pagination from (0-based).
    /// * `limit` - The maximum number of arbitrators to return.
    ///
    /// # Returns
    /// A tuple containing:
    /// 1. A page of arbitrator addresses.
    /// 2. `Some(next_cursor)` if more results remain, or `None` if pagination is complete.
    pub fn get_arbitrators_page(e: Env, cursor: u32, limit: u32) -> (Vec<Address>, Option<u32>) {
        bump_instance_ttl(&e);
        let registry: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::ArbitratorRegistry)
            .unwrap_or_else(|| Vec::new(&e));

        let registry_len = registry.len();

        if cursor >= registry_len {
            return (Vec::new(&e), None);
        }

        const MAX_ITER_HARD_CAP: u32 = 200;
        const DEFAULT_MAX_ITER: u32 = 50;

        let effective_limit = if limit == 0 {
            DEFAULT_MAX_ITER
        } else {
            limit.min(MAX_ITER_HARD_CAP)
        };

        let end = (cursor + effective_limit).min(registry_len);
        let mut page = Vec::new(&e);
        for i in cursor..end {
            if let Some(addr) = registry.get(i) {
                page.push_back(addr);
            }
        }

        let next_cursor = if end >= registry_len { None } else { Some(end) };

        (page, next_cursor)
    }

    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        bump_instance_ttl(&e);
        pausable::pause(&e, &caller)
    }

    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        bump_instance_ttl(&e);
        pausable::unpause(&e, &caller)
    }

    pub fn is_paused(e: Env) -> bool {
        bump_instance_ttl(&e);
        pausable::is_paused(&e)
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
mod test;

#[cfg(test)]
mod test_pausable;

#[cfg(test)]
mod test_lifecycle;

#[cfg(test)]
mod test_auth;
