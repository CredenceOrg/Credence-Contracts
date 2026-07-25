#![cfg(test)]

use crate::migration::{ensure_migration_not_in_progress, MigrationStatus};
use soroban_sdk::Env;

#[test]
#[should_panic(expected = "migration in progress")]
fn migration_guard_rejects_in_progress_state() {
    let e = Env::default();

    ensure_migration_not_in_progress(&e, MigrationStatus::InProgress);
}

#[test]
fn migration_guard_allows_completed_state() {
    let e = Env::default();

    ensure_migration_not_in_progress(&e, MigrationStatus::Completed);
}

#[test]
fn migration_guard_allows_unset_state() {
    let e = Env::default();

    ensure_migration_not_in_progress(&e, MigrationStatus::None);
}
