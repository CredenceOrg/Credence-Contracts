use crate::{BondTier, DataKey, IdentityBond};

use credence_errors::ContractError;
use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, Env, String};

/// Deliberately divergent fork: tier thresholds are shifted so that every amount >= 1 returns Gold.
/// This is used by the differential harness to prove it can catch behavioural divergence.

#[contracttype]
#[derive(Clone, Debug)]
pub struct Attestation {
    pub id: u64,
    pub attester: Address,
    pub subject: Address,
    pub attestation_data: String,
    pub timestamp: u64,
    pub revoked: bool,
}

#[contract]
pub struct CredenceBond;

#[contractimpl]
impl CredenceBond {
    pub fn initialize(e: Env, admin: Address) {
        e.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn create_bond(
        e: Env,
        identity: Address,
        amount: i128,
        duration: u64,
        is_rolling: bool,
        notice_period_duration: u64,
    ) -> IdentityBond {
        identity.require_auth();
        let bond_start = e.ledger().timestamp();
        let bond = IdentityBond {
            identity: identity.clone(),
            bonded_amount: amount,
            bond_start,
            bond_duration: duration,
            slashed_amount: 0,
            active: true,
            is_rolling,
            withdrawal_requested_at: 0,
            notice_period_duration,
        };
        let key = DataKey::Bond;
        e.storage().instance().set(&key, &bond);
        bond
    }

    pub fn get_identity_state(e: Env) -> IdentityBond {
        e.storage()
            .instance()
            .get::<_, IdentityBond>(&DataKey::Bond)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound))
    }

    /// Deliberately wrong tier logic: everything >= 1 is Gold.
    pub fn get_tier(e: Env) -> BondTier {
        let bond = Self::get_identity_state(e);
        if bond.bonded_amount >= 1 {
            BondTier::Gold
        } else {
            BondTier::Bronze
        }
    }

    pub fn slash(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        let new_slashed = bond.slashed_amount + amount;
        bond.slashed_amount = if new_slashed > bond.bonded_amount {
            bond.bonded_amount
        } else {
            new_slashed
        };
        e.storage().instance().set(&key, &bond);
        bond
    }

    pub fn top_up(e: Env, amount: i128) -> IdentityBond {
        let key = DataKey::Bond;
        let mut bond = e
            .storage()
            .instance()
            .get::<_, IdentityBond>(&key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::BondNotFound));
        bond.bonded_amount += amount;
        e.storage().instance().set(&key, &bond);
        bond
    }
}
