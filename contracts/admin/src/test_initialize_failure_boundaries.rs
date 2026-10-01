//! Deterministic failure-boundary coverage for the admin `initialize` entry point.
//!
//! [`AdminContract::initialize`] (declared in `lib.rs`) is a one-shot, privileged
//! bootstrap: it establishes the super admin, the admin-count configuration, and
//! the initial pause / role / ownership state, then emits exactly one
//! `admin_initialized` event. Because it is one-shot, it must behave
//! deterministically for every input class and must never leave partial state:
//!
//! * **valid**         - a first call with `1 <= min_admins <= max_admins` succeeds;
//! * **invalid**       - `min_admins == 0` or `min_admins > max_admins` is rejected
//!                       with `InvalidPauseAction` and writes nothing;
//! * **duplicate**     - a second call is rejected with `AlreadyInitialized` and
//!                       cannot overwrite the original configuration;
//! * **boundary**      - `min == max`, and `max == u32::MAX`, are accepted;
//! * **authorization** - the bootstrap requires the super admin's signature.
//!
//! The shared invariant is atomicity: a rejected `initialize` rolls the whole
//! invocation back, so no storage key and no event is touched. That is asserted
//! directly (configuration state and event count) after every rejection.

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

// Wire-stable error discriminants (`credence_errors::ContractError`).
const ERR_NOT_INITIALIZED: u32 = 1;
const ERR_ALREADY_INITIALIZED: u32 = 2;
const ERR_INVALID_PAUSE_ACTION: u32 = 107;

/// Register a fresh admin contract without initializing it, so callers control
/// whether auth is mocked and whether `initialize` has run.
fn setup() -> (Env, AdminContractClient<'static>) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    (e, client)
}

fn event_count(e: &Env) -> u32 {
    e.events().all().len()
}

#[test]
fn initialize_sets_the_full_bootstrap_state() {
    let (e, client) = setup();
    let super_admin = Address::generate(&e);
    e.mock_all_auths();

    let events_before = event_count(&e);
    client.initialize(&super_admin, &2u32, &10u32);

    // Configuration is stored verbatim.
    assert_eq!(client.get_config(), (2u32, 10u32));

    // The super admin owns the contract and is the sole registered admin.
    assert_eq!(client.get_owner(), super_admin);
    assert_eq!(client.get_admin_count(), 1);
    let info = client.get_admin_info(&super_admin);
    assert_eq!(info.role, AdminRole::SuperAdmin);
    assert_eq!(info.assigned_by, super_admin);
    assert!(info.active);

    let all = client.get_all_admins();
    assert_eq!(all.len(), 1);
    assert_eq!(all.get(0).unwrap(), super_admin);
    assert_eq!(client.get_admins_by_role(&AdminRole::SuperAdmin).len(), 1);
    assert_eq!(client.get_admins_by_role(&AdminRole::Admin).len(), 0);
    assert_eq!(client.get_admins_by_role(&AdminRole::Operator).len(), 0);

    // Pause state starts unpaused.
    assert!(!client.is_paused());

    // Exactly one lifecycle event is emitted.
    assert_eq!(event_count(&e), events_before + 1);
}

#[test]
fn initialize_requires_super_admin_authorization() {
    let (e, client) = setup();
    let super_admin = Address::generate(&e);

    // No auths mocked: the signed bootstrap must fail.
    let result = client.try_initialize(&super_admin, &1u32, &100u32);
    assert!(result.is_err(), "initialize must require super admin auth");

    // The failed call wrote nothing.
    let err = client.try_get_config().unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_NOT_INITIALIZED)
    );
    assert_eq!(event_count(&e), 0);
}

#[test]
fn initialize_rejects_zero_min_admins_without_writing() {
    let (e, client) = setup();
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    let events_before = event_count(&e);

    let err = client
        .try_initialize(&super_admin, &0u32, &100u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_PAUSE_ACTION)
    );

    // Atomic rejection: nothing stored, no event.
    let cfg_err = client.try_get_config().unwrap_err().unwrap();
    assert_eq!(
        cfg_err,
        soroban_sdk::Error::from_contract_error(ERR_NOT_INITIALIZED)
    );
    assert_eq!(event_count(&e), events_before);

    // A subsequent valid initialize still succeeds, proving no partial state.
    client.initialize(&super_admin, &1u32, &100u32);
    assert_eq!(client.get_config(), (1u32, 100u32));
}

#[test]
fn initialize_rejects_min_greater_than_max_without_writing() {
    let (e, client) = setup();
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    let events_before = event_count(&e);

    let err = client
        .try_initialize(&super_admin, &11u32, &10u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_PAUSE_ACTION)
    );

    let cfg_err = client.try_get_config().unwrap_err().unwrap();
    assert_eq!(
        cfg_err,
        soroban_sdk::Error::from_contract_error(ERR_NOT_INITIALIZED)
    );
    assert_eq!(event_count(&e), events_before);
}

#[test]
fn initialize_rejects_duplicate_without_overwriting() {
    let (e, client) = setup();
    let super_admin = Address::generate(&e);
    let other = Address::generate(&e);
    e.mock_all_auths();

    client.initialize(&super_admin, &1u32, &100u32);
    let events_after_first = event_count(&e);

    // A second bootstrap must be rejected, even from a different admin.
    let err = client
        .try_initialize(&other, &5u32, &5u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_ALREADY_INITIALIZED)
    );

    // Original configuration, owner and admin set are untouched.
    assert_eq!(client.get_config(), (1u32, 100u32));
    assert_eq!(client.get_owner(), super_admin);
    assert_eq!(client.get_admin_count(), 1);
    let all = client.get_all_admins();
    assert_eq!(all.len(), 1);
    assert_eq!(all.get(0).unwrap(), super_admin);
    assert_eq!(event_count(&e), events_after_first);
}

#[test]
fn initialize_accepts_min_equal_to_max_boundary() {
    let (e, client) = setup();
    let super_admin = Address::generate(&e);
    e.mock_all_auths();

    client.initialize(&super_admin, &3u32, &3u32);

    assert_eq!(client.get_config(), (3u32, 3u32));
}

#[test]
fn initialize_accepts_u32_max_boundary() {
    let (e, client) = setup();
    let super_admin = Address::generate(&e);
    e.mock_all_auths();

    client.initialize(&super_admin, &1u32, &u32::MAX);

    assert_eq!(client.get_config(), (1u32, u32::MAX));
}
