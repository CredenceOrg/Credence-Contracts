//! Tests for Attestation data structure: validation, serialization, and dedup key.

use crate::types::attestation::{
    DEFAULT_ATTESTATION_WEIGHT, MAX_ATTESTATION_DATA_LENGTH, MAX_ATTESTATION_WEIGHT,
};
use crate::types::{Attestation, AttestationDedupKey};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Env, String};
use std::string::String as StdString;

#[test]
fn attestation_weight_validation_accepts_valid() {
    Attestation::validate_weight(1);
    Attestation::validate_weight(100);
    Attestation::validate_weight(MAX_ATTESTATION_WEIGHT);
}

#[test]
#[should_panic(expected = "attestation weight must be positive")]
fn attestation_weight_validation_rejects_zero() {
    Attestation::validate_weight(0);
}

#[test]
#[should_panic(expected = "attestation weight exceeds maximum")]
fn attestation_weight_validation_rejects_over_max() {
    Attestation::validate_weight(MAX_ATTESTATION_WEIGHT + 1);
}

#[test]
fn attestation_validate_accepts_valid() {
    let e = Env::default();
    let att = Attestation {
        id: 0,
        verifier: soroban_sdk::Address::generate(&e),
        identity: soroban_sdk::Address::generate(&e),
        timestamp: 0,
        weight: DEFAULT_ATTESTATION_WEIGHT,
        attestation_data: String::from_str(&e, "x"),
        revoked: false,
    };
    att.validate();
}

#[test]
fn attestation_validate_accepts_empty_data() {
    let e = Env::default();
    let att = Attestation {
        id: 0,
        verifier: soroban_sdk::Address::generate(&e),
        identity: soroban_sdk::Address::generate(&e),
        timestamp: 0,
        weight: DEFAULT_ATTESTATION_WEIGHT,
        attestation_data: String::from_str(&e, ""),
        revoked: false,
    };
    att.validate();
}

#[test]
#[should_panic(expected = "attestation weight must be positive")]
fn attestation_validate_rejects_zero_weight() {
    let e = Env::default();
    let att = Attestation {
        id: 0,
        verifier: soroban_sdk::Address::generate(&e),
        identity: soroban_sdk::Address::generate(&e),
        timestamp: 0,
        weight: 0,
        attestation_data: String::from_str(&e, "x"),
        revoked: false,
    };
    att.validate();
}

#[test]
#[should_panic(expected = "attestation weight exceeds maximum")]
fn attestation_validate_rejects_over_max_weight() {
    let e = Env::default();
    let att = Attestation {
        id: 0,
        verifier: soroban_sdk::Address::generate(&e),
        identity: soroban_sdk::Address::generate(&e),
        timestamp: 0,
        weight: MAX_ATTESTATION_WEIGHT + 1,
        attestation_data: String::from_str(&e, "x"),
        revoked: false,
    };
    att.validate();
}

#[test]
fn attestation_is_active() {
    let e = Env::default();
    let verifier = soroban_sdk::Address::generate(&e);
    let identity = soroban_sdk::Address::generate(&e);
    let data = String::from_str(&e, "data");
    let att = Attestation {
        id: 0,
        verifier: verifier.clone(),
        identity: identity.clone(),
        timestamp: 0,
        weight: DEFAULT_ATTESTATION_WEIGHT,
        attestation_data: data,
        revoked: false,
    };
    assert!(att.is_active());
    let mut revoked = att.clone();
    revoked.revoked = true;
    assert!(!revoked.is_active());
}

#[test]
fn attestation_dedup_key_equality() {
    let e = Env::default();
    let verifier = soroban_sdk::Address::generate(&e);
    let identity = soroban_sdk::Address::generate(&e);
    let d = String::from_str(&e, "x");
    let k1 = AttestationDedupKey {
        verifier: verifier.clone(),
        identity: identity.clone(),
        attestation_data: d.clone(),
    };
    let k2 = AttestationDedupKey {
        verifier,
        identity,
        attestation_data: d,
    };
    assert_eq!(k1, k2);
}

#[test]
#[should_panic(expected = "attestation data exceeds maximum length")]
fn attestation_validate_rejects_too_long_data() {
    let e = Env::default();
    let long_str: StdString = core::iter::repeat('a')
        .take((MAX_ATTESTATION_DATA_LENGTH + 1) as usize)
        .collect();
    let data = String::from_str(&e, &long_str);
    Attestation::validate_data(&data);
}

/// Serialization is exercised via add_attestation/get_attestation (contract storage) in test_attestation.
/// Attestation and AttestationDedupKey use #[contracttype] for Soroban instance storage.

#[test]
fn attestation_boundary_weight_max() {
    Attestation::validate_weight(MAX_ATTESTATION_WEIGHT);
}

#[test]
fn attestation_boundary_weight_min() {
    Attestation::validate_weight(1);
}

// ─── Shared validator consistency tests (issue #1028) ──────────────────────
//
// These tests pin the centralised validation contract that every contract-facing
// mutator (`add_attestation`, `add_attestation_batch`) now routes through
// [`Attestation::validate_input`]. They are deliberately structured as
// boundary sweeps + composition checks so that any future drift in panic
// message wording is caught immediately, and so that a regression where a
// call site drops back to inlining one half of the check is loud rather
// than silent.

// Composition guarantee: `Attestation::validate(&self)` is exactly
// `validate_weight(self.weight); validate_data(&self.attestation_data);`.
// If both halves are independently exercised, the combined call cannot
// reject a value both halves accept, and cannot accept a value either
// half rejects.

#[test]
fn validate_is_composition_of_validate_weight_and_validate_data() {
    let e = Env::default();
    let verifier = soroban_sdk::Address::generate(&e);

    // Valid weight + valid data: composition accepts.
    let ok_small = Attestation {
        id: 1,
        verifier: verifier.clone(),
        identity: verifier.clone(),
        timestamp: 0,
        weight: 1,
        attestation_data: String::from_str(&e, "x"),
        revoked: false,
    };
    ok_small.validate();

    // Valid weight + valid boundary-length data: composition accepts.
    let at_len = Attestation {
        id: 2,
        verifier: verifier.clone(),
        identity: verifier.clone(),
        timestamp: 0,
        weight: MAX_ATTESTATION_WEIGHT,
        attestation_data: String::from_str(&e, &"a".repeat(MAX_ATTESTATION_DATA_LENGTH as usize)),
        revoked: false,
    };
    at_len.validate();

    // Empty data + valid weight: composition accepts (empty is allowed by spec).
    let empty_ok = Attestation {
        id: 3,
        verifier: verifier.clone(),
        identity: verifier.clone(),
        timestamp: 0,
        weight: DEFAULT_ATTESTATION_WEIGHT,
        attestation_data: String::from_str(&e, ""),
        revoked: false,
    };
    empty_ok.validate();
}

#[test]
#[should_panic(expected = "attestation weight must be positive")]
fn validate_composition_rejects_when_weight_zero() {
    let e = Env::default();
    let v = soroban_sdk::Address::generate(&e);
    let bad_weight = Attestation {
        id: 0,
        verifier: v.clone(),
        identity: v.clone(),
        timestamp: 0,
        weight: 0,
        attestation_data: String::from_str(&e, "x"),
        revoked: false,
    };
    bad_weight.validate();
}

#[test]
#[should_panic(expected = "attestation weight exceeds maximum")]
fn validate_composition_rejects_when_weight_over_max() {
    let e = Env::default();
    let v = soroban_sdk::Address::generate(&e);
    let bad_weight = Attestation {
        id: 0,
        verifier: v.clone(),
        identity: v.clone(),
        timestamp: 0,
        weight: MAX_ATTESTATION_WEIGHT + 1,
        attestation_data: String::from_str(&e, "x"),
        revoked: false,
    };
    bad_weight.validate();
}

#[test]
#[should_panic(expected = "attestation data exceeds maximum length")]
fn validate_composition_rejects_when_data_over_max_len() {
    let e = Env::default();
    let v = soroban_sdk::Address::generate(&e);
    let long_str: StdString = core::iter::repeat('a')
        .take((MAX_ATTESTATION_DATA_LENGTH + 1) as usize)
        .collect();
    let bad_data = Attestation {
        id: 0,
        verifier: v.clone(),
        identity: v.clone(),
        timestamp: 0,
        weight: DEFAULT_ATTESTATION_WEIGHT,
        attestation_data: String::from_str(&e, &long_str),
        revoked: false,
    };
    bad_data.validate();
}

// `validate_input` is the entry-point helper contract-facing mutators
// (add_attestation, add_attestation_batch) call. These tests lock down
// the helper's surface so a future inline-check regression in lib.rs is
// caught by reading test names.

#[test]
fn validate_input_accepts_valid_weight_and_short_data() {
    let e = Env::default();
    let d = String::from_str(&e, "kyc:verified");
    Attestation::validate_input(1, &d);
    Attestation::validate_input(MAX_ATTESTATION_WEIGHT, &d);
}

#[test]
fn validate_input_accepts_empty_data() {
    let e = Env::default();
    let d = String::from_str(&e, "");
    Attestation::validate_input(DEFAULT_ATTESTATION_WEIGHT, &d);
}

#[test]
fn validate_input_accepts_data_of_exactly_max_length() {
    let e = Env::default();
    let d = String::from_str(&e, &"a".repeat(MAX_ATTESTATION_DATA_LENGTH as usize));
    Attestation::validate_input(1, &d);
}

#[test]
#[should_panic(expected = "attestation weight must be positive")]
fn validate_input_rejects_zero_weight_even_with_valid_data() {
    let e = Env::default();
    let d = String::from_str(&e, "ok");
    Attestation::validate_input(0, &d);
}

#[test]
#[should_panic(expected = "attestation weight exceeds maximum")]
fn validate_input_rejects_over_max_weight_even_with_valid_data() {
    let e = Env::default();
    let d = String::from_str(&e, "ok");
    Attestation::validate_input(MAX_ATTESTATION_WEIGHT + 1, &d);
}

#[test]
#[should_panic(expected = "attestation data exceeds maximum length")]
fn validate_input_rejects_over_max_data_even_with_valid_weight() {
    let e = Env::default();
    let long_str: StdString = core::iter::repeat('a')
        .take((MAX_ATTESTATION_DATA_LENGTH + 1) as usize)
        .collect();
    let d = String::from_str(&e, &long_str);
    Attestation::validate_input(DEFAULT_ATTESTATION_WEIGHT, &d);
}

// `validate_data` must accept edge inputs (empty, exactly-max) without
// panicking. These are the call-site guarantees every contract-facing
// mutator relies on when accepting caller-supplied `attestation_data`.

#[test]
fn validate_data_accepts_short_inputs() {
    let e = Env::default();
    Attestation::validate_data(&String::from_str(&e, ""));
    Attestation::validate_data(&String::from_str(&e, "x"));
    Attestation::validate_data(&String::from_str(&e, "kyc:verified"));
}

#[test]
fn validate_data_accepts_exactly_max_length() {
    let e = Env::default();
    let d = String::from_str(&e, &"a".repeat(MAX_ATTESTATION_DATA_LENGTH as usize));
    Attestation::validate_data(&d);
    // Confirms the boundary is inclusive: MAX is allowed, MAX+1 is rejected.
    assert_eq!(d.len(), MAX_ATTESTATION_DATA_LENGTH);
}

#[test]
#[should_panic(expected = "attestation data exceeds maximum length")]
fn validate_data_rejects_one_over_max_length() {
    let e = Env::default();
    let d = String::from_str(&e, &"a".repeat((MAX_ATTESTATION_DATA_LENGTH + 1) as usize));
    Attestation::validate_data(&d);
}
