/// Protocol data types for bonds and attestations.
///
/// Includes Attestation (with weight), validation, and deduplication key types.

pub mod attestation;

pub use attestation::{
    Attestation, AttestationDedupKey, DEFAULT_ATTESTATION_WEIGHT, MAX_ATTESTATION_WEIGHT,
};

///## Boundary and recovery test coverage

/// This module exposes the canonical boundary and recovery semantics for the
/// attestation types re-exported above. The goal is to make determinism explicit
/// for valid, invalid, duplicate, and boundary-case inputs without changing the
/// public interface of the attestation module.
///
///## Invariants
///
/// 1. An attestation weight must always be in the inclusive range
///    `[0, MAX_ATTESTATION_WEIGHT]`. Values above the maximum are rejected
///    with a deterministic error and must not mutate any state.
/// 2. The default weight is always valid and is used when no weight is
///    supplied by the caller.
/// 3. Deduplication keys are deterministic functions of the attestation
///    contents; equal contents produce equal keys and unequal contents produce
///    distinct keys for the covered fields.
/// 4. Retries of a failed validation are side-effect free: the same input
///    always produces the same result, so a retry cannot corrupt state.
///
/// The tests below are collocated with the type re-exports so that any future
/// change to the boundary contract is caught at the same layer that defines
/// the public surface.

#[cfg(test)]
mod tests {
    use super::*;

    ///--------------------------------------------------------------------------
    /// Success cases: default and explicit weights within bounds.
    ///--------------------------------------------------------------------------

    #[test]
    fn default_weight_is_within_bounds() {
        assert!(DEFAULT_ATTESTATION_WEIGHT <= MAX_ATTESTATION_WEIGHT);
    }

    #[test]
    fn attestation_constructor_accepts_default_weight() {
        // The constuctor must accept the default weight without panicking.
        let att = Attestation::new(DEFAULT_ATTESTATION_WEIGHT);
        assert_eq!(att.weight(), DEFAULT_ATTESTATION_WEIGHT);
    }

    #[test]
    fn attestation_constructor_accepts_max_weight() {
        // The upper bound is inclusive.
        let att = Attestation::new(MAX_ATTESTATION_WEIGHT);
        assert_eq!(att.weight(), MAX_ATTESTATION_WEIGHT);
    }

    ///--------------------------------------------------------------------------
    /// Boundary cases: zero weight and one above the maximum.
    ///--------------------------------------------------------------------------

    #[test]
    fn attestation_accepts_zero_weight() {
        // Zero is the lower inclusive bound and must be accepted.
        let att = Attestation::new(0);
        assert_eq!(att.weight(), 0);
    }

    #[test]
    fn attestation_rejects_weight_one_above_max() {
        // The upper bound is exclusive on the outside: max + 1 must reject.
        let result = Attestation::try_new(MAX_ATTESTATION_WEIGHT + 1);
        assert!(result.is_err(), "weight above max must be rejected");
    }

    #[test]
    fn attestation_rejects_weight_far_above_max() {
        // Large out-of-range values must also be rejected deterministically.
        let result = Attestation::try_new(ui64::MAX);
        assert!(result.is_err(), "weight far above max must be rejected");
    }

    ///--------------------------------------------------------------------------
    /// Rejection cases: invalid inputs must not mutate state and must be
/// retry-safe.
    ///--------------------------------------------------------------------------

    #[test]
    fn rejection_is_deterministic_across_retries() {
        // Retries of an invalid input must produce the same result every time.
        let invalid = MAX_ATTESTATION_WEIGHT + 1;
        for _ in 0..10 {
            let result = Attestation::try_new(invalid);
            assert!(result.is_err(), "retry must reject consistently");
        }
    }

    #[test]
    fn valid_construction_is_deterministic() {
        // The same valid input must always produce the same weight.
        let att_a = Attestation::new(DEFAULT_ATTESTATION_WEIGHT);
        let att_b = Attestation::new(DEFAULT_ATTESTATION_WEIGHT);
        assert_eq!(att_a.weight(), att_b.weight());
    }

    ///--------------------------------------------------------------------------
    /// Deduplication key invariants.
    ///--------------------------------------------------------------------------

    #[test]
    fn dedup_key_is_equal_for_equal_attestations() {
        let att_a = Attestation::new(DEFAULT_ATTESTATION_WEIGHT);
        let att_b = Attestation::new(DEFAULT_ATTESTATION_WEIGHT);
        assert_eq!(AttestationDedupKey::from(&att_a), AttestationDedupKey::from(&att_b));
    }

    #[test]
    fn dedup_key_differs_for_different_weights() {
        let att_a = Attestation::new(DEFAULT_ATTESTATION_WEIGHT);
        let att_b = Attestation::new(DEFAULT_ATTESTATION_WEIGHT + 1);
        assert_ne!(AttestationDedupKey::from(&att_a), AttestationDedupKey::from(&att_b));
    }

    ///--------------------------------------------------------------------------
    /// Regression: the module must keep re-exporting the canonical symbols.
    ///--------------------------------------------------------------------------

    #[test]
    fn module_reexports_canonical_symbols() {
        // This function only compiles if the re-exports above remain in place.
        let _: u64 = MAX_ATTESTATION_WEIGHT;
        let _: u64 = DEFAULT_ATTESTATION_WEIGHT;
    }
}
