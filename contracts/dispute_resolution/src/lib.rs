#![no_std]
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env,
};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Dispute(u64),
    DisputeCounter,
    Vote(u64, Address),
    DisputeVoteCount(u64),
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum DisputeStatus {
    Open,
    Resolved,
    Rejected,
    Expired,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum DisputeOutcome {
    None,
    FavorDisputer,
    FavorSlasher,
}

#[contracterror]
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    DisputeNotFound = 1,
    AlreadyVoted = 2,
    DisputeNotOpen = 3,
    DeadlineNotReached = 4,
    DeadlineExpired = 5,
    Unauthorized = 6,
    InsufficientStake = 7,
    InvalidDeadline = 8,
    TransferFailed = 9,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeCreated {
    pub dispute_id: u64,
    pub disputer: Address,
    pub slash_request_id: u64,
    pub stake: i128,
    pub deadline: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoteCast {
    pub dispute_id: u64,
    pub arbitrator: Address,
    pub favor_disputer: bool,
}

#[contractevent]
#[derive(Clone, Debug, PartialEq)]
pub struct DisputeResolved {
    pub dispute_id: u64,
    pub outcome: DisputeOutcome,
    pub votes_for_disputer: u64,
    pub votes_for_slasher: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeExpired {
    pub dispute_id: u64,
    pub expired_at: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct Dispute {
    pub id: u64,
    pub disputer: Address,
    pub slash_request_id: u64,
    pub stake: i128,
    pub token: Address,
    pub status: DisputeStatus,
    pub outcome: DisputeOutcome,
    pub deadline: u64,
    pub votes_for_disputer: u64,
    pub votes_for_slasher: u64,
    pub created_at: u64,
}

pub const MIN_STAKE: i128 = 100;

#[contract]
pub struct DisputeContract;

#[contractimpl]
impl DisputeContract {
    pub fn create_dispute(
        env: Env,
        disputer: Address,
        slash_request_id: u64,
        stake: i128,
        token: Address,
        resolution_deadline: u64,
    ) -> Result<u64, Error> {
        disputer.require_auth();

        if stake < MIN_STAKE {
            return Err(Error::InsufficientStake);
        }

        if resolution_deadline == 0 {
            return Err(Error::InvalidDeadline);
        }

        let current_time = env.ledger().timestamp();
        let deadline = current_time + resolution_deadline;

        let token_client = soroban_sdk::token::Client::new(&env, &token);
        let contract_address = env.current_contract_address();

        token_client.transfer_from(&contract_address, &disputer, &contract_address, &stake);

        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeCounter)
            .unwrap_or(0);
        let dispute_id = counter + 1;

        let dispute = Dispute {
            id: dispute_id,
            disputer: disputer.clone(),
            slash_request_id,
            stake,
            token,
            status: DisputeStatus::Open,
            outcome: DisputeOutcome::None,
            deadline,
            votes_for_disputer: 0,
            votes_for_slasher: 0,
            created_at: current_time,
        };

        env.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);
        env.storage()
            .instance()
            .set(&DataKey::DisputeCounter, &dispute_id);

        DisputeCreated {
            dispute_id,
            disputer,
            slash_request_id,
            stake,
            deadline,
        }
        .publish(&env);

        Ok(dispute_id)
    }

    pub fn get_dispute(env: &Env, dispute_id: u64) -> Dispute {
        env.storage()
            .instance()
            .get(&DataKey::Dispute(dispute_id))
            .expect("Dispute not found")
    }

    pub fn cast_vote(
        env: Env,
        arbitrator: Address,
        dispute_id: u64,
        favor_disputer: bool,
    ) -> Result<(), Error> {
        arbitrator.require_auth();

        if !env.storage().instance().has(&DataKey::Dispute(dispute_id)) {
            return Err(Error::DisputeNotFound);
        }

        let mut dispute = DisputeContract::get_dispute(&env, dispute_id);

        if dispute.status != DisputeStatus::Open {
            return Err(Error::DisputeNotOpen);
        }

        if env.ledger().timestamp() > dispute.deadline {
            return Err(Error::DeadlineExpired);
        }

        if env
            .storage()
            .instance()
            .has(&DataKey::Vote(dispute_id, arbitrator.clone()))
        {
            return Err(Error::AlreadyVoted);
        }

        env.storage().instance().set(
            &DataKey::Vote(dispute_id, arbitrator.clone()),
            &favor_disputer,
        );

        if favor_disputer {
            dispute.votes_for_disputer += 1;
        } else {
            dispute.votes_for_slasher += 1;
        }

        env.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        VoteCast {
            dispute_id,
            arbitrator,
            favor_disputer,
        }
        .publish(&env);

        Ok(())
    }

    pub fn resolve_dispute(env: Env, dispute_id: u64) -> Result<(), Error> {
        if !env.storage().instance().has(&DataKey::Dispute(dispute_id)) {
            return Err(Error::DisputeNotFound);
        }

        let mut dispute = DisputeContract::get_dispute(&env, dispute_id);

        if dispute.status != DisputeStatus::Open {
            return Err(Error::DisputeNotOpen);
        }

        if env.ledger().timestamp() <= dispute.deadline {
            return Err(Error::DeadlineNotReached);
        }

        let token_client = soroban_sdk::token::Client::new(&env, &dispute.token);
        let contract_address = env.current_contract_address();

        let outcome = if dispute.votes_for_disputer > dispute.votes_for_slasher {
            token_client.transfer(&contract_address, &dispute.disputer, &dispute.stake);
            DisputeOutcome::FavorDisputer
        } else {
            DisputeOutcome::FavorSlasher
        };

        dispute.status = DisputeStatus::Resolved;
        dispute.outcome = outcome.clone();

        env.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        DisputeResolved {
            dispute_id,
            outcome,
            votes_for_disputer: dispute.votes_for_disputer,
            votes_for_slasher: dispute.votes_for_slasher,
        }
        .publish(&env);

        Ok(())
    }

    pub fn expire_dispute(env: Env, dispute_id: u64) -> Result<(), Error> {
        if !env.storage().instance().has(&DataKey::Dispute(dispute_id)) {
            return Err(Error::DisputeNotFound);
        }

        let mut dispute = DisputeContract::get_dispute(&env, dispute_id);

        if dispute.status != DisputeStatus::Open {
            return Err(Error::DisputeNotOpen);
        }

        if env.ledger().timestamp() <= dispute.deadline {
            return Err(Error::DeadlineNotReached);
        }

        dispute.status = DisputeStatus::Expired;

        env.storage()
            .instance()
            .set(&DataKey::Dispute(dispute_id), &dispute);

        DisputeExpired {
            dispute_id,
            expired_at: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn has_voted(env: Env, dispute_id: u64, arbitrator: Address) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::Vote(dispute_id, arbitrator))
    }

    pub fn get_dispute_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::DisputeCounter)
            .unwrap_or(0)
    }
}

mod test;
