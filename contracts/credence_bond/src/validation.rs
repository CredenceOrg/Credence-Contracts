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
