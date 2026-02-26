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
//! Duration Validation Module
//!
//! Provides validation logic for bond durations including minimum and maximum limit
//! enforcement. All bond creations must pass duration validation before proceeding.
//!
//! ## Constraints
//! - **Minimum Duration**: Bonds must have a duration of at least 1 day (86_400 seconds)
//!   to prevent trivially short bonds that offer no meaningful commitment.
//! - **Maximum Duration**: Bonds are capped at 365 days (31_536_000 seconds) to limit
//!   excessive lock-up risk and contract state lifetime.
//!
//! ## Error Messages
//! - `"bond duration too short: minimum is 86400 seconds (1 day)"` — when duration < MIN
//! - `"bond duration too long: maximum is 31536000 seconds (365 days)"` — when duration > MAX

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
pub fn validate_bond_duration(duration: u64) {
    if duration < MIN_BOND_DURATION {
        panic!("bond duration too short: minimum is 86400 seconds (1 day)");
    }
    if duration > MAX_BOND_DURATION {
        panic!("bond duration too long: maximum is 31536000 seconds (365 days)");
    }
}
