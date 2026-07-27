#![cfg(test)]

use crate::migration::{require_no_ongoing_migration, MigrationStatus};
use soroban_sdk::Env;

#[test]
#[should_panic(expected = "Error(Contract, 125)")]
fn migration_guard_rejects_in_progress_state() {
    let e = Env::default();

    require_no_ongoing_migration(&e, MigrationStatus::InProgress);
}

#[test]
fn migration_guard_allows_completed_state() {
    let e = Env::default();

    require_no_ongoing_migration(&e, MigrationStatus::Completed);
}

#[test]
fn migration_guard_allows_unset_state() {
    let e = Env::default();

    require_no_ongoing_migration(&e, MigrationStatus::None);
}
