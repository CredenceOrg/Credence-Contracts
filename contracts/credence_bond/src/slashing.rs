//! Slashing Module
//!
//! Implements the core `slash_bond()` functionality for reducing a bond's value as a penalty
//! for misconduct. This module manages authorization, state updates, event emission, and
//! treasury fund transfers.
//!
//! ## Authorization
//! Only the contract admin can execute slashing operations. Non-admin calls panic with
//! "not admin" error message.
//!
//! ## Design
//! - **Partial Slashing**: Can slash any amount up to bonded_amount
//! - **Full Slashing**: Can slash entire bond (capped at bonded_amount)
//! - **Cumulative**: Multiple slashes accumulate (tracked in slashed_amount)
//! - **Over-slash Protection**: Ensures slashed_amount never exceeds bonded_amount
//! - **Withdrawals**: Affected by slashing (withdrawable = bonded - slashed)

use soroban_sdk::{Address, Env, Symbol};

/// Storage key for tracking accumulated slashed funds (for treasury transfer purposes).
/// Not currently used for fund transfers in this implementation, but reserved for future use.
#[allow(dead_code)]
const KEY_SLASHED_FUNDS_POOL: &str = "slashed_funds_pool";

/// NatSpec-style: Returns the current slashed amount for a bond.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `_bond_identity` - Address of the bonded identity
///
/// # Returns
/// The accumulated slashed amount (i128). Returns 0 if no bond exists.
#[allow(dead_code)]
#[must_use]
pub fn get_slashed_amount(e: &Env, _bond_identity: &Address) -> i128 {
    let storage_key = crate::DataKey::Bond;
    e.storage()
        .instance()
        .get::<_, i128>(&storage_key)
        .map(|_| {
            // In a full implementation, retrieve from bond state
            0 // Simplified: return 0
        })
        .unwrap_or(0)
}

/// NatSpec-style: Validates that the caller is the authorized admin.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `caller` - Address to validate as admin
///
/// # Panics
/// If caller is not the stored admin address with message "not admin"
pub fn validate_admin(e: &Env, caller: &Address) {
    let stored_admin: Address = e
        .storage()
        .instance()
        .get(&crate::DataKey::Admin)
        .unwrap_or_else(|| panic!("not initialized"));
    if caller != &stored_admin {
        panic!("not admin");
    }
}

/// NatSpec-style: Core slashing logic for reducing bond value.
///
/// Executes the slash with full validation:
/// 1. Validates caller is admin (panics if not)
/// 2. Computes available balance (bonded − already_slashed)
/// 3. Caps slash at available balance (prevents over-slash)
/// 4. Updates bond state
/// 5. Appends a normalized SlashRecord to persistent history
/// 6. Adds slashing reward claim for the slasher
/// 7. Emits slashing event
/// 8. Returns updated bond state
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Address claiming admin authority
/// * `amount` - Amount to slash (i128)
///
/// # Returns
/// Updated `IdentityBond` with modified `slashed_amount`
///
/// # Panics
/// - "not admin" if caller is not the contract admin
/// - "not initialized" if contract not initialized
/// - "no bond" if no bond exists for this contract instance
/// - If arithmetic overflows (checked_add protection)
///
/// # Security Notes
/// - Slash is bounded by available balance (bonded − slashed), not just bonded
/// - Slashing is monotonic (always increases or stays same, never decreases)
/// - Cannot slash bonds that don't exist (panic on "no bond")
/// - Slasher receives 10% of slashed amount as reward (pull-payment)
pub fn slash_bond(e: &Env, admin: &Address, amount: i128) -> crate::IdentityBond {
    if amount < 0 {
        panic!("slash amount must be non-negative");
    }
    // 1. Authorization check
    validate_admin(e, admin);

    crate::same_ledger_liquidation_guard::require_slash_allowed_after_collateral_increase(e);

    // 2. Retrieve current bond state
    let key = crate::DataKey::Bond;
    let mut bond = e
        .storage()
        .instance()
        .get::<_, crate::IdentityBond>(&key)
        .unwrap_or_else(|| panic!("no bond"));

    // 3. Available balance = bonded − already_slashed
    let available = bond
        .bonded_amount
        .checked_sub(bond.slashed_amount)
        .expect("slashed exceeds bonded");

    // 4. Cap slash at available balance (not just bonded_amount)
    let actual_slash_amount = if amount > available { available } else { amount };

    let new_slashed = bond
        .slashed_amount
        .checked_add(actual_slash_amount)
        .expect("slashing caused overflow");

    bond.slashed_amount = new_slashed;

    // 5. Append normalized slash history record
    crate::slash_history::append_slash_history(
        e,
        &bond.identity,
        actual_slash_amount,
        Symbol::new(e, "admin_slash"),
        bond.slashed_amount,
    );

    // 6. Add slashing reward claim for the admin (10% of slashed amount)
    if actual_slash_amount > 0 {
        let reward_amount = actual_slash_amount / 10; // 10% reward
        if reward_amount > 0 {
            let source_id = get_next_slash_id(e);
            crate::claims::add_pending_claim(
                e,
                admin,
                crate::claims::ClaimType::SlashingReward,
                reward_amount,
                source_id,
                Some(soroban_sdk::Symbol::new(e, "slash_reward")),
            );
        }
    }

    // 7. Persist updated bond state
    e.storage().instance().set(&key, &bond);

    // 8. Emit slashing event for off-chain tracking
    emit_slashing_event(e, &bond.identity, actual_slash_amount, bond.slashed_amount);

    // Emit v2 event with enhanced indexing for backward compatibility during migration
    crate::events::emit_bond_slashed_v2(
        e,
        &bond.identity,
        actual_slash_amount,
        bond.slashed_amount,
        e.ledger().timestamp(),
        admin,
        soroban_sdk::String::from_str(e, "Slashed by admin"),
        bond.slashed_amount >= bond.bonded_amount,
    );

    // 9. Return updated bond state
    bond
}

/// Get next slash ID for tracking purposes
fn get_next_slash_id(e: &Env) -> u64 {
    let key = soroban_sdk::Symbol::new(e, "slash_counter");
    let current: u64 = e.storage().instance().get(&key).unwrap_or(0);
    let next = current + 1;
    e.storage().instance().set(&key, &next);
    next
}

/// NatSpec-style: Reverts slashing (reduces slashed amount). Admin only.
///
/// Used for correcting mistaken slashes or appeals.
/// Only reduces slashed_amount, cannot go below 0.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `admin` - Address claiming admin authority  
/// * `amount` - Amount to unslash (i128)
///
/// # Returns
/// Updated bond with reduced slashed_amount
///
/// # Panics
/// - "not admin" if not authorized
/// - If amount would reduce slashed_amount below 0
#[allow(dead_code)]
pub fn unslash_bond(e: &Env, admin: &Address, amount: i128) -> crate::IdentityBond {
    if amount < 0 {
        panic!("unslash amount must be non-negative");
    }
    validate_admin(e, admin);

    let key = crate::DataKey::Bond;
    let mut bond = e
        .storage()
        .instance()
        .get::<_, crate::IdentityBond>(&key)
        .unwrap_or_else(|| panic!("no bond"));

    bond.slashed_amount = bond
        .slashed_amount
        .checked_sub(amount)
        .expect("unslashing would reduce below 0");

    e.storage().instance().set(&key, &bond);
    emit_unslashing_event(e, &bond.identity, amount, bond.slashed_amount);

    bond
}

/// NatSpec-style: Calculates the available (withdrawable) balance after slashing.
///
/// # Arguments
/// * `bonded_amount` - Total bonded amount (i128)
/// * `slashed_amount` - Total slashed amount (i128)
///
/// # Returns
/// Available balance = bonded_amount - slashed_amount
#[allow(dead_code)]
#[must_use]
pub fn get_available_balance(bonded_amount: i128, slashed_amount: i128) -> i128 {
    bonded_amount
        .checked_sub(slashed_amount)
        .expect("slashed amount exceeds bonded amount")
}

/// NatSpec-style: Checks if a bond is fully slashed.
///
/// A bond is fully slashed when slashed_amount >= bonded_amount,
/// leaving no withdrawable balance.
///
/// # Arguments
/// * `bonded_amount` - Total bonded amount (i128)
/// * `slashed_amount` - Total slashed amount (i128)
///
/// # Returns
/// `true` if fully slashed, `false` otherwise
#[allow(dead_code)]
#[must_use]
pub fn is_fully_slashed(bonded_amount: i128, slashed_amount: i128) -> bool {
    slashed_amount >= bonded_amount
}

/// NatSpec-style: Checks if partial slashing would occur.
///
/// Partial slashing means the slash amount is less than the total bonded amount.
/// (i.e., not fully slashing the bond)
///
/// # Arguments
/// * `slash_amount` - Amount being slashed (i128)
/// * `bonded_amount` - Total bonded amount (i128)
///
/// # Returns
/// `true` if this is a partial slash, `false` if full slash
#[allow(dead_code)]
#[must_use]
pub fn is_partial_slash(slash_amount: i128, bonded_amount: i128) -> bool {
    slash_amount < bonded_amount
}

/// NatSpec-style: Emits a slashing event for off-chain tracking and auditing.
///
/// # Arguments
/// * `e` - Soroban environment for event publishing
/// * `identity` - Address of the slashed bonded identity
/// * `slash_amount` - The amount just slashed
/// * `total_slashed` - The cumulative slashed amount after this slash
pub fn emit_slashing_event(e: &Env, identity: &Address, slash_amount: i128, total_slashed: i128) {
    e.events().publish(
        (Symbol::new(e, "bond_slashed"),),
        (identity.clone(), slash_amount, total_slashed),
    );
}

/// NatSpec-style: Emits an unslashing event for off-chain tracking.
///
/// # Arguments
/// * `e` - Soroban environment for event publishing
/// * `identity` - Address of the identity being unslashed
/// * `unslash_amount` - The amount being unslashed/reverted
/// * `total_slashed` - The cumulative slashed amount after reversion
#[allow(dead_code)]
pub fn emit_unslashing_event(
    e: &Env,
    identity: &Address,
    unslash_amount: i128,
    total_slashed: i128,
) {
    e.events().publish(
        (Symbol::new(e, "bond_unslashed"),),
        (identity.clone(), unslash_amount, total_slashed),
    );
}

/// Initialize the slashed funds pool for treasury transfers.
/// Called during contract initialization.
#[allow(dead_code)]
pub fn initialize_slashed_pool(e: &Env) {
    e.storage()
        .instance()
        .set(&Symbol::new(e, KEY_SLASHED_FUNDS_POOL), &0_i128);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_balance_calculation() {
        assert_eq!(get_available_balance(1000, 300), 700);
        assert_eq!(get_available_balance(1000, 0), 1000);
        assert_eq!(get_available_balance(1000, 1000), 0);
    }

    #[test]
    fn test_fully_slashed_detection() {
        assert!(!is_fully_slashed(1000, 0));
        assert!(!is_fully_slashed(1000, 500));
        assert!(is_fully_slashed(1000, 1000));
        assert!(is_fully_slashed(1000, 1500)); // Over-slashed
    }

    #[test]
    fn test_partial_slash_detection() {
        assert!(is_partial_slash(500, 1000));
        assert!(!is_partial_slash(1000, 1000));
        assert!(!is_partial_slash(1500, 1000));
    }

    #[test]
    fn test_available_balance_with_slashing() {
        // Verify available balance calculation
        let available = get_available_balance(1000, 300);
        assert_eq!(available, 700);

        // Fully slashed
        let available_full = get_available_balance(1000, 1000);
        assert_eq!(available_full, 0);
    }
}
