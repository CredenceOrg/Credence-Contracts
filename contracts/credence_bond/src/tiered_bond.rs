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
    }
}
