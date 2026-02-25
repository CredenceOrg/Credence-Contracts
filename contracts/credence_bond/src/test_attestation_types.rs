//! Tests for Attestation data structure: validation, serialization, and dedup key.

#![cfg(test)]

use crate::types::attestation::{DEFAULT_ATTESTATION_WEIGHT, MAX_ATTESTATION_WEIGHT};
use crate::types::{Attestation, AttestationDedupKey};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Env, String};

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
    let v = soroban_sdk::Address::generate(&e);
    let i = soroban_sdk::Address::generate(&e);
    let d = String::from_str(&e, "x");
    let k1 = AttestationDedupKey {
        verifier: v.clone(),
        identity: i.clone(),
        attestation_data: d.clone(),
    };
    let k2 = AttestationDedupKey {
        verifier: v,
        identity: i,
        attestation_data: d,
    };
    assert_eq!(k1, k2);
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
