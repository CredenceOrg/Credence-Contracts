#![no_std]
#![deny(clippy::float_arithmetic)]
// Disallow bare format!/write!/println! macros in release/WASM builds; test
// code is excluded so diagnostics remain unrestricted.
#![cfg_attr(not(any(test, feature = "testutils")), deny(clippy::disallowed_macros))]

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, Address, Env, Map,
    String, Symbol, Vec,
};

/// Signature domain identifier for the CredenceArbitration contract.
///
/// This constant binds signatures to this specific contract, preventing
/// cross-contract replay attacks where a signature intended for one contract
/// could be replayed against another. Each contract in the Credence system
/// has a unique signature domain constant.
///
/// # Security
///
/// Without domain separation, a signature created for contract A could be
/// replayed against contract B if both contracts share the same nonce namespace
/// and signature verification logic. By including this domain in the signed
/// payload hash, we ensure signatures are only valid for their intended contract.
///
/// # Value
///
/// The domain is a human-readable string that uniquely identifies this contract
/// within the Credence system. It should be included in the signed payload hash
/// along with other payload fields (nonce, deadline, etc.).
#[allow(dead_code)]
const SIGNATURE_DOMAIN: &str = "CredenceArbitration";

pub mod pausable;
pub mod status;

use status::ArbitrationError as Error;
use status::{require_transition, ArbitrationError, DisputeStatus};

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

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
    /// Winning outcome (0 for Tied or NoQuorum, >0 = specific outcome).
    /// The outcome field must never be 0 if status == Resolved, as vote() rejects outcome 0 as InvalidOutcome.
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
    ActiveDispute(Address),
    CreatorDispute(Address, u32),
    CreatorDisputeCount(Address),
    DisputeWeightSnapshot(u64),
    /// Appeal window in seconds. Default: 86400 (24 hours).
    AppealWindow,
    /// Round-two minimum total weight. Must exceed round-one MinTotalWeight.
    AppealMinTotalWeight,
    /// Round-two minimum voter count. Must exceed round-one MinVoters.
    AppealMinVoters,
    /// Timestamp when the dispute was resolved/tied/no-quorum (for appeal window).
    ResolvedAt(u64),
    /// How many times this dispute has been appealed (max 1).
    AppealCount(u64),
    /// Set of round-one voters for this dispute (keyed by voter address → bool).
    RoundOneVoter(u64, Address),
}

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;
const PERSISTENT_TTL_EXTEND_TO: u32 = 3_110_400;
const PERSISTENT_TTL_THRESHOLD: u32 = PERSISTENT_TTL_EXTEND_TO / 2;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

fn require_no_ongoing_dispute(e: &Env, creator: &Address) -> Result<(), ArbitrationError> {
    // Prevent a creator from re-entering the dispute lifecycle while an
    // unresolved dispute remains active for the same address.
    //
    // A stale `ActiveDispute` record can outlive the underlying dispute (e.g.
    // after a prior resolved/cancelled state or storage drift). Treat that as a
    // recoverable stale state instead of permanently locking the creator out.
    let key = DataKey::ActiveDispute(creator.clone());
    let Some(active_id) = e.storage().instance().get::<DataKey, u64>(&key) else {
        return Ok(());
    };

    let Some(dispute) = e
        .storage()
        .instance()
        .get::<DataKey, Dispute>(&DataKey::Dispute(active_id))
    else {
        e.storage().instance().remove(&key);
        return Ok(());
    };

    if matches!(
        dispute.status,
        DisputeStatus::Open | DisputeStatus::Voting | DisputeStatus::Resolving
    ) {
        return Err(ArbitrationError::OngoingDispute);
    }

    e.storage().instance().remove(&key);
    Ok(())
}

fn clear_active_dispute(e: &Env, creator: &Address) {
    e.storage()
        .instance()
        .remove(&DataKey::ActiveDispute(creator.clone()));
}

/// Capture a frozen snapshot of all currently registered arbitrators and their
/// voting weights for `dispute_id` in persistent storage.
///
/// Storing the snapshot in persistent storage keeps instance storage bounded
/// across disputes. Gas cost scales linearly (`O(N)`) with the number of
/// registered arbitrators in `ArbitratorRegistry`.
fn snapshot_arbitrator_weights(e: &Env, dispute_id: u64) {
    let registry: Vec<Address> = e
        .storage()
        .instance()
        .get(&DataKey::ArbitratorRegistry)
        .unwrap_or_else(|| Vec::new(e));

    let mut snapshot: Map<Address, i128> = Map::new(e);
    for addr in registry.iter() {
        if let Some(weight) = e
            .storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::Arbitrator(addr.clone()))
        {
            snapshot.set(addr, weight);
        }
    }

    let key = DataKey::DisputeWeightSnapshot(dispute_id);
    e.storage().persistent().set(&key, &snapshot);
    e.storage().persistent().extend_ttl(
        &key,
        PERSISTENT_TTL_THRESHOLD,
        PERSISTENT_TTL_EXTEND_TO,
    );
}

/// Resolve the effective voting weight of `voter` for `dispute_id`.
///
/// - If a frozen weight snapshot exists for `dispute_id`, only arbitrators
///   present in that snapshot may vote and their snapshotted weight is used.
/// - If no snapshot exists (legacy disputes created before snapshotting was
///   introduced), falls back to the live arbitrator registry for
///   migration-safe backwards compatibility.
fn get_effective_voter_weight(
    e: &Env,
    dispute_id: u64,
    voter: &Address,
) -> Result<i128, ArbitrationError> {
    let snapshot_key = DataKey::DisputeWeightSnapshot(dispute_id);
    if let Some(snapshot) = e
        .storage()
        .persistent()
        .get::<DataKey, Map<Address, i128>>(&snapshot_key)
    {
        e.storage().persistent().extend_ttl(
            &snapshot_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND_TO,
        );
        snapshot
            .get(voter.clone())
            .ok_or(ArbitrationError::NotArbitrator)
    } else {
        e.storage()
            .instance()
            .get::<DataKey, i128>(&DataKey::Arbitrator(voter.clone()))
            .ok_or(ArbitrationError::NotArbitrator)
    }
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

        require_no_ongoing_dispute(&e, &creator)?;

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
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .set(&DataKey::ActiveDispute(creator.clone()), &id);
        snapshot_arbitrator_weights(&e, id);

        let creator_dispute_count_key = DataKey::CreatorDisputeCount(creator.clone());
        let creator_dispute_count: u32 = e
            .storage()
            .persistent()
            .get(&creator_dispute_count_key)
            .unwrap_or(0);
        let next_creator_dispute_count = creator_dispute_count
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        let creator_dispute_key = DataKey::CreatorDispute(creator.clone(), creator_dispute_count);
        e.storage().persistent().set(&creator_dispute_key, &id);
        e.storage().persistent().extend_ttl(
            &creator_dispute_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND_TO,
        );
        e.storage()
            .persistent()
            .set(&creator_dispute_count_key, &next_creator_dispute_count);
        e.storage().persistent().extend_ttl(
            &creator_dispute_count_key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND_TO,
        );

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
            symbol_short!("creator")
        } else if caller == admin {
            symbol_short!("admin")
        } else {
            return Err(ArbitrationError::NotAuthorized);
        };

        let from = dispute.status.clone();
        require_transition(from, DisputeStatus::Cancelled)?;

        // Creator may only cancel before any votes are cast and within the voting window.
        // Admin retains a moderation override to cancel a Voting dispute at any time.
        if role == symbol_short!("creator") {
            let voter_count: u32 = e
                .storage()
                .instance()
                .get(&DataKey::VoterCounter(dispute_id))
                .unwrap_or(0);
            let now = e.ledger().timestamp();
            if voter_count != 0 || now > dispute.voting_end {
                return Err(ArbitrationError::CancelNotAllowed);
            }
        }

        let from = dispute.status;
        require_transition(from, DisputeStatus::Cancelled)?;

        dispute.status = DisputeStatus::Cancelled;
        dispute.cancellation_reason = reason.clone();
        dispute.cancelled_by_role = Some(role.clone());
        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);
        bump_instance_ttl(&e);

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

        let weight: i128 = get_effective_voter_weight(&e, dispute_id, &voter)?;

        let dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;

        if voter == dispute.creator {
            return Err(ArbitrationError::ConflictOfInterest);
        }

        // Must be in Voting status
        if dispute.status != DisputeStatus::Voting {
        // Must be in Voting or Appealed status
        if dispute.status != DisputeStatus::Voting && dispute.status != DisputeStatus::Appealed {
            return Err(ArbitrationError::VotingInactive);
        }

        let now = e.ledger().timestamp();
        if now < dispute.voting_start || now > dispute.voting_end {
            return Err(ArbitrationError::VotingInactive);
        }

        // Round-two: reject arbitrators who voted in round one.
        if dispute.status == DisputeStatus::Appealed {
            let round_one_key = DataKey::RoundOneVoter(dispute_id, voter.clone());
            if e.storage().instance().has(&round_one_key) {
                return Err(ArbitrationError::ArbitratorVotedInRoundOne);
            }
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
    /// Also resolves a second round (Appealed → Resolved/Tied/NoQuorum).
    pub fn resolve_dispute(e: Env, dispute_id: u64) -> Result<u32, ArbitrationError> {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);

        let mut dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;

        // Must be Voting to start resolution
        require_transition(dispute.status, DisputeStatus::Resolving)?;
        // Must be Voting or Appealed to trigger resolution
        match dispute.status {
            DisputeStatus::Voting | DisputeStatus::Appealed => {}
            _ => return Err(ArbitrationError::InvalidTransition),
        }

        // For the standard (round-one) path, assert the Voting→Resolving transition
        // is legal before touching anything.
        if dispute.status == DisputeStatus::Voting {
            require_transition(DisputeStatus::Voting, DisputeStatus::Resolving)?;
        }

        let now = e.ledger().timestamp();
        if now <= dispute.voting_end {
            return Err(ArbitrationError::VotingNotEnded);
        }

        // --- Quorum check ---
        let min_total_weight: i128 = e
            .storage()
            .instance()
            .get(&DataKey::MinTotalWeight)
            .unwrap_or(0);
        let min_voters: u32 = e.storage().instance().get(&DataKey::MinVoters).unwrap_or(0);
        let is_appeal_round = dispute.status == DisputeStatus::Appealed;

        // --- Quorum check (use appeal quorum when in Appealed round) ---
        let (min_total_weight, min_voters) = if is_appeal_round {
            let w: i128 = e
                .storage()
                .instance()
                .get(&DataKey::AppealMinTotalWeight)
                .unwrap_or(1);
            let v: u32 = e
                .storage()
                .instance()
                .get(&DataKey::AppealMinVoters)
                .unwrap_or(1);
            (w, v)
        } else {
            let w: i128 = e
                .storage()
                .instance()
                .get(&DataKey::MinTotalWeight)
                .unwrap_or(0);
            let v: u32 = e.storage().instance().get(&DataKey::MinVoters).unwrap_or(0);
            (w, v)
        };

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
                require_transition(DisputeStatus::Voting, DisputeStatus::Resolving)?;
                dispute.status = DisputeStatus::Resolving;
                e.events().publish(
                    (Symbol::new(&e, "status_transition"), dispute_id),
                    (
                        DisputeStatus::Voting as u32,
                        DisputeStatus::Resolving as u32,
                    ),
                );

                require_transition(DisputeStatus::Resolving, DisputeStatus::NoQuorum)?;
                dispute.status = DisputeStatus::NoQuorum;
                dispute.outcome = 0;
                e.storage()
                    .instance()
                    .set(&DataKey::Dispute(dispute_id), &dispute);
                clear_active_dispute(&e, &dispute.creator);

                e.events().publish(
                    (Symbol::new(&e, "status_transition"), dispute_id),
                    (
                        DisputeStatus::Resolving as u32,
                        DisputeStatus::NoQuorum as u32,
                    ),
                );
                e.events().publish(
                    (Symbol::new(&e, "quorum_not_met"), dispute_id),
                    (total_weight, min_total_weight, voter_count, min_voters),
                );
                return Ok(0);
                // In an appeal round, quorum failure is a final NoQuorum outcome.
                if is_appeal_round {
                    let from = dispute.status;
                    require_transition(from, DisputeStatus::NoQuorum)?;
                    dispute.status = DisputeStatus::NoQuorum;
                    dispute.outcome = 0;
                    e.storage()
                        .instance()
                        .set(&DataKey::Dispute(dispute_id), &dispute);
                    clear_active_dispute(&e, &dispute.creator);
                    e.storage()
                        .instance()
                        .set(&DataKey::ResolvedAt(dispute_id), &now);
                    e.events().publish(
                        (Symbol::new(&e, "status_transition"), dispute_id),
                        (from as u32, DisputeStatus::NoQuorum as u32),
                    );
                    e.events().publish(
                        (Symbol::new(&e, "dispute_no_quorum"), dispute_id),
                        (),
                    );
                    return Ok(0);
                }
                return Err(ArbitrationError::QuorumNotMet);
            }
        }
        // --- End quorum check ---

        let prev_status = dispute.status;

        // For round-one, emit the Voting → Resolving intermediate step.
        if !is_appeal_round {
            dispute.status = DisputeStatus::Resolving;
            e.events().publish(
                (Symbol::new(&e, "status_transition"), dispute_id),
                (DisputeStatus::Voting as u32, DisputeStatus::Resolving as u32),
            );
        }

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

        // If no votes cast (max_weight never updated), treat as implicit tie
        if max_weight < 0 {
            is_tie = true;
        }

        // Resolving → Resolved
        require_transition(DisputeStatus::Resolving, DisputeStatus::Resolved)?;
        dispute.status = DisputeStatus::Resolved;
        dispute.outcome = winning_outcome;
        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);
        bump_instance_ttl(&e);
        if is_tie {
            // Resolving/Appealed → Tied
            let from_for_tie = if is_appeal_round {
                DisputeStatus::Appealed
            } else {
                DisputeStatus::Resolving
            };
            require_transition(from_for_tie, DisputeStatus::Tied)?;
            dispute.status = DisputeStatus::Tied;
            dispute.outcome = 0;
            e.storage()
                .instance()
                .set(&DataKey::Dispute(dispute_id), &dispute);
            clear_active_dispute(&e, &dispute.creator);
            e.storage()
                .instance()
                .set(&DataKey::ResolvedAt(dispute_id), &now);

            e.events().publish(
                (Symbol::new(&e, "status_transition"), dispute_id),
                (from_for_tie as u32, DisputeStatus::Tied as u32),
            );
            e.events()
                .publish((Symbol::new(&e, "dispute_tied"), dispute_id), ());

            Ok(0)
        } else {
            // Resolving/Appealed → Resolved
            let from_for_resolved = if is_appeal_round {
                DisputeStatus::Appealed
            } else {
                DisputeStatus::Resolving
            };
            require_transition(from_for_resolved, DisputeStatus::Resolved)?;
            dispute.status = DisputeStatus::Resolved;
            dispute.outcome = winning_outcome;
            e.storage()
                .instance()
                .set(&DataKey::Dispute(dispute_id), &dispute);
            clear_active_dispute(&e, &dispute.creator);
            e.storage()
                .instance()
                .set(&DataKey::ResolvedAt(dispute_id), &now);

            e.events().publish(
                (Symbol::new(&e, "status_transition"), dispute_id),
                (from_for_resolved as u32, DisputeStatus::Resolved as u32),
            );
            e.events().publish(
                (Symbol::new(&e, "dispute_resolved"), dispute_id),
                winning_outcome,
            );

            Ok(winning_outcome)
        }
    }

    /// Set quorum requirements for dispute resolution.
    ///
    /// Once set, `resolve_dispute` will finalize as `NoQuorum` unless:
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

    /// Configure the appeal window and round-two quorum.
    ///
    /// - `appeal_window_secs`: seconds after resolution during which an appeal is
    ///   allowed. Must be > 0. Default: 86 400 (24 h).
    /// - `appeal_min_total_weight`: minimum cumulative vote weight for round two.
    ///   Must be strictly greater than the current round-one `min_total_weight`.
    ///   Pass 0 to inherit a permissive default (1 more than round-one, minimum 1).
    /// - `appeal_min_voters`: minimum distinct voter count for round two. Must be
    ///   strictly greater than the current round-one `min_voters`.
    ///   Pass 0 to inherit a permissive default (1 more than round-one, minimum 1).
    pub fn set_appeal_config(
        e: Env,
        admin: Address,
        appeal_window_secs: u64,
        appeal_min_total_weight: i128,
        appeal_min_voters: u32,
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
        if appeal_window_secs == 0 {
            return Err(ArbitrationError::InvalidTransition); // repurpose closest available error; window must be > 0
        }
        // Round-two quorum must be strictly greater than round-one quorum.
        let r1_weight: i128 = e
            .storage()
            .instance()
            .get(&DataKey::MinTotalWeight)
            .unwrap_or(0);
        let r1_voters: u32 = e.storage().instance().get(&DataKey::MinVoters).unwrap_or(0);
        if appeal_min_total_weight <= r1_weight {
            return Err(ArbitrationError::WeightNotPositive);
        }
        if appeal_min_voters <= r1_voters {
            return Err(ArbitrationError::WeightNotPositive);
        }
        e.storage()
            .instance()
            .set(&DataKey::AppealWindow, &appeal_window_secs);
        e.storage()
            .instance()
            .set(&DataKey::AppealMinTotalWeight, &appeal_min_total_weight);
        e.storage()
            .instance()
            .set(&DataKey::AppealMinVoters, &appeal_min_voters);
        e.events().publish(
            (Symbol::new(&e, "appeal_config_set"),),
            (appeal_window_secs, appeal_min_total_weight, appeal_min_voters),
        );
        Ok(())
    }

    /// Get the current appeal configuration.
    /// Returns (appeal_window_secs, appeal_min_total_weight, appeal_min_voters).
    pub fn get_appeal_config(e: Env) -> (u64, i128, u32) {
        bump_instance_ttl(&e);
        let window: u64 = e
            .storage()
            .instance()
            .get(&DataKey::AppealWindow)
            .unwrap_or(86_400);
        let weight: i128 = e
            .storage()
            .instance()
            .get(&DataKey::AppealMinTotalWeight)
            .unwrap_or(1);
        let voters: u32 = e
            .storage()
            .instance()
            .get(&DataKey::AppealMinVoters)
            .unwrap_or(1);
        (window, weight, voters)
    }

    /// Mark a dispute as NoQuorum when the voting period has ended and quorum
    /// was not met. This is a one-way transition from Voting to NoQuorum.
    /// Any caller may invoke this after the voting period expires.
    pub fn no_quorum_resolve(e: Env, dispute_id: u64) -> Result<(), ArbitrationError> {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);

        let mut dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;

        if dispute.status != DisputeStatus::Voting {
            return Err(ArbitrationError::InvalidTransition);
        }

        let now = e.ledger().timestamp();
        if now <= dispute.voting_end {
            return Err(ArbitrationError::VotingNotEnded);
        }

        // Verify quorum was actually NOT met. If quorum is met, caller should
        // use resolve_dispute instead.
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

            if weight_met && voters_met {
                // Quorum is actually met; this function should not be called.
                return Err(ArbitrationError::InvalidTransition);
            }
        } else {
            // No quorum configured — cannot reach NoQuorum state.
            return Err(ArbitrationError::InvalidTransition);
        }

        require_transition(DisputeStatus::Voting, DisputeStatus::NoQuorum)?;
        dispute.status = DisputeStatus::NoQuorum;
        dispute.outcome = 0;
        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);
        clear_active_dispute(&e, &dispute.creator);

        // Record resolution timestamp for appeal window.
        e.storage()
            .instance()
            .set(&DataKey::ResolvedAt(dispute_id), &now);

        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (DisputeStatus::Voting as u32, DisputeStatus::NoQuorum as u32),
        );
        e.events().publish(
            (Symbol::new(&e, "dispute_no_quorum"), dispute_id),
            (),
        );
        Ok(())
    }

    /// Appeal a dispute. Allowed only by a party (the dispute creator).
    ///
    /// Conditions:
    /// - Status must be Resolved, Tied, or NoQuorum.
    /// - Current time must be within the appeal window after resolution.
    /// - The dispute must not have been appealed before (max one appeal).
    ///
    /// Resets voting state for a second round using the larger appeal quorum.
    /// Records round-one voters so they are excluded from round two.
    pub fn appeal_dispute(
        e: Env,
        appellant: Address,
        dispute_id: u64,
        duration: u64,
    ) -> Result<(), ArbitrationError> {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        appellant.require_auth();

        let mut dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;

        // Only the dispute creator counts as a "party".
        if appellant != dispute.creator {
            return Err(ArbitrationError::NotAParty);
        }

        // Dispute must be in an appealable status.
        match dispute.status {
            DisputeStatus::Resolved | DisputeStatus::Tied | DisputeStatus::NoQuorum => {}
            _ => return Err(ArbitrationError::NotAppealable),
        }

        // Max one appeal.
        let appeal_count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::AppealCount(dispute_id))
            .unwrap_or(0);
        if appeal_count > 0 {
            return Err(ArbitrationError::AlreadyAppealed);
        }

        // Appeal window check.
        let resolved_at: u64 = e
            .storage()
            .instance()
            .get(&DataKey::ResolvedAt(dispute_id))
            .unwrap_or(0);
        let appeal_window: u64 = e
            .storage()
            .instance()
            .get(&DataKey::AppealWindow)
            .unwrap_or(86_400);
        let window_end = resolved_at
            .checked_add(appeal_window)
            .unwrap_or(u64::MAX);
        let now = e.ledger().timestamp();
        // Inclusive upper bound: allowed while now <= window_end.
        // The window closes one second after window_end.
        if now > window_end {
            return Err(ArbitrationError::AppealWindowClosed);
        }

        // Snapshot round-one voters before clearing vote state.
        let registry: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::ArbitratorRegistry)
            .unwrap_or_else(|| Vec::new(&e));
        for addr in registry.iter() {
            let voter_key = DataKey::VoterCasted(dispute_id, addr.clone());
            if e.storage().instance().has(&voter_key) {
                e.storage()
                    .instance()
                    .set(&DataKey::RoundOneVoter(dispute_id, addr), &true);
            }
        }

        // Transition to Appealed.
        let from = dispute.status;
        require_transition(from, DisputeStatus::Appealed)?;
        dispute.status = DisputeStatus::Appealed;

        // Reset vote state for round two.
        e.storage()
            .instance()
            .remove(&DataKey::VoterCounter(dispute_id));
        e.storage()
            .instance()
            .remove(&DataKey::DisputeVotes(dispute_id));
        for addr in registry.iter() {
            let voter_casted_key = DataKey::VoterCasted(dispute_id, addr);
            e.storage().instance().remove(&voter_casted_key);
        }

        // Set new voting window.
        let new_start = now;
        let new_end = now
            .checked_add(duration)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        dispute.voting_start = new_start;
        dispute.voting_end = new_end;
        dispute.outcome = 0;

        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        // Re-establish active dispute guard: the dispute is live again.
        e.storage().instance().set(
            &DataKey::ActiveDispute(dispute.creator.clone()),
            &dispute_id,
        );

        // Increment appeal count.
        e.storage()
            .instance()
            .set(&DataKey::AppealCount(dispute_id), &(appeal_count + 1));

        let round: u32 = 2;
        e.events().publish(
            (Symbol::new(&e, "dispute_appealed"), dispute_id),
            (appellant, round, now),
        );
        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (from as u32, DisputeStatus::Appealed as u32),
        );
        Ok(())
    }

    /// Get dispute details.
    pub fn get_dispute(e: Env, dispute_id: u64) -> Result<Dispute, ArbitrationError> {
        let dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;
        bump_instance_ttl(&e);
        Ok(dispute)
    }

    /// Return the total number of disputes ever created on this contract.
    ///
    /// This is a read-only view of the internal dispute counter. It reflects
    /// every dispute regardless of its current status (Open, Closed, Cancelled).
    /// The return value monotonically increases and never decreases when disputes
    /// are cancelled.
    ///
    /// Indexers and frontends can use this to know the range of valid dispute IDs
    /// (`1..=get_dispute_count()`) without scanning storage.
    pub fn get_dispute_count(e: Env) -> u64 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::DisputeCounter)
            .unwrap_or(0)
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
    /// A `Result` containing a tuple of:
    /// 1. A page of arbitrator addresses.
    /// 2. `Some(next_cursor)` if more results remain, or `None` if pagination is complete.
    ///
    /// # Errors
    /// Returns [`ArbitrationError::CursorOutOfRange`] when `cursor >= registry_len`.
    /// This prevents callers from synthesising a completed-scan response without
    /// actually scanning any entries.
    pub fn get_arbitrators_page(
        e: Env,
        cursor: u32,
        limit: u32,
    ) -> Result<(Vec<Address>, Option<u32>), ArbitrationError> {
        bump_instance_ttl(&e);
        let registry: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::ArbitratorRegistry)
            .unwrap_or_else(|| Vec::new(&e));

        let registry_len = registry.len();

        if cursor >= registry_len {
            return Err(ArbitrationError::CursorOutOfRange);
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

        Ok((page, next_cursor))
    }

    /// Get a paginated list of dispute IDs created by an address.
    ///
    /// The cursor is a 0-based index in creation order. Each page reads at most
    /// 200 indexed entries; a zero limit uses the default of 50.
    ///
    /// Returns [`ArbitrationError::CursorOutOfRange`] when `cursor` is greater
    /// than or equal to the creator's dispute count.
    pub fn get_disputes_by_creator(
        e: Env,
        creator: Address,
        cursor: u32,
        limit: u32,
    ) -> Result<(Vec<u64>, Option<u32>), ArbitrationError> {
        bump_instance_ttl(&e);
        let count_key = DataKey::CreatorDisputeCount(creator.clone());
        let dispute_count: u32 = if let Some(count) = e.storage().persistent().get(&count_key) {
            e.storage().persistent().extend_ttl(
                &count_key,
                PERSISTENT_TTL_THRESHOLD,
                PERSISTENT_TTL_EXTEND_TO,
            );
            count
        } else {
            0
        };

        if cursor >= dispute_count {
            return Err(ArbitrationError::CursorOutOfRange);
        }

        const MAX_ITER_HARD_CAP: u32 = 200;
        const DEFAULT_MAX_ITER: u32 = 50;

        let effective_limit = if limit == 0 {
            DEFAULT_MAX_ITER
        } else {
            limit.min(MAX_ITER_HARD_CAP)
        };
        let end = cursor.saturating_add(effective_limit).min(dispute_count);
        let mut page = Vec::new(&e);
        for index in cursor..end {
            let key = DataKey::CreatorDispute(creator.clone(), index);
            if let Some(dispute_id) = e.storage().persistent().get::<_, u64>(&key) {
                e.storage().persistent().extend_ttl(
                    &key,
                    PERSISTENT_TTL_THRESHOLD,
                    PERSISTENT_TTL_EXTEND_TO,
                );
                page.push_back(dispute_id);
            }
        }

        let next_cursor = if end >= dispute_count {
            None
        } else {
            Some(end)
        };
        Ok((page, next_cursor))
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

    /// Archive a finalized dispute. Only callable by admin.
    /// Dispute must be in Resolved, Tied, NoQuorum, or Cancelled status.
    pub fn archive_dispute(
        e: Env,
        admin: Address,
        dispute_id: u64,
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
        let mut dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;
        let from = dispute.status;
        require_transition(from, DisputeStatus::Archived)?;
        dispute.status = DisputeStatus::Archived;
        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);
        e.storage()
            .persistent()
            .remove(&DataKey::DisputeWeightSnapshot(dispute_id));
        e.events().publish(
            (Symbol::new(&e, "dispute_archived"), dispute_id),
            from as u32,
        );
        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (from as u32, DisputeStatus::Archived as u32),
        );
        Ok(())
    }

    /// Reopen an archived dispute, moving it back to Voting status.
    /// Only callable by admin. Voting period must be set fresh via duration parameter.
    pub fn reopen_dispute(
        e: Env,
        admin: Address,
        dispute_id: u64,
        duration: u64,
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
        let mut dispute: Dispute = e
            .storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(ArbitrationError::DisputeNotFound)?;
        let from = dispute.status;
        require_transition(from, DisputeStatus::Voting)?;
        let now = e.ledger().timestamp();
        let new_end = now
            .checked_add(duration)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        dispute.status = DisputeStatus::Voting;
        dispute.voting_start = now;
        dispute.voting_end = new_end;
        dispute.outcome = 0;
        dispute.cancellation_reason = None;
        dispute.cancelled_by_role = None;
        e.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        // Re-establish the creator's active-dispute guard. Resolving a dispute
        // clears this marker, so a reopened dispute would otherwise leave the
        // creator free to open a second concurrent dispute, defeating the
        // one-active-dispute-per-creator invariant enforced by create_dispute.
        e.storage().instance().set(
            &DataKey::ActiveDispute(dispute.creator.clone()),
            &dispute_id,
        );

        // Clear vote tracking data for the reopened dispute
        let voter_counter_key = DataKey::VoterCounter(dispute_id);
        e.storage().instance().remove(&voter_counter_key);
        let votes_key = DataKey::DisputeVotes(dispute_id);
        e.storage().instance().remove(&votes_key);

        // Clear VoterCasted entries for all registered arbitrators
        let registry: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::ArbitratorRegistry)
            .unwrap_or_else(|| Vec::new(&e));
        for addr in registry.iter() {
            let voter_casted_key = DataKey::VoterCasted(dispute_id, addr.clone());
            e.storage().instance().remove(&voter_casted_key);
            // Also clear round-one voter markers so the reopened dispute starts clean.
            let round_one_key = DataKey::RoundOneVoter(dispute_id, addr);
            e.storage().instance().remove(&round_one_key);
        }

        // Refresh the frozen arbitrator weight snapshot for the new voting period
        snapshot_arbitrator_weights(&e, dispute_id);
        // Clear appeal metadata so the reopened dispute can be appealed again.
        e.storage()
            .instance()
            .remove(&DataKey::AppealCount(dispute_id));
        e.storage()
            .instance()
            .remove(&DataKey::ResolvedAt(dispute_id));
        e.events().publish(
            (Symbol::new(&e, "dispute_reopened"), dispute_id),
            from as u32,
        );
        e.events().publish(
            (Symbol::new(&e, "status_transition"), dispute_id),
            (from as u32, DisputeStatus::Voting as u32),
        );
        Ok(())
    }

    /// Transfer administrative control to a new address.
    ///
    /// # Invariants
    ///
    /// - Requires authorization from the current admin. Non-admin callers are
    ///   rejected before any state is mutated, so a failed transfer leaves the
    ///   previous admin fully in control.
    /// - The replacement is atomic: the old admin loses all privileges the
    ///   moment the new admin is written, and there is never an intermediate
    ///   state in which neither address holds control.
    pub fn transfer_admin(e: Env, new_admin: Address) {
        bump_instance_ttl(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ArbitrationError::NotInitialized));
        admin.require_auth();

        e.storage().instance().set(&DataKey::Admin, &new_admin);

        e.events().publish(
            (soroban_sdk::Symbol::new(&e, "admin_transferred"),),
            (admin, new_admin),
        );
    }
}

#[contractimpl]
impl Governable for CredenceArbitration {
    fn get_admin(e: Env) -> Address {
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ArbitrationError::NotInitialized))
    }

    fn set_admin(e: Env, new_admin: Address) {
        Self::transfer_admin(e, new_admin);
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_dispute_guard;

#[cfg(test)]
mod test_pausable;

#[cfg(test)]
mod test_lifecycle;

#[cfg(test)]
mod test_auth;

#[cfg(test)]
mod test_archive_reopen;

#[cfg(test)]
mod test_events_schema;

#[cfg(test)]
mod test_creator_disputes;

#[cfg(test)]
mod test_dispute_count;
mod test_appeal;
