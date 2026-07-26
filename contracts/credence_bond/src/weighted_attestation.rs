//! Weighted attestation system: attestation value depends on attester's credibility.
//!
//! ## Overview
//! Attestation weight is derived from the attester's bond (or configured stake), with
//! a configurable multiplier (basis points) and a protocol cap. When attester bond changes,
//! new attestations use the new weight; existing attestations retain their stored weight.
//!
//! ## Rounding semantics (documented invariants)
//!
//! The weight formula is:
//! ```text
//! multiplier = min(config_multiplier_bps, MAX_WEIGHT_MULTIPLIER_BPS)
//! raw = floor(stake * multiplier / BPS_DENOMINATOR)   // integer floor division
//! weight = clamp(raw, DEFAULT_ATTESTATION_WEIGHT, min(config_max, MAX_ATTESTATION_WEIGHT))
//! ```
//!
//! Key invariants that are enforced and regression-tested:
//!
//! 1. **Floor division** — fractional results are always truncated toward zero.
//!    e.g. `stake=9_999, mult=100` → `floor(99.99) = 99`, not 100.
//!
//! 2. **Lower bound** — weight is always `>= DEFAULT_ATTESTATION_WEIGHT` (1).
//!    A raw result of 0 (e.g. tiny stake or zero multiplier) is clamped up to 1.
//!
//! 3. **Upper bound** — weight is always `<= MAX_ATTESTATION_WEIGHT`.
//!    Both the config max and the protocol hard cap are enforced independently.
//!
//! 4. **Determinism** — identical `(stake, multiplier_bps, config_max)` inputs
//!    always produce the same output; there is no randomness or ledger-time dependency.
//!
//! 5. **Monotonicity** — for a fixed config, increasing stake never decreases weight
//!    (until the cap is reached).
//!
//! 6. **Immutability of stored weights** — once an attestation is written to storage,
//!    its `weight` field is never mutated. Subsequent stake/config changes only affect
//!    future attestations.
//!
//! 7. **Config clamping** — `set_weight_config` silently clamps `multiplier_bps`
//!    to `MAX_WEIGHT_MULTIPLIER_BPS` and `max_weight` to `MAX_ATTESTATION_WEIGHT`;
//!    the stored value reflects the clamped result.
//!
//! ## Security
//! - Maximum weight is capped by `MAX_ATTESTATION_WEIGHT` to limit influence.
//! - Negative stake is rejected in `set_attester_stake`.
//! - Weight config is admin-only (enforced by contract entrypoints).
//! - `stake` is converted to `u128` before basis-point multiplication and split into
//!   quotient/remainder terms, so max-range stake inputs cannot overflow before the cap.

use soroban_sdk::Env;

use crate::math;
use crate::types::attestation::MAX_ATTESTATION_WEIGHT;
use crate::DataKey;

/// Default weight multiplier in basis points (1 = 0.01%). Formula: weight = stake * multiplier_bps / BPS_DENOMINATOR.
pub const DEFAULT_WEIGHT_MULTIPLIER_BPS: u32 = 100;

/// Maximum configurable weight multiplier in basis points (10_000 = 100%).
pub const MAX_WEIGHT_MULTIPLIER_BPS: u32 = 10_000;

/// Default maximum attestation weight when no config is set.
pub const DEFAULT_MAX_WEIGHT: u32 = 100_000;

/// Storage key for weight config (multiplier_bps, max weight). Stored as (u32, u32).
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

/// Sets weight config (admin only; caller must enforce). multiplier_bps in basis points
/// and capped by MAX_WEIGHT_MULTIPLIER_BPS; max_weight is capped by MAX_ATTESTATION_WEIGHT.
pub fn set_weight_config(e: &Env, multiplier_bps: u32, max_weight: u32) {
    let multiplier = core::cmp::min(multiplier_bps, MAX_WEIGHT_MULTIPLIER_BPS);
    let cap = core::cmp::min(max_weight, MAX_ATTESTATION_WEIGHT);
    e.storage()
        .instance()
        .set(&weight_config_key(e), &(multiplier, cap));
}

/// Returns the attester's stake (bond amount or configured stake). 0 if not set.
#[must_use]
pub fn get_attester_stake(e: &Env, attester: &soroban_sdk::Address) -> i128 {
    e.storage()
        .instance()
        .get(&DataKey::AttesterStake(attester.clone()))
        .unwrap_or(0)
}

/// Sets attester stake (e.g. from bond). Caller must be admin. Rejects negative amount.
///
/// # Errors
/// Panics if amount < 0.
pub fn set_attester_stake(e: &Env, attester: &soroban_sdk::Address, amount: i128) {
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
pub fn compute_weight(e: &Env, attester: &soroban_sdk::Address) -> u32 {
    use crate::types::attestation::DEFAULT_ATTESTATION_WEIGHT;

    let stake = get_attester_stake(e, attester);
    let (multiplier_bps, max_weight) = get_weight_config(e);

    if stake <= 0 {
        return DEFAULT_ATTESTATION_WEIGHT;
    }

    // weight = floor(stake * multiplier_bps / BPS_DENOMINATOR), capped by config
    // and protocol max. Split quotient/remainder to avoid overflowing stake * bps.
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
