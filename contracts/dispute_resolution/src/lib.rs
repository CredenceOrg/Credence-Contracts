#![no_std]
use interfaces::arbitration::{ArbitrationClient, ArbitrationStatus};
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

/// Binding record linking a dispute to an arbitration dispute.
///
/// Stored in persistent storage keyed by `DataKey::ArbitrationBinding(u64)` and
/// written once when an arbitration-backed dispute is created. When present it
/// makes the bound `arbitration_contract` the sole authority allowed to move the
/// dispute to `Resolved`, and it forces `resolve` to verify the arbitration
/// dispute's final ruling before accepting it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArbitrationBinding {
    /// Arbitration contract that rules on this dispute (also the dispute's
    /// `resolver`).
    pub arbitration_contract: Address,
    /// Dispute id inside the arbitration contract.
    pub arbitration_dispute_id: u64,
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

/// Read a dispute's arbitration binding, refreshing its TTL when present.
fn get_binding(e: &Env, id: u64) -> Option<ArbitrationBinding> {
    let key = DataKey::ArbitrationBinding(id);
    let binding = e
        .storage()
        .persistent()
        .get::<DataKey, ArbitrationBinding>(&key)?;
    e.storage()
        .persistent()
        .extend_ttl(&key, STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
    Some(binding)
}

/// Map a verified arbitration ruling onto this contract's resolution outcome.
///
/// Only a final `Resolved` ruling with a non-zero winning outcome is accepted.
/// Ties, no-quorum, and any non-final status are rejected with a typed error so
/// callers can distinguish the failure mode.
fn verify_ruling(status: ArbitrationStatus, outcome: u32) -> Result<(), DisputeError> {
    match status {
        ArbitrationStatus::Resolved => {
            if outcome == 0 {
                Err(DisputeError::ArbitrationNotResolved)
            } else {
                Ok(())
            }
        }
        ArbitrationStatus::Tied => Err(DisputeError::ArbitrationTied),
        ArbitrationStatus::NoQuorum => Err(DisputeError::ArbitrationNoQuorum),
        _ => Err(DisputeError::ArbitrationNotResolved),
    }
}

#[contracttype]
pub enum DataKey {
    DisputeCounter,
    Dispute(u64),
    /// Arbitration binding for a dispute, if any (`DataKey::ArbitrationBinding(id)`).
    ArbitrationBinding(u64),
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

    /// Create a new dispute whose resolver is an arbitration contract.
    ///
    /// The `creator` must authorize the call. The dispute's `resolver` is set to
    /// `arbitration_contract`, and a binding record ties the dispute to
    /// `arbitration_dispute_id` inside that contract. Only the bound arbitration
    /// contract may then call `resolve()`, which verifies the arbitration ruling
    /// before accepting it.
    ///
    /// # Events
    /// Publishes `("created", id)` → `(creator, arbitration_contract)` and
    /// `("arb_bound", id)` → `(arbitration_contract, arbitration_dispute_id)`.
    pub fn create_arbitration_dispute(
        env: Env,
        creator: Address,
        arbitration_contract: Address,
        arbitration_dispute_id: u64,
    ) -> u64 {
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
            resolver: arbitration_contract.clone(),
        };
        env.storage().persistent().set(&dispute_key, &dispute);
        bump_persistent_ttl(&env, dispute_key);

        let binding_key = DataKey::ArbitrationBinding(id);
        let binding = ArbitrationBinding {
            arbitration_contract: arbitration_contract.clone(),
            arbitration_dispute_id,
        };
        env.storage().persistent().set(&binding_key, &binding);
        bump_persistent_ttl(&env, binding_key);

        env.events().publish(
            (symbol_short!("created"), id),
            (creator, arbitration_contract.clone()),
        );
        env.events().publish(
            (symbol_short!("arb_bound"), id),
            (arbitration_contract, arbitration_dispute_id),
        );

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
    /// - `ArbitrationNotResolved` – the bound arbitration dispute has no final
    ///                        `Resolved` ruling.
    /// - `ArbitrationTied`  – the bound arbitration dispute ended in a tie.
    /// - `ArbitrationNoQuorum` – the bound arbitration dispute missed quorum.
    ///
    /// # Events
    /// Publishes `("resolved", id)` → `(creator, caller)` on success. For an
    /// arbitration-backed dispute it also publishes `("arb_res", id)` →
    /// `(arbitration_dispute_id, outcome)`.
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

        // Arbitration-backed disputes must present a verified final ruling from
        // the bound arbitration contract before they can be resolved. The ruling
        // is read through the arbitration client; a tie, a no-quorum outcome, or
        // any non-final status leaves the dispute `Open` so it can be retried.
        let mut arbitration_outcome: Option<(u64, u32)> = None;
        if let Some(binding) = get_binding(&env, id) {
            if caller != binding.arbitration_contract {
                return Err(DisputeError::Unauthorized);
            }
            let ruling = ArbitrationClient::new(&env, &binding.arbitration_contract)
                .get_dispute(&binding.arbitration_dispute_id);
            verify_ruling(ruling.status, ruling.outcome)?;
            arbitration_outcome = Some((binding.arbitration_dispute_id, ruling.outcome));
        }

        dispute.status = DisputeStatus::Resolved;
        env.storage().persistent().set(&key, &dispute);
        bump_persistent_ttl(&env, key);

        env.events().publish(
            (symbol_short!("resolved"), id),
            (dispute.creator.clone(), caller),
        );

        if let Some((arbitration_dispute_id, outcome)) = arbitration_outcome {
            env.events().publish(
                (symbol_short!("arb_res"), id),
                (arbitration_dispute_id, outcome),
            );
        }

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
