use crate::types::MAX_ATTESTATION_WEIGHT;
use crate::DataKey;
use soroban_sdk::{contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightConfig {
    pub multiplier_bps: u32,
    pub max_weight: u32,
}

#[allow(dead_code)]
pub const MAX_WEIGHT_CONFIG_MULTIPLIER_BPS: u32 = 10_000;
#[allow(dead_code)]
pub const DEFAULT_WEIGHT_CONFIG_MAX_WEIGHT: u32 = MAX_ATTESTATION_WEIGHT;
const WEIGHT_BASIS_POINTS_DENOMINATOR: i128 = 10_000;

#[allow(dead_code)]
pub fn set_attester_stake(e: &Env, attester: &Address, amount: i128) {
    if amount < 0 {
        panic!("stake cannot be negative");
    }
    e.storage()
        .instance()
        .set(&DataKey::AttesterStake(attester.clone()), &amount);
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
    (config.multiplier_bps, config.max_weight)
}

pub fn compute_weight(e: &Env, attester: &Address) -> u32 {
    let (multiplier_bps, max_weight) = get_weight_config(e);
    let stake: i128 = e
        .storage()
        .instance()
        .get(&DataKey::AttesterStake(attester.clone()))
        .unwrap_or(0);

    let raw_weight = stake
        .saturating_mul(multiplier_bps as i128)
        .checked_div(WEIGHT_BASIS_POINTS_DENOMINATOR)
        .unwrap_or(0)
        .max(0);

    let mut weight = if max_weight == 0 {
        0
    } else {
        raw_weight
            .max(1)
            .min(max_weight as i128)
            .min(MAX_ATTESTATION_WEIGHT as i128)
    };

    if weight < 0 {
        weight = 0;
    }
    weight as u32
}
