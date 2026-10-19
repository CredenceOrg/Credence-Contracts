#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Symbol, Vec};

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

pub fn query_expiring_bounties(
    env: Env,
    window_seconds: u64,
    start: u32,
    limit: u32,
) -> Vec<u64> {
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

pub fn get_depositor_stats(
    env: Env,
    depositor: Address,
    start: u32,
    limit: u32,
) -> DepositorStats {
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
                total_value_locked =
                    total_value_locked.saturating_add(escrow.remaining_amount);
                total_original_amount =
                    total_original_amount.saturating_add(escrow.amount);
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

#[contract]
pub struct BountyEscrow;

#[contractimpl]
impl BountyEscrow {
    pub fn initialize(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        assert!(
            !env.storage().instance().has(&DataKey::Admin),
            "already initialized"
        );
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
        assert!(amount > 0, "amount must be positive");
        let now = env.ledger().timestamp();
        assert!(expires_at > now, "expires_at must be in the future");

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .expect("not initialized");
        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(1u64);
        env.storage()
            .instance()
            .set(&DataKey::NextId, &(id.checked_add(1).expect("id overflow")));

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

    pub fn release(env: Env, id: u64) {
        let key = DataKey::Escrow(id);
        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("escrow not found");
        escrow.depositor.require_auth();
        assert!(
            escrow.status == EscrowStatus::Locked
                || escrow.status == EscrowStatus::PartiallyRefunded,
            "escrow not releasable"
        );
        let payout = escrow.remaining_amount;
        assert!(payout > 0, "nothing to release");

        escrow.remaining_amount = 0;
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&key, &escrow);
        bump_persistent(&env, &key);

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .expect("not initialized");
        token::Client::new(&env, &token_addr).transfer(
            &env.current_contract_address(),
            &escrow.beneficiary,
            &payout,
        );

        env.events().publish(
            (Symbol::new(&env, "escrow_released"), id),
            (escrow.beneficiary, payout),
        );
    }

    pub fn refund(env: Env, id: u64, amount: i128) {
        assert!(amount > 0, "refund amount must be positive");
        let key = DataKey::Escrow(id);
        let mut escrow: Escrow = env
            .storage()
            .persistent()
            .get(&key)
            .expect("escrow not found");
        escrow.depositor.require_auth();
        assert!(
            escrow.status == EscrowStatus::Locked
                || escrow.status == EscrowStatus::PartiallyRefunded,
            "escrow not refundable"
        );
        let now = env.ledger().timestamp();
        assert!(now >= escrow.expires_at, "escrow has not expired");
        assert!(
            amount <= escrow.remaining_amount,
            "refund exceeds remaining amount"
        );

        escrow.remaining_amount -= amount;
        escrow.status = if escrow.remaining_amount == 0 {
            EscrowStatus::Refunded
        } else {
            EscrowStatus::PartiallyRefunded
        };
        env.storage().persistent().set(&key, &escrow);
        bump_persistent(&env, &key);

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .expect("not initialized");
        token::Client::new(&env, &token_addr).transfer(
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

    pub fn get_high_value_bounties(
        env: Env,
        min_amount: i128,
        start: u32,
        limit: u32,
    ) -> Vec<u64> {
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

    pub fn get_aggregate_stats_full_scan(
        env: Env,
        start: u32,
        limit: u32,
    ) -> AggregateStats {
        get_aggregate_stats_full_scan(env, start, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{token, vec, Address, Env};

    fn setup_fixture() -> (
        Env,
        BountyEscrowClient<'static>,
        Address,
        Address,
        Address,
        token::Client<'static>,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| li.timestamp = 1000);

        let admin = Address::generate(&env);
        let depositor = Address::generate(&env);
        let beneficiary = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin);
        let token_addr = token_contract.address();
        let sac = token::StellarAssetClient::new(&env, &token_addr);
        let token_client = token::Client::new(&env, &token_addr);
        sac.mint(&depositor, &100_000);

        let contract_id = env.register(BountyEscrow, ());
        let client = BountyEscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &token_addr);

        (env, client, admin, depositor, beneficiary, token_client)
    }

    #[test]
    fn lock_release_happy_path() {
        let (_env, client, _admin, depositor, beneficiary, token_client) = setup_fixture();
        let id = client.lock(&depositor, &beneficiary, &2_500, &2_000);
        assert_eq!(id, 1);
        assert_eq!(token_client.balance(&depositor), 97_500);
        assert_eq!(token_client.balance(&beneficiary), 0);

        client.release(&id);
        assert_eq!(token_client.balance(&beneficiary), 2_500);
        let escrow = client.get_escrow(&id).unwrap();
        assert_eq!(escrow.status, EscrowStatus::Released);
        assert_eq!(escrow.remaining_amount, 0);
    }

    #[test]
    #[should_panic(expected = "escrow not releasable")]
    fn double_release_rejected() {
        let (_env, client, _admin, depositor, beneficiary, _token_client) = setup_fixture();
        let id = client.lock(&depositor, &beneficiary, &1_000, &2_000);
        client.release(&id);
        client.release(&id);
    }

    #[test]
    #[should_panic(expected = "escrow has not expired")]
    fn refund_before_expiry_rejected() {
        let (_env, client, _admin, depositor, beneficiary, _token_client) = setup_fixture();
        let id = client.lock(&depositor, &beneficiary, &1_000, &2_000);
        client.refund(&id, &500);
    }

    #[test]
    fn partial_then_full_refund() {
        let (env, client, _admin, depositor, beneficiary, token_client) = setup_fixture();
        let id = client.lock(&depositor, &beneficiary, &1_000, &2_000);
        env.ledger().with_mut(|li| li.timestamp = 2_000);

        client.refund(&id, &400);
        let after_partial = client.get_escrow(&id).unwrap();
        assert_eq!(after_partial.status, EscrowStatus::PartiallyRefunded);
        assert_eq!(after_partial.remaining_amount, 600);
        assert_eq!(token_client.balance(&depositor), 99_400);

        client.refund(&id, &600);
        let after_full = client.get_escrow(&id).unwrap();
        assert_eq!(after_full.status, EscrowStatus::Refunded);
        assert_eq!(after_full.remaining_amount, 0);
        assert_eq!(token_client.balance(&depositor), 100_000);
    }

    #[test]
    #[should_panic(expected = "escrow not releasable")]
    fn release_after_full_refund_rejected() {
        let (env, client, _admin, depositor, beneficiary, _token_client) = setup_fixture();
        let id = client.lock(&depositor, &beneficiary, &1_000, &2_000);
        env.ledger().with_mut(|li| li.timestamp = 2_000);
        client.refund(&id, &1_000);
        client.release(&id);
    }

    #[test]
    fn views_reflect_contract_state() {
        let (env, client, _admin, depositor, beneficiary, _token_client) = setup_fixture();
        let id1 = client.lock(&depositor, &beneficiary, &1_000, &1_400);
        let id2 = client.lock(&depositor, &beneficiary, &3_000, &1_800);
        let id3 = client.lock(&depositor, &beneficiary, &5_000, &1_300);

        client.release(&id3);
        env.ledger().with_mut(|li| li.timestamp = 1_400);
        client.refund(&id1, &300);

        let high_val = client.get_high_value_bounties(&1_000, &0, &10);
        assert_eq!(high_val, vec![&env, id2]);

        let expiring = client.query_expiring_bounties(&500, &0, &10);
        assert_eq!(expiring, vec![&env, id1, id2]);

        let dep_stats = client.get_depositor_stats(&depositor, &0, &10);
        assert_eq!(dep_stats.total_deposited, 4_000);
        assert_eq!(dep_stats.total_remaining, 3_700);
        assert_eq!(dep_stats.active_count, 2);

        let agg = client.get_aggregate_stats_full_scan(&0, &10);
        assert_eq!(agg.total_value_locked, 3_700);
        assert_eq!(agg.total_original_amount, 4_000);
        assert_eq!(agg.active_count, 2);
    }

    #[test]
    fn views_are_paginated() {
        let (env, client, _admin, depositor, beneficiary, _token_client) = setup_fixture();
        let id1 = client.lock(&depositor, &beneficiary, &1_000, &2_000);
        let id2 = client.lock(&depositor, &beneficiary, &2_000, &2_000);
        let id3 = client.lock(&depositor, &beneficiary, &3_000, &2_000);

        let page1 = client.get_high_value_bounties(&500, &0, &2);
        assert_eq!(page1, vec![&env, id1, id2]);

        let page2 = client.get_high_value_bounties(&500, &2, &2);
        assert_eq!(page2, vec![&env, id3]);

        let agg_slice = client.get_aggregate_stats_full_scan(&1, &1);
        assert_eq!(agg_slice.total_value_locked, 2_000);
        assert_eq!(agg_slice.active_count, 1);
    }
}
