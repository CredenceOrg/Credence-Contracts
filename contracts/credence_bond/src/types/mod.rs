//! Protocol data types for bonds and attestations.
//!
//! Includes Attestation (with weight), validation, and deduplication key types.

pub mod attestation;

pub use attestation::{Attestation, AttestationDedupKey};
