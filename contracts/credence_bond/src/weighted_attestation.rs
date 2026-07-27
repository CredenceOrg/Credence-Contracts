//! Weighted attestation helpers: stake-to-weight derivation for the CredenceBond contract.
//!
//! # Weight formula
//!
//! ```text
//! weight = max(DEFAULT_ATTESTATION_WEIGHT,
//!              min(floor(stake × multiplier_bps / BPS_DENOMINATOR),
//!                  config_max,
//!                  MAX_ATTESTATION_WEIGHT))
//! ```
//!
//! where `BPS_DENOMINATOR = 10_000`.
//!
//! ## Step-by-step derivation
//!
//! 1. **Basis-point scaling** — multiply `stake` (non-negative `i128` treated as
//!    `u128`) by `multiplier_bps`, then integer-divide by `BPS_DENOMINATOR`
//!    (10 000). This is computed in two parts to avoid overflow on very large
//!    stakes:
//!
//!    ```text
//!    quotient  = (stake / 10_000) × multiplier_bps
//!    remainder = (stake % 10_000) × multiplier_bps / 10_000
//!    raw       = quotient + remainder          (saturating)
//!    ```
//!
//! 2. **Rounding** — integer division truncates towards zero, so the result is
//!    always the mathematical floor.  A remainder is discarded, **never** rounded
//!    up.  Examples:
//!    - `stake=9_999`, `multiplier_bps=100` → `floor(9_999 × 100 / 10_000)` =
//!      `floor(99.99)` = **99** (not 100).
//!    - `stake=10_000`, `multiplier_bps=100` → `floor(10_000 × 100 / 10_000)` =
//!      `floor(100.00)` = **100** (exact, no rounding).
//!    - `stake=33_333`, `multiplier_bps=300` → `floor(33_333 × 300 / 10_000)` =
//!      `floor(999.99)` = **999** (not 1_000).
//!    - `stake=33_334`, `multiplier_bps=300` → `floor(33_334 × 300 / 10_000)` =
//!      `floor(1_000.02)` = **1_000**.
//!
//! 3. **Upper clamp** — `raw` is clamped by `min(config_max, MAX_ATTESTATION_WEIGHT)`.
//!    Both guards are applied even if only one would be sufficient, making the
//!    clamp unconditional and immune to future config-storage bugs.
//!
//! 4. **Lower clamp** — if the clamped value is zero (e.g. `multiplier_bps=0`
//!    or stake too small to produce ≥ 1), it is raised to
//!    `DEFAULT_ATTESTATION_WEIGHT` (1).  This ensures every attester can always
//!    produce a valid attestation.
//!
//! ## Determinism guarantee
//!
//! `compute_weight` is a **pure function** of the attester's stored stake and the
//! contract-wide weight config.  It performs no ledger reads beyond those two
//! storage entries, uses only deterministic integer arithmetic, and is
//! guaranteed to return the same value every time it is called with the same
//! stored state.  There is no floating-point, no random input, and no
//! ledger-sequence-dependent branching.
//!
//! ## Overflow safety
//!
//! `stake` is an `i128` but is cast to `u128` after the non-negative guard.
//! `multiplier_bps` is at most `MAX_WEIGHT_MULTIPLIER_BPS = 10_000` (also
//! enforced on storage).  The worst-case intermediate product is
//! `u128::MAX × 10_000`, which fits in a `u128` thanks to the split-multiply
//! pattern above and saturating arithmetic.
//!
//! ## Stored-weight immutability
//!
//! `compute_weight` is called at attestation-creation time and the resulting
//! `u32` is stored in the `Attestation` record.  Subsequent changes to stake
//! or config do **not** retroactively alter existing attestations; each
//! attestation captures the weight that was in effect when it was created.

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
///
/// Values supplied to [`set_weight_config`] above this ceiling are silently
/// clamped down to `MAX_WEIGHT_MULTIPLIER_BPS` before storage.
pub const MAX_WEIGHT_MULTIPLIER_BPS: u32 = 10_000;

/// Default maximum attestation weight when no config is set.
pub const DEFAULT_MAX_WEIGHT: u32 = 100_000;

fn weight_config_key(e: &Env) -> Symbol {
    Symbol::new(e, "weight_cfg")
}

/// Returns `(multiplier_bps, max_weight)`.
///
/// Falls back to `(DEFAULT_WEIGHT_MULTIPLIER_BPS, DEFAULT_MAX_WEIGHT)` when
/// the config has never been written.
#[must_use]
pub fn get_weight_config(e: &Env) -> (u32, u32) {
    e.storage()
        .instance()
        .get::<_, (u32, u32)>(&weight_config_key(e))
        .unwrap_or((DEFAULT_WEIGHT_MULTIPLIER_BPS, DEFAULT_MAX_WEIGHT))
}

/// Persists the weight config, silently clamping both fields to their
/// respective protocol ceilings.
///
/// - `multiplier_bps` is clamped to [`MAX_WEIGHT_MULTIPLIER_BPS`] (10_000).
/// - `max_weight` is clamped to [`MAX_ATTESTATION_WEIGHT`] (1_000_000).
///
/// The stored values are what `get_weight_config` returns afterwards; callers
/// must use `get_weight_config` to inspect the effective (post-clamp) config.
pub fn set_weight_config(e: &Env, multiplier_bps: u32, max_weight: u32) {
    let multiplier = core::cmp::min(multiplier_bps, MAX_WEIGHT_MULTIPLIER_BPS);
    let cap = core::cmp::min(max_weight, MAX_ATTESTATION_WEIGHT);
    e.storage()
        .instance()
        .set(&weight_config_key(e), &(multiplier, cap));
}

/// Returns the stake (non-negative token units) recorded for `attester`.
///
/// Returns **0** if no stake has been set, making the absence of a record
/// indistinguishable from an explicit zero-stake, which in turn causes
/// [`compute_weight`] to return `DEFAULT_ATTESTATION_WEIGHT`.
#[must_use]
pub fn get_attester_stake(e: &Env, attester: &Address) -> i128 {
    e.storage()
        .instance()
        .get(&DataKey::AttesterStake(attester.clone()))
        .unwrap_or(0)
}

/// Stores `amount` as the attester's stake.
///
/// # Panics
///
/// Panics with `"attester stake cannot be negative"` if `amount < 0`.  Negative
/// stakes are meaningless in the weight formula and could cause silent sign
/// errors if the guard were absent.
pub fn set_attester_stake(e: &Env, attester: &Address, amount: i128) {
    if amount < 0 {
        panic!("attester stake cannot be negative");
    }
    e.storage()
        .instance()
        .set(&DataKey::AttesterStake(attester.clone()), &amount);
    crate::bump_instance_ttl(e);
}

#[allow(dead_code)]
pub fn set_weight_config(e: &Env, multiplier_bps: u32, max_weight: u32) {
    if multiplier_bps > MAX_WEIGHT_CONFIG_MULTIPLIER_BPS {
        panic!("multiplier_bps exceeds maximum");
    }
    if max_weight > MAX_ATTESTATION_WEIGHT {
        panic!("max_weight exceeds maximum");
    }

    let key = DataKey::WeightConfig;
    let old_config: WeightConfig = e.storage().instance().get(&key).unwrap_or(WeightConfig {
        multiplier_bps: 0,
        max_weight: DEFAULT_WEIGHT_CONFIG_MAX_WEIGHT,
    });

    let new_config = WeightConfig {
        multiplier_bps,
        max_weight,
    };
    e.storage().instance().set(&key, &new_config);
    crate::bump_instance_ttl(e);

    e.events().publish(
        (Symbol::new(e, "weight_config_set"),),
        (
            old_config.multiplier_bps,
            old_config.max_weight,
            multiplier_bps,
            max_weight,
        ),
    );
}

pub fn get_weight_config(e: &Env) -> (u32, u32) {
    let key = DataKey::WeightConfig;
    let config: WeightConfig = e.storage().instance().get(&key).unwrap_or(WeightConfig {
        multiplier_bps: 0,
        max_weight: DEFAULT_WEIGHT_CONFIG_MAX_WEIGHT,
    });
    crate::bump_instance_ttl(e);
    (config.multiplier_bps, config.max_weight)
}

pub fn compute_weight(e: &Env, attester: &Address) -> u32 {
    use crate::types::attestation::DEFAULT_ATTESTATION_WEIGHT;

    let stake = get_attester_stake(e, attester);
    let (multiplier_bps, max_weight) = get_weight_config(e);
    let stake: i128 = e
        .storage()
        .instance()
        .get(&DataKey::AttesterStake(attester.clone()))
        .unwrap_or(0);
    crate::bump_instance_ttl(e);

    // Short-circuit: zero (or missing) stake always returns the default weight.
    // This preserves the invariant that every registered attester can produce
    // at least one valid attestation regardless of whether they hold any stake.
    if stake <= 0 {
        return DEFAULT_ATTESTATION_WEIGHT;
    }

    // Cast to u128 after the non-negative guard above.
    let stake_u128 = stake.unsigned_abs();

    // BPS_DENOMINATOR = 10_000.  The multiplier is re-clamped here as a
    // defence-in-depth measure even though set_weight_config already clamps it
    // on the way in.
    let denom = math::BPS_DENOMINATOR as u128;
    let mult = core::cmp::min(multiplier_bps, MAX_WEIGHT_MULTIPLIER_BPS) as u128;

    // Split-multiply pattern: avoid overflow on large stakes by computing
    //   (whole_part × mult) + (fractional_part × mult / denom)
    // Both halves use saturating arithmetic so extreme inputs cannot wrap.
    // The final addition is also saturating; overflow would pin at u128::MAX
    // which is then clamped to MAX_ATTESTATION_WEIGHT below.
    let raw = (stake_u128 / denom)
        .saturating_mul(mult)
        .saturating_add((stake_u128 % denom).saturating_mul(mult) / denom);

    // Upper clamp: both config_max and the protocol hard-cap are enforced
    // unconditionally so neither can be bypassed by a stale config value.
    let cap = core::cmp::min(max_weight, MAX_ATTESTATION_WEIGHT) as u128;
    let capped = core::cmp::min(raw, cap);
    let bounded = core::cmp::min(capped, MAX_ATTESTATION_WEIGHT as u128) as u32;

    // Lower clamp: if the result rounds to zero, raise it to the default.
    bounded.max(DEFAULT_ATTESTATION_WEIGHT)
}
