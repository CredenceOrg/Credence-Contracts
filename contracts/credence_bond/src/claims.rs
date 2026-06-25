//! # Pull-Payment Claims Module
//!
//! Implements a pull-payment pattern for reward claims to prevent griefing attacks
//! and failed transfers due to recipient contract fallback behavior.
//!
//! ## Features
//! - Pending claims tracking per user
//! - Batch claim processing
//! - Claim expiry mechanism
//! - Comprehensive event emission
//! - Gas-optimized claim operations

#![allow(dead_code)]

use crate::{events, DataKey};
use soroban_sdk::{contracttype, Address, Env, Map, Symbol, Vec};

/// Maximum number of claims that can be processed in a single batch
const MAX_BATCH_CLAIMS: u32 = 50;

/// Default claim expiry period (30 days in seconds)
const DEFAULT_CLAIM_EXPIRY: u64 = 30 * 24 * 60 * 60;

/// Types of claimable rewards
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimType {
    /// Verifier rewards from successful attestations
    VerifierReward = 0,
    /// Slashing rewards for successful challenges
    SlashingReward = 1,
    /// Early exit penalty refunds (partial)
    PenaltyRefund = 2,
    /// Protocol fee rebates
    FeeRebate = 3,
    /// Dispute resolution rewards
    DisputeReward = 4,
}

/// A pending claim for a user
#[contracttype]
#[derive(Clone, Debug)]
pub struct PendingClaim {
    /// Unique claim ID
    pub claim_id: u64,
    /// Type of claim
    pub claim_type: ClaimType,
    /// Amount to be claimed
    pub amount: i128,
    /// When the claim was created
    pub created_at: u64,
    /// When the claim expires (0 = no expiry)
    pub expires_at: u64,
    /// Source transaction or event that generated this claim
    pub source_id: u64,
    /// Additional metadata (optional)
    pub metadata: Symbol,
    /// Whether this claim has been processed
    pub processed: bool,
}

/// Result of a claim operation
#[contracttype]
#[derive(Clone, Debug)]
pub struct ClaimResult {
    /// Number of claims processed
    pub processed_count: u32,
    /// Total amount claimed
    pub total_amount: i128,
    /// List of claim types processed
    pub claim_types: Vec<ClaimType>,
}

/// Storage keys for claims
impl DataKey {
    /// Pending claims for a user: DataKey::PendingClaims(user) -> Vec<PendingClaim>
    pub fn pending_claims(user: &Address) -> DataKey {
        DataKey::PendingClaims(user.clone())
    }

    /// Total claimable amount for a user: DataKey::ClaimableAmount(user) -> i128
    pub fn claimable_amount(user: &Address) -> DataKey {
        DataKey::ClaimableAmount(user.clone())
    }

    /// Claim history counter: DataKey::ClaimCounter -> u64
    pub fn claim_counter() -> DataKey {
        DataKey::ClaimCounter
    }

    /// Individual claim by ID: DataKey::ClaimById(claim_id) -> PendingClaim
    pub const fn claim_by_id(claim_id: u64) -> DataKey {
        DataKey::ClaimById(claim_id)
    }
}

/// Add a new pending claim for a user
///
/// # Arguments
/// * `e` - Contract environment
/// * `user` - Address of the user who can claim
/// * `claim_type` - Type of claim being added
/// * `amount` - Amount to be claimed
/// * `source_id` - Source transaction/event ID
/// * `metadata` - Optional metadata
///
/// # Panics
/// * If amount is negative or zero
pub fn add_pending_claim(
    e: &Env,
    user: &Address,
    claim_type: ClaimType,
    amount: i128,
    source_id: u64,
    metadata: Option<Symbol>,
) -> u64 {
    if amount <= 0 {
        panic!("claim amount must be positive");
    }

    // Get next claim ID
    let claim_id = get_next_claim_id(e);

    let now = e.ledger().timestamp();
    let expires_at = now + DEFAULT_CLAIM_EXPIRY;

    let claim = PendingClaim {
        claim_id,
        claim_type,
        amount,
        created_at: now,
        expires_at,
        source_id,
        metadata: metadata.unwrap_or(Symbol::new(e, "")),
        processed: false,
    };

    // Store claim by ID for direct access
    e.storage()
        .persistent()
        .set(&DataKey::ClaimById(claim_id), &claim.clone());

    // Get existing claims or create new vector
    let mut claims: Vec<PendingClaim> = e
        .storage()
        .persistent()
        .get(&DataKey::PendingClaims(user.clone()))
        .unwrap_or(Vec::new(e));

    claims.push_back(claim.clone());

    // Update storage
    e.storage()
        .persistent()
        .set(&DataKey::PendingClaims(user.clone()), &claims);

    // Update total claimable amount
    let current_total: i128 = e
        .storage()
        .persistent()
        .get(&DataKey::ClaimableAmount(user.clone()))
        .unwrap_or(0);

    let new_total = current_total
        .checked_add(amount)
        .expect("claimable amount overflow");

    e.storage()
        .persistent()
        .set(&DataKey::ClaimableAmount(user.clone()), &new_total);

    // Emit event
    events::emit_claim_added(e, user, &claim);

    claim_id
}

/// Get the next claim ID
fn get_next_claim_id(e: &Env) -> u64 {
    let current: u64 = e
        .storage()
        .persistent()
        .get(&DataKey::ClaimCounter)
        .unwrap_or(0);
    let next = current.checked_add(1).expect("claim counter overflow");
    e.storage().persistent().set(&DataKey::ClaimCounter, &next);
    next
}

/// Get all pending claims for a user
///
/// # Arguments
/// * `e` - Contract environment
/// * `user` - Address to check claims for
///
/// # Returns
/// Vector of pending claims (empty if none)
pub fn get_pending_claims(e: &Env, user: &Address) -> Vec<PendingClaim> {
    e.storage()
        .persistent()
        .get(&DataKey::PendingClaims(user.clone()))
        .unwrap_or(Vec::new(e))
}

/// Get total claimable amount for a user
///
/// # Arguments
/// * `e` - Contract environment
/// * `user` - Address to check amount for
///
/// # Returns
/// Total amount that can be claimed
pub fn get_claimable_amount(e: &Env, user: &Address) -> i128 {
    e.storage()
        .persistent()
        .get(&DataKey::ClaimableAmount(user.clone()))
        .unwrap_or(0)
}

/// Process claims for a user (pull-payment pattern)
///
/// # Arguments
/// * `e` - Contract environment
/// * `user` - Address claiming rewards
/// * `claim_types` - Optional filter for specific claim types (empty = all types)
/// * `max_claims` - Maximum number of claims to process (0 = no limit, capped at MAX_BATCH_CLAIMS)
///
/// # Returns
/// ClaimResult with details of processed claims
///
/// # Panics
/// * If user has no pending claims
/// * If token transfer fails
pub fn process_claims(
    e: &Env,
    user: &Address,
    claim_types: Vec<ClaimType>,
    max_claims: u32,
) -> ClaimResult {
    user.require_auth();

    let now = e.ledger().timestamp();
    let claims = get_pending_claims(e, user);

    if claims.is_empty() {
        panic!("no pending claims");
    }

    // Filter claims by type if specified
    let filter_types = !claim_types.is_empty();
    let type_set: Map<ClaimType, bool> = if filter_types {
        let mut map = Map::new(e);
        for i in 0..claim_types.len() {
            map.set(claim_types.get(i).unwrap(), true);
        }
        map
    } else {
        Map::new(e)
    };

    let mut processed_claims = Vec::new(e);
    let mut remaining_claims = Vec::new(e);
    let mut total_amount = 0i128;
    let mut processed_types = Vec::new(e);
    let limit = if max_claims == 0 {
        MAX_BATCH_CLAIMS
    } else {
        max_claims.min(MAX_BATCH_CLAIMS)
    };

    // Process claims
    for i in 0..claims.len() {
        if processed_claims.len() >= limit {
            // Add remaining claims back to the list
            for j in i..claims.len() {
                remaining_claims.push_back(claims.get(j).unwrap());
            }
            break;
        }

        let claim = claims.get(i).unwrap();

        // Skip expired claims
        if claim.expires_at > 0 && now > claim.expires_at {
            continue;
        }

        // Skip if not in filter
        if filter_types && !type_set.contains_key(claim.claim_type) {
            remaining_claims.push_back(claim);
            continue;
        }

        // Process this claim
        processed_claims.push_back(claim.clone());
        total_amount = total_amount
            .checked_add(claim.amount)
            .expect("claim total overflow");

        // Track unique claim types
        let mut type_exists = false;
        for j in 0..processed_types.len() {
            if processed_types.get(j).unwrap() == claim.claim_type {
                type_exists = true;
                break;
            }
        }
        if !type_exists {
            processed_types.push_back(claim.claim_type);
        }
    }

    if processed_claims.is_empty() {
        panic!("no valid claims to process");
    }

    // Update storage with remaining claims
    if remaining_claims.is_empty() {
        e.storage()
            .persistent()
            .remove(&DataKey::PendingClaims(user.clone()));
        e.storage()
            .persistent()
            .remove(&DataKey::ClaimableAmount(user.clone()));
    } else {
        e.storage()
            .persistent()
            .set(&DataKey::PendingClaims(user.clone()), &remaining_claims);

        let remaining_amount = get_claimable_amount(e, user)
            .checked_sub(total_amount)
            .expect("claimable amount underflow");

        e.storage()
            .persistent()
            .set(&DataKey::ClaimableAmount(user.clone()), &remaining_amount);
    }

    // Transfer tokens to user using safe token operations
    if total_amount > 0 {
        let token: Address = e
            .storage()
            .instance()
            .get(&DataKey::BondToken)
            .expect("token not configured");

        let contract = e.current_contract_address();
        soroban_sdk::token::TokenClient::new(e, &token).transfer(&contract, user, &total_amount);
    }

    let result = ClaimResult {
        processed_count: processed_claims.len(),
        total_amount,
        claim_types: processed_types,
    };

    // Emit events
    events::emit_claims_processed(e, user, &result, &processed_claims);

    result
}

/// Clean up expired claims for a user (can be called by anyone for gas efficiency)
///
/// # Arguments
/// * `e` - Contract environment
/// * `user` - Address to clean up claims for
///
/// # Returns
/// Number of expired claims removed
pub fn cleanup_expired_claims(e: &Env, user: &Address) -> u32 {
    let now = e.ledger().timestamp();
    let claims = get_pending_claims(e, user);

    if claims.is_empty() {
        return 0;
    }

    let mut valid_claims = Vec::new(e);
    let mut expired_amount = 0i128;
    let mut expired_count = 0u32;

    for i in 0..claims.len() {
        let claim = claims.get(i).unwrap();

        if claim.expires_at > 0 && now > claim.expires_at {
            expired_amount = expired_amount
                .checked_add(claim.amount)
                .expect("expired amount overflow");
            expired_count += 1;
        } else {
            valid_claims.push_back(claim);
        }
    }

    if expired_count > 0 {
        // Update storage
        if valid_claims.is_empty() {
            e.storage()
                .persistent()
                .remove(&DataKey::PendingClaims(user.clone()));
            e.storage()
                .persistent()
                .remove(&DataKey::ClaimableAmount(user.clone()));
        } else {
            e.storage()
                .persistent()
                .set(&DataKey::PendingClaims(user.clone()), &valid_claims);

            let remaining_amount = get_claimable_amount(e, user)
                .checked_sub(expired_amount)
                .expect("claimable amount underflow");

            e.storage()
                .persistent()
                .set(&DataKey::ClaimableAmount(user.clone()), &remaining_amount);
        }

        // Emit event
        events::emit_claims_expired(e, user, expired_count, expired_amount);
    }

    expired_count
}

/// Get claims summary for a user
///
/// # Arguments
/// * `e` - Contract environment
/// * `user` - Address to get summary for
///
/// # Returns
/// Map of claim types to total amounts
pub fn get_claims_summary(e: &Env, user: &Address) -> Map<ClaimType, i128> {
    let claims = get_pending_claims(e, user);
    let mut summary = Map::new(e);

    for i in 0..claims.len() {
        let claim = claims.get(i).unwrap();
        let current = summary.get(claim.claim_type).unwrap_or(0);
        summary.set(claim.claim_type, current + claim.amount);
    }

    summary
}

/// Permissionless, bounded sweep to expire stale pending claims.
///
/// Scans up to `max_iter` of the user's pending claims, removes those past
/// `expires_at`, and returns the count pruned. Never touches claims with
/// `expires_at == 0` (no expiry) or processed claims.
///
/// # Arguments
/// * `e` - Contract environment
/// * `user` - Address whose claims to prune
/// * `max_iter` - Maximum number of claims to scan (hard-capped at MAX_BATCH_CLAIMS)
///
/// # Returns
/// Number of expired claims removed
pub fn expire_claims_bounded(e: &Env, user: &Address, max_iter: u32) -> u32 {
    let now = e.ledger().timestamp();
    let claims = get_pending_claims(e, user);

    if claims.is_empty() {
        return 0;
    }

    // Cap the iteration limit at MAX_BATCH_CLAIMS for gas safety
    let limit = if max_iter == 0 {
        MAX_BATCH_CLAIMS
    } else {
        max_iter.min(MAX_BATCH_CLAIMS)
    };

    let mut valid_claims = Vec::new(e);
    let mut expired_amount = 0i128;
    let mut expired_count = 0u32;
    let mut scan_count = 0u32;

    // Scan up to `limit` claims, preserving order
    #[allow(clippy::explicit_counter_loop)]
    for i in 0..claims.len() {
        if scan_count >= limit {
            // Add remaining unscanned claims to output
            for j in i..claims.len() {
                valid_claims.push_back(claims.get(j).unwrap());
            }
            break;
        }

        let claim = claims.get(i).unwrap();
        scan_count += 1;

        // Skip claims with no expiry (expires_at == 0) or already processed
        if claim.expires_at == 0 || claim.processed {
            valid_claims.push_back(claim);
            continue;
        }

        // Check if expired
        if now > claim.expires_at {
            expired_amount = expired_amount
                .checked_add(claim.amount)
                .expect("expired amount overflow");
            expired_count += 1;
            // Don't add to valid_claims; this removes it
        } else {
            valid_claims.push_back(claim);
        }
    }

    if expired_count > 0 {
        // Update storage with the pruned claims
        if valid_claims.is_empty() {
            e.storage()
                .persistent()
                .remove(&DataKey::PendingClaims(user.clone()));
            e.storage()
                .persistent()
                .remove(&DataKey::ClaimableAmount(user.clone()));
        } else {
            e.storage()
                .persistent()
                .set(&DataKey::PendingClaims(user.clone()), &valid_claims);

            let remaining_amount = get_claimable_amount(e, user)
                .checked_sub(expired_amount)
                .expect("claimable amount underflow");

            e.storage()
                .persistent()
                .set(&DataKey::ClaimableAmount(user.clone()), &remaining_amount);
        }

        // Emit event with pruned count
        events::emit_claims_expired(e, user, expired_count, expired_amount);
    }

    expired_count
}
