#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, symbol_short, Address, Env,
};

mod error;
pub use error::DisputeError;

#[cfg(test)]
mod test_dispute_error;

/// Soroban network maximum persistent/instance entry TTL (~180 days at 5s/ledger).
pub const STORAGE_TTL_EXTEND_TO: u32 = 3_110_400;
/// Extend entry TTL whenever remaining lifetime drops below half of `STORAGE_TTL_EXTEND_TO`.
pub const STORAGE_TTL_THRESHOLD: u32 = STORAGE_TTL_EXTEND_TO / 2;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    DisputeCounter,
    Dispute(u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open,
    Resolved,
    Closed,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub id: u64,
    pub creator: Address,
    pub status: DisputeStatus,
    pub resolver: Address,
}

fn bump_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_THRESHOLD, STORAGE_TTL_EXTEND_TO);
}

fn bump_dispute_ttl(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, STORAGE_TTL_THRESHOLD, STORAGE_TTL_EXTEND_TO);
}

#[contract]
pub struct DisputeResolutionContract;

#[contractimpl]
impl DisputeResolutionContract {
    pub fn create_dispute(env: Env, creator: Address, resolver: Address) -> u64 {
        creator.require_auth();
        bump_instance_ttl(&env);

        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeCounter)
            .unwrap_or(0);
        let key = DataKey::Dispute(id);
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, DisputeError::AlreadyClosed);
        }

        let next_id = id.checked_add(1).unwrap();
        env.storage()
            .instance()
            .set(&DataKey::DisputeCounter, &next_id);

        let dispute = Dispute {
            id,
            creator: creator.clone(),
            status: DisputeStatus::Open,
            resolver: resolver.clone(),
        };
        env.storage().persistent().set(&key, &dispute);
        bump_dispute_ttl(&env, &key);

        env.events()
            .publish((symbol_short!("created"), id), (creator, resolver));

        id
    }

    pub fn resolve(env: Env, caller: Address, id: u64) -> Result<(), DisputeError> {
        caller.require_auth();
        bump_instance_ttl(&env);

        let key = DataKey::Dispute(id);
        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(DisputeError::DisputeNotFound)?;

        if caller != dispute.resolver {
            return Err(DisputeError::Unauthorized);
        }

        if dispute.status != DisputeStatus::Open {
            return Err(DisputeError::AlreadyClosed);
        }

        dispute.status = DisputeStatus::Resolved;
        env.storage().persistent().set(&key, &dispute);
        bump_dispute_ttl(&env, &key);

        env.events().publish(
            (symbol_short!("resolved"), id),
            (symbol_short!("by"), caller),
        );

        Ok(())
    }

    pub fn close(env: Env, caller: Address, id: u64) -> Result<(), DisputeError> {
        caller.require_auth();
        bump_instance_ttl(&env);

        let key = DataKey::Dispute(id);
        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(DisputeError::DisputeNotFound)?;

        if caller != dispute.resolver {
            return Err(DisputeError::Unauthorized);
        }

        if dispute.status == DisputeStatus::Closed {
            return Err(DisputeError::AlreadyClosed);
        }

        dispute.status = DisputeStatus::Closed;
        env.storage().persistent().set(&key, &dispute);
        bump_dispute_ttl(&env, &key);

        env.events()
            .publish((symbol_short!("closed"), id), (symbol_short!("by"), caller));

        Ok(())
    }

    pub fn get_dispute(env: Env, id: u64) -> Result<Dispute, DisputeError> {
        bump_instance_ttl(&env);

        let key = DataKey::Dispute(id);
        let dispute: Dispute = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(DisputeError::DisputeNotFound)?;
        bump_dispute_ttl(&env, &key);
        Ok(dispute)
    }
}
