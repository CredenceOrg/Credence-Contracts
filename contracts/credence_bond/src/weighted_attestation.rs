//! Weighted attestation system: attestation value depends on attester's credibility.
//!
//! Weight is derived from the attester's bond (or configured stake), with
//! a configurable multiplier and a protocol cap. When attester bond changes,
//! new attestations use the new weight; existing attestations retain their stored weight.

use soroban_sdk::Env;

use crate::types::attestation::MAX_ATTESTATION_WEIGHT;
use crate::DataKey;

/// Default weight multiplier in basis points (1 = 0.01%). weight = stake * multiplier_bps / 10_000.
pub const DEFAULT_WEIGHT_MULTIPLIER_BPS: u32 = 100;

/// Default maximum attestation weight when no config is set.
pub const DEFAULT_MAX_WEIGHT: u32 = 100_000;

/// Storage key for weight config (multiplier bps, max weight). Stored as (u32, u32).
fn weight_config_key(e: &Env) -> soroban_sdk::Symbol {
    soroban_sdk::Symbol::new(e, "weight_cfg")
}

/// Returns (multiplier_bps, max_weight). Uses defaults if not set.
#[must_use]
pub fn get_weight_config(e: &Env) -> (u32, u32) {
    e.storage()
        .instance()
        .get::<_, (u32, u32)>(&weight_config_key(e))
        .unwrap_or((DEFAULT_WEIGHT_MULTIPLIER_BPS, DEFAULT_MAX_WEIGHT))
}

/// Sets weight config (admin only; caller must enforce). multiplier_bps in basis points, max_weight capped by MAX_ATTESTATION_WEIGHT.
pub fn set_weight_config(e: &Env, multiplier_bps: u32, max_weight: u32) {
    let cap = core::cmp::min(max_weight, MAX_ATTESTATION_WEIGHT);
    e.storage()
        .instance()
        .set(&weight_config_key(e), &(multiplier_bps, cap));
}

/// Returns the attester's stake (bond amount or configured stake). 0 if not set.
#[must_use]
pub fn get_attester_stake(e: &Env, attester: &soroban_sdk::Address) -> i128 {
    e.storage()
        .instance()
        .get(&DataKey::AttesterStake(attester.clone()))
        .unwrap_or(0)
}

/// Sets attester stake (e.g. from bond). Caller must be admin.
pub fn set_attester_stake(e: &Env, attester: &soroban_sdk::Address, amount: i128) {
    if amount < 0 {
        panic!("attester stake cannot be negative");
    }
    e.storage()
        .instance()
        .set(&DataKey::AttesterStake(attester.clone()), &amount);
}

/// Computes attestation weight from attester stake using config. Capped by config max and MAX_ATTESTATION_WEIGHT.
/// If stake is 0, returns default weight (1) so attestations are still allowed.
#[must_use]
pub fn compute_weight(e: &Env, attester: &soroban_sdk::Address) -> u32 {
    use crate::types::attestation::DEFAULT_ATTESTATION_WEIGHT;

    let stake = get_attester_stake(e, attester);
    let (multiplier_bps, max_weight) = get_weight_config(e);

    if stake <= 0 {
        return DEFAULT_ATTESTATION_WEIGHT;
    }

    // weight = (stake * multiplier_bps / 10_000) capped at max_weight and MAX_ATTESTATION_WEIGHT
    let stake_u64 = stake.unsigned_abs() as u64;
    let w = (stake_u64 * (multiplier_bps as u64) / 10_000) as u32;
    let capped = core::cmp::min(w, max_weight);
    core::cmp::min(capped, MAX_ATTESTATION_WEIGHT).max(DEFAULT_ATTESTATION_WEIGHT)
}
