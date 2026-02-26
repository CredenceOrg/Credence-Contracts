//! Protocol data types for bonds and attestations.
//!
//! Includes Attestation (with weight), validation, and deduplication key types.

pub mod attestation;

pub use attestation::{
    Attestation, AttestationDedupKey, DEFAULT_ATTESTATION_WEIGHT, MAX_ATTESTATION_WEIGHT,
};
