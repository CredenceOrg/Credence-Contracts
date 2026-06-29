use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Symbol};

use crate::DataKey;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EarlyExitConfig {
    pub treasury: Address,
    pub penalty_bps: u32,
}

const MAX_PENALTY_BPS: u32 = 10_000;
const PENALTY_BASIS_POINTS_DENOMINATOR: i128 = 10_000;

pub fn set_config(e: &Env, treasury: Address, penalty_bps: u32) {
    if penalty_bps > MAX_PENALTY_BPS {
        panic!("penalty_bps must be <= 10000");
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

pub fn get_config(e: &Env) -> (Address, u32) {
    let key = DataKey::EarlyExitConfig;
    e.storage()
        .instance()
        .get(&key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized))
}

pub fn calculate_penalty(amount: i128, remaining: u64, duration: u64, penalty_bps: u32) -> i128 {
    if duration == 0 {
        return 0;
    }
    let charge = amount
        .checked_mul(penalty_bps as i128)
        .unwrap_or(0)
        .checked_div(PENALTY_BASIS_POINTS_DENOMINATOR)
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
