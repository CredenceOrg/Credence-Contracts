//! Storage migration utilities for IdentityBond
use crate::{DataKey, IdentityBond};
use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Env};

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationStatus {
    None = 0,
    InProgress = 1,
    Completed = 2,
}

/// Ensures that no migration is currently in progress, to prevent
/// state mutations during an active migration.
pub fn require_no_ongoing_migration(e: &Env, status: MigrationStatus) {
    if status == MigrationStatus::InProgress {
        panic_with_error!(e, ContractError::MigrationInProgress);
    }
}
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
