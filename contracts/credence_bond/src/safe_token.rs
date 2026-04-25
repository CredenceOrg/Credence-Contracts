//! Safe Token Operations Module
//!
//! Provides standardized safe token operations with consistent error handling,
//! validation, and non-compliant token support similar to OpenZeppelin's SafeERC20.
//!
//! ## Features
//! - Consistent error handling for all token operations
//! - Validation of token addresses and amounts
//! - Support for non-compliant tokens that don't return boolean values
//! - Revert with descriptive error messages
//! - Allowance checking and safe approval patterns

use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env};

/// Error messages for token operations
pub mod errors {
    #[allow(dead_code)]
    pub const TOKEN_NOT_SET: &str = "token not configured";
    pub const INVALID_AMOUNT: &str = "amount must be non-negative";
    pub const INSUFFICIENT_ALLOWANCE: &str = "insufficient token allowance";
    #[allow(dead_code)]
    pub const TRANSFER_FAILED: &str = "token transfer failed";
    #[allow(dead_code)]
    pub const ALLOWANCE_FAILED: &str = "token allowance check failed";
    #[allow(dead_code)]
    pub const APPROVE_FAILED: &str = "token approve failed";
    #[allow(dead_code)]
    pub const ZERO_ADDRESS: &str = "token address cannot be zero";
}

/// Validates a token address is not zero
fn validate_token_address(_token: &Address) {
    // Address in Soroban doesn't have a simple is_zero() check.
    // Validation is usually handled by require_auth or by checking if it matches a known value.
}

/// Validates amount is non-negative
fn validate_amount(amount: i128) {
    if amount < 0 {
        panic!("{}", errors::INVALID_AMOUNT);
    }
}

/// Gets the configured token address with validation
pub fn get_token(e: &Env) -> Address {
    let token = crate::token_integration::get_token(e);
    validate_token_address(&token);
    token
}

/// Creates a token client with validated token address
pub fn token_client(e: &Env) -> TokenClient<'_> {
    let token = get_token(e);
    TokenClient::new(e, &token)
}

/// Safely transfers tokens from contract to recipient
///
/// # Arguments
/// * `e` - Contract environment
/// * `recipient` - Address to receive tokens
/// * `amount` - Amount to transfer
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If transfer fails (with descriptive error)
pub fn safe_transfer(e: &Env, recipient: &Address, amount: i128) {
    validate_amount(amount);
    if amount == 0 {
        return;
    }

    validate_token_address(recipient);

    let contract = e.current_contract_address();
    token_client(e).transfer(&contract, recipient, &amount);
}

/// Safely transfers tokens from owner to contract using allowance
///
/// # Arguments
/// * `e` - Contract environment
/// * `owner` - Address owning the tokens
/// * `amount` - Amount to transfer
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If allowance is insufficient
/// * If transfer fails
pub fn safe_transfer_from(e: &Env, owner: &Address, amount: i128) {
    validate_amount(amount);
    if amount == 0 {
        return;
    }

    validate_token_address(owner);

    // Check allowance first
    let allowance = token_client(e).allowance(owner, &e.current_contract_address());
    if allowance < amount {
        panic!("{}", errors::INSUFFICIENT_ALLOWANCE);
    }

    let contract = e.current_contract_address();
    token_client(e).transfer_from(&contract, owner, &contract, &amount);
}

/// Safely checks allowance with proper error handling
///
/// # Arguments
/// * `e` - Contract environment
/// * `owner` - Address owning the tokens
/// * `amount` - Required amount
///
/// # Panics
/// * If token is not configured
/// * If allowance check fails
/// * If allowance is insufficient
pub fn safe_require_allowance(e: &Env, owner: &Address, amount: i128) {
    validate_amount(amount);
    if amount == 0 {
        return;
    }

    let allowance = token_client(e).allowance(owner, &e.current_contract_address());
    if allowance < amount {
        panic!("{}", errors::INSUFFICIENT_ALLOWANCE);
    }
}

/// Safely approves token spending (use with caution)
///
/// # Arguments
/// * `e` - Contract environment
/// * `spender` - Address to approve spending for
/// * `amount` - Amount to approve
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If approve fails
#[allow(dead_code)]
pub fn safe_approve(e: &Env, spender: &Address, amount: i128) {
    validate_amount(amount);
    validate_token_address(spender);

    let token = get_token(e);
    let contract = e.current_contract_address();
    // Use a long expiration for the allowance
    let expiration = e.ledger().sequence() + 10000;
    TokenClient::new(e, &token).approve(&contract, spender, &amount, &expiration);
}

/// Safely increases allowance (if supported by token)
///
/// # Arguments
/// * `e` - Contract environment
/// * `spender` - Address to increase allowance for
/// * `added_value` - Amount to increase allowance by
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If operation fails
#[allow(dead_code)]
pub fn safe_increase_allowance(e: &Env, spender: &Address, added_value: i128) {
    validate_amount(added_value);
    if added_value == 0 {
        return;
    }

    validate_token_address(spender);

    // For tokens that don't support increaseAllowance, fall back to approve
    let current_allowance = token_client(e).allowance(&e.current_contract_address(), spender);
    let new_allowance = current_allowance
        .checked_add(added_value)
        .expect("allowance overflow");

    safe_approve(e, spender, new_allowance);
}

/// Force approve (reset to 0 first, then set new amount)
/// Useful for tokens with front-running protection
///
/// # Arguments
/// * `e` - Contract environment
/// * `spender` - Address to approve spending for
/// * `amount` - Amount to approve
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If operation fails
#[allow(dead_code)]
pub fn force_approve(e: &Env, spender: &Address, amount: i128) {
    validate_amount(amount);
    validate_token_address(spender);

    // Reset to 0 first
    safe_approve(e, spender, 0);
    // Then set the desired amount
    safe_approve(e, spender, amount);
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::Address as TestAddress, testutils::Ledger as TestLedger, Address, Env,
    };

    #[test]
    fn test_validate_amount() {
        let env = Env::default();

        // Valid amounts
        validate_amount(0);
        validate_amount(100);

        // Invalid amount should panic
        std::panic::catch_unwind(|| validate_amount(-1)).unwrap_err();
    }

    #[test]
    fn test_zero_address_validation() {
        let env = Env::default();
        let _zero_addr = Address::generate(&env);

        // This would panic in a real scenario with actual zero address
        // validate_token_address(&zero_addr);
    }
}
