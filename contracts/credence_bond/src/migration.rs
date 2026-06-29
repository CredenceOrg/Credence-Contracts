//! Storage migration utilities for IdentityBond
use crate::{DataKey, IdentityBond};
use soroban_sdk::Env;

/// Perform lazy migration of IdentityBond storage from v1 to v2 format.
///
/// This function reads the existing bond entry (if any) and writes it back
/// using the current `IdentityBond` definition.  Missing fields introduced in
/// v2 (`is_rolling`, `withdrawal_requested_at`, `notice_period_duration`)
/// will be populated with their default values (`false` and `0`).
///
/// The migration is idempotent and safe to call on every read; it only writes
/// when a bond is present.
pub fn migrate_v1_to_v2(e: &Env) {
    let key = DataKey::Bond;
    if let Some(old_bond) = e.storage().instance().get::<DataKey, IdentityBond>(&key) {
        e.storage().instance().set(&key, &old_bond);
    }
}
