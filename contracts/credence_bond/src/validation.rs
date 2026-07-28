//! Bond Amount Validation Module
//!
//! Provides validation functions for bond amounts to ensure they fall within acceptable ranges.
//! This module centralizes the validation logic for minimum and maximum bond amounts.
//!
//! # Important: Decimal Normalization
//! All validation constants are expressed in **normalized 18-decimal format**.
//! The bond contract normalizes all token amounts to 18 decimals before validation,
//! ensuring consistent behavior across tokens with different decimal places.

#![allow(dead_code)]

use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Address, Env, String, Vec};

/// Maximum accepted length of a hex/base64 byte string before decoding.
///
/// The bound matches the contract's existing maximum attestation payload size
/// and keeps validation cost and stack usage predictable.
pub const MAX_STRINGIFIED_BYTES_LENGTH: u32 = 4_096;

/// Verify that `value` is a bounded, strictly decodable hex or base64 string.
///
/// Hex input must contain an even number of ASCII hexadecimal digits. Base64
/// input uses the RFC 4648 standard alphabet, accepts canonical padded or
/// unpadded form, and rejects non-zero trailing bits.
///
/// Rejecting malformed input before it reaches storage or downstream decoders
/// prevents attackers from injecting opaque values that different consumers
/// may parse inconsistently or fail to parse at all.
pub fn verify_stringified_bytes(value: &String) -> Result<(), ContractError> {
    let len = value.len();
    if len > MAX_STRINGIFIED_BYTES_LENGTH {
        return Err(ContractError::InvalidStringifiedBytes);
    }

    let len = len as usize;
    let mut buffer = [0_u8; MAX_STRINGIFIED_BYTES_LENGTH as usize];
    let encoded = buffer
        .get_mut(..len)
        .ok_or(ContractError::InvalidStringifiedBytes)?;
    value.copy_into_slice(encoded);

    if is_hex(encoded) || is_base64(encoded) {
        Ok(())
    } else {
        Err(ContractError::InvalidStringifiedBytes)
    }
}

fn is_hex(encoded: &[u8]) -> bool {
    encoded.len() % 2 == 0 && encoded.iter().all(u8::is_ascii_hexdigit)
}

fn is_base64(encoded: &[u8]) -> bool {
    let mut data_len = 0_usize;
    let mut padding_len = 0_usize;
    let mut padding_started = false;
    let mut last_value = 0_u8;

    for byte in encoded.iter().copied() {
        if byte == b'=' {
            padding_started = true;
            padding_len += 1;
            continue;
        }

        if padding_started {
            return false;
        }

        let Some(value) = base64_value(byte) else {
            return false;
        };
        last_value = value;
        data_len += 1;
    }

    let remainder = data_len % 4;
    match padding_len {
        0 => match remainder {
            0 => true,
            2 => last_value & 0x0f == 0,
            3 => last_value & 0x03 == 0,
            _ => false,
        },
        1 => encoded.len() % 4 == 0 && remainder == 3 && last_value & 0x03 == 0,
        2 => encoded.len() % 4 == 0 && remainder == 2 && last_value & 0x0f == 0,
        _ => false,
    }
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte.wrapping_sub(b'A')),
        b'a'..=b'z' => Some(byte.wrapping_sub(b'a').wrapping_add(26)),
        b'0'..=b'9' => Some(byte.wrapping_sub(b'0').wrapping_add(52)),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

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

#[cfg(not(test))]
/// Minimum bond amount in normalized 18-decimal format (1 token = 10^18).
/// This ensures consistent validation regardless of underlying token decimals.
pub const MIN_BOND_AMOUNT: i128 = 1_000_000_000_000_000_000; // 1 * 10^18 (1 token)

#[cfg(test)]
/// Minimum bond amount in tests. Set to 1000 to match legacy tests.
/// See docs/known-simplifications.md § 4.2 for production vs test bounds.
pub const MIN_BOND_AMOUNT: i128 = 1_000;

#[cfg(not(test))]
/// Maximum bond amount in normalized 18-decimal format (100 million tokens = 10^8 * 10^18 = 10^26).
/// This prevents overflow in accounting calculations.
pub const MAX_BOND_AMOUNT: i128 = 100_000_000_000_000_000_000_000_000; // 100M * 10^18

#[cfg(test)]
/// Maximum bond amount in tests. Set to 100_000_000_000_000 to match legacy tests.
/// See docs/known-simplifications.md § 4.3 for production vs test bounds.
pub const MAX_BOND_AMOUNT: i128 = 100_000_000_000_000;

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
        panic!(
            "bond amount below minimum required: {} (minimum: {})",
            amount, MIN_BOND_AMOUNT
        );
    }

    if amount > MAX_BOND_AMOUNT {
        panic!(
            "bond amount exceeds maximum allowed: {} (maximum: {})",
            amount, MAX_BOND_AMOUNT
        );
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
/// * `"bond duration too short: minimum is credence_math::Timestamp::SECONDS_PER_DAY seconds (1 day)"` if `duration` < `MIN_BOND_DURATION`
/// * `"bond duration too long: maximum is 31536000 seconds (365 days)"` if `duration` > `MAX_BOND_DURATION`
pub fn validate_bond_duration(duration: u64) {
    if duration < MIN_BOND_DURATION {
        panic!("bond duration too short: minimum is credence_math::Timestamp::SECONDS_PER_DAY seconds (1 day)");
    }
    if duration > MAX_BOND_DURATION {
        panic!("bond duration too long: maximum is 31536000 seconds (365 days)");
    }
}

/// Require a vector to be non-empty, rejecting empty vectors with a typed error
/// rather than downstream panics.
pub fn require_non_empty_vec<T>(e: &Env, v: &Vec<T>) {
    if v.is_empty() {
        panic_with_error!(e, ContractError::EmptyBatch);
    }
}

/// Verifies that a batch size is non-zero and does not exceed the maximum allowed size.
///
/// # Arguments
/// * `e` - Soroban environment
/// * `len` - The actual size of the batch
/// * `max_size` - The maximum allowed size for this batch operation
///
/// # Panics
/// * `ContractError::EmptyBatch` if `len == 0`
/// * `ContractError::BatchTooLarge` if `len > max_size`
pub fn verify_batch_size(e: &Env, len: u32, max_size: u32) {
    if len == 0 {
        panic_with_error!(e, ContractError::EmptyBatch);
    }
    if len > max_size {
        panic_with_error!(e, ContractError::BatchTooLarge);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, String};

    #[test]
    fn test_verify_stringified_bytes_accepts_hex_and_base64() {
        let env = Env::default();

        assert_eq!(
            verify_stringified_bytes(&String::from_str(&env, "deadbeef")),
            Ok(())
        );
        assert_eq!(
            verify_stringified_bytes(&String::from_str(&env, "SGVsbG8=")),
            Ok(())
        );
        assert_eq!(
            verify_stringified_bytes(&String::from_str(&env, "SGVsbG8")),
            Ok(())
        );
    }

    #[test]
    fn test_verify_stringified_bytes_rejects_malformed_input() {
        let env = Env::default();

        for input in ["not-valid@", "A", "AB==", "SGV=sbG8"] {
            assert_eq!(
                verify_stringified_bytes(&String::from_str(&env, input)),
                Err(ContractError::InvalidStringifiedBytes),
                "input should be rejected: {input}"
            );
        }
    }

    #[test]
    fn test_verify_stringified_bytes_rejects_oversized_input() {
        let env = Env::default();
        let oversized = "A".repeat(MAX_STRINGIFIED_BYTES_LENGTH as usize + 1);

        assert_eq!(
            verify_stringified_bytes(&String::from_str(&env, &oversized)),
            Err(ContractError::InvalidStringifiedBytes)
        );
    }

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
