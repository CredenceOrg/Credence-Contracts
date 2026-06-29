//! Same-ledger collateral increase vs slashing guard.
//!
//! ## Rationale
//!
//! In one ledger, transaction ordering can let a slash ("liquidation") run in the
//! same block as a collateral increase ("borrow" / top-up). That enables unfair,
//! sandwich-like outcomes against the bond holder. Recording the ledger sequence
//! whenever collateral is added and rejecting slashes while it still matches the
//! current ledger closes that edge case.
//!
//! This is **not** a protocol-wide throttle: it only touches slash entry points and
//! does not limit attestations, withdrawals, or unrelated accounts.

use crate::DataKey;
use soroban_sdk::Env;

/// Persist the current ledger sequence after a successful collateral increase.
#[allow(dead_code)]
pub fn record_collateral_increase(e: &Env) {
    let seq = e.ledger().sequence();
    e.storage()
        .instance()
        .set(&DataKey::LastCollateralIncreaseLedger, &seq);
}

/// Panics if the last collateral increase happened in the current ledger.
///
/// If the key was never set (e.g. pre-upgrade storage), slashing is allowed so
/// existing bonds are not bricked.
pub fn require_slash_allowed_after_collateral_increase(e: &Env) {
    let current = e.ledger().sequence();
    if let Some(last) = e
        .storage()
        .instance()
        .get::<_, u32>(&DataKey::LastCollateralIncreaseLedger)
    {
        if last == current {
            panic!("slash blocked: collateral increased in this ledger");
        }
    }
}
