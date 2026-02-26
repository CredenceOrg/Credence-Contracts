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

use soroban_sdk::{Address, Env, Symbol};

const KEY_COOLDOWN_PERIOD: &str = "cooldown_period";

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

/// Returns `true` when the cooldown window is still active (withdrawal not yet
/// permitted). A request_time of 0 means no request was made.
#[must_use]
pub fn is_cooldown_active(now: u64, request_time: u64, cooldown_period: u64) -> bool {
    if request_time == 0 {
        return false;
    }
    let end = request_time.saturating_add(cooldown_period);
    now < end
}

/// Returns `true` when a withdrawal request exists and the cooldown has fully
/// elapsed, meaning the holder may now execute.
#[must_use]
pub fn can_withdraw(now: u64, request_time: u64, cooldown_period: u64) -> bool {
    if request_time == 0 {
        return false;
    }
    let end = request_time.saturating_add(cooldown_period);
    now >= end
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
