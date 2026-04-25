//! Address Validation Module
//!
//! Provides validation functions for recipient addresses to prevent transfers
//! to invalid or inappropriate addresses.

use soroban_sdk::Address;

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

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};
    use soroban_sdk::testutils::Address as _;

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
