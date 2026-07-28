#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec, symbol_short};

mod error;
use error::DisputeError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open,
    Resolved,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub id: u64,
    pub status: DisputeStatus,
    pub resolver: Address,
}

#[contract]
pub struct DisputeResolutionContract;

#[contractimpl]
impl DisputeResolutionContract {
    // Placeholder for storage key
    const DISPUTE_KEY: u64 = 0;

    pub fn create_dispute(env: Env, resolver: Address) -> u64 {
        let id = env.prng().generate::<u64>().unwrap_or(1);
        let dispute = Dispute {
            id,
            status: DisputeStatus::Open,
            resolver,
        };
        env.storage().instance().set(&id, &dispute);
        id
    }

    pub fn get_dispute(env: Env, id: u64) -> Result<Dispute, DisputeError> {
        env.storage()
            .instance()
            .get(&id)
            .ok_or(DisputeError::DisputeNotFound)
    }

    pub fn close(env: Env, id: u64) -> Result<(), DisputeError> {
        let mut dispute = Self::get_dispute(env.clone(), id)?;

        // Invariant 1: No double-close
        if dispute.status == DisputeStatus::Closed {
            return Err(DisputeError::AlreadyClosed);
        }

        // Invariant 2: No unauthorized close
        let caller = env.invoker();
        if caller != dispute.resolver {
            return Err(DisputeError::Unauthorized);
        }

        // Update to terminal state
        dispute.status = DisputeStatus::Closed;
        env.storage().instance().set(&id, &dispute);

        // Emit event
        env.events().publish(
            (symbol_short!("closed"), id),
            (symbol_short!("by"), caller),
        );

        Ok(())
    }
}