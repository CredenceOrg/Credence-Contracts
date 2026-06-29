//! Attestation data structure and validation.
//!
//! Defines the Attestation type used for credibility attestations: verifier (attester),
//! subject (identity), timestamp, weight. Supports serialization via ContractType
//! and validation methods for storage efficiency and safety.

use soroban_sdk::{contracttype, Address, String};

/// Maximum allowed attestation weight (prevents overflow and caps influence).
pub const MAX_ATTESTATION_WEIGHT: u32 = 1_000_000;

/// Default weight when attester has no stake configured.
pub const DEFAULT_ATTESTATION_WEIGHT: u32 = 1;

/// Maximum allowed attestation data length (in bytes).
/// Prevents unbounded storage and enforces reasonable data sizes.
pub const MAX_ATTESTATION_DATA_LENGTH: u32 = 4096;

/// Attestation record: a verifier's credibility attestation for an identity.
///
/// # Fields
/// * `id` - Unique attestation identifier.
/// * `verifier` - Address of the authorized attester (verifier).
/// * `identity` - Address of the subject (identity) being attested.
/// * `timestamp` - Ledger timestamp when the attestation was added.
/// * `weight` - Credibility weight (e.g. derived from attester bond); capped by protocol.
/// * `attestation_data` - Opaque attestation payload (e.g. claim type or hash).
/// * `revoked` - Whether this attestation has been revoked.
///
/// # Serialization
/// Uses `#[contracttype]` for Soroban instance storage; space-efficient (u64, u32, bool, Address, String).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    pub id: u64,
    pub verifier: Address,
    pub identity: Address,
    pub timestamp: u64,
    pub weight: u32,
    pub attestation_data: String,
    pub revoked: bool,
}

impl Attestation {
    /// Validates that weight is within allowed bounds.
    ///
    /// # Errors
    /// Panics if `weight` is zero or exceeds `MAX_ATTESTATION_WEIGHT`.
    #[inline]
    pub fn validate_weight(weight: u32) {
        if weight == 0 {
            panic!("attestation weight must be positive");
        }
        if weight > MAX_ATTESTATION_WEIGHT {
            panic!("attestation weight exceeds maximum");
        }
    }

    /// Validates that attestation data is within allowed bounds.
    ///
    /// # Arguments
    /// * `data` - The attestation data to validate
    ///
    /// # Errors
    /// Panics if `data` exceeds `MAX_ATTESTATION_DATA_LENGTH`.
    #[inline]
    pub fn validate_data(data: &String) {
        let len = data.len();
        if len > MAX_ATTESTATION_DATA_LENGTH {
            panic!("attestation data exceeds maximum length");
        }
    }

    /// Validates this attestation (weight and data bounds). Use after deserialization or before storage.
    ///
    /// # Errors
    /// Panics if weight or data are invalid.
    #[inline]
    pub fn validate(&self) {
        Self::validate_weight(self.weight);
        Self::validate_data(&self.attestation_data);
    }

    /// Returns true if this attestation is currently active (not revoked).
    #[must_use]
    #[inline]
    pub fn is_active(&self) -> bool {
        !self.revoked
    }
}

/// Key used to detect duplicate attestations: same verifier, identity, and data.
/// Stored in instance storage to prevent adding the same attestation twice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestationDedupKey {
    pub verifier: Address,
    pub identity: Address,
    pub attestation_data: String,
}
