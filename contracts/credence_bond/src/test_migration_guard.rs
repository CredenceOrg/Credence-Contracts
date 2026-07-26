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

use crate::{CredenceBond, CredenceBondClient};

#[test]
#[should_panic(expected = "Migration is in progress")]
fn test_mutating_functions_fail_during_migration() {
    let e = Env::default();
    let admin = soroban_sdk::Address::generate(&e);
    let identity = soroban_sdk::Address::generate(&e);
    let client = CredenceBondClient::new(&e, &e.register_contract(None, CredenceBond {}));

    // Initialize the contract
    client.initialize(&admin);

    // Set migration status in storage
    e.as_contract(&client.address, || {
        e.storage().instance().set(
            &crate::DataKey::MigrationStatus,
            &crate::migration::MigrationStatus::InProgress,
        );
    });

    // Try a mutating function, it should fail
    client.top_up(&identity, &100);
}
