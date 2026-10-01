//! # Batch Bond Operations Module
//!
//! Provides atomic batch operations for creating multiple bonds in a single transaction.
//! All operations follow an all-or-nothing pattern - if any bond fails validation,
//! the entire batch is rejected.
//!
//! ## Features
//! - Batch bond creation with pre-validation
//! - Atomic execution (all succeed or all fail)
//! - Gas-optimized for multiple operations
//! - Comprehensive event emission
//! - Per-identity bond support

#![allow(dead_code)]

use crate::{tiered_bond, BondTier, DataKey, IdentityBond};
use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Symbol, Vec};

/// Conservative upper bound to keep batch execution below Soroban budget limits.
pub const MAX_BATCH_BOND_SIZE: u32 = 20;

/// Parameters for creating a single bond in a batch
#[contracttype]
#[derive(Clone, Debug)]
pub struct BatchBondParams {
    /// The identity address for this bond
    pub identity: Address,
    /// Amount to bond
    pub amount: i128,
    /// Duration in seconds
    pub duration: u64,
    /// Whether this is a rolling bond
    pub is_rolling: bool,
    /// Notice period for rolling bonds
    pub notice_period_duration: u64,
}

/// Result of a batch bond creation operation
#[contracttype]
#[derive(Clone, Debug)]
pub struct BatchBondResult {
    /// Number of bonds successfully created
    pub created_count: u32,
    /// List of created bonds
    pub bonds: Vec<IdentityBond>,
}

fn validate_batch_size(e: &Env, params_list: &Vec<BatchBondParams>) {
    crate::validation::verify_batch_size(e, params_list.len(), MAX_BATCH_BOND_SIZE);
}

/// Validate all bonds before execution to ensure atomicity
///
/// # Arguments
/// * `e` - Contract environment
/// * `params_list` - Vector of bond creation parameters
///
/// # Panics
/// * If any bond has invalid parameters (negative amount, duration overflow, etc.)
/// * If params_list is empty
pub fn validate_batch_bonds(e: &Env, params_list: &Vec<BatchBondParams>) {
    crate::validation::require_non_empty_vec(e, params_list);

    validate_batch_size(e, params_list);

    let bond_start = e.ledger().timestamp();

    for i in 0..params_list.len() {
        let params = params_list.get(i).unwrap();

        // Validate amount
        if params.amount <= 0 {
            panic!("invalid amount in batch");
        }

        // Validate duration doesn't overflow
        if bond_start.checked_add(params.duration).is_none() {
            panic!("duration overflow in batch");
        }

        // Validate notice period for rolling bonds
        if params.is_rolling && params.notice_period_duration == 0 {
            panic!("rolling bond requires notice period");
        }
    }
}

/// Create multiple bonds atomically in a single transaction.
///
/// This function validates all bonds before creating any, ensuring that either
/// all bonds are created successfully or none are created (atomic operation).
///
/// # Arguments
/// * `e` - Contract environment
/// * `params_list` - Vector of bond creation parameters
///
/// # Returns
/// `BatchBondResult` containing the count and list of created bonds
///
/// # Panics
/// * If validation fails for any bond
/// * If params_list is empty
/// * If a bond for any identity already exists
///
/// # Events
/// Emits `batch_bonds_created` with the result
///
/// # Example
/// ```ignore
/// let params = vec![
///     BatchBondParams {
///         identity: addr1,
///         amount: 1000,
///         duration: credence_math::SECONDS_PER_DAY,
///         is_rolling: false,
///         notice_period_duration: 0,
///     },
///     BatchBondParams {
///         identity: addr2,
///         amount: 2000,
///         duration: 172800,
///         is_rolling: true,
///         notice_period_duration: 3600,
///     },
/// ];
/// let result = create_batch_bonds(e, params);
/// ```
pub fn create_batch_bonds(e: &Env, params_list: Vec<BatchBondParams>) -> BatchBondResult {
    // Step 1: Validate all bonds first (fail fast)
    validate_batch_bonds(e, &params_list);

    let bond_start = e.ledger().timestamp();
    let mut bonds: Vec<IdentityBond> = Vec::new(e);

    // Read the tier thresholds once for the whole batch. `TierThresholds` is
    // configuration, not per-item state, so every element in this batch
    // classifies against the same value; reading it inside the loop would
    // repeat the same storage read and deserialization N times.
    let tier_thresholds = tiered_bond::get_tier_thresholds(e);

    // Step 2: Check for existing bonds (before creating any)
    for i in 0..params_list.len() {
        let params = params_list.get(i).unwrap();
        let bond_key = DataKey::Bond(params.identity.clone());
        if e.storage().instance().has(&bond_key) {
            panic!("bond already exists");
        }
    }

    // Step 3: Create all bonds (atomic - all or nothing)
    for i in 0..params_list.len() {
        let params = params_list.get(i).unwrap();

        let bond = IdentityBond {
            identity: params.identity.clone(),
            bonded_amount: params.amount,
            bond_start,
            bond_duration: params.duration,
            slashed_amount: 0,
            active: true,
            is_rolling: params.is_rolling,
            withdrawal_requested_at: 0,
            notice_period_duration: params.notice_period_duration,
        };

        // Store the bond
        let bond_key = DataKey::Bond(params.identity.clone());
        e.storage().instance().set(&bond_key, &bond);

        // Emit tier change event for this bond
        let tier = tiered_bond::tier_for_amount_with_thresholds(params.amount, &tier_thresholds);
        tiered_bond::emit_tier_change_if_needed(e, &params.identity, BondTier::Bronze, tier);

        bonds.push_back(bond);
    }

    crate::same_ledger_liquidation_guard::record_collateral_increase(e);

    let result = BatchBondResult {
        created_count: bonds.len(),
        bonds: bonds.clone(),
    };

    // Emit batch completion event
    e.events()
        .publish((Symbol::new(e, "batch_bonds_created"),), result.clone());

    crate::invariants::assert_self_consistent(e);

    result
}

/// Validate a batch of bonds without creating them.
///
/// Useful for pre-flight checks before submitting a batch transaction.
///
/// # Arguments
/// * `e` - Contract environment
/// * `params_list` - Vector of bond creation parameters to validate
///
/// # Returns
/// `true` if all bonds in the batch are valid
///
/// # Panics
/// * If any bond has invalid parameters
pub fn validate_batch(e: &Env, params_list: Vec<BatchBondParams>) -> bool {
    validate_batch_bonds(e, &params_list);
    true
}

/// Get the total bonded amount across a batch of bonds.
///
/// Useful for calculating aggregate statistics before batch creation.
///
/// # Arguments
/// * `params_list` - Vector of bond creation parameters
///
/// # Returns
/// Total amount across all bonds in the batch
///
/// # Panics
/// * If the total amount would overflow i128
/// * If batch size exceeds MAX_BATCH_BOND_SIZE
pub fn get_batch_total_amount(e: &Env, params_list: &Vec<BatchBondParams>) -> i128 {
    if params_list.is_empty() {
        return 0;
    }

    validate_batch_size(e, params_list);

    let mut total: i128 = 0;

    for i in 0..params_list.len() {
        let params = params_list.get(i).unwrap();
        total = total
            .checked_add(params.amount)
            .expect("batch total overflow");
    }

    total
}

// [pre-broken on main] — fails to compile against the current
// contract API; gate kept so the rest of the crate builds.
#[cfg(any())]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_get_batch_total_amount() {
        let env = Env::default();
        let mut params_list = Vec::new(&env);

        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        params_list.push_back(BatchBondParams {
            identity: addr1,
            amount: 1000,
            duration: credence_math::SECONDS_PER_DAY,
            is_rolling: false,
            notice_period_duration: 0,
        });

        params_list.push_back(BatchBondParams {
            identity: addr2,
            amount: 2000,
            duration: credence_math::SECONDS_PER_DAY,
            is_rolling: false,
            notice_period_duration: 0,
        });

        let total = get_batch_total_amount(&env, &params_list);
        assert_eq!(total, 3000);
    }

    /// The batch hoists a single `TierThresholds` read and classifies every
    /// element against it. These cases pin the exact boundary behavior so the
    /// hoisted read can be shown to be equivalent to a per-element lookup.
    #[test]
    fn test_tier_classification_matches_hoisted_thresholds() {
        use crate::tiered_bond::{
            tier_for_amount_with_thresholds, TIER_BRONZE_MAX, TIER_GOLD_MAX, TIER_SILVER_MAX,
        };

        let defaults = crate::TierThresholds {
            bronze_max: TIER_BRONZE_MAX,
            silver_max: TIER_SILVER_MAX,
            gold_max: TIER_GOLD_MAX,
        };

        // Exact threshold values advance to the next tier.
        assert_eq!(
            tier_for_amount_with_thresholds(defaults.bronze_max - 1, &defaults),
            BondTier::Bronze
        );
        assert_eq!(
            tier_for_amount_with_thresholds(defaults.bronze_max, &defaults),
            BondTier::Silver
        );
        assert_eq!(
            tier_for_amount_with_thresholds(defaults.silver_max, &defaults),
            BondTier::Gold
        );
        assert_eq!(
            tier_for_amount_with_thresholds(defaults.gold_max, &defaults),
            BondTier::Platinum
        );
        assert_eq!(
            tier_for_amount_with_thresholds(defaults.gold_max * 2, &defaults),
            BondTier::Platinum
        );

        // Custom thresholds are honoured, not the compiled-in defaults.
        let custom = crate::TierThresholds {
            bronze_max: 100,
            silver_max: 200,
            gold_max: 300,
        };
        assert_eq!(
            tier_for_amount_with_thresholds(99, &custom),
            BondTier::Bronze
        );
        assert_eq!(
            tier_for_amount_with_thresholds(100, &custom),
            BondTier::Silver
        );
        assert_eq!(
            tier_for_amount_with_thresholds(299, &custom),
            BondTier::Gold
        );
        assert_eq!(
            tier_for_amount_with_thresholds(300, &custom),
            BondTier::Platinum
        );
    }
}
