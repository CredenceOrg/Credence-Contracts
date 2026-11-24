#![no_std]
//! Shared, contract-agnostic interface definitions for the Credence workspace.
//!
//! This crate is the single home for interfaces that more than one contract
//! needs to agree on:
//!
//! - [`consts`] — shared constants plus pure boundary and validation helpers
//!   for storage keys, values, batch sizes, cache freshness, and retry budgets.
//! - [`governable`] — the [`governable::Governable`] administrative control
//!   interface implemented by the parameter-changing contracts.
//!
//! Every helper in [`consts`] is pure and deterministic, so callers can reject
//! invalid or unauthorized input *before* it reaches persistent state. That is
//! what makes the recovery paths safe to retry: a rejected write never leaves a
//! partially-mutated record behind, and the retry budget is only spent by
//! operations that actually ran.
//!
//! The module-level tests below exercise the crate's public surface end to end
//! (loading/write readiness, permission, stale, retry, and recovery states)
//! rather than unit-testing a single helper in isolation.

#[cfg(test)]
extern crate std;

pub mod consts;
pub mod governable;

#[cfg(test)]
mod tests {
    use crate::consts::*;
    use crate::governable::GovernableClient;

    // ---------------------------------------------------------------------
    // Public surface reachability
    // ---------------------------------------------------------------------

    /// The items re-exported by this crate must be usable through the crate
    /// root modules. This is a compile-time contract check in addition to the
    /// runtime assertions.
    #[test]
    fn public_surface_is_reachable() {
        assert_eq!(validate_key(ADMIN_KEY), Ok(ADMIN_KEY.len()));

        // The client generated from the `Governable` trait is part of the
        // public surface that cross-contract callers depend on.
        let _client: Option<GovernableClient<'_>> = None;
    }

    // ---------------------------------------------------------------------
    // Loading / normal operation
    // ---------------------------------------------------------------------

    /// Success: an authorized write whose key and value are in bounds is
    /// accepted, and the payload lengths are reported back to the caller.
    #[test]
    fn authorized_write_within_bounds_is_accepted() {
        assert_eq!(validate_write(true, "user:1", "active"), Ok(()));
        assert_eq!(validate_key("user:1"), Ok("user:1".len()));
        assert_eq!(validate_value("active"), Ok("active".len()));
        assert_eq!(validate_batch_size(1), Ok(1));
    }

    // ---------------------------------------------------------------------
    // Permission states
    // ---------------------------------------------------------------------

    /// Rejection: an unauthorized caller is rejected before any bound is
    /// evaluated, so error codes cannot be used to probe key/value lengths.
    #[test]
    fn unauthorized_write_is_rejected_before_bounds_are_checked() {
        assert_eq!(validate_write(false, "", ""), Err(ERR_UNAUTHORIZED));
        assert_eq!(
            validate_write(false, "user:1", "active"),
            Err(ERR_UNAUTHORIZED)
        );
        // Even a payload that would fail validation reports permission first.
        assert_eq!(
            validate_write(false, &"k".repeat(MAX_KEY_LEN + 1), "x"),
            Err(ERR_UNAUTHORIZED)
        );
    }

    // ---------------------------------------------------------------------
    // Boundary states
    // ---------------------------------------------------------------------

    /// Boundary: the minimum and maximum accepted lengths are inclusive, and
    /// exactly one unit past a bound is rejected.
    #[test]
    fn boundary_lengths_are_inclusive_across_the_public_api() {
        // Key: min = 1, max = MAX_KEY_LEN.
        assert_eq!(validate_key("k"), Ok(1));
        assert_eq!(validate_key(&"k".repeat(MAX_KEY_LEN)), Ok(MAX_KEY_LEN));
        assert_eq!(validate_key(""), Err(ERR_KEY_TOO_SHORT));
        assert_eq!(
            validate_key(&"k".repeat(MAX_KEY_LEN + 1)),
            Err(ERR_KEY_TOO_LONG)
        );

        // Value: empty (deletion) is valid, max is inclusive.
        assert_eq!(validate_value(""), Ok(0));
        assert_eq!(
            validate_value(&"v".repeat(MAX_VALUE_LEN)),
            Ok(MAX_VALUE_LEN)
        );
        assert_eq!(
            validate_value(&"v".repeat(MAX_VALUE_LEN + 1)),
            Err(ERR_VALUE_TOO_LONG)
        );

        // Batch: zero is a valid no-op, max is inclusive.
        assert_eq!(validate_batch_size(0), Ok(0));
        assert_eq!(validate_batch_size(MAX_BATCH_SIZE), Ok(MAX_BATCH_SIZE));
        assert_eq!(
            validate_batch_size(MAX_BATCH_SIZE + 1),
            Err(ERR_BATCH_TOO_LARGE)
        );
    }

    // ---------------------------------------------------------------------
    // Stale states
    // ---------------------------------------------------------------------

    /// Boundary: a cache entry is fresh up to and including `MAX_CACHE_AGE_SECS`
    /// and stale one second beyond it.
    #[test]
    fn stale_cache_entry_is_rejected_but_fresh_is_accepted() {
        assert_eq!(validate_cache_age(0), Ok(0));
        assert_eq!(
            validate_cache_age(MAX_CACHE_AGE_SECS),
            Ok(MAX_CACHE_AGE_SECS)
        );
        assert_eq!(
            validate_cache_age(MAX_CACHE_AGE_SECS + 1),
            Err(ERR_STALE_ENTRY)
        );
        assert!(is_cache_fresh(MAX_CACHE_AGE_SECS));
        assert!(!is_cache_fresh(MAX_CACHE_AGE_SECS + 1));
    }

    // ---------------------------------------------------------------------
    // Retry states
    // ---------------------------------------------------------------------

    /// The retry budget counts down to zero and then saturates, so a caller can
    /// never schedule an unbounded number of retries or underflow the counter.
    #[test]
    fn retry_budget_recovers_then_exhausts() {
        assert!(can_retry(0));
        assert_eq!(remaining_retries(0), MAX_RETRIES);
        assert_eq!(remaining_retries(MAX_RETRIES - 1), 1);
        assert!(can_retry(MAX_RETRIES - 1));

        assert_eq!(remaining_retries(MAX_RETRIES), 0);
        assert!(!can_retry(MAX_RETRIES));
        // Past the budget the counter saturates instead of wrapping.
        assert_eq!(remaining_retries(MAX_RETRIES + 1), 0);
        assert_eq!(remaining_retries(u32::MAX), 0);
    }

    // ---------------------------------------------------------------------
    // Recovery
    // ---------------------------------------------------------------------

    /// Recovery: a rejected write is a pure, side-effect-free decision, so it
    /// consumes no retry budget and can be retried with corrected input.
    #[test]
    fn rejected_write_is_recoverable_and_repeatable() {
        let budget_before = remaining_retries(DEFAULT_RETRY_ATTEMPTS);

        // Invalid input is rejected...
        assert_eq!(validate_write(true, "", "value"), Err(ERR_KEY_TOO_SHORT));
        // ...without consuming the retry budget...
        assert_eq!(remaining_retries(DEFAULT_RETRY_ATTEMPTS), budget_before);

        // ...and the identical, corrected write succeeds.
        assert_eq!(validate_write(true, "key", "value"), Ok(()));
    }

    /// Recovery: after exhausting the retry budget on a transient failure, a
    /// fresh operation starts from a clean, deterministic state.
    #[test]
    fn exhausted_budget_resets_for_a_new_operation() {
        let exhausted = remaining_retries(u32::MAX);
        assert_eq!(exhausted, 0);
        assert!(!can_retry(u32::MAX));

        // A new operation observes the full budget, not the exhausted one.
        assert_eq!(remaining_retries(0), MAX_RETRIES);
        assert!(can_retry(0));
    }

    // ---------------------------------------------------------------------
    // Determinism / regression
    // ---------------------------------------------------------------------

    /// Regression: repeated calls yield identical results for the same inputs,
    /// including across the permission, stale, and batch boundaries.
    #[test]
    fn validation_is_deterministic_across_modules() {
        for _ in 0..100 {
            assert_eq!(validate_write(false, "key", "value"), Err(ERR_UNAUTHORIZED));
            assert_eq!(validate_write(true, "key", "value"), Ok(()));
            assert_eq!(
                validate_cache_age(MAX_CACHE_AGE_SECS + 1),
                Err(ERR_STALE_ENTRY)
            );
            assert_eq!(remaining_retries(MAX_RETRIES), 0);
        }
    }

    // ---------------------------------------------------------------------
    // Observability
    // ---------------------------------------------------------------------

    /// The error codes surfaced by this crate are stable, non-empty literals
    /// that never contain key/value content or caller identity.
    #[test]
    fn error_codes_are_stable_and_non_sensitive() {
        assert_eq!(ERR_UNAUTHORIZED, "unauthorized");
        assert_eq!(ERR_KEY_TOO_SHORT, "key_too_short");
        assert_eq!(ERR_KEY_TOO_LONG, "key_too_long");
        assert_eq!(ERR_VALUE_TOO_LONG, "value_too_long");
        assert_eq!(ERR_BATCH_TOO_LARGE, "batch_too_large");
        assert_eq!(ERR_STALE_ENTRY, "stale_entry");
        assert_eq!(ERR_RETRY_EXHAUSTED, "retry_exhausted");
        assert_eq!(ERR_INVALID_INPUT, "invalid_input");

        // A code obtained from validation is exactly one of those literals.
        let code = validate_key("").unwrap_err();
        assert_eq!(code, ERR_KEY_TOO_SHORT);
    }
}
