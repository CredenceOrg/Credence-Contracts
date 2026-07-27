use credence_errors::ContractError;
use soroban_sdk::{contracttype, Address, Env, Symbol};

use crate::math::BPS_DENOMINATOR;
use crate::DataKey;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarlyExitConfig {
    pub treasury: Address,
    pub penalty_bps: u32,
}

pub fn set_config(e: &Env, treasury: Address, penalty_bps: u32) {
    if penalty_bps > BPS_DENOMINATOR as u32 {
        panic!("penalty_bps must be <= {}", BPS_DENOMINATOR);
    }
    let key = DataKey::EarlyExitConfig;
    e.storage().instance().set(
        &key,
        &EarlyExitConfig {
            treasury: treasury.clone(),
            penalty_bps,
        },
    );
    e.events().publish(
        (Symbol::new(e, "early_exit_config_set"),),
        (treasury, penalty_bps),
    );
}

pub fn get_config(e: &Env) -> Result<EarlyExitConfig, ContractError> {
    let key = DataKey::EarlyExitConfig;
    let config = e.storage()
        .instance()
        .get(&key)
        .ok_or(ContractError::EarlyExitConfigNotSet)?;
    Ok(config)
}

pub fn calculate_penalty(amount: i128, remaining: u64, duration: u64, penalty_bps: u32) -> i128 {
    if duration == 0 {
        return 0;
    }
    let charge = amount
        .checked_mul(penalty_bps as i128)
        .unwrap_or(0)
        .checked_div(BPS_DENOMINATOR)
        .unwrap_or(0);
    charge
        .checked_mul(remaining as i128)
        .unwrap_or(0)
        .checked_div(duration as i128)
        .unwrap_or(0)
}

pub fn emit_penalty_event(
    e: &Env,
    identity: &Address,
    amount: i128,
    penalty: i128,
    treasury: &Address,
) {
    e.events().publish(
        (Symbol::new(e, "early_exit_penalty"),),
        (identity.clone(), amount, penalty, treasury.clone()),
    );
}
