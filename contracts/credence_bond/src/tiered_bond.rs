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

/// Tiered Bond System
///
/// Assigns identity tiers (Bronze, Silver, Gold, Platinum) based on bonded amount thresholds.

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
    identity: &soroban_sdk::Address,
    old_tier: BondTier,
    new_tier: BondTier,
) {
    if old_tier != new_tier {
        e.events().publish(
            (Symbol::new(e, "tier_changed"),),
            (identity.clone(), new_tier.clone()),
        );
    }
    if core::mem::discriminant(&old_tier) == core::mem::discriminant(&new_tier) {
        return;
    }

    // v1: identity, new_tier
    e.events().publish(
        (Symbol::new(e, "tier_changed"),),
        (identity.clone(), new_tier.clone()),
    );

    // v2: indexed identity topic + (old_tier, new_tier, timestamp) data
    e.events().publish(
        (Symbol::new(e, "tier_changed_v2"), identity.clone()),
        (old_tier, new_tier, e.ledger().timestamp()),
    );
}
