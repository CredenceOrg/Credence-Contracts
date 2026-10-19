//! Cooldown Window Mechanism
//!
//! Enforces a configurable delay between a withdrawal request and the actual
//! withdrawal execution. This prevents instant fund extraction and gives the
//! protocol time to detect and respond to malicious activity.
//!
//! The flow is:
//!   1. Admin sets a cooldown period via `set_cooldown_period`.
//!   2. A bond holder calls `request_cooldown_withdrawal` to signal intent.
//!   3. After the cooldown period elapses, the holder calls
//!      `execute_cooldown_withdrawal` to finalize the withdrawal.
//!   4. At any point before execution, the holder may cancel via
//!      `cancel_cooldown`.
//!
//! ## Sequencing protection
//!
//! The guard in `same_ledger_liquidation_guard` records the ledger sequence
//! whenever collateral is increased (via `create_bond` or `top_up`).
//! `execute_cooldown_withdrawal` calls
//! `require_cooldown_allowed_after_collateral_increase`, which panics if the
//! current ledger sequence still matches the recorded one.  This prevents an
//! attacker from sandwiching a collateral increase and a cooldown withdrawal
//! within the same ledger entry.
//!
//! `request_cooldown_withdrawal` also records the current ledger sequence in
//! `DataKey::CooldownRequestLedger` so that a subsequent same-ledger execution
//! can be correlated.  The recorder is written *after* the request is stored
//! so that a panicking `assert_self_consistent` call still rolls it back.

use crate::DataKey;
use soroban_sdk::{contracttype, Address, Env, Symbol};

const KEY_COOLDOWN_PERIOD: &str = "cooldown_period";
const KEY_COOLDOWN_REQUEST: &str = "cooldown_request";

/// Cooldown withdrawal request state stored per-identity.
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct CooldownRequest {
    /// Address of the bond holder who requested the withdrawal.
    pub requester: Address,
    /// Amount requested for withdrawal.
    pub amount: i128,
    /// Ledger timestamp when the request was made.
    pub requested_at: u64,
    /// Ledger sequence at which the request was recorded (used for
    /// same-ledger sequencing guard).
    pub ledger_sequence: u32,
}

/// Store the cooldown period (seconds). Caller is responsible for admin checks.
pub fn set_cooldown_period(e: &Env, period: u64) {
    e.storage()
        .instance()
        .set(&Symbol::new(e, KEY_COOLDOWN_PERIOD), &period);
}

/// Read the configured cooldown period. Returns 0 if unset.
pub fn get_cooldown_period(e: &Env) -> u64 {
    e.storage()
        .instance()
        .get::<_, u64>(&Symbol::new(e, KEY_COOLDOWN_PERIOD))
        .unwrap_or(0)
}

/// Compute the inclusive deadline for a cooldown request.
/// The boundary itself is treated as the moment the cooldown has elapsed:
/// `now == request_time + cooldown_period` allows execution.
#[must_use]
pub(crate) fn cooldown_deadline(request_time: u64, cooldown_period: u64) -> u64 {
    request_time.saturating_add(cooldown_period)
}

/// Returns `true` when the cooldown window is still active (withdrawal not yet
/// permitted). A request_time of 0 means no request was made.
#[must_use]
#[allow(dead_code)]
pub fn is_cooldown_active(now: u64, request_time: u64, cooldown_period: u64) -> bool {
    if request_time == 0 {
        return false;
    }
    let end = cooldown_deadline(request_time, cooldown_period);
    now < end
}

/// Returns `true` when a withdrawal request exists and the cooldown has fully
/// elapsed, meaning the holder may now execute.
#[must_use]
pub fn can_withdraw(now: u64, request_time: u64, cooldown_period: u64) -> bool {
    if request_time == 0 {
        return false;
    }
    let end = cooldown_deadline(request_time, cooldown_period);
    now >= end
}

/// Persist a cooldown withdrawal request.
pub fn set_cooldown_request(e: &Env, identity: &Address, request: &CooldownRequest) {
    e.storage()
        .instance()
        .set(&DataKey::CooldownRequest(identity.clone()), request);
}

/// Read the stored cooldown request for `identity`, if any.
pub fn get_cooldown_request(e: &Env, identity: &Address) -> Option<CooldownRequest> {
    e.storage()
        .instance()
        .get::<_, CooldownRequest>(&DataKey::CooldownRequest(identity.clone()))
}

/// Remove the stored cooldown request for `identity`.
pub fn clear_cooldown_request(e: &Env, identity: &Address) {
    e.storage()
        .instance()
        .remove(&DataKey::CooldownRequest(identity.clone()));
}

/// Record the current ledger sequence for cooldown sequencing guard.
pub fn record_cooldown_request(e: &Env) {
    let seq = e.ledger().sequence();
    e.storage()
        .instance()
        .set(&DataKey::CooldownRequestLedger, &seq);
}

/// Emit an event when a cooldown withdrawal is requested.
pub fn emit_cooldown_requested(e: &Env, requester: &Address, amount: i128) {
    e.events().publish(
        (Symbol::new(e, "cooldown_requested"),),
        (requester.clone(), amount),
    );
}

/// Emit an event when a cooldown withdrawal is executed.
pub fn emit_cooldown_executed(e: &Env, requester: &Address, amount: i128) {
    e.events().publish(
        (Symbol::new(e, "cooldown_executed"),),
        (requester.clone(), amount),
    );
}

/// Emit an event when a cooldown withdrawal is cancelled.
pub fn emit_cooldown_cancelled(e: &Env, requester: &Address) {
    e.events()
        .publish((Symbol::new(e, "cooldown_cancelled"),), requester.clone());
}

/// Emit an event when the cooldown period is updated by the admin.
pub fn emit_cooldown_period_updated(e: &Env, old_period: u64, new_period: u64) {
    e.events().publish(
        (Symbol::new(e, "cooldown_period_updated"),),
        (old_period, new_period),
    );
}
