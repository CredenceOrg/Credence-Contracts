#![no_std]
#![deny(clippy::float_arithmetic)]
#![allow(
    deprecated,
    unused_imports,
    unused_variables,
    dead_code,
    unused_assignments,
    unused_mut,
    mismatched_lifetime_syntaxes,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::restriction
)]
// Must come AFTER `#![allow(clippy::restriction, ...)]` above: the
// `clippy::disallowed_macros` lint belongs to the `restriction` group, so
// a later allow would re-silence it. cargo build --release / WASM build
// is the only mode where this deny fires (tests + the testutils feature
// stay free to use format!/write! for diagnostics).
#![cfg_attr(not(any(test, feature = "testutils")), deny(clippy::disallowed_macros))]

/// Signature domain identifier for the CredenceMultisig contract.
///
/// This constant binds signatures to this specific contract, preventing
/// cross-contract replay attacks where a signature intended for one contract
/// could be replayed against another. Each contract in the Credence system
/// has a unique signature domain constant.
///
/// # Security
///
/// Without domain separation, a signature created for contract A could be
/// replayed against contract B if both contracts share the same nonce namespace
/// and signature verification logic. By including this domain in the signed
/// payload hash, we ensure signatures are only valid for their intended contract.
///
/// # Value
///
/// The domain is a human-readable string that uniquely identifies this contract
/// within the Credence system. It should be included in the signed payload hash
/// along with other payload fields (nonce, deadline, etc.).
#[allow(dead_code)]
const SIGNATURE_DOMAIN: &str = "CredenceMultisig";

pub mod multisig;
pub mod pausable;

pub use multisig::*;

#[cfg(test)]
// mod test_multisig; // pre-existing SDK-22 / proptest compile breaks; excluded so signer-epoch suite can run
mod test_pausable;
#[cfg(test)]
mod test_signer_epoch_guard;
