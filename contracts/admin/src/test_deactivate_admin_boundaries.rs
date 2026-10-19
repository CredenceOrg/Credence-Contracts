//! Deterministic failure-boundary and recovery coverage for
//! `AdminContract::deactivate_admin` (`contracts/admin/src/lib.rs`, issue #1419).
//!
//! `deactivate_admin` is the reversible half of the admin lifecycle: it sets
//! `active = false` while leaving the role in place, so `reactivate_admin` can
//! restore it. Coverage pins:
//!
//! * **authorization** — the caller must strictly outrank the target;
//! * **validation** — unknown targets and the zero/invalid sentinel are
//!   rejected;
//! * **state transitions** — a second deactivation is `AlreadyDeactivated`, and
//!   a rejected call never mutates state or the config epoch;
//! * **paused gating** — governance mutations are blocked while paused and
//!   succeed after an unpause;
//! * **recovery** — a deactivated admin loses every role check until
//!   reactivation restores them.

#![cfg(test)]

use crate::*;
use credence_errors::Role;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

// Wire-stable discriminants (`credence_errors::ContractError`).
const ERR_NOT_ADMIN: u32 = 100;
const ERR_CONTRACT_PAUSED: u32 = 106;
const ERR_INVALID_ADMIN_ADDRESS: u32 = 110;
const ERR_ALREADY_DEACTIVATED: u32 = 404;

fn setup() -> (Env, AdminContractClient<'static>, Address, Address, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    client.initialize(&super_admin, &1u32, &100u32);

    let admin = Address::generate(&e);
    let operator = Address::generate(&e);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    client.add_admin(&admin, &operator, &AdminRole::Operator);

    (e, client, super_admin, admin, operator)
}

fn zero_address(e: &Env) -> Address {
    Address::from_string(&String::from_str(
        e,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ))
}

fn is_active(client: &AdminContractClient, who: &Address) -> bool {
    client.get_admin_info(who).active
}

// ---------------------------------------------------------------------------
// Authorization boundaries
// ---------------------------------------------------------------------------

/// The caller must strictly outrank the target: equal-role (including
/// self-deactivation) and lower-role callers are rejected with `NotAdmin` and
/// leave the target untouched; a higher-role caller succeeds.
#[test]
fn deactivate_requires_strictly_higher_role() {
    let (e, client, super_admin, admin, operator) = setup();
    let admin2 = Address::generate(&e);
    client.add_admin(&super_admin, &admin2, &AdminRole::Admin);

    // Equal role (different admin) rejected.
    let err = client
        .try_deactivate_admin(&admin, &admin2)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    // Self-deactivation rejected (role is not strictly higher than itself).
    let err = client
        .try_deactivate_admin(&admin, &admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    // Lower role rejected.
    let err = client
        .try_deactivate_admin(&operator, &admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    assert!(is_active(&client, &admin));
    assert!(is_active(&client, &admin2));

    // Strictly higher role succeeds; the SuperAdmin is never blocked.
    client.deactivate_admin(&admin, &operator);
    assert!(!is_active(&client, &operator));
    client.deactivate_admin(&super_admin, &admin);
    assert!(!is_active(&client, &admin));
}

/// Deactivating an address that was never an admin is `NotAdmin` and changes
/// nothing.
#[test]
fn deactivate_unknown_target_is_not_admin() {
    let (e, client, super_admin, _, _) = setup();
    let stranger = Address::generate(&e);

    let epoch_before = client.get_config_epoch();
    let err = client
        .try_deactivate_admin(&super_admin, &stranger)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));
    assert_eq!(client.get_config_epoch(), epoch_before);
}

/// The zero/invalid sentinel and the contract's own address cannot be targeted;
/// the address validation runs before any state is touched.
#[test]
fn deactivate_zero_address_is_rejected() {
    let (e, client, super_admin, _, _) = setup();
    let epoch_before = client.get_config_epoch();

    let err = client
        .try_deactivate_admin(&super_admin, &zero_address(&e))
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_ADMIN_ADDRESS)
    );
    assert_eq!(client.get_config_epoch(), epoch_before);
}

// ---------------------------------------------------------------------------
// State-transition boundaries
// ---------------------------------------------------------------------------

/// A second deactivation is `AlreadyDeactivated`; the rejected call does not
/// bump the config epoch and the target stays inactive.
#[test]
fn deactivate_twice_is_already_deactivated() {
    let (_, client, super_admin, admin, _) = setup();

    client.deactivate_admin(&super_admin, &admin);
    assert!(!is_active(&client, &admin));

    let epoch_after_first = client.get_config_epoch();
    let err = client
        .try_deactivate_admin(&super_admin, &admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_ALREADY_DEACTIVATED)
    );
    assert_eq!(client.get_config_epoch(), epoch_after_first);
    assert!(!is_active(&client, &admin));
}

/// A successful deactivation advances the config epoch exactly once and emits
/// to the audit log.
#[test]
fn deactivate_bumps_epoch_and_emits_event() {
    let (e, client, super_admin, admin, _) = setup();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    client.deactivate_admin(&super_admin, &admin);

    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    assert!(e.events().all().len() > events_before);
    assert!(!is_active(&client, &admin));
}

// ---------------------------------------------------------------------------
// Paused gating + recovery
// ---------------------------------------------------------------------------

/// While the contract is paused, deactivation is rejected with `ContractPaused`
/// and the admin remains active; unpausing lets the same call succeed.
#[test]
fn deactivate_rejected_while_paused_then_recovers() {
    let (_, client, super_admin, admin, _) = setup();

    client.pause(&super_admin);
    assert!(client.is_paused());

    let err = client
        .try_deactivate_admin(&super_admin, &admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_CONTRACT_PAUSED)
    );
    assert!(is_active(&client, &admin));

    client.unpause(&super_admin);
    client.deactivate_admin(&super_admin, &admin);
    assert!(!is_active(&client, &admin));
}

/// A deactivated admin immediately fails role checks, and reactivation restores
/// full privileges (the reversible half of the lifecycle).
#[test]
fn deactivate_removes_privileges_then_reactivate_restores() {
    let (_, client, super_admin, admin, _) = setup();

    client.deactivate_admin(&super_admin, &admin);
    assert_eq!(client.is_admin(&admin), Role::User);
    assert!(!client.has_role_at_least(&admin, &AdminRole::Operator));

    client.reactivate_admin(&super_admin, &admin);
    assert_eq!(client.is_admin(&admin), Role::Admin);
    assert!(client.has_role_at_least(&admin, &AdminRole::Admin));
}
