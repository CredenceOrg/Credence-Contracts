//! # Emergency Drain to Treasury
//!
//! Provides `emergency_drain_to_treasury`: a one-shot, timelocked, pause-gated
//! path for the admin to drain residual USDC from the bond contract to the
//! configured treasury address in a catastrophic incident.
//!
//! ## Security gates (all must pass in order)
//!
//! 1. **Paused** — the contract must be paused; draining while live is rejected
//!    with [`ContractError::EmergencyDrainNotPermitted`].
//! 2. **Timelock** — a drain ETA must have been scheduled with
//!    [`schedule_drain`] and the current ledger timestamp must be ≥ that ETA.
//!    Attempting to drain before the ETA is rejected with
//!    [`ContractError::TimelockNotReady`].
//! 3. **Admin auth** — the caller must be the stored admin and must sign the
//!    transaction.
//! 4. **Treasury recipient** — the `recipient` argument must equal the treasury
//!    address stored in the emergency config; any other recipient is rejected
//!    with a descriptive panic.
//!
//! ## Immutable audit record
//!
//! Every successful drain is persisted as a [`DrainRecord`] in persistent
//! storage under [`DrainDataKey::DrainRecord`].  Records are append-only and
//! keyed by a monotonic sequence number.
//!
//! ## Events
//!
//! A `emergency_drain` event is published on every successful execution,
//! carrying `(amount, recipient, drain_id, timestamp)`.

use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Symbol};

use crate::safe_token;

/// Minimum delay (seconds) that must elapse between scheduling and executing
/// a drain — mirrors the timelock contract's `min_delay_seconds()`.
pub const DRAIN_TIMELOCK_SECONDS: u64 = 86_400; // 24 hours

/// Storage key for the drain timelock ETA (ledger timestamp when drain becomes
/// executable).  Stored in instance storage so it is cleared on upgrade.
pub const KEY_DRAIN_ETA: &str = "drain_eta";

/// Storage key for the drain sequence counter.
pub const KEY_DRAIN_SEQ: &str = "drain_seq";

/// Persistent storage key for individual drain audit records.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DrainDataKey {
    /// Drain audit record by sequence id.
    DrainRecord(u64),
    /// Monotonic drain sequence counter.
    DrainSeq,
}

/// Immutable audit record for a single emergency drain execution.
///
/// Stored in persistent (not instance) storage so it survives contract
/// upgrades and instance TTL bumps, providing a permanent forensic trail.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrainRecord {
    /// Monotonic id, starting at 1.
    pub id: u64,
    /// USDC amount drained.
    pub amount: i128,
    /// Recipient — must equal treasury.
    pub recipient: Address,
    /// Admin who triggered the drain.
    pub admin: Address,
    /// Ledger timestamp at execution.
    pub timestamp: u64,
    /// ETA that was stored when the drain was scheduled.
    pub scheduled_eta: u64,
}

// ---------------------------------------------------------------------------
// Schedule
// ---------------------------------------------------------------------------

/// Schedule an emergency drain by recording an ETA of `now + delay`.
///
/// # Preconditions
/// - `delay` must be ≥ [`DRAIN_TIMELOCK_SECONDS`].
/// - Only callable by the stored admin (caller must `require_auth()`).
/// - Contract **must be paused** before scheduling is allowed.
///
/// # Errors
/// - Panics with `EmergencyDrainNotPermitted` when contract is not paused.
/// - Panics with `TimelockNotReady` when `delay < DRAIN_TIMELOCK_SECONDS`.
pub fn schedule_drain(e: &Env, admin: &Address, delay: u64) {
    // Gate: must be paused.
    if !crate::pausable::is_paused(e) {
        panic_with_error!(e, ContractError::EmergencyDrainNotPermitted);
    }
    // Validate delay meets minimum.
    if delay < DRAIN_TIMELOCK_SECONDS {
        panic_with_error!(e, ContractError::TimelockNotReady);
    }

    let now = e.ledger().timestamp();
    let eta = now
        .checked_add(delay)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow));

    e.storage()
        .instance()
        .set(&Symbol::new(e, KEY_DRAIN_ETA), &eta);

    e.events().publish(
        (Symbol::new(e, "drain_scheduled"), admin.clone()),
        (eta, delay),
    );
}

/// Return the currently scheduled drain ETA, or `None` if not yet scheduled.
pub fn get_drain_eta(e: &Env) -> Option<u64> {
    e.storage()
        .instance()
        .get(&Symbol::new(e, KEY_DRAIN_ETA))
}

/// Cancel a pending drain schedule (admin-only; contract must remain paused).
///
/// Removes the stored ETA.  A subsequent drain attempt will panic with
/// `EmergencyDrainNotPermitted` (no ETA found).
pub fn cancel_drain(e: &Env, admin: &Address) {
    e.storage()
        .instance()
        .remove(&Symbol::new(e, KEY_DRAIN_ETA));

    e.events()
        .publish((Symbol::new(e, "drain_cancelled"), admin.clone()), ());
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

/// Execute an emergency drain of `amount` USDC to `recipient`.
///
/// # Preconditions (checked in this order — ALL must hold)
///
/// 1. **Paused** — `is_paused(e)` must return `true`.
/// 2. **Timelock elapsed** — a drain ETA must have been stored via
///    [`schedule_drain`] and `now >= eta`.
/// 3. **Admin** — `admin` must equal the stored admin and have signed.
/// 4. **Recipient is treasury** — `recipient` must match
///    `emergency_config.treasury`.
/// 5. **Positive amount** — `amount` must be > 0.
///
/// # Returns
/// The assigned [`DrainRecord`] id.
///
/// # Errors
/// - `ContractError::EmergencyDrainNotPermitted` — not paused or no ETA scheduled.
/// - `ContractError::TimelockNotReady` — ETA has not yet been reached.
/// - Panics with "not admin" — caller is not the stored admin.
/// - Panics with "recipient must be treasury" — `recipient != treasury`.
/// - Panics with "amount must be positive" — `amount <= 0`.
#[allow(clippy::too_many_arguments)]
pub fn execute_drain(
    e: &Env,
    admin: &Address,
    amount: i128,
    recipient: &Address,
    treasury: &Address,
) -> u64 {
    // Gate 1: contract must be paused.
    if !crate::pausable::is_paused(e) {
        panic_with_error!(e, ContractError::EmergencyDrainNotPermitted);
    }

    // Gate 2: timelock must be elapsed.
    let eta: u64 = e
        .storage()
        .instance()
        .get(&Symbol::new(e, KEY_DRAIN_ETA))
        .unwrap_or_else(|| panic_with_error!(e, ContractError::EmergencyDrainNotPermitted));

    let now = e.ledger().timestamp();
    if now < eta {
        panic_with_error!(e, ContractError::TimelockNotReady);
    }

    // Gate 3: amount must be positive.
    if amount <= 0 {
        panic!("amount must be positive");
    }

    // Gate 4: recipient must be treasury.
    if recipient != treasury {
        panic!("recipient must be treasury");
    }

    // Execute token transfer.
    safe_token::safe_transfer(e, recipient, amount);

    // Clear the drain ETA so it cannot be replayed without re-scheduling.
    e.storage()
        .instance()
        .remove(&Symbol::new(e, KEY_DRAIN_ETA));

    // Persist immutable audit record.
    let drain_id = increment_drain_seq(e);
    let record = DrainRecord {
        id: drain_id,
        amount,
        recipient: recipient.clone(),
        admin: admin.clone(),
        timestamp: now,
        scheduled_eta: eta,
    };
    e.storage()
        .persistent()
        .set(&DrainDataKey::DrainRecord(drain_id), &record);

    // Emit event.
    e.events().publish(
        (
            Symbol::new(e, "emergency_drain"),
            drain_id,
            admin.clone(),
        ),
        (amount, recipient.clone(), now),
    );

    drain_id
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// Return the latest drain record id (0 when no drain has been executed yet).
pub fn latest_drain_id(e: &Env) -> u64 {
    e.storage()
        .persistent()
        .get(&DrainDataKey::DrainSeq)
        .unwrap_or(0)
}

/// Retrieve a drain record by id.  Panics when the id does not exist.
pub fn get_drain_record(e: &Env, id: u64) -> DrainRecord {
    e.storage()
        .persistent()
        .get(&DrainDataKey::DrainRecord(id))
        .unwrap_or_else(|| panic!("drain record not found"))
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn increment_drain_seq(e: &Env) -> u64 {
    let seq: u64 = e
        .storage()
        .persistent()
        .get(&DrainDataKey::DrainSeq)
        .unwrap_or(0);
    let next = seq.checked_add(1).expect("drain sequence overflow");
    e.storage()
        .persistent()
        .set(&DrainDataKey::DrainSeq, &next);
    next
}
