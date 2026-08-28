#![no_std]
#![deny(clippy::float_arithmetic)]
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use soroban_sdk::{contracttype, Address, Env, Vec};

// ── Types ───────────────────────────────────────────────────────────────────

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
    pub created_at: u64,
    pub expires_at: u64,
}

#[contracttype]
pub enum DataKey {
    EscrowIndex,
    Escrow(u64),
}

// ── Analytics views ─────────────────────────────────────────────────────────

/// Get high-value bounties (above a threshold) for risk monitoring.
///
/// Only returns bounties that are currently at risk:
/// - Status must be Locked or PartiallyRefunded (funds still escrowed).
/// - Uses remaining_amount (actual funds at risk) rather than the original amount.
///
/// # Arguments
/// * `env` - The Soroban environment.
/// * `min_amount` - Minimum remaining_amount threshold.
/// * `limit` - Maximum number of results to return.
///
/// # Returns
/// A vector of bounty IDs matching the criteria.
pub fn get_high_value_bounties(env: Env, min_amount: i128, limit: u32) -> Vec<u64> {
    let index: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::EscrowIndex)
        .unwrap_or(Vec::new(&env));
    let mut results = Vec::new(&env);
    let mut count = 0u32;
    for i in 0..index.len() {
        if count >= limit {
            break;
        }
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
    }
    results
}

/// Query bounties that expire within a given time window (in seconds from now).
///
/// Only considers bounties that are currently at risk (Locked or PartiallyRefunded).
pub fn query_expiring_bounties(
    env: Env,
    window_seconds: u64,
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
    for i in 0..index.len() {
        if count >= limit {
            break;
        }
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
    }
    results
}

/// Get aggregated deposit stats for a given depositor address.
///
/// Only counts bounties that are currently at risk (Locked or PartiallyRefunded).
pub fn get_depositor_stats(env: Env, depositor: Address) -> DepositorStats {
    let index: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::EscrowIndex)
        .unwrap_or(Vec::new(&env));
    let mut total_deposited: i128 = 0;
    let mut total_remaining: i128 = 0;
    let mut active_count: u32 = 0;
    for i in 0..index.len() {
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
    }
    DepositorStats {
        total_deposited,
        total_remaining,
        active_count,
    }
}

/// Aggregate statistics across all escrows, computed via full scan.
///
/// Only includes bounties that are currently at risk (Locked or PartiallyRefunded).
pub fn get_aggregate_stats_full_scan(env: Env) -> AggregateStats {
    let index: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::EscrowIndex)
        .unwrap_or(Vec::new(&env));
    let mut total_value_locked: i128 = 0;
    let mut total_original_amount: i128 = 0;
    let mut active_count: u32 = 0;
    for i in 0..index.len() {
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
    }
    AggregateStats {
        total_value_locked,
        total_original_amount,
        active_count,
    }
}

// ── Stats types ─────────────────────────────────────────────────────────────

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

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{vec, Address, Env};

    fn setup_bounty(
        env: &Env,
        id: u64,
        amount: i128,
        remaining_amount: i128,
        status: EscrowStatus,
    ) {
        let depositor = Address::generate(env);
        let escrow = Escrow {
            amount,
            remaining_amount,
            status,
            depositor,
            created_at: 1000,
            expires_at: 2000,
        };
        env.storage().persistent().set(&DataKey::Escrow(id), &escrow);
    }

    fn setup_index(env: &Env, ids: &[u64]) {
        let mut index: Vec<u64> = Vec::new(env);
        for &id in ids {
            index.push_back(id);
        }
        env.storage()
            .persistent()
            .set(&DataKey::EscrowIndex, &index);
    }

    // ── get_high_value_bounties ──────────────────────────────────────────

    #[test]
    fn high_value_locked_bounty_appears() {
        let env = Env::default();
        setup_bounty(&env, 1, 1000, 1000, EscrowStatus::Locked);
        setup_bounty(&env, 2, 500, 500, EscrowStatus::Locked);
        setup_index(&env, &[1, 2]);

        let results = get_high_value_bounties(env, 600, 10);
        let expected: Vec<u64> = vec![&env, 1u64];
        assert_eq!(results, expected);
    }

    #[test]
    fn high_value_partially_refunded_appears() {
        let env = Env::default();
        setup_bounty(&env, 1, 1000, 800, EscrowStatus::PartiallyRefunded);
        setup_index(&env, &[1]);

        let results = get_high_value_bounties(env, 700, 10);
        let expected: Vec<u64> = vec![&env, 1u64];
        assert_eq!(results, expected);
    }

    #[test]
    fn released_bounty_omitted() {
        let env = Env::default();
        // Original amount was 5000, but bounty was Released
        setup_bounty(&env, 1, 5000, 0, EscrowStatus::Released);
        setup_index(&env, &[1]);

        let results = get_high_value_bounties(env, 100, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn refunded_bounty_omitted() {
        let env = Env::default();
        // Original amount was 5000, but bounty was Refunded
        setup_bounty(&env, 1, 5000, 0, EscrowStatus::Refunded);
        setup_index(&env, &[1]);

        let results = get_high_value_bounties(env, 100, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn released_bounty_with_high_amount_omitted() {
        let env = Env::default();
        setup_bounty(&env, 99, 1_000_000, 0, EscrowStatus::Released);
        setup_index(&env, &[99]);

        // Even though amount is huge, status is Released → must not appear
        let results = get_high_value_bounties(env, 1, 10);
        assert!(
            results.is_empty(),
            "Released bounty with high original amount must NOT appear"
        );
    }

    #[test]
    fn below_threshold_omitted() {
        let env = Env::default();
        setup_bounty(&env, 1, 100, 100, EscrowStatus::Locked);
        setup_index(&env, &[1]);

        let results = get_high_value_bounties(env, 500, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn remaining_amount_below_threshold_omitted() {
        let env = Env::default();
        // Original amount is 5000, but only 50 remains (below threshold)
        setup_bounty(&env, 1, 5000, 50, EscrowStatus::PartiallyRefunded);
        setup_index(&env, &[1]);

        let results = get_high_value_bounties(env, 100, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn limit_respected() {
        let env = Env::default();
        setup_bounty(&env, 1, 1000, 1000, EscrowStatus::Locked);
        setup_bounty(&env, 2, 1000, 1000, EscrowStatus::Locked);
        setup_bounty(&env, 3, 1000, 1000, EscrowStatus::Locked);
        setup_index(&env, &[1, 2, 3]);

        let results = get_high_value_bounties(env.clone(), 500, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn empty_index_returns_empty() {
        let env = Env::default();
        setup_index(&env, &[]);
        let results = get_high_value_bounties(env, 100, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn locked_and_partially_refunded_both_appear() {
        let env = Env::default();
        setup_bounty(&env, 1, 1000, 1000, EscrowStatus::Locked);
        setup_bounty(&env, 2, 1000, 700, EscrowStatus::PartiallyRefunded);
        setup_bounty(&env, 3, 1000, 0, EscrowStatus::Released);
        setup_index(&env, &[1, 2, 3]);

        let results = get_high_value_bounties(env, 500, 10);
        assert_eq!(results.len(), 2);
        assert!(results.contains(&1u64));
        assert!(results.contains(&2u64));
    }

    // ── query_expiring_bounties ──────────────────────────────────────────

    #[test]
    fn expiring_bounties_returned() {
        let env = Env::default();
        env.ledger().with_mut(|li| li.timestamp = 1000);
        setup_bounty(&env, 1, 1000, 1000, EscrowStatus::Locked);
        setup_index(&env, &[1]);

        let results = query_expiring_bounties(env, 500, 10);
        let expected: Vec<u64> = vec![&env, 1u64];
        assert_eq!(results, expected);
    }

    #[test]
    fn expiring_released_bounty_omitted() {
        let env = Env::default();
        env.ledger().with_mut(|li| li.timestamp = 1000);
        setup_bounty(&env, 1, 1000, 0, EscrowStatus::Released);
        setup_index(&env, &[1]);

        let results = query_expiring_bounties(env, 5000, 10);
        assert!(results.is_empty());
    }

    // ── get_depositor_stats ──────────────────────────────────────────────

    #[test]
    fn depositor_stats_active_only() {
        let env = Env::default();
        let depositor = Address::generate(&env);
        let escrow_active = Escrow {
            amount: 1000,
            remaining_amount: 800,
            status: EscrowStatus::Locked,
            depositor: depositor.clone(),
            created_at: 1000,
            expires_at: 2000,
        };
        let escrow_released = Escrow {
            amount: 5000,
            remaining_amount: 0,
            status: EscrowStatus::Released,
            depositor: depositor.clone(),
            created_at: 1000,
            expires_at: 2000,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(1), &escrow_active);
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(2), &escrow_released);
        setup_index(&env, &[1, 2]);

        let stats = get_depositor_stats(env, depositor);
        assert_eq!(stats.total_deposited, 1000);
        assert_eq!(stats.total_remaining, 800);
        assert_eq!(stats.active_count, 1);
    }

    // ── get_aggregate_stats_full_scan ────────────────────────────────────

    #[test]
    fn aggregate_stats_active_only() {
        let env = Env::default();
        setup_bounty(&env, 1, 1000, 1000, EscrowStatus::Locked);
        setup_bounty(&env, 2, 2000, 1500, EscrowStatus::PartiallyRefunded);
        setup_bounty(&env, 3, 5000, 0, EscrowStatus::Released);
        setup_index(&env, &[1, 2, 3]);

        let stats = get_aggregate_stats_full_scan(env);
        assert_eq!(stats.total_value_locked, 2500); // 1000 + 1500
        assert_eq!(stats.total_original_amount, 3000); // 1000 + 2000
        assert_eq!(stats.active_count, 2);
    }
}