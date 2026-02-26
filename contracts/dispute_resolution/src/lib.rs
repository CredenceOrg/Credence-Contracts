//! # Dispute Resolution Contract
//!
//! Manages on-chain disputes raised against slash requests.
//!
//! ## Storage Layout
//!
//! | Key                          | Tier         | Lifecycle      |
//! |------------------------------|--------------|----------------|
//! | `DataKey::DisputeCounter`    | `instance()` | Entire contract|
//! | `DataKey::Dispute(id)`       | `persistent()`| Per dispute   |
//! | `DataKey::Vote(id, address)` | `persistent()`| Per vote      |
//!
//! **Why two tiers?**
//! `instance()` storage shares the contract's rent TTL and is intended for a
//! small, bounded set of global values (here: a single u64 counter).
//! `persistent()` storage is independently rentable — each dispute and each
//! vote has its own TTL that can be bumped cheaply, preventing unbounded
//! growth of the instance footprint.

#![no_std]
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env,
};

// ─── TTL constants ────────────────────────────────────────────────────────────

/// Minimum ledger sequence TTL before a bump is requested (~1 day at 5 s/ledger).
const BUMP_THRESHOLD: u32 = 17_280;
/// Target TTL after a bump (~30 days).
const BUMP_TARGET: u32 = 518_400;

// ─── Storage keys ─────────────────────────────────────────────────────────────

/// Keys for each logical piece of contract state.
///
/// * `DisputeCounter` lives in `instance()` — one entry, tiny, always needed.
/// * `Dispute(id)` and `Vote(id, addr)` live in `persistent()` — unbounded
///   sets that must not bloat the instance footprint.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Global monotonically increasing dispute counter. Stored in `instance()`.
    DisputeCounter,
    /// Full dispute record keyed by its ID. Stored in `persistent()`.
    Dispute(u64),
    /// Boolean vote record keyed by (dispute_id, arbitrator). Stored in `persistent()`.
    Vote(u64, Address),
}

// ─── Domain types ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum DisputeStatus {
    Open,
    Resolved,
    Rejected,
    Expired,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum DisputeOutcome {
    None,
    FavorDisputer,
    FavorSlasher,
}

#[contracterror]
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    DisputeNotFound = 1,
    AlreadyVoted = 2,
    DisputeNotOpen = 3,
    DeadlineNotReached = 4,
    DeadlineExpired = 5,
    Unauthorized = 6,
    InsufficientStake = 7,
    InvalidDeadline = 8,
    TransferFailed = 9,
}

// ─── Events ───────────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeCreated {
    pub dispute_id: u64,
    pub disputer: Address,
    pub slash_request_id: u64,
    pub stake: i128,
    pub deadline: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoteCast {
    pub dispute_id: u64,
    pub arbitrator: Address,
    pub favor_disputer: bool,
}

#[contractevent]
#[derive(Clone, Debug, PartialEq)]
pub struct DisputeResolved {
    pub dispute_id: u64,
    pub outcome: DisputeOutcome,
    pub votes_for_disputer: u64,
    pub votes_for_slasher: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeExpired {
    pub dispute_id: u64,
    pub expired_at: u64,
}

// ─── Data structures ──────────────────────────────────────────────────────────

/// A single dispute record.
///
/// **Note:** The `id` field was removed — it was redundant because the dispute
/// ID is already used as the `DataKey::Dispute(id)` storage key. Callers that
/// need the ID already hold it as a local variable or return value.
#[derive(Clone)]
#[contracttype]
pub struct Dispute {
    pub disputer: Address,
    pub slash_request_id: u64,
    pub stake: i128,
    pub token: Address,
    pub status: DisputeStatus,
    pub outcome: DisputeOutcome,
    pub deadline: u64,
    pub votes_for_disputer: u64,
    pub votes_for_slasher: u64,
    pub created_at: u64,
}

// ─── Constants ────────────────────────────────────────────────────────────────

/// Minimum token amount required to open a dispute.
pub const MIN_STAKE: i128 = 100;

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct DisputeContract;

#[contractimpl]
impl DisputeContract {
    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Read a `Dispute` from `persistent()` storage, bump its TTL, and return
    /// it — or return `Err(Error::DisputeNotFound)` without a panic.
    ///
    /// Using a single helper eliminates the anti-pattern of calling `.has()`
    /// followed by `.get()`, which would hit persistent storage twice.
    fn load_dispute(env: &Env, dispute_id: u64) -> Result<Dispute, Error> {
        let key = DataKey::Dispute(dispute_id);
        let storage = env.storage().persistent();
        let dispute: Dispute = storage.get(&key).ok_or(Error::DisputeNotFound)?;
        storage.extend_ttl(&key, BUMP_THRESHOLD, BUMP_TARGET);
        Ok(dispute)
    }

    /// Persist a `Dispute` back to `persistent()` storage and bump its TTL.
    fn save_dispute(env: &Env, dispute_id: u64, dispute: &Dispute) {
        let key = DataKey::Dispute(dispute_id);
        env.storage().persistent().set(&key, dispute);
        env.storage()
            .persistent()
            .extend_ttl(&key, BUMP_THRESHOLD, BUMP_TARGET);
    }

    // ── Public interface ──────────────────────────────────────────────────────

    /// Open a new dispute against a slash request.
    ///
    /// The disputer's `stake` is transferred from their account to the contract
    /// and held until the dispute is resolved or expired.
    ///
    /// # Errors
    /// * `InsufficientStake` — `stake < MIN_STAKE`
    /// * `InvalidDeadline` — `resolution_deadline == 0`
    pub fn create_dispute(
        env: Env,
        disputer: Address,
        slash_request_id: u64,
        stake: i128,
        token: Address,
        resolution_deadline: u64,
    ) -> Result<u64, Error> {
        disputer.require_auth();

        if stake < MIN_STAKE {
            return Err(Error::InsufficientStake);
        }

        if resolution_deadline == 0 {
            return Err(Error::InvalidDeadline);
        }

        let current_time = env.ledger().timestamp();
        let deadline = current_time + resolution_deadline;

        // Transfer stake into the contract — one storage-read-free cross-contract call.
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        let contract_address = env.current_contract_address();
        token_client.transfer_from(&contract_address, &disputer, &contract_address, &stake);

        // Increment the global counter (instance storage — always loaded with the contract).
        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeCounter)
            .unwrap_or(0);
        let dispute_id = counter + 1;
        env.storage()
            .instance()
            .set(&DataKey::DisputeCounter, &dispute_id);

        // Write the dispute record to persistent storage with a fresh TTL.
        let dispute = Dispute {
            disputer: disputer.clone(),
            slash_request_id,
            stake,
            token,
            status: DisputeStatus::Open,
            outcome: DisputeOutcome::None,
            deadline,
            votes_for_disputer: 0,
            votes_for_slasher: 0,
            created_at: current_time,
        };
        Self::save_dispute(&env, dispute_id, &dispute);

        DisputeCreated {
            dispute_id,
            disputer,
            slash_request_id,
            stake,
            deadline,
        }
        .publish(&env);

        Ok(dispute_id)
    }

    /// Retrieve a dispute record by ID.
    ///
    /// Panics with `"Dispute not found"` if the ID does not exist, preserving
    /// the original public API contract expected by callers and tests.
    pub fn get_dispute(env: &Env, dispute_id: u64) -> Dispute {
        Self::load_dispute(env, dispute_id).expect("Dispute not found")
    }

    /// Cast an arbitrator vote on an open dispute.
    ///
    /// # Errors
    /// * `DisputeNotFound` — unknown `dispute_id`
    /// * `DisputeNotOpen` — dispute is no longer accepting votes
    /// * `DeadlineExpired` — voting period has closed
    /// * `AlreadyVoted` — `arbitrator` has already cast a vote on this dispute
    pub fn cast_vote(
        env: Env,
        arbitrator: Address,
        dispute_id: u64,
        favor_disputer: bool,
    ) -> Result<(), Error> {
        arbitrator.require_auth();

        // Single persistent-storage read: load-or-error (replaces has() + get()).
        let mut dispute = Self::load_dispute(&env, dispute_id)?;

        if dispute.status != DisputeStatus::Open {
            return Err(Error::DisputeNotOpen);
        }

        if env.ledger().timestamp() > dispute.deadline {
            return Err(Error::DeadlineExpired);
        }

        let vote_key = DataKey::Vote(dispute_id, arbitrator.clone());
        let vote_storage = env.storage().persistent();

        if vote_storage.has(&vote_key) {
            return Err(Error::AlreadyVoted);
        }

        // Record the vote in persistent storage with a fresh TTL.
        vote_storage.set(&vote_key, &favor_disputer);
        vote_storage.extend_ttl(&vote_key, BUMP_THRESHOLD, BUMP_TARGET);

        if favor_disputer {
            dispute.votes_for_disputer += 1;
        } else {
            dispute.votes_for_slasher += 1;
        }

        // Persist updated vote tallies back to the dispute record.
        Self::save_dispute(&env, dispute_id, &dispute);

        VoteCast {
            dispute_id,
            arbitrator,
            favor_disputer,
        }
        .publish(&env);

        Ok(())
    }

    /// Resolve a dispute after its deadline has passed.
    ///
    /// Whichever side holds the majority vote wins. On a `FavorDisputer`
    /// outcome the staked tokens are returned to the disputer; otherwise they
    /// remain in the contract (forfeited to the slasher side).
    ///
    /// # Errors
    /// * `DisputeNotFound` — unknown `dispute_id`
    /// * `DisputeNotOpen` — dispute is already resolved/expired
    /// * `DeadlineNotReached` — voting period is still active
    pub fn resolve_dispute(env: Env, dispute_id: u64) -> Result<(), Error> {
        let mut dispute = Self::load_dispute(&env, dispute_id)?;

        if dispute.status != DisputeStatus::Open {
            return Err(Error::DisputeNotOpen);
        }

        if env.ledger().timestamp() <= dispute.deadline {
            return Err(Error::DeadlineNotReached);
        }

        let token_client = soroban_sdk::token::Client::new(&env, &dispute.token);
        let contract_address = env.current_contract_address();

        let outcome = if dispute.votes_for_disputer > dispute.votes_for_slasher {
            token_client.transfer(&contract_address, &dispute.disputer, &dispute.stake);
            DisputeOutcome::FavorDisputer
        } else {
            DisputeOutcome::FavorSlasher
        };

        dispute.status = DisputeStatus::Resolved;
        dispute.outcome = outcome.clone();

        Self::save_dispute(&env, dispute_id, &dispute);

        DisputeResolved {
            dispute_id,
            outcome,
            votes_for_disputer: dispute.votes_for_disputer,
            votes_for_slasher: dispute.votes_for_slasher,
        }
        .publish(&env);

        Ok(())
    }

    /// Mark a dispute as `Expired` when no arbitrators resolved it after the
    /// deadline.
    ///
    /// # Errors
    /// * `DisputeNotFound` — unknown `dispute_id`
    /// * `DisputeNotOpen` — dispute is already resolved/expired
    /// * `DeadlineNotReached` — deadline has not yet passed
    pub fn expire_dispute(env: Env, dispute_id: u64) -> Result<(), Error> {
        let mut dispute = Self::load_dispute(&env, dispute_id)?;

        if dispute.status != DisputeStatus::Open {
            return Err(Error::DisputeNotOpen);
        }

        if env.ledger().timestamp() <= dispute.deadline {
            return Err(Error::DeadlineNotReached);
        }

        dispute.status = DisputeStatus::Expired;

        Self::save_dispute(&env, dispute_id, &dispute);

        DisputeExpired {
            dispute_id,
            expired_at: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    /// Returns `true` if `arbitrator` has already cast a vote on `dispute_id`.
    pub fn has_voted(env: Env, dispute_id: u64, arbitrator: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Vote(dispute_id, arbitrator))
    }

    /// Returns the total number of disputes ever created (monotonically
    /// increasing; IDs start at 1).
    pub fn get_dispute_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::DisputeCounter)
            .unwrap_or(0)
    }
}

mod test;
#[cfg(test)]
mod test_gas;
