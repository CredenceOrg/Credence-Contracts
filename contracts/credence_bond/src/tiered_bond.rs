use crate::BondTier;
use soroban_sdk::{Address, Env, Symbol};

const TIER_BRONZE_MAX: i128 = 1_000;
const TIER_SILVER_MAX: i128 = 5_000;
const TIER_GOLD_MAX: i128 = 20_000;

pub fn get_tier_for_amount(amount: i128) -> BondTier {
    match amount {
        x if x < 0 => BondTier::Bronze,
        x if x < TIER_BRONZE_MAX => BondTier::Bronze,
        x if x < TIER_SILVER_MAX => BondTier::Silver,
        x if x < TIER_GOLD_MAX => BondTier::Gold,
        _ => BondTier::Platinum,
//! Tiered Bond System
//!
//! Assigns identity tiers (Bronze, Silver, Gold, Platinum) based on bonded amount thresholds.

use crate::BondTier;
use soroban_sdk::{Address, Env, Symbol};

const TIER_BRONZE_MAX: i128 = 1_000;
const TIER_SILVER_MAX: i128 = 5_000;
const TIER_GOLD_MAX: i128 = 20_000;

pub fn get_tier_for_amount(amount: i128) -> BondTier {
    match amount {
        x if x < 0 => BondTier::Bronze,
        x if x < TIER_BRONZE_MAX => BondTier::Bronze,
        x if x < TIER_SILVER_MAX => BondTier::Silver,
        x if x < TIER_GOLD_MAX => BondTier::Gold,
        _ => BondTier::Platinum,
    }
}
//! Tiered Bond System
//!
//! Assigns identity tiers (Bronze, Silver, Gold, Platinum) based on bonded amount thresholds.

use crate::events;
use crate::BondTier;
use soroban_sdk::{Address, Env};

pub const TIER_BRONZE_MAX: i128 = 1_000_000_000_000_000_000_000;
pub const TIER_SILVER_MAX: i128 = 5_000_000_000_000_000_000_000;
pub const TIER_GOLD_MAX: i128 = 20_000_000_000_000_000_000_000;

/// Read the configured tier thresholds from contract storage, falling back to
/// the compiled-in defaults when the contract has never been configured.
///
/// Hoisting this out of a loop avoids re-reading and re-deserializing the same
/// `TierThresholds` entry for every element of a batch.
#[must_use]
pub fn get_tier_thresholds(e: &Env) -> crate::TierThresholds {
    e.storage()
        .instance()
        .get::<_, crate::TierThresholds>(&crate::DataKey::TierThresholds)
        .unwrap_or(crate::TierThresholds {
            bronze_max: TIER_BRONZE_MAX,
            silver_max: TIER_SILVER_MAX,
            gold_max: TIER_GOLD_MAX,
        })
}

#[must_use]
pub fn get_tier_for_amount(e: &Env, amount: i128) -> BondTier {
    let thresholds = get_tier_thresholds(e);

    tier_for_amount_with_thresholds(amount, &thresholds)
}

/// Convert an amount into a tier using the configured thresholds.
/// Exact threshold values advance to the next tier so boundary values remain
/// deterministic and stable across top-ups, withdrawals, and slashing.
#[must_use]
pub(crate) fn tier_for_amount_with_thresholds(
    amount: i128,
    thresholds: &crate::TierThresholds,
) -> BondTier {
    if amount < thresholds.bronze_max {
        BondTier::Bronze
    } else if amount < thresholds.silver_max {
        BondTier::Silver
    } else if amount < thresholds.gold_max {
        BondTier::Gold
    } else {
        BondTier::Platinum
    }
}

/// Emits both the v1 `tier_changed` event and the v2 `tier_changed_v2` event
/// when a bond crosses a tier threshold.
pub fn emit_tier_change_if_needed(
    e: &Env,
    identity: &Address,
    old_tier: BondTier,
    new_tier: BondTier,
) {
    if old_tier != new_tier {
        e.events().publish(
            (Symbol::new(e, "tier_changed"),),
            (identity.clone(), new_tier),
        );
            (identity.clone(), new_tier.clone()),
        );
    }
    if core::mem::discriminant(&old_tier) == core::mem::discriminant(&new_tier) {
        return;
    }

    let timestamp = e.ledger().timestamp();
    events::emit_tier_changed(e, identity, new_tier.clone());
    events::emit_tier_changed_v2(e, identity, old_tier, new_tier, timestamp);
}
