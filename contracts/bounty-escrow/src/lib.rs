#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, token, Address, Env, Symbol, Vec,
};

const DAY_IN_LEDGERS: u32 = 17_280;
const PERSISTENT_TTL_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
const PERSISTENT_TTL_EXTEND_TO: u32 = 30 * DAY_IN_LEDGERS;
const MAX_SCAN_LIMIT: u32 = 200;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EscrowStatus {
    Locked = 0,
    PartiallyRefunded = 1,
    Released = 2,
    Refunded = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escrow {
    pub amount: i128,
    pub remaining_amount: i128,
    pub status: EscrowStatus,
    pub depositor: Address,
    pub beneficiary: Address,
    pub created_at: u64,
    pub expires_at: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    NextId,
    EscrowIndex,
    Escrow(u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositorStats {
    pub total_deposited: i128,
    pub total_remaining: i128,
    pub active_count: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregateStats {
    pub total_value_locked: i128,
    pub total_original_amount: i128,
    pub active_count: u32,
}

fn bump_persistent<K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, key: &K) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND_TO);
}

pub fn get_high_value_bounties(env: Env, min_amount: i128, start: u32, limit: u32) -> Vec<u64> {
    let index: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::EscrowIndex)
        .unwrap_or(Vec::new(&env));
    let mut results = Vec::new(&env);
    let mut count = 0u32;
    let len = index.len();
    let mut i = start;
    while i < len && count < limit {
        let bounty_id = index.get(i).unwrap();
        if let Some(escrow) = env
            .storage()
            .persistent()
            .get::<DataKey, Escrow>(&DataKey::Escrow(bounty_id))
        {
            if (escrow.status == EscrowStatus::Locked
                || escrow.status == EscrowStatus::PartiallyRefunded)
                && escrow.remaining_amount >= min_amount
            {
                results.push_back(bounty_id);
                count += 1;
            }
        }
        i += 1;
    }
    results
}

pub fn query_expiring_bounties(env: Env, window_seconds: u64, start: u32, limit: u32) -> Vec<u64> {
    let index: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::EscrowIndex)
        .unwrap_or(Vec::new(&env));
    let now = env.ledger().timestamp();
    let deadline = now.saturating_add(window_seconds);
    let mut results = Vec::new(&env);
    let mut count = 0u32;
    let len = index.len();
    let mut i = start;
    while i < len && count < limit {
        let bounty_id = index.get(i).unwrap();
        if let Some(escrow) = env
            .storage()
            .persistent()
            .get::<DataKey, Escrow>(&DataKey::Escrow(bounty_id))
        {
            if (escrow.status == EscrowStatus::Locked
                || escrow.status == EscrowStatus::PartiallyRefunded)
                && escrow.expires_at <= deadline
                && escrow.expires_at >= now
            {
                results.push_back(bounty_id);
                count += 1;
            }
        }
        i += 1;
    }
    results
}

pub fn get_depositor_stats(env: Env, depositor: Address, start: u32, limit: u32) -> DepositorStats {
    let index: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::EscrowIndex)
        .unwrap_or(Vec::new(&env));
    let mut total_deposited: i128 = 0;
    let mut total_remaining: i128 = 0;
    let mut active_count: u32 = 0;
    let len = index.len();
    let end = start.saturating_add(limit).min(len);
    let mut i = start.min(len);
    while i < end {
        let bounty_id = index.get(i).unwrap();
        if let Some(escrow) = env
            .storage()
            .persistent()
            .get::<DataKey, Escrow>(&DataKey::Escrow(bounty_id))
        {
            if escrow.depositor == depositor
                && (escrow.status == EscrowStatus::Locked
                    || escrow.status == EscrowStatus::PartiallyRefunded)
            {
                total_deposited = total_deposited.saturating_add(escrow.amount);
                total_remaining = total_remaining.saturating_add(escrow.remaining_amount);
                active_count = active_count.saturating_add(1);
            }
        }
        i += 1;
    }
    DepositorStats {
        total_deposited,
        total_remaining,
        active_count,
    }
}

pub fn get_aggregate_stats_full_scan(env: Env, start: u32, limit: u32) -> AggregateStats {
    let index: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::EscrowIndex)
        .unwrap_or(Vec::new(&env));
    let mut total_value_locked: i128 = 0;
    let mut total_original_amount: i128 = 0;
    let mut active_count: u32 = 0;
    let capped_limit = limit.min(MAX_SCAN_LIMIT);
    let len = index.len();
    let end = start.saturating_add(capped_limit).min(len);
    let mut i = start.min(len);
    while i < end {
        let bounty_id = index.get(i).unwrap();
        if let Some(escrow) = env
            .storage()
            .persistent()
            .get::<DataKey, Escrow>(&DataKey::Escrow(bounty_id))
        {
            if escrow.status == EscrowStatus::Locked
                || escrow.status == EscrowStatus::PartiallyRefunded
            {
                total_value_locked = total_value_locked.saturating_add(escrow.remaining_amount);
                total_original_amount = total_original_amount.saturating_add(escrow.amount);
                active_count = active_count.saturating_add(1);
            }
        }
        i += 1;
    }
    AggregateStats {
        total_value_locked,
        total_original_amount,
        active_count,
    }
}

fn token_address(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Token)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::NotInitialized))
}

fn load_escrow(env: &Env, key: &DataKey) -> Escrow {
    env.storage()
        .persistent()
        .get(key)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::EscrowNotFound))
}

fn is_open(status: EscrowStatus) -> bool {
    status == EscrowStatus::Locked || status == EscrowStatus::PartiallyRefunded
}

#[contract]
pub struct BountyEscrow;

#[contractimpl]
impl BountyEscrow {
    pub fn initialize(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, ContractError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::NextId, &1u64);
    }

    pub fn lock(
        env: Env,
        depositor: Address,
        beneficiary: Address,
        amount: i128,
        expires_at: u64,
    ) -> u64 {
        depositor.require_auth();
        if amount <= 0 {
            panic_with_error!(&env, ContractError::AmountMustBePositive);
        }
        let now = env.ledger().timestamp();
        if expires_at <= now {
            panic_with_error!(&env, ContractError::InvalidEscrowExpiry);
        }

        let token_addr = token_address(&env);
        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(1u64);
        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::Overflow));
        env.storage().instance().set(&DataKey::NextId, &next_id);

        token::Client::new(&env, &token_addr).transfer(
            &depositor,
            &env.current_contract_address(),
            &amount,
        );

        let escrow = Escrow {
            amount,
            remaining_amount: amount,
            status: EscrowStatus::Locked,
            depositor: depositor.clone(),
            beneficiary: beneficiary.clone(),
            created_at: now,
            expires_at,
        };
        let key = DataKey::Escrow(id);
        env.storage().persistent().set(&key, &escrow);
        bump_persistent(&env, &key);

        let mut index: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::EscrowIndex)
            .unwrap_or(Vec::new(&env));
        index.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::EscrowIndex, &index);
        bump_persistent(&env, &DataKey::EscrowIndex);

        env.events().publish(
            (Symbol::new(&env, "escrow_locked"), id),
            (depositor, beneficiary, amount, expires_at),
        );
        id
    }

    /// Pays the full remaining balance to the beneficiary. `caller` must be the
    /// escrow's depositor or the contract admin and must authorize the call.
    pub fn release(env: Env, caller: Address, id: u64) {
        caller.require_auth();
        let key = DataKey::Escrow(id);
        let mut escrow = load_escrow(&env, &key);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized));
        if caller != escrow.depositor && caller != admin {
            panic_with_error!(&env, ContractError::NotEscrowParty);
        }
        if !is_open(escrow.status) || escrow.remaining_amount <= 0 {
            panic_with_error!(&env, ContractError::EscrowNotReleasable);
        }

        let payout = escrow.remaining_amount;
        escrow.remaining_amount = 0;
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&key, &escrow);
        bump_persistent(&env, &key);

        token::Client::new(&env, &token_address(&env)).transfer(
            &env.current_contract_address(),
            &escrow.beneficiary,
            &payout,
        );

        env.events().publish(
            (Symbol::new(&env, "escrow_released"), id),
            (escrow.beneficiary, payout),
        );
    }

    /// Returns `amount` to the depositor once the escrow has expired. Leaves the
    /// escrow `PartiallyRefunded` until the remaining balance reaches zero.
    pub fn refund(env: Env, id: u64, amount: i128) {
        if amount <= 0 {
            panic_with_error!(&env, ContractError::AmountMustBePositive);
        }
        let key = DataKey::Escrow(id);
        let mut escrow = load_escrow(&env, &key);
        escrow.depositor.require_auth();
        if !is_open(escrow.status) {
            panic_with_error!(&env, ContractError::EscrowNotRefundable);
        }
        if env.ledger().timestamp() < escrow.expires_at {
            panic_with_error!(&env, ContractError::EscrowNotExpired);
        }
        if amount > escrow.remaining_amount {
            panic_with_error!(&env, ContractError::RefundExceedsRemaining);
        }

        escrow.remaining_amount -= amount;
        escrow.status = if escrow.remaining_amount == 0 {
            EscrowStatus::Refunded
        } else {
            EscrowStatus::PartiallyRefunded
        };
        env.storage().persistent().set(&key, &escrow);
        bump_persistent(&env, &key);

        token::Client::new(&env, &token_address(&env)).transfer(
            &env.current_contract_address(),
            &escrow.depositor,
            &amount,
        );

        env.events().publish(
            (Symbol::new(&env, "escrow_refunded"), id),
            (escrow.depositor, amount, escrow.remaining_amount),
        );
    }

    pub fn get_escrow(env: Env, id: u64) -> Option<Escrow> {
        env.storage().persistent().get(&DataKey::Escrow(id))
    }

    pub fn get_high_value_bounties(env: Env, min_amount: i128, start: u32, limit: u32) -> Vec<u64> {
        get_high_value_bounties(env, min_amount, start, limit)
    }

    pub fn query_expiring_bounties(
        env: Env,
        window_seconds: u64,
        start: u32,
        limit: u32,
    ) -> Vec<u64> {
        query_expiring_bounties(env, window_seconds, start, limit)
    }

    pub fn get_depositor_stats(
        env: Env,
        depositor: Address,
        start: u32,
        limit: u32,
    ) -> DepositorStats {
        get_depositor_stats(env, depositor, start, limit)
    }

    pub fn get_aggregate_stats_full_scan(env: Env, start: u32, limit: u32) -> AggregateStats {
        get_aggregate_stats_full_scan(env, start, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{token, vec, Address, Env, Error};

    struct Fixture {
        env: Env,
        client: BountyEscrowClient<'static>,
        admin: Address,
        depositor: Address,
        beneficiary: Address,
        token: token::Client<'static>,
    }

    fn setup() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| li.timestamp = 1_000);

        let admin = Address::generate(&env);
        let depositor = Address::generate(&env);
        let beneficiary = Address::generate(&env);

        let token_contract = env.register_stellar_asset_contract_v2(Address::generate(&env));
        let token_addr = token_contract.address();
        token::StellarAssetClient::new(&env, &token_addr).mint(&depositor, &100_000);
        let token = token::Client::new(&env, &token_addr);

        let contract_id = env.register(BountyEscrow, ());
        let client = BountyEscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &token_addr);

        Fixture {
            env,
            client,
            admin,
            depositor,
            beneficiary,
            token,
        }
    }

    fn contract_error(e: ContractError) -> Error {
        Error::from_contract_error(e as u32)
    }

    #[test]
    fn lock_release_happy_path() {
        let f = setup();
        let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        assert_eq!(f.token.balance(&f.depositor), 99_000);
        assert_eq!(f.token.balance(&f.client.address), 1_000);

        f.client.release(&f.depositor, &id);

        assert_eq!(f.token.balance(&f.beneficiary), 1_000);
        assert_eq!(f.token.balance(&f.client.address), 0);
        let escrow = f.client.get_escrow(&id).unwrap();
        assert_eq!(escrow.status, EscrowStatus::Released);
        assert_eq!(escrow.remaining_amount, 0);
    }

    #[test]
    fn double_release_rejected() {
        let f = setup();
        let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        f.client.release(&f.depositor, &id);

        assert_eq!(
            f.client.try_release(&f.depositor, &id),
            Err(Ok(contract_error(ContractError::EscrowNotReleasable)))
        );
        assert_eq!(f.token.balance(&f.beneficiary), 1_000);
        assert_eq!(f.token.balance(&f.depositor), 99_000);
        assert_eq!(f.token.balance(&f.client.address), 0);
    }

    #[test]
    fn refund_before_expiry_rejected() {
        let f = setup();
        let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);

        assert_eq!(
            f.client.try_refund(&id, &500),
            Err(Ok(contract_error(ContractError::EscrowNotExpired)))
        );
        assert_eq!(f.token.balance(&f.client.address), 1_000);
        assert_eq!(
            f.client.get_escrow(&id).unwrap().status,
            EscrowStatus::Locked
        );
    }

    #[test]
    fn partial_then_full_refund() {
        let f = setup();
        let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        f.env.ledger().with_mut(|li| li.timestamp = 2_000);

        f.client.refund(&id, &300);
        let partial = f.client.get_escrow(&id).unwrap();
        assert_eq!(partial.status, EscrowStatus::PartiallyRefunded);
        assert_eq!(partial.remaining_amount, 700);
        assert_eq!(f.token.balance(&f.depositor), 99_300);

        f.client.refund(&id, &700);
        let full = f.client.get_escrow(&id).unwrap();
        assert_eq!(full.status, EscrowStatus::Refunded);
        assert_eq!(full.remaining_amount, 0);
        assert_eq!(f.token.balance(&f.depositor), 100_000);

        assert_eq!(
            f.client.try_refund(&id, &1),
            Err(Ok(contract_error(ContractError::EscrowNotRefundable)))
        );
        assert_eq!(f.token.balance(&f.depositor), 100_000);
    }

    #[test]
    fn refund_exceeding_remaining_rejected() {
        let f = setup();
        let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        f.env.ledger().with_mut(|li| li.timestamp = 2_000);

        assert_eq!(
            f.client.try_refund(&id, &1_001),
            Err(Ok(contract_error(ContractError::RefundExceedsRemaining)))
        );
        assert_eq!(f.token.balance(&f.client.address), 1_000);
    }

    #[test]
    fn release_after_full_refund_rejected() {
        let f = setup();
        let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        f.env.ledger().with_mut(|li| li.timestamp = 2_000);
        f.client.refund(&id, &1_000);

        assert_eq!(
            f.client.try_release(&f.depositor, &id),
            Err(Ok(contract_error(ContractError::EscrowNotReleasable)))
        );
        assert_eq!(f.token.balance(&f.beneficiary), 0);
        assert_eq!(f.token.balance(&f.depositor), 100_000);
    }

    #[test]
    fn admin_can_release() {
        let f = setup();
        let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);

        f.client.release(&f.admin, &id);

        assert_eq!(f.token.balance(&f.beneficiary), 1_000);
        assert_eq!(
            f.client.get_escrow(&id).unwrap().status,
            EscrowStatus::Released
        );
    }

    #[test]
    fn non_party_release_rejected() {
        let f = setup();
        let id = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        let stranger = Address::generate(&f.env);

        assert_eq!(
            f.client.try_release(&stranger, &id),
            Err(Ok(contract_error(ContractError::NotEscrowParty)))
        );
        assert_eq!(f.token.balance(&f.client.address), 1_000);
    }

    #[test]
    fn initialize_twice_rejected() {
        let f = setup();
        let token_addr = f.token.address.clone();

        assert_eq!(
            f.client.try_initialize(&f.admin, &token_addr),
            Err(Ok(contract_error(ContractError::AlreadyInitialized)))
        );
    }

    #[test]
    fn views_reflect_contract_state() {
        let f = setup();
        let id1 = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        let id2 = f.client.lock(&f.depositor, &f.beneficiary, &3_000, &2_000);
        let id3 = f.client.lock(&f.depositor, &f.beneficiary, &5_000, &2_000);

        f.client.release(&f.depositor, &id2);

        assert_eq!(
            f.client.get_high_value_bounties(&0, &0, &10),
            vec![&f.env, id1, id3]
        );

        let agg = f.client.get_aggregate_stats_full_scan(&0, &10);
        assert_eq!(agg.total_value_locked, 6_000);
        assert_eq!(agg.total_original_amount, 6_000);
        assert_eq!(agg.active_count, 2);

        let stats = f.client.get_depositor_stats(&f.depositor, &0, &10);
        assert_eq!(stats.total_deposited, 6_000);
        assert_eq!(stats.total_remaining, 6_000);
        assert_eq!(stats.active_count, 2);
    }

    #[test]
    fn views_are_paginated() {
        let f = setup();
        for _ in 0..250 {
            f.client.lock(&f.depositor, &f.beneficiary, &100, &2_000);
        }

        // Each page must fit in a fresh default budget; exceeding it panics.
        f.env.cost_estimate().budget().reset_default();
        let first = f.client.get_depositor_stats(&f.depositor, &0, &200);
        f.env.cost_estimate().budget().reset_default();
        let second = f.client.get_depositor_stats(&f.depositor, &200, &200);

        assert_eq!(first.active_count, 200);
        assert_eq!(second.active_count, 50);
        assert_eq!(first.total_deposited + second.total_deposited, 25_000);
        assert_eq!(first.total_remaining + second.total_remaining, 25_000);
        assert_eq!(first.active_count + second.active_count, 250);
    }
}
