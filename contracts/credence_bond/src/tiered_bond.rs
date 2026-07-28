//! Tiered Bond System
//!
//! Assigns identity tiers (Bronze, Silver, Gold, Platinum) based on bonded amount thresholds.

use crate::events;
use crate::BondTier;
use soroban_sdk::{Address, Env};

pub const TIER_BRONZE_MAX: i128 = 1_000_000_000_000_000_000_000;
pub const TIER_SILVER_MAX: i128 = 5_000_000_000_000_000_000_000;
pub const TIER_GOLD_MAX: i128 = 20_000_000_000_000_000_000_000;

#[must_use]
pub fn get_tier_for_amount(e: &Env, amount: i128) -> BondTier {
    let thresholds = e
        .storage()
        .instance()
        .get::<_, crate::TierThresholds>(&crate::DataKey::TierThresholds)
        .unwrap_or(crate::TierThresholds {
            bronze_max: TIER_BRONZE_MAX,
            silver_max: TIER_SILVER_MAX,
            gold_max: TIER_GOLD_MAX,
        });

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

/// Comparator for [`BondTier`] values. Returns the rank (Bronze=0, Silver=1,
/// Gold=2, Platinum=3). Used by the boundary/fuzz test suite to compare tier
/// transitions in a single integer cell.
#[must_use]
pub(crate) fn tier_rank(t: &BondTier) -> u8 {
    match t {
        BondTier::Bronze => 0,
        BondTier::Silver => 1,
        BondTier::Gold => 2,
        BondTier::Platinum => 3,
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
    if core::mem::discriminant(&old_tier) == core::mem::discriminant(&new_tier) {
        return;
    }

    let timestamp = e.ledger().timestamp();
    events::emit_tier_changed(e, identity, new_tier.clone());
    events::emit_tier_changed_v2(e, identity, old_tier, new_tier, timestamp);
}
