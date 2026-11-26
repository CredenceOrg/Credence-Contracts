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
    // pause‑governance keys (mirrored from credence_registry)
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u32),
    PauseApproval(u32, Address),
    PauseApprovalCount(u32),
    PendingAdmin,
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
        let key = DataKey::Escrow(id);
        let escrow: Option<Escrow> = env.storage().persistent().get(&key);
        // Refresh the TTL on read so monitoring/indexer polling keeps live
        // records from expiring.
        if escrow.is_some() {
            bump_persistent(&env, &key);
        }
        escrow
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

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized))
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PendingAdmin)
    }

    pub fn propose_admin(env: Env, new_admin: Address) {
        let current_admin = Self::get_admin(env.clone());
        current_admin.require_auth();

        if new_admin == current_admin {
            panic_with_error!(&env, ContractError::AdminUnchanged);
        }
        if new_admin == env.current_contract_address() {
            panic_with_error!(&env, ContractError::InvalidAdminAddress);
        }

        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);

        env.events().publish(
            (Symbol::new(&env, "admin_proposed"),),
            (current_admin, new_admin),
        );
    }

    pub fn accept_admin(env: Env) {
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NoPendingAdmin));
        pending.require_auth();

        let current_admin = Self::get_admin(env.clone());

        env.storage().instance().set(&DataKey::Admin, &pending);
        env.storage().instance().remove(&DataKey::PendingAdmin);

        env.events().publish(
            (Symbol::new(&env, "admin_transferred"),),
            (current_admin, pending),
        );
    }

    pub fn cancel_admin_proposal(env: Env) {
        let current_admin = Self::get_admin(env.clone());
        current_admin.require_auth();

        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NoPendingAdmin));

        env.storage().instance().remove(&DataKey::PendingAdmin);

        env.events().publish(
            (Symbol::new(&env, "admin_proposal_cancelled"),),
            (current_admin, pending),
        );
    }
}

#[cfg(test)]
mod test_events_schema;

#[cfg(test)]
mod test_ttl;

#[cfg(test)]
mod test_admin;

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

    #[test]
    fn pagination_start_at_or_after_end() {
        let f = setup();
        let _id1 = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        let _id2 = f.client.lock(&f.depositor, &f.beneficiary, &2_000, &2_000);

        // len is 2. Test start == len (at the end)
        assert_eq!(f.client.get_high_value_bounties(&0, &2, &10), vec![&f.env]);
        assert_eq!(
            f.client.query_expiring_bounties(&5_000, &2, &10),
            vec![&f.env]
        );
        let dep_stats_at_end = f.client.get_depositor_stats(&f.depositor, &2, &10);
        assert_eq!(dep_stats_at_end.active_count, 0);
        assert_eq!(dep_stats_at_end.total_deposited, 0);
        assert_eq!(dep_stats_at_end.total_remaining, 0);
        let agg_stats_at_end = f.client.get_aggregate_stats_full_scan(&2, &10);
        assert_eq!(agg_stats_at_end.active_count, 0);
        assert_eq!(agg_stats_at_end.total_original_amount, 0);
        assert_eq!(agg_stats_at_end.total_value_locked, 0);

        // Test start > len (after the end)
        assert_eq!(f.client.get_high_value_bounties(&0, &5, &10), vec![&f.env]);
        assert_eq!(
            f.client.query_expiring_bounties(&5_000, &5, &10),
            vec![&f.env]
        );
        let dep_stats_after = f.client.get_depositor_stats(&f.depositor, &5, &10);
        assert_eq!(dep_stats_after.active_count, 0);
        assert_eq!(dep_stats_after.total_deposited, 0);
        assert_eq!(dep_stats_after.total_remaining, 0);
        let agg_stats_after = f.client.get_aggregate_stats_full_scan(&5, &10);
        assert_eq!(agg_stats_after.active_count, 0);
        assert_eq!(agg_stats_after.total_original_amount, 0);
        assert_eq!(agg_stats_after.total_value_locked, 0);
    }

    #[test]
    fn pagination_limit_zero() {
        let f = setup();
        let _id1 = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        let _id2 = f.client.lock(&f.depositor, &f.beneficiary, &2_000, &2_000);

        // limit = 0 from start = 0
        assert_eq!(f.client.get_high_value_bounties(&0, &0, &0), vec![&f.env]);
        assert_eq!(
            f.client.query_expiring_bounties(&5_000, &0, &0),
            vec![&f.env]
        );
        let dep_stats = f.client.get_depositor_stats(&f.depositor, &0, &0);
        assert_eq!(dep_stats.active_count, 0);
        assert_eq!(dep_stats.total_deposited, 0);
        assert_eq!(dep_stats.total_remaining, 0);
        let agg_stats = f.client.get_aggregate_stats_full_scan(&0, &0);
        assert_eq!(agg_stats.active_count, 0);
        assert_eq!(agg_stats.total_original_amount, 0);
        assert_eq!(agg_stats.total_value_locked, 0);

        // limit = 0 from non-zero start
        assert_eq!(f.client.get_high_value_bounties(&0, &1, &0), vec![&f.env]);
        assert_eq!(
            f.client.query_expiring_bounties(&5_000, &1, &0),
            vec![&f.env]
        );
        let dep_stats_mid = f.client.get_depositor_stats(&f.depositor, &1, &0);
        assert_eq!(dep_stats_mid.active_count, 0);
        assert_eq!(dep_stats_mid.total_deposited, 0);
        assert_eq!(dep_stats_mid.total_remaining, 0);
        let agg_stats_mid = f.client.get_aggregate_stats_full_scan(&1, &0);
        assert_eq!(agg_stats_mid.active_count, 0);
        assert_eq!(agg_stats_mid.total_original_amount, 0);
        assert_eq!(agg_stats_mid.total_value_locked, 0);
    }

    #[test]
    fn pagination_limit_larger_than_cap() {
        let f = setup();
        for _ in 0..250 {
            f.client.lock(&f.depositor, &f.beneficiary, &100, &2_000);
        }

        // MAX_SCAN_LIMIT is 200. Passing limit = 250 must be capped at 200.
        f.env.cost_estimate().budget().reset_default();
        let agg = f.client.get_aggregate_stats_full_scan(&0, &250);
        assert_eq!(agg.active_count, 200);
        assert_eq!(agg.total_original_amount, 20_000);
        assert_eq!(agg.total_value_locked, 20_000);

        // Passing limit larger than total count and cap (e.g. u32::MAX)
        f.env.cost_estimate().budget().reset_default();
        let agg_max = f.client.get_aggregate_stats_full_scan(&0, &u32::MAX);
        assert_eq!(agg_max.active_count, 200);
        assert_eq!(agg_max.total_original_amount, 20_000);
        assert_eq!(agg_max.total_value_locked, 20_000);
    }

    #[test]
    fn pagination_exact_multiple_page_sizes() {
        let f = setup();
        let id1 = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        let id2 = f.client.lock(&f.depositor, &f.beneficiary, &2_000, &2_000);
        let id3 = f.client.lock(&f.depositor, &f.beneficiary, &3_000, &2_000);
        let id4 = f.client.lock(&f.depositor, &f.beneficiary, &4_000, &2_000);

        // 4 items with page size = 2 (exact multiple: 2 pages of 2 items)
        let page1 = f.client.get_high_value_bounties(&0, &0, &2);
        let page2 = f.client.get_high_value_bounties(&0, &2, &2);
        let page3 = f.client.get_high_value_bounties(&0, &4, &2);

        assert_eq!(page1, vec![&f.env, id1, id2]);
        assert_eq!(page2, vec![&f.env, id3, id4]);
        assert_eq!(page3, vec![&f.env]);

        let exp1 = f.client.query_expiring_bounties(&5_000, &0, &2);
        let exp2 = f.client.query_expiring_bounties(&5_000, &2, &2);
        let exp3 = f.client.query_expiring_bounties(&5_000, &4, &2);

        assert_eq!(exp1, vec![&f.env, id1, id2]);
        assert_eq!(exp2, vec![&f.env, id3, id4]);
        assert_eq!(exp3, vec![&f.env]);

        let dep1 = f.client.get_depositor_stats(&f.depositor, &0, &2);
        let dep2 = f.client.get_depositor_stats(&f.depositor, &2, &2);
        let dep3 = f.client.get_depositor_stats(&f.depositor, &4, &2);

        assert_eq!(dep1.active_count, 2);
        assert_eq!(dep1.total_deposited, 3_000);
        assert_eq!(dep2.active_count, 2);
        assert_eq!(dep2.total_deposited, 7_000);
        assert_eq!(dep3.active_count, 0);
        assert_eq!(dep3.total_deposited, 0);

        let agg1 = f.client.get_aggregate_stats_full_scan(&0, &2);
        let agg2 = f.client.get_aggregate_stats_full_scan(&2, &2);
        let agg3 = f.client.get_aggregate_stats_full_scan(&4, &2);

        assert_eq!(agg1.active_count, 2);
        assert_eq!(agg1.total_original_amount, 3_000);
        assert_eq!(agg2.active_count, 2);
        assert_eq!(agg2.total_original_amount, 7_000);
        assert_eq!(agg3.active_count, 0);
        assert_eq!(agg3.total_original_amount, 0);
    }

    #[test]
    fn pagination_stats_accumulation_across_pages() {
        let f = setup();
        let other_depositor = Address::generate(&f.env);
        token::StellarAssetClient::new(&f.env, &f.token.address).mint(&other_depositor, &50_000);

        // Escrow 1: Depositor, 1000, Locked
        let _id1 = f.client.lock(&f.depositor, &f.beneficiary, &1_000, &2_000);
        // Escrow 2: Depositor, 2000, PartiallyRefunded (refund 500 -> remaining 1500)
        let id2 = f.client.lock(&f.depositor, &f.beneficiary, &2_000, &2_000);
        // Escrow 3: Depositor, 3000, Released (inactive)
        let id3 = f.client.lock(&f.depositor, &f.beneficiary, &3_000, &2_000);
        // Escrow 4: Depositor, 4000, Refunded (inactive)
        let id4 = f.client.lock(&f.depositor, &f.beneficiary, &4_000, &2_000);
        // Escrow 5: Other depositor, 5000, Locked
        let _id5 = f.client.lock(&other_depositor, &f.beneficiary, &5_000, &2_000);
        // Escrow 6: Depositor, 6000, Locked
        let _id6 = f.client.lock(&f.depositor, &f.beneficiary, &6_000, &2_000);

        // Set up statuses
        f.client.release(&f.depositor, &id3);
        f.env.ledger().with_mut(|li| li.timestamp = 2_000);
        f.client.refund(&id2, &500);
        f.client.refund(&id4, &4_000);

        // Total 6 escrows, page size = 2 (3 pages total)
        let mut agg_tvl = 0i128;
        let mut agg_orig = 0i128;
        let mut agg_active = 0u32;

        let mut dep_deposited = 0i128;
        let mut dep_remaining = 0i128;
        let mut dep_active = 0u32;

        for page in 0..3 {
            let start = page * 2;
            let agg = f.client.get_aggregate_stats_full_scan(&start, &2);
            agg_tvl += agg.total_value_locked;
            agg_orig += agg.total_original_amount;
            agg_active += agg.active_count;

            let dep = f.client.get_depositor_stats(&f.depositor, &start, &2);
            dep_deposited += dep.total_deposited;
            dep_remaining += dep.total_remaining;
            dep_active += dep.active_count;
        }

        // Expected aggregate:
        // Active: id1 (1000/1000), id2 (2000/1500), id5 (5000/5000), id6 (6000/6000)
        // Original: 1000 + 2000 + 5000 + 6000 = 14000
        // Locked: 1000 + 1500 + 5000 + 6000 = 13500
        // Count: 4
        assert_eq!(agg_active, 4);
        assert_eq!(agg_orig, 14_000);
        assert_eq!(agg_tvl, 13_500);

        // Expected depositor:
        // Active for f.depositor: id1 (1000/1000), id2 (2000/1500), id6 (6000/6000)
        // Deposited: 1000 + 2000 + 6000 = 9000
        // Remaining: 1000 + 1500 + 6000 = 8500
        // Count: 3
        assert_eq!(dep_active, 3);
        assert_eq!(dep_deposited, 9_000);
        assert_eq!(dep_remaining, 8_500);

        // Also test accumulating aggregate stats for 250 items across MAX_SCAN_LIMIT boundaries
        let f2 = setup();
        for _ in 0..250 {
            f2.client.lock(&f2.depositor, &f2.beneficiary, &100, &2_000);
        }
        f2.env.cost_estimate().budget().reset_default();
        let agg_page1 = f2.client.get_aggregate_stats_full_scan(&0, &200);
        f2.env.cost_estimate().budget().reset_default();
        let agg_page2 = f2.client.get_aggregate_stats_full_scan(&200, &200);

        assert_eq!(agg_page1.active_count, 200);
        assert_eq!(agg_page2.active_count, 50);
        assert_eq!(
            agg_page1.total_original_amount + agg_page2.total_original_amount,
            25_000
        );
        assert_eq!(
            agg_page1.total_value_locked + agg_page2.total_value_locked,
            25_000
        );
        assert_eq!(agg_page1.active_count + agg_page2.active_count, 250);
    }
}
