//! Tiered Bond System
//!
//! Assigns identity tiers (Bronze, Silver, Gold, Platinum) based on bonded amount thresholds.
//! Supports tier upgrade on bond increase and tier downgrade on partial withdrawal.
//! Emits tier change events when tier changes.

use crate::BondTier;
use soroban_sdk::Env;

/// Tier thresholds (in smallest unit, e.g. 6 decimals for USDC).
/// Bronze: [0, BRONZE_MAX), Silver: [BRONZE_MAX, SILVER_MAX), Gold: [SILVER_MAX, GOLD_MAX), Platinum: [GOLD_MAX, ..)
pub const TIER_BRONZE_MAX: i128 = 1_000_000_000;   // 1000 * 10^6
pub const TIER_SILVER_MAX: i128 = 5_000_000_000;   // 5000 * 10^6
pub const TIER_GOLD_MAX: i128 = 20_000_000_000;   // 20000 * 10^6

/// Returns the tier for a given bonded amount.
#[must_use]
pub fn get_tier_for_amount(amount: i128) -> BondTier {
    if amount < TIER_BRONZE_MAX {
        BondTier::Bronze
    } else if amount < TIER_SILVER_MAX {
        BondTier::Silver
    } else if amount < TIER_GOLD_MAX {
        BondTier::Gold
    } else {
        BondTier::Platinum
    }
}

/// Emits a tier change event if the tier changed.
pub fn emit_tier_change_if_needed(e: &Env, identity: &soroban_sdk::Address, old_tier: BondTier, new_tier: BondTier) {
    if core::mem::discriminant(&old_tier) != core::mem::discriminant(&new_tier) {
        e.events().publish(
            (soroban_sdk::Symbol::new(e, "tier_changed"),),
            (identity.clone(), new_tier),
        );
    }
}
