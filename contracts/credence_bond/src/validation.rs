//! Bond Amount Validation Module
//!
//! Provides validation functions for bond amounts to ensure they fall within acceptable ranges.
//! This module centralizes the validation logic for minimum and maximum bond amounts.

/// Minimum bond amount (1 USDC with 6 decimals = 1_000_000)
pub const MIN_BOND_AMOUNT: i128 = 1_000_000; // 1 token (assuming 6 decimals like USDC)

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
pub fn validate_bond_amount(amount: i128) {
    if amount < 0 {
        panic!("bond amount cannot be negative");
    }
    
    if amount < MIN_BOND_AMOUNT {
        panic!("bond amount below minimum required: {} (minimum: {})", amount, MIN_BOND_AMOUNT);
    }
    
    if amount > MAX_BOND_AMOUNT {
        panic!("bond amount exceeds maximum allowed: {} (maximum: {})", amount, MAX_BOND_AMOUNT);
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_bond_amount_valid() {
        // Test valid amounts within range
        validate_bond_amount(MIN_BOND_AMOUNT);
        validate_bond_amount(MAX_BOND_AMOUNT);
        validate_bond_amount((MIN_BOND_AMOUNT + MAX_BOND_AMOUNT) / 2);
    }

    #[test]
    #[should_panic(expected = "bond amount below minimum required")]
    fn test_validate_bond_amount_below_minimum() {
        validate_bond_amount(MIN_BOND_AMOUNT - 1);
    }

    #[test]
    #[should_panic(expected = "bond amount below minimum required")]
    fn test_validate_bond_amount_zero() {
        validate_bond_amount(0);
    }

    #[test]
    #[should_panic(expected = "bond amount cannot be negative")]
    fn test_validate_bond_amount_negative() {
        validate_bond_amount(-1);
    }

    #[test]
    #[should_panic(expected = "bond amount exceeds maximum allowed")]
    fn test_validate_bond_amount_above_maximum() {
        validate_bond_amount(MAX_BOND_AMOUNT + 1);
    }


}