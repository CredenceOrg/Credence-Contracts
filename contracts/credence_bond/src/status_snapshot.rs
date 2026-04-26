//! Bond Status Snapshot
//!
//! Read-only helper returning a stable, backend-friendly snapshot of the current
//! bond state: tier, cooldown remaining, and emergency mode flag.

use soroban_sdk::{contracttype, Env};

use crate::{tiered_bond, BondTier, DataKey};

/// Stable snapshot of bond status for backend consumption.
///
/// All fields are safe to serialize and index. The struct is intentionally
/// flat so backends can ingest it without further joins.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BondStatusSnapshot {
    /// Current tier derived from `bonded_amount`.
    pub tier: BondTier,
    /// Seconds remaining in the cooldown window, or 0 if no request is pending
    /// or the cooldown has already elapsed.
    pub cooldown_remaining_secs: u64,
    /// Whether emergency mode is currently enabled.
    pub emergency_mode: bool,
    /// Net available balance (`bonded_amount - slashed_amount`).
    pub available_balance: i128,
    /// Ledger timestamp at which the snapshot was taken.
    pub snapshot_timestamp: u64,
}

/// Build and return a `BondStatusSnapshot` for the current contract state.
///
/// # Panics
/// - `"no bond"` if no bond has been created yet.
/// - `"emergency config not set"` if emergency config was never initialised
///   (only relevant when `emergency_mode` is read).
#[must_use]
pub fn get_bond_status_snapshot(e: &Env) -> BondStatusSnapshot {
    let bond = e
        .storage()
        .instance()
        .get::<_, crate::IdentityBond>(&DataKey::Bond)
        .unwrap_or_else(|| panic!("no bond"));

    let tier = tiered_bond::get_tier_for_amount(bond.bonded_amount);

    let now = e.ledger().timestamp();

    // Cooldown remaining
    let cooldown_remaining_secs = {
        let cooldown_period = crate::cooldown::get_cooldown_period(e);
        let req_at = bond.withdrawal_requested_at;
        if req_at == 0 || cooldown_period == 0 {
            0
        } else {
            let end = req_at.saturating_add(cooldown_period);
            end.saturating_sub(now)
        }
    };

    // Emergency mode — defaults to false if config was never set
    let emergency_mode = e
        .storage()
        .instance()
        .get::<_, crate::emergency::EmergencyConfig>(&soroban_sdk::Symbol::new(
            e,
            "emergency_config",
        ))
        .map(|cfg| cfg.enabled)
        .unwrap_or(false);

    let available_balance = bond
        .bonded_amount
        .checked_sub(bond.slashed_amount)
        .unwrap_or(0);

    BondStatusSnapshot {
        tier,
        cooldown_remaining_secs,
        emergency_mode,
        available_balance,
        snapshot_timestamp: now,
    }
}
