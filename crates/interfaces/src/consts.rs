//! Shared constants, boundary predicates, and validation helpers for the
//! Credence interface layer.
//!
//! Every function in this module is a **pure** function: it has no storage
//! side effects, does not touch the ledger, and returns the same result for
//! the same inputs on every call. Callers use these helpers to reject invalid
//! input *before* it reaches persistent state, so a rejected write can never
//! leave a partially-mutated record behind.
//!
//! ## Invariants
//!
//! - Length bounds are **inclusive**: exactly `MIN_KEY_LEN` / `MAX_KEY_LEN`
//!   (and `MAX_VALUE_LEN`, `MAX_BATCH_SIZE`, `MAX_CACHE_AGE_SECS`) are valid.
//! - The retry budget **saturates at zero** and never wraps around, even when
//!   the attempt count exceeds [`MAX_RETRIES`].
//! - [`validate_write`] checks authorization **first**, then the key, then the
//!   value. An unauthorized caller therefore cannot probe key/value bounds
//!   through the returned error code.
//! - Error values are `&'static str` codes that carry no key/value content and
//!   no caller identity, so they are safe to surface to callers.

/// The storage key used to hold the administrative address.
pub const ADMIN_KEY: &str = "admin";

/// Maximum length allowed for a storage key string.
pub const MAX_KEY_LEN: usize = 64;

/// Maximum length allowed for a storage value string.
pub const MAX_VALUE_LEN: usize = 1024;

/// Minimum length allowed for a storage key string.
pub const MIN_KEY_LEN: usize = 1;

/// Maximum number of retry attempts for a failed operation.
pub const MAX_RETRIES: u32 = 3;

/// Maximum number of entries allowed in a single batch operation.
pub const MAX_BATCH_SIZE: usize = 128;

/// Maximum age (in seconds) before a cached entry is considered stale.
pub const MAX_CACHE_AGE_SECS: u64 = 86_400;

/// Default number of attempts for a transient operation.
pub const DEFAULT_RETRY_ATTEMPTS: u32 = 1;

/// Error code returned when a storage key is empty or too short.
pub const ERR_KEY_TOO_SHORT: &str = "key_too_short";

/// Error code returned when a storage key exceeds the maximum length.
pub const ERR_KEY_TOO_LONG: &str = "key_too_long";

/// Error code returned when a storage value exceeds the maximum length.
pub const ERR_VALUE_TOO_LONG: &str = "value_too_long";

/// Error code returned when a retry budget has been exhausted.
pub const ERR_RETRY_EXHAUSTED: &str = "retry_exhausted";

/// Error code returned when a batch exceeds the maximum allowed size.
pub const ERR_BATCH_TOO_LARGE: &str = "batch_too_large";

/// Error code returned when a cached entry is stale.
pub const ERR_STALE_ENTRY: &str = "stale_entry";

/// Error code returned when the caller is not authorized.
pub const ERR_UNAUTHORIZED: &str = "unauthorized";

/// Error code returned when an input fails general validation.
pub const ERR_INVALID_INPUT: &str = "invalid_input";

/// Returns `true` if the provided key length is within the accepted bounds.
///
/// This is a boundary check used by callers to reject empty or overly long
/// keys before they touch persistent state. It is deterministic and has no
/// side effects.
pub fn is_valid_key_len(len: usize) -> bool {
    (MIN_KEY_LEN..=MAX_KEY_LEN).contains(&len)
}

/// Returns `true` if the provided value length is within the accepted bounds.
///
/// A length of zero is allowed because deletion is represented by an empty
/// value; only the upper bound is enforced here.
pub fn is_valid_value_len(len: usize) -> bool {
    len <= MAX_VALUE_LEN
}

/// Returns `true` if the provided batch size is within the accepted bounds.
///
/// A batch of zero entries is valid and represents a no-op.
pub fn is_valid_batch_size(size: usize) -> bool {
    size <= MAX_BATCH_SIZE
}

/// Returns `true` if the cache entry age is within the freshness window.
///
/// An entry exactly at the age limit is considered fresh; only ages strictly
/// greater than [`MAX_CACHE_AGE_SECS`] are stale.
pub fn is_cache_fresh(age_secs: u64) -> bool {
    age_secs <= MAX_CACHE_AGE_SECS
}

/// Returns the number of retries remaining after `attempts` have been made.
///
/// Saturates at zero so a caller can never observe a wrap-around or underflow
/// when the attempt count exceeds the budget.
pub fn remaining_retries(attempts: u32) -> u32 {
    MAX_RETRIES.saturating_sub(attempts)
}

/// Returns `true` if another retry attempt is allowed.
///
/// This is the gate used before scheduling a retry; it is deterministic and
/// does not mutate any shared state.
pub fn can_retry(attempts: u32) -> bool {
    remaining_retries(attempts) > 0
}

/// Validates a storage key and returns a deterministic error code on failure.
///
/// The returned error code is safe to expose to callers because it contains no
/// sensitive data and no key content.
pub fn validate_key(key: &str) -> Result<usize, &'static str> {
    let len = key.len();
    if len < MIN_KEY_LEN {
        return Err(ERR_KEY_TOO_SHORT);
    }
    if len > MAX_KEY_LEN {
        return Err(ERR_KEY_TOO_LONG);
    }
    Ok(len)
}

/// Validates a storage value and returns a deterministic error code on
/// failure.
///
/// Empty values are allowed (deletion); only the upper bound is enforced.
pub fn validate_value(value: &str) -> Result<usize, &'static str> {
    let len = value.len();
    if len > MAX_VALUE_LEN {
        return Err(ERR_VALUE_TOO_LONG);
    }
    Ok(len)
}

/// Validates a batch size and returns a deterministic error code on failure.
pub fn validate_batch_size(size: usize) -> Result<usize, &'static str> {
    if size > MAX_BATCH_SIZE {
        return Err(ERR_BATCH_TOO_LARGE);
    }
    Ok(size)
}

/// Validates that a cache entry is fresh and returns a deterministic error code
/// when it is stale.
pub fn validate_cache_age(age_secs: u64) -> Result<u64, &'static str> {
    if age_secs > MAX_CACHE_AGE_SECS {
        return Err(ERR_STALE_ENTRY);
    }
    Ok(age_secs)
}

/// Validates that the caller is authorized and returns a deterministic error
/// code otherwise.
///
/// The check is purely boolean and never leaks the caller identity or the
/// expected administrator address.
pub fn validate_authorization(is_admin: bool) -> Result<(), &'static str> {
    if !is_admin {
        return Err(ERR_UNAUTHORIZED);
    }
    Ok(())
}

/// Combines the individual validation checks into a single decision for a
/// storage write.
///
/// The order of checks is deterministic: authorization first, then key, then
/// value. This ensures an unauthorized caller cannot probe key or value bounds
/// through error codes.
pub fn validate_write(is_admin: bool, key: &str, value: &str) -> Result<(), &'static str> {
    validate_authorization(is_admin)?;
    validate_key(key)?;
    validate_value(value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Boundary: key length ---

    #[test]
    fn key_length_boundaries_are_inclusive() {
        assert!(!is_valid_key_len(0));
        assert!(is_valid_key_len(MIN_KEY_LEN));
        assert!(is_valid_key_len(MIN_KEY_LEN + 1));
        assert!(is_valid_key_len(MAX_KEY_LEN - 1));
        assert!(is_valid_key_len(MAX_KEY_LEN));
        assert!(!is_valid_key_len(MAX_KEY_LEN + 1));
    }

    #[test]
    fn validate_key_rejects_too_short_and_too_long() {
        assert_eq!(validate_key(""), Err(ERR_KEY_TOO_SHORT));
        let long = "a".repeat(MAX_KEY_LEN + 1);
        assert_eq!(validate_key(&long), Err(ERR_KEY_TOO_LONG));
    }

    #[test]
    fn validate_key_accepts_boundary_lengths() {
        assert_eq!(validate_key("a"), Ok(1));
        let max = "a".repeat(MAX_KEY_LEN);
        assert_eq!(validate_key(&max), Ok(MAX_KEY_LEN));
    }

    // --- Boundary: value length ---

    #[test]
    fn value_length_boundaries_are_inclusive() {
        assert!(is_valid_value_len(0));
        assert!(is_valid_value_len(MAX_VALUE_LEN));
        assert!(!is_valid_value_len(MAX_VALUE_LEN + 1));
    }

    #[test]
    fn validate_value_allows_empty_and_rejects_overlong() {
        assert_eq!(validate_value(""), Ok(0));
        let max = "a".repeat(MAX_VALUE_LEN);
        assert_eq!(validate_value(&max), Ok(MAX_VALUE_LEN));
        let over = "a".repeat(MAX_VALUE_LEN + 1);
        assert_eq!(validate_value(&over), Err(ERR_VALUE_TOO_LONG));
    }

    // --- Boundary: batch size ---

    #[test]
    fn batch_size_boundaries_are_inclusive() {
        assert!(is_valid_batch_size(0));
        assert!(is_valid_batch_size(MAX_BATCH_SIZE));
        assert!(!is_valid_batch_size(MAX_BATCH_SIZE + 1));
    }

    #[test]
    fn validate_batch_size_rejects_overlong() {
        assert_eq!(validate_batch_size(0), Ok(0));
        assert_eq!(validate_batch_size(MAX_BATCH_SIZE), Ok(MAX_BATCH_SIZE));
        assert_eq!(
            validate_batch_size(MAX_BATCH_SIZE + 1),
            Err(ERR_BATCH_TOO_LARGE)
        );
    }

    // --- Boundary: cache age ---

    #[test]
    fn cache_freshness_boundaries_are_inclusive() {
        assert!(is_cache_fresh(0));
        assert!(is_cache_fresh(MAX_CACHE_AGE_SECS));
        assert!(!is_cache_fresh(MAX_CACHE_AGE_SECS + 1));
    }

    #[test]
    fn validate_cache_age_rejects_stale() {
        assert_eq!(validate_cache_age(0), Ok(0));
        assert_eq!(
            validate_cache_age(MAX_CACHE_AGE_SECS),
            Ok(MAX_CACHE_AGE_SECS)
        );
        assert_eq!(
            validate_cache_age(MAX_CACHE_AGE_SECS + 1),
            Err(ERR_STALE_ENTRY)
        );
    }

    // --- Retry budget ---

    #[test]
    fn retry_budget_saturates_at_zero() {
        assert_eq!(remaining_retries(0), MAX_RETRIES);
        assert_eq!(remaining_retries(1), MAX_RETRIES - 1);
        assert_eq!(remaining_retries(MAX_RETRIES), 0);
        assert_eq!(remaining_retries(MAX_RETRIES + 1), 0);
        assert_eq!(remaining_retries(u32::MAX), 0);
    }

    #[test]
    fn can_retry_gate_boundaries() {
        assert!(can_retry(0));
        assert!(can_retry(MAX_RETRIES - 1));
        assert!(!can_retry(MAX_RETRIES));
        assert!(!can_retry(MAX_RETRIES + 1));
    }

    // --- Authorization ---

    #[test]
    fn authorization_rejects_non_admin() {
        assert_eq!(validate_authorization(false), Err(ERR_UNAUTHORIZED));
        assert_eq!(validate_authorization(true), Ok(()));
    }

    // --- Combined write validation ---

    #[test]
    fn validate_write_checks_authorization_first() {
        // Unauthorized callers must not learn key/value bounds.
        assert_eq!(validate_write(false, "", ""), Err(ERR_UNAUTHORIZED));
        assert_eq!(validate_write(false, "a", "a"), Err(ERR_UNAUTHORIZED));
    }

    #[test]
    fn validate_write_rejects_invalid_key_and_value() {
        assert_eq!(validate_write(true, "", "value"), Err(ERR_KEY_TOO_SHORT));
        let long_key = "a".repeat(MAX_KEY_LEN + 1);
        assert_eq!(
            validate_write(true, &long_key, "value"),
            Err(ERR_KEY_TOO_LONG)
        );
        let long_value = "a".repeat(MAX_VALUE_LEN + 1);
        assert_eq!(
            validate_write(true, "key", &long_value),
            Err(ERR_VALUE_TOO_LONG)
        );
    }

    #[test]
    fn validate_write_accepts_valid_input() {
        assert_eq!(validate_write(true, "key", "value"), Ok(()));
        // Empty value is a valid deletion marker.
        assert_eq!(validate_write(true, "key", ""), Ok(()));
    }

    // --- Regression: determinism ---

    #[test]
    fn validation_is_deterministic_for_repeated_calls() {
        for _ in 0..100 {
            assert_eq!(validate_key(""), Err(ERR_KEY_TOO_SHORT));
            assert_eq!(validate_write(true, "key", "value"), Ok(()));
            assert_eq!(remaining_retries(MAX_RETRIES), 0);
        }
    }

    // --- Regression: admin key is a well-formed key ---

    #[test]
    fn admin_key_passes_validation() {
        assert_eq!(validate_key(ADMIN_KEY), Ok(ADMIN_KEY.len()));
        assert!(is_valid_key_len(ADMIN_KEY.len()));
    }
}
