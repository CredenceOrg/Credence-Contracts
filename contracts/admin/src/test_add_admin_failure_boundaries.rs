//! Deterministic failure-boundary coverage for `add_admin` (issue #1416).
//!
//! Every rejected generated-client invocation must preserve the existing
//! admin record, both indexes, configuration epoch, and committed event log.
//! Once its rejecting condition is resolved, the same intended change can be
//! retried and committed exactly once.

#![cfg(test)]

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Error, String,
};

fn setup(max_admins: u32) -> (Env, Address, AdminContractClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&env, &contract_id);
    let super_admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&super_admin, &1u32, &max_admins);

    (env, contract_id, client, super_admin)
}

fn assert_add_rejected_without_mutation(
    env: &Env,
    client: &AdminContractClient,
    caller: &Address,
    target: &Address,
    role: AdminRole,
    expected_error: u32,
) {
    let count_before = client.get_admin_count();
    let admins_before = client.get_all_admins();
    let role_admins_before = client.get_admins_by_role(&role);
    let epoch_before = client.get_config_epoch();
    let events_before = env.events().all().len();

    let error = client
        .try_add_admin(caller, target, &role)
        .unwrap_err()
        .unwrap();

    assert_eq!(error, Error::from_contract_error(expected_error));
    assert_eq!(client.get_admin_count(), count_before);
    assert_eq!(client.get_all_admins(), admins_before);
    assert_eq!(client.get_admins_by_role(&role), role_admins_before);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(env.events().all().len(), events_before);
}

#[test]
fn successful_add_commits_record_indexes_events_and_one_epoch() {
    let (env, _contract_id, client, super_admin) = setup(2);
    let target = Address::generate(&env);
    let timestamp = env.ledger().timestamp();
    let events_before = env.events().all().len();

    let info = client.add_admin(&super_admin, &target, &AdminRole::Admin);

    assert_eq!(info.address, target);
    assert_eq!(info.role, AdminRole::Admin);
    assert_eq!(info.assigned_at, timestamp);
    assert_eq!(info.assigned_by, super_admin);
    assert!(info.active);
    assert_eq!(info.suspended_until, 0);
    assert_eq!(client.get_admin_info(&target).role, AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);
    assert_eq!(client.get_admins_by_role(&AdminRole::Admin).len(), 1);
    assert_eq!(client.get_config_epoch(), 1);
    assert_eq!(env.events().all().len(), events_before + 2);
}

#[test]
fn invalid_addresses_are_rejected_atomically_and_valid_retry_succeeds() {
    let (env, contract_id, client, super_admin) = setup(3);
    let contract_address = contract_id.clone();
    let zero_address = Address::from_string(&String::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));

    assert_add_rejected_without_mutation(
        &env,
        &client,
        &super_admin,
        &contract_address,
        AdminRole::Admin,
        110,
    );
    assert_add_rejected_without_mutation(
        &env,
        &client,
        &super_admin,
        &zero_address,
        AdminRole::Admin,
        110,
    );

    let valid_target = Address::generate(&env);
    client.add_admin(&super_admin, &valid_target, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);
    assert_eq!(client.get_config_epoch(), 1);
}

#[test]
fn duplicate_and_underprivileged_attempts_preserve_state_and_allow_retry() {
    let (env, _contract_id, client, super_admin) = setup(4);
    let admin = Address::generate(&env);
    let target = Address::generate(&env);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);

    assert_add_rejected_without_mutation(
        &env,
        &client,
        &super_admin,
        &admin,
        AdminRole::Admin,
        405,
    );
    assert_add_rejected_without_mutation(&env, &client, &admin, &target, AdminRole::Admin, 100);

    client.add_admin(&super_admin, &target, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 3);
    assert_eq!(client.get_admin_role(&target), AdminRole::Admin);
    assert_eq!(client.get_config_epoch(), 2);
}

#[test]
fn max_admin_boundary_rejects_overflow_then_recovers_for_retry() {
    let (env, _contract_id, client, super_admin) = setup(2);
    let first = Address::generate(&env);
    let retry_target = Address::generate(&env);
    client.add_admin(&super_admin, &first, &AdminRole::Admin);

    // The configured maximum itself is valid; only the next add is rejected.
    assert_eq!(client.get_admin_count(), 2);
    assert_add_rejected_without_mutation(
        &env,
        &client,
        &super_admin,
        &retry_target,
        AdminRole::Admin,
        601,
    );

    client.remove_admin(&super_admin, &first);
    let before_retry_epoch = client.get_config_epoch();
    client.add_admin(&super_admin, &retry_target, &AdminRole::Admin);

    assert_eq!(client.get_admin_count(), 2);
    assert_eq!(client.get_admin_role(&retry_target), AdminRole::Admin);
    assert_eq!(client.get_config_epoch(), before_retry_epoch + 1);
}

#[test]
fn paused_add_is_atomic_and_succeeds_after_unpause() {
    let (env, _contract_id, client, super_admin) = setup(2);
    let target = Address::generate(&env);
    client.pause(&super_admin);

    assert_add_rejected_without_mutation(
        &env,
        &client,
        &super_admin,
        &target,
        AdminRole::Admin,
        106,
    );

    client.unpause(&super_admin);
    client.add_admin(&super_admin, &target, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);
    assert_eq!(client.get_admin_role(&target), AdminRole::Admin);
}
