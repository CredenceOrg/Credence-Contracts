//! Bond Amount Validation Module
//!
//! Provides validation functions for bond amounts to ensure they fall within acceptable ranges.
//! This module centralizes the validation logic for minimum and maximum bond amounts.

use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Address, Env};

// ─── Address Validation ─────────────────────────────────────────────────────

/// Validates that a recipient address is valid for token transfers.
///
/// # Arguments
/// * `recipient` - The address to validate
/// * `contract` - The contract's own address (to prevent self-transfers)
///
/// # Panics
/// * `"recipient cannot be the contract itself"` if recipient equals the contract
///
/// # Security Note
/// Transferring tokens to an invalid or inappropriate recipient can result in
/// permanent loss of tokens. This validation provides defense-in-depth by:
///
/// 1. Preventing self-transfers (contract sending to itself) which could
///    cause accounting inconsistencies or reentrancy issues.
/// 2. Documenting the requirement that all recipients must be validated.
///
/// Note: Unlike Ethereum, Soroban does not have a "zero address" concept.
/// Addresses in Soroban are validated by the framework through the auth system.
/// The primary validation is that recipients should be able to receive tokens.
/// This function provides explicit checking at transfer call sites.
#[allow(dead_code)]
pub fn validate_recipient(recipient: &Address, contract: &Address) {
    // Prevent self-transfers: the contract should not transfer tokens to itself
    // as this could cause accounting issues or be a sign of a logic error.
    if recipient == contract {
        panic!("recipient cannot be the contract itself");
    }

    // Note: In Soroban, addresses are validated through the auth system.
    // We don't need to check for "zero address" as that concept doesn't exist.
    // The require_auth() calls in the calling code provide the primary validation.
}

/// Minimum bond amount accepted by the bond contract test suite.
pub const MIN_BOND_AMOUNT: i128 = 1_000;

/// Maximum bond amount (100 million USDC with 6 decimals = 100_000_000_000_000)
pub const MAX_BOND_AMOUNT: i128 = 100_000_000_000_000; // 100M tokens (assuming 6 decimals)

/// Validates that a bond amount is within acceptable bounds.
///
/// # Arguments
/// * `amount` - The bond amount to validate
///
/// # Panics
/// * If amount is less than MIN_BOND_AMOUNT
/// * If amount is greater than MAX_BOND_AMOUNT
/// * If amount is negative
pub fn validate_bond_amount(e: &Env, amount: i128) {
    if amount < 0 {
        panic_with_error!(e, ContractError::InvalidBondInput);
    }

    if amount < MIN_BOND_AMOUNT {
        panic_with_error!(e, ContractError::InvalidBondInput);
    }

    if amount > MAX_BOND_AMOUNT {
        panic_with_error!(e, ContractError::InvalidBondInput);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    #[test]
    fn test_validate_bond_amount_valid() {
        // Test valid amounts within range
        let e = Env::default();
        validate_bond_amount(&e, MIN_BOND_AMOUNT);
        validate_bond_amount(&e, MAX_BOND_AMOUNT);
        validate_bond_amount(&e, (MIN_BOND_AMOUNT + MAX_BOND_AMOUNT) / 2);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #214)")]
    fn test_validate_bond_amount_below_minimum() {
        let e = Env::default();
        validate_bond_amount(&e, MIN_BOND_AMOUNT - 1);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #214)")]
    fn test_validate_bond_amount_zero() {
        let e = Env::default();
        validate_bond_amount(&e, 0);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #214)")]
    fn test_validate_bond_amount_negative() {
        let e = Env::default();
        validate_bond_amount(&e, -1);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #214)")]
    fn test_validate_bond_amount_above_maximum() {
        let e = Env::default();
        validate_bond_amount(&e, MAX_BOND_AMOUNT + 1);
    }

    // ─── Address Validation Tests ─────────────────────────────────────────

    #[test]
    fn test_validate_recipient_valid() {
        let env = Env::default();
        let recipient = Address::generate(&env);
        let contract = Address::generate(&env);
        // Should not panic for valid, different addresses
        validate_recipient(&recipient, &contract);
    }

    #[test]
    #[should_panic(expected = "recipient cannot be the contract itself")]
    fn test_validate_recipient_self_transfer() {
        let env = Env::default();
        let address = Address::generate(&env);
        // Should panic when recipient equals contract
        validate_recipient(&address, &address);
    }
}

// Duration Validation Module
//
// Provides validation logic for bond durations including minimum and maximum limit
// enforcement. All bond creations must pass duration validation before proceeding.
//
// Constraints:
// - Minimum Duration: Bonds must have a duration of at least 1 day (86_400 seconds)
//   to prevent trivially short bonds that offer no meaningful commitment.
// - Maximum Duration: Bonds are capped at 365 days (31_536_000 seconds) to limit
//   excessive lock-up risk and contract state lifetime.

/// Minimum bond duration in seconds (1 day = 86_400 seconds).
pub const MIN_BOND_DURATION: u64 = 86_400;

/// Maximum bond duration in seconds (365 days = 31_536_000 seconds).
pub const MAX_BOND_DURATION: u64 = 31_536_000;

/// Validate that a bond duration falls within the allowed range.
///
/// # Arguments
/// * `duration` - The bond duration in seconds to validate.
///
/// # Panics
/// * `"bond duration too short: minimum is 86400 seconds (1 day)"` if `duration` < `MIN_BOND_DURATION`
/// * `"bond duration too long: maximum is 31536000 seconds (365 days)"` if `duration` > `MAX_BOND_DURATION`
pub fn validate_bond_duration(e: &Env, duration: u64) {
    if duration < MIN_BOND_DURATION {
        panic_with_error!(e, ContractError::InvalidBondInput);
    }
    if duration > MAX_BOND_DURATION {
        panic_with_error!(e, ContractError::InvalidBondInput);
    }
}
