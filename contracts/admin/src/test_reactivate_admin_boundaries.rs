//! Deterministic failure-boundary and recovery coverage for
//! `AdminContract::reactivate_admin` (`contracts/admin/src/lib.rs`, issue #1420).
//!
//! `reactivate_admin` reverses `deactivate_admin`: it sets `active = true` on an
//! admin record. Unlike deactivation it only requires the caller to hold **at
//! least** the target's role, so a peer admin can restore a suspended colleague
//! without a SuperAdmin. Coverage pins:
//!
//! * **authorization** — lower-role callers are rejected; equal role and above
//!   are accepted;
//! * **validation** — unknown targets and the zero sentinel are rejected;
//! * **state transitions** — reactivating an active admin is `AlreadyActive`,
//!   and a rejected call does not advance the config epoch;
//! * **paused gating** — governance mutations are blocked while paused and
//!   succeed after an unpause;
//! * **recovery** — reactivation restores every role check.

#![cfg(test)]

use crate::*;
use credence_errors::Role;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

// Wire-stable discriminants (`credence_errors::ContractError`).
const ERR_NOT_ADMIN: u32 = 100;
const ERR_CONTRACT_PAUSED: u32 = 106;
const ERR_INVALID_ADMIN_ADDRESS: u32 = 110;
const ERR_ALREADY_ACTIVE: u32 = 405;

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

/// A caller below the target's role is rejected with `NotAdmin`; an equal-role
/// peer (and the SuperAdmin) may reactivate. The target stays inactive on every
/// rejected call.
#[test]
fn reactivate_requires_at_least_target_role() {
    let (e, client, super_admin, admin, operator) = setup();
    let admin2 = Address::generate(&e);
    client.add_admin(&super_admin, &admin2, &AdminRole::Admin);

    client.deactivate_admin(&super_admin, &admin);
    assert!(!is_active(&client, &admin));

    // Lower role (Operator) cannot reactivate an Admin.
    let err = client
        .try_reactivate_admin(&operator, &admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));
    assert!(!is_active(&client, &admin));

    // Equal role (peer Admin) is allowed.
    client.reactivate_admin(&admin2, &admin);
    assert!(is_active(&client, &admin));

    // Recovery: the SuperAdmin is always authorized too.
    client.deactivate_admin(&super_admin, &admin);
    client.reactivate_admin(&super_admin, &admin);
    assert!(is_active(&client, &admin));
}

/// Reactivating an address that was never an admin is `NotAdmin` and changes
/// nothing.
#[test]
fn reactivate_unknown_target_is_not_admin() {
    let (e, client, super_admin, _, _) = setup();
    let stranger = Address::generate(&e);

    let epoch_before = client.get_config_epoch();
    let err = client
        .try_reactivate_admin(&super_admin, &stranger)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));
    assert_eq!(client.get_config_epoch(), epoch_before);
}

/// The zero/invalid sentinel cannot be targeted; validation runs before any
/// state is touched.
#[test]
fn reactivate_zero_address_is_rejected() {
    let (e, client, super_admin, _, _) = setup();
    let epoch_before = client.get_config_epoch();

    let err = client
        .try_reactivate_admin(&super_admin, &zero_address(&e))
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

/// Reactivating an already-active admin is `AlreadyActive`; the rejected call
/// does not bump the config epoch.
#[test]
fn reactivate_active_admin_is_already_active() {
    let (_, client, super_admin, admin, _) = setup();

    let epoch_before = client.get_config_epoch();
    let err = client
        .try_reactivate_admin(&super_admin, &admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_ALREADY_ACTIVE)
    );
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert!(is_active(&client, &admin));
}

/// A second reactivation after a successful one is also `AlreadyActive` — the
/// operation is not idempotent, so a retry must be guarded by the caller.
#[test]
fn reactivate_twice_is_already_active() {
    let (_, client, super_admin, admin, _) = setup();

    client.deactivate_admin(&super_admin, &admin);
    client.reactivate_admin(&super_admin, &admin);
    assert!(is_active(&client, &admin));

    let err = client
        .try_reactivate_admin(&super_admin, &admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_ALREADY_ACTIVE)
    );
}

/// A successful reactivation advances the config epoch exactly once and emits to
/// the audit log.
#[test]
fn reactivate_bumps_epoch_and_emits_event() {
    let (e, client, super_admin, admin, _) = setup();
    client.deactivate_admin(&super_admin, &admin);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    client.reactivate_admin(&super_admin, &admin);

    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    assert!(e.events().all().len() > events_before);
    assert!(is_active(&client, &admin));
}

// ---------------------------------------------------------------------------
// Paused gating + recovery
// ---------------------------------------------------------------------------

/// While the contract is paused, reactivation is rejected with `ContractPaused`
/// and the admin stays inactive; unpausing lets the same call succeed.
#[test]
fn reactivate_rejected_while_paused_then_recovers() {
    let (_, client, super_admin, admin, _) = setup();
    client.deactivate_admin(&super_admin, &admin);

    client.pause(&super_admin);
    assert!(client.is_paused());

    let err = client
        .try_reactivate_admin(&super_admin, &admin)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_CONTRACT_PAUSED)
    );
    assert!(!is_active(&client, &admin));

    client.unpause(&super_admin);
    client.reactivate_admin(&super_admin, &admin);
    assert!(is_active(&client, &admin));
}

/// Reactivation restores every role check the deactivation revoked.
#[test]
fn reactivate_restores_role_checks() {
    let (_, client, super_admin, admin, _) = setup();

    client.deactivate_admin(&super_admin, &admin);
    assert_eq!(client.is_admin(&admin), Role::User);

    client.reactivate_admin(&super_admin, &admin);
    assert_eq!(client.is_admin(&admin), Role::Admin);
    assert!(client.has_role_at_least(&admin, &AdminRole::Admin));
}
