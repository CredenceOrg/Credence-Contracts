//! Early Exit Penalty Mechanism
//!
//! Charges a configurable fee when users withdraw before the lock-up period ends.
//! Penalty is proportional to remaining lock time and is transferred to the treasury.

use soroban_sdk::{Address, Env, Symbol};

/// Storage key for treasury address.
const KEY_TREASURY: &str = "treasury";
/// Storage key for early exit penalty rate in basis points (e.g. 500 = 5%).
const KEY_PENALTY_BPS: &str = "early_exit_penalty_bps";

/// Returns (treasury, penalty_bps). Panics if config not set.
pub fn get_config(e: &Env) -> (Address, u32) {
    let treasury = e.storage().instance().get::<_, Address>(&Symbol::new(e, KEY_TREASURY))
        .unwrap_or_else(|| panic!("early exit config not set"));
    let bps = e.storage().instance().get::<_, u32>(&Symbol::new(e, KEY_PENALTY_BPS))
        .unwrap_or_else(|| panic!("early exit penalty bps not set"));
    (treasury, bps)
}

/// Set early exit config. Only admin should call (enforced by caller).
pub fn set_config(e: &Env, treasury: Address, penalty_bps: u32) {
    if penalty_bps > 10_000 {
        panic!("penalty_bps must be <= 10000 (100%)");
    }
    e.storage().instance().set(&Symbol::new(e, KEY_TREASURY), &treasury);
    e.storage().instance().set(&Symbol::new(e, KEY_PENALTY_BPS), &penalty_bps);
}

/// Calculate early exit penalty based on remaining lock time.
/// penalty = (amount * penalty_bps / 10000) * remaining_time / total_duration
/// Uses integer math to avoid overflow: (amount * penalty_bps / 10000) * remaining_time / total_duration
#[must_use]
pub fn calculate_penalty(
    amount: i128,
    remaining_time: u64,
    total_duration: u64,
    penalty_bps: u32,
) -> i128 {
    if total_duration == 0 || penalty_bps == 0 {
        return 0;
    }
    let base = amount.checked_mul(penalty_bps as i128).unwrap_or(0) / 10_000;
    let penalty = (base * (remaining_time as i128)) / (total_duration as i128);
    penalty
}

/// Emit early exit penalty event.
pub fn emit_penalty_event(
    e: &Env,
    identity: &Address,
    withdraw_amount: i128,
    penalty_amount: i128,
    treasury: &Address,
) {
    e.events().publish(
        (Symbol::new(e, "early_exit_penalty"),),
        (identity.clone(), withdraw_amount, penalty_amount, treasury.clone()),
    );
}
