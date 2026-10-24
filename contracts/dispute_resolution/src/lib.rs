#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};

mod error;
pub use error::DisputeError;

#[cfg(test)]
mod test_dispute_error;

/// Lifecycle state of a dispute.
///
/// Invariants:
/// - A dispute starts in `Open`.
/// - `Open` → `Resolved` via `resolve()` (resolver only).
/// - `Open` or `Resolved` → `Closed`  via `close()` (resolver only).
/// - Once `Closed` the record is terminal; no further transitions are allowed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open,
    Resolved,
    Closed,
}

/// On-ledger dispute record.
///
/// Stored in persistent storage keyed by `DataKey::Dispute(u64)`. All fields
/// are written once at creation and then updated only through the guarded entry
/// points (`resolve`, `close`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub id: u64,
    pub creator: Address,
    pub status: DisputeStatus,
    pub resolver: Address,
}

const STORAGE_TTL_EXTEND_TO: u32 = 3600;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

fn bump_persistent_ttl(e: &Env, key: DataKey) {
    e.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

#[contracttype]
pub enum DataKey {
    DisputeCounter,
    Dispute(u64),
}

#[contract]
pub struct DisputeResolutionContract;

#[contractimpl]
impl DisputeResolutionContract {
    // -----------------------------------------------------------------------
    // Write entry points
    // -----------------------------------------------------------------------

    /// Create a new dispute and return its unique `u64` ID.
    ///
    /// The `creator` must authorize the call. The `resolver` is the only
    /// address that may subsequently call `resolve()` or `close()`.
    ///
    /// IDs are assigned from a monotonically increasing counter.
    ///
    /// # Events
    /// Publishes `("created", id)` → `(creator, resolver)` on success.
    pub fn create_dispute(env: Env, creator: Address, resolver: Address) -> u64 {
        bump_instance_ttl(&env);
        creator.require_auth();

        let counter_key = DataKey::DisputeCounter;
        let id: u64 = env.storage().instance().get(&counter_key).unwrap_or(0);
        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(DisputeError::DisputeNotFound));
        env.storage().instance().set(&counter_key, &next_id);

        // Defensive: ensure we never overwrite an existing dispute.
        let dispute_key = DataKey::Dispute(id);
        if env.storage().persistent().has(&dispute_key) {
            env.panic_with_error(DisputeError::DisputeNotFound);
        }

        let dispute = Dispute {
            id,
            creator: creator.clone(),
            status: DisputeStatus::Open,
            resolver: resolver.clone(),
        };

        // Store in persistent storage with TTL extension.
        env.storage().persistent().set(&dispute_key, &dispute);
        bump_persistent_ttl(&env, dispute_key);

        env.events()
            .publish((symbol_short!("created"), id), (creator, resolver));

        id
    }

    /// Mark an `Open` dispute as `Resolved`.
    ///
    /// Only the named `resolver` may call this. A dispute that is already
    /// `Resolved` or `Closed` cannot be re-resolved.
    ///
    /// # Errors
    /// - `DisputeNotFound`  – no dispute exists with the given `id`.
    /// - `AlreadyClosed`    – the dispute is already in a terminal state
    ///                        (`Resolved` or `Closed`).
    /// - `Unauthorized`     – `caller` is not the dispute's resolver.
    ///
    /// # Events
    /// Publishes `("resolved", id)` → `(creator, caller)` on success.
    pub fn resolve(env: Env, caller: Address, id: u64) -> Result<(), DisputeError> {
        bump_instance_ttl(&env);
        caller.require_auth();

        let key = DataKey::Dispute(id);
        let mut dispute = Self::get_dispute(env.clone(), id)?;

        // Authorization check BEFORE status check: a non-resolver always gets
        // Unauthorized, even on closed disputes.
        if caller != dispute.resolver {
            return Err(DisputeError::Unauthorized);
        }

        // Terminal-state guard: both Resolved and Closed are terminal for this
        // transition so that idempotent callers get a clear error rather than
        // silently re-resolving.
        if dispute.status != DisputeStatus::Open {
            return Err(DisputeError::AlreadyClosed);
        }

        dispute.status = DisputeStatus::Resolved;
        env.storage().persistent().set(&key, &dispute);
        bump_persistent_ttl(&env, key);

        env.events().publish(
            (symbol_short!("resolved"), id),
            (dispute.creator.clone(), caller),
        );

        Ok(())
    }

    /// Mark a dispute as `Closed` (terminal state).
    ///
    /// Accepts disputes in either `Open` or `Resolved` state so that callers
    /// can skip the intermediate resolve step if needed. Once `Closed` the
    /// record is immutable.
    ///
    /// # Errors
    /// - `DisputeNotFound`  – no dispute exists with the given `id`.
    /// - `AlreadyClosed`    – the dispute is already `Closed`.
    /// - `Unauthorized`     – `caller` is not the dispute's resolver.
    ///
    /// # Events
    /// Publishes `("closed", id)` → `(creator, caller)` on success.
    pub fn close(env: Env, caller: Address, id: u64) -> Result<(), DisputeError> {
        bump_instance_ttl(&env);
        caller.require_auth();

        let key = DataKey::Dispute(id);
        let mut dispute = Self::get_dispute(env.clone(), id)?;

        // Authorization check BEFORE status check: a non-resolver always gets
        // Unauthorized, even on closed disputes.
        if caller != dispute.resolver {
            return Err(DisputeError::Unauthorized);
        }

        // Invariant: no double-close.
        if dispute.status == DisputeStatus::Closed {
            return Err(DisputeError::AlreadyClosed);
        }

        dispute.status = DisputeStatus::Closed;
        env.storage().persistent().set(&key, &dispute);
        bump_persistent_ttl(&env, key);

        env.events().publish(
            (symbol_short!("closed"), id),
            (dispute.creator.clone(), caller),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read entry points
    // -----------------------------------------------------------------------

    /// Fetch a dispute by ID.
    ///
    /// # Errors
    /// - `DisputeNotFound` – no dispute exists with the given `id`.
    pub fn get_dispute(env: Env, id: u64) -> Result<Dispute, DisputeError> {
        bump_instance_ttl(&env);
        let key = DataKey::Dispute(id);
        let dispute = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(DisputeError::DisputeNotFound)?;
        bump_persistent_ttl(&env, key);
        Ok(dispute)
    }
}
