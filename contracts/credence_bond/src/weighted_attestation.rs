use crate::math;
use crate::types::attestation::MAX_ATTESTATION_WEIGHT;
use crate::DataKey;
use soroban_sdk::{contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightConfig {
    pub multiplier_bps: u32,
    pub max_weight: u32,
}

/// Default weight multiplier in basis points (1 = 0.01%).
pub const DEFAULT_WEIGHT_MULTIPLIER_BPS: u32 = 100;

/// Maximum configurable weight multiplier in basis points (10_000 = 100%).
pub const MAX_WEIGHT_MULTIPLIER_BPS: u32 = 10_000;

/// Default maximum attestation weight when no config is set.
pub const DEFAULT_MAX_WEIGHT: u32 = 100_000;

fn weight_config_key(e: &Env) -> Symbol {
    Symbol::new(e, "weight_cfg")
}

/// Returns (multiplier_bps, max_weight). Uses defaults if not set.
#[must_use]
pub fn get_weight_config(e: &Env) -> (u32, u32) {
    e.storage()
        .instance()
        .get::<_, (u32, u32)>(&weight_config_key(e))
        .unwrap_or((DEFAULT_WEIGHT_MULTIPLIER_BPS, DEFAULT_MAX_WEIGHT))
}

/// Sets weight config. multiplier_bps and max_weight are clamped to protocol caps.
pub fn set_weight_config(e: &Env, multiplier_bps: u32, max_weight: u32) {
    let multiplier = core::cmp::min(multiplier_bps, MAX_WEIGHT_MULTIPLIER_BPS);
    let cap = core::cmp::min(max_weight, MAX_ATTESTATION_WEIGHT);
    e.storage()
        .instance()
        .set(&weight_config_key(e), &(multiplier, cap));
}

/// Returns the attester's stake. 0 if not set.
#[must_use]
pub fn get_attester_stake(e: &Env, attester: &Address) -> i128 {
    e.storage()
        .instance()
        .get(&DataKey::AttesterStake(attester.clone()))
        .unwrap_or(0)
}

/// Sets attester stake. Rejects negative amounts.
pub fn set_attester_stake(e: &Env, attester: &Address, amount: i128) {
    if amount < 0 {
        panic!("attester stake cannot be negative");
    }
    e.storage()
        .instance()
        .set(&DataKey::AttesterStake(attester.clone()), &amount);
}

/// Computes attestation weight from attester stake using config. Capped by config max and
/// MAX_ATTESTATION_WEIGHT. If stake is 0, returns default weight (1) so attestations are still allowed.
#[must_use]
pub fn compute_weight(e: &Env, attester: &Address) -> u32 {
    use crate::types::attestation::DEFAULT_ATTESTATION_WEIGHT;

    let stake = get_attester_stake(e, attester);
    let (multiplier_bps, max_weight) = get_weight_config(e);

    if stake <= 0 {
        return DEFAULT_ATTESTATION_WEIGHT;
    }

    let stake_u128 = stake.unsigned_abs();
    let denom = math::BPS_DENOMINATOR as u128;
    let mult = core::cmp::min(multiplier_bps, MAX_WEIGHT_MULTIPLIER_BPS) as u128;
    let raw = (stake_u128 / denom)
        .saturating_mul(mult)
        .saturating_add((stake_u128 % denom).saturating_mul(mult) / denom);
    let cap = core::cmp::min(max_weight, MAX_ATTESTATION_WEIGHT) as u128;
    let capped = core::cmp::min(raw, cap);
    let bounded = core::cmp::min(capped, MAX_ATTESTATION_WEIGHT as u128) as u32;
    bounded.max(DEFAULT_ATTESTATION_WEIGHT)
}
