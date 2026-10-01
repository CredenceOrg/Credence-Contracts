#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env};

mod error;
pub use error::DisputeError;

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
/// Stored in instance storage keyed by the dispute `id`.  All fields are
/// written once at creation and then updated only through the guarded entry
/// points (`resolve`, `close`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub id: u64,
    pub status: DisputeStatus,
    pub resolver: Address,
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
    /// The `resolver` is the only address that may subsequently call
    /// `resolve()` or `close()`.  No authorization is required from the
    /// *creator*; the caller simply names who will arbitrate the dispute.
    ///
    /// # Events
    /// Publishes `("created", id)` → `("resolver", resolver)` on success.
    pub fn create_dispute(env: Env, resolver: Address) -> u64 {
        let id: u64 = env.prng().gen();
        let dispute = Dispute {
            id,
            status: DisputeStatus::Open,
            resolver: resolver.clone(),
        };
        env.storage().instance().set(&id, &dispute);

        env.events().publish(
            (symbol_short!("created"), id),
            (symbol_short!("resolver"), resolver),
        );

        id
    }

    /// Mark an `Open` dispute as `Resolved`.
    ///
    /// Only the named `resolver` may call this.  A dispute that is already
    /// `Resolved` or `Closed` cannot be re-resolved.
    ///
    /// # Errors
    /// - `DisputeNotFound`  – no dispute exists with the given `id`.
    /// - `AlreadyClosed`    – the dispute is already in a terminal state
    ///                        (`Resolved` or `Closed`).
    /// - `Unauthorized`     – `caller` is not the dispute's resolver.
    ///
    /// # Events
    /// Publishes `("resolved", id)` → `("by", caller)` on success.
    pub fn resolve(env: Env, caller: Address, id: u64) -> Result<(), DisputeError> {
        caller.require_auth();

        let mut dispute = Self::get_dispute(env.clone(), id)?;

        // Terminal-state guard: both Resolved and Closed are terminal for this
        // transition so that idempotent callers get a clear error rather than
        // silently re-resolving.
        if dispute.status != DisputeStatus::Open {
            return Err(DisputeError::AlreadyClosed);
        }

        if caller != dispute.resolver {
            return Err(DisputeError::Unauthorized);
        }

        dispute.status = DisputeStatus::Resolved;
        env.storage().instance().set(&id, &dispute);

        env.events().publish(
            (symbol_short!("resolved"), id),
            (symbol_short!("by"), caller),
        );

        Ok(())
    }

    /// Mark a dispute as `Closed` (terminal state).
    ///
    /// Accepts disputes in either `Open` or `Resolved` state so that callers
    /// can skip the intermediate resolve step if needed.  Once `Closed` the
    /// record is immutable.
    ///
    /// # Errors
    /// - `DisputeNotFound`  – no dispute exists with the given `id`.
    /// - `AlreadyClosed`    – the dispute is already `Closed`.
    /// - `Unauthorized`     – `caller` is not the dispute's resolver.
    ///
    /// # Events
    /// Publishes `("closed", id)` → `("by", caller)` on success.
    pub fn close(env: Env, caller: Address, id: u64) -> Result<(), DisputeError> {
        caller.require_auth();

        let mut dispute = Self::get_dispute(env.clone(), id)?;

        // Invariant: no double-close.
        if dispute.status == DisputeStatus::Closed {
            return Err(DisputeError::AlreadyClosed);
        }

        // Invariant: only the named resolver may close.
        if caller != dispute.resolver {
            return Err(DisputeError::Unauthorized);
        }

        dispute.status = DisputeStatus::Closed;
        env.storage().instance().set(&id, &dispute);

        env.events()
            .publish((symbol_short!("closed"), id), (symbol_short!("by"), caller));

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
        env.storage()
            .instance()
            .get(&id)
            .ok_or(DisputeError::DisputeNotFound)
    }
}
