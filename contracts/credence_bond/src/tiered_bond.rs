//! Tiered Bond System
//!
//! Assigns identity tiers (Bronze, Silver, Gold, Platinum) based on bonded amount thresholds.

use crate::BondTier;
use soroban_sdk::Env;

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

/// Emits both the v1 `tier_changed` event (backwards-compatible) and the v2
/// indexer event `tier_changed_v2` when a bond crosses a tier threshold.
///
/// # v1 event (`tier_changed`)
///
/// # Topics
/// * `Symbol` - "tier_changed"
///
/// # Data
/// * `Address` - The identity whose tier changed
/// * `crate::BondTier` - The new tier
///
/// # v2 event (`tier_changed_v2`)
///
/// # Topics
/// * `Symbol` - "tier_changed_v2"
/// * `Address` - The identity whose tier changed (indexed)
///
/// # Data
/// * `crate::BondTier` - The previous tier
/// * `crate::BondTier` - The new tier
/// * `u64` - Ledger timestamp when the transition occurred
///
/// # Replay semantics
/// **Derived, not authoritative.** A bond's tier is a pure function of
/// `bonded_amount` (see [`get_tier_for_amount`]), so a replayer recomputes it
/// from reconstructed state and does not need these events to rebuild
/// `IdentityBond`. They are emitted for indexer convenience/alerting only and
/// are safe to ignore during replay; they must never be the sole source of a
/// balance change.
pub fn emit_tier_change_if_needed(
    e: &Env,
    identity: &soroban_sdk::Address,
    old_tier: BondTier,
    new_tier: BondTier,
) {
    if core::mem::discriminant(&old_tier) == core::mem::discriminant(&new_tier) {
        return;
    }

    // v1: identity, new_tier
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "tier_changed"),),
        (identity.clone(), new_tier.clone()),
    );

    // v2: indexed identity topic + (old_tier, new_tier, timestamp) data
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "tier_changed_v2"), identity.clone()),
        (old_tier, new_tier, e.ledger().timestamp()),
    );
}
