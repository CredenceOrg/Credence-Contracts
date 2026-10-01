//! Deterministic failure-boundary coverage for the admin `set_pause_threshold`
//! entry point.
//!
//! [`AdminContract::set_pause_threshold`] (declared in `lib.rs`, implemented in
//! [`crate::pausable`]) selects between the two pause modes:
//!
//! * `threshold == 0` — direct SuperAdmin toggle;
//! * `threshold >= 1` — multisig, where `threshold` signer approvals are needed.
//!
//! The value is therefore a governance-critical boundary. Coverage spans:
//!
//! * **valid**     — every value in `0..=signer_count` is accepted;
//! * **invalid**   — non-SuperAdmin callers (`NotAdmin`) and values above the
//!                   signer count (`ThresholdExceedsSigners`) are rejected
//!                   without touching state, events, or the config epoch;
//! * **duplicate** — re-setting the current value is an event-free no-op;
//! * **boundary**  — the inclusive upper boundary, the zero/direct boundary,
//!                   and the automatic clamp when a signer is removed.

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

// Wire-stable error discriminants (`credence_errors::ContractError`).
const ERR_NOT_ADMIN: u32 = 100;
const ERR_THRESHOLD_EXCEEDS_SIGNERS: u32 = 601;

fn setup() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, super_admin)
}

/// Enable `n` fresh signers (leaving the threshold untouched).
fn add_signers(
    e: &Env,
    client: &AdminContractClient,
    admin: &Address,
    n: usize,
) -> soroban_sdk::Vec<Address> {
    let mut signers = soroban_sdk::Vec::new(e);
    for _ in 0..n {
        let s = Address::generate(e);
        client.set_pause_signer(admin, &s, &true);
        signers.push_back(s);
    }
    signers
}

/// Read the threshold straight from contract storage (no public getter exists).
fn stored_threshold(e: &Env, client: &AdminContractClient) -> u32 {
    e.as_contract(&client.address, || {
        e.storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0)
    })
}

// ---------------------------------------------------------------------------
// Authorization boundary
// ---------------------------------------------------------------------------

/// Only the SuperAdmin may set the threshold. Lesser admin roles and strangers
/// are rejected with `NotAdmin`; the stored threshold, event log, and config
/// epoch are untouched, and a valid set still works afterwards.
#[test]
fn set_pause_threshold_requires_super_admin() {
    let (e, client, super_admin) = setup();
    add_signers(&e, &client, &super_admin, 1);

    let operator = Address::generate(&e);
    client.add_admin(&super_admin, &operator, &AdminRole::Operator);
    let mid = Address::generate(&e);
    client.add_admin(&super_admin, &mid, &AdminRole::Admin);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    for caller in [operator, mid, Address::generate(&e)] {
        let err = client
            .try_set_pause_threshold(&caller, &1u32)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));
    }

    assert_eq!(stored_threshold(&e, &client), 0);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // Recovery: the SuperAdmin can still set it.
    client.set_pause_threshold(&super_admin, &1u32);
    assert_eq!(stored_threshold(&e, &client), 1);
}

// ---------------------------------------------------------------------------
// Validation boundaries
// ---------------------------------------------------------------------------

/// A value above the number of enabled signers is rejected as
/// `ThresholdExceedsSigners` with no state change; exactly the signer count is
/// accepted (inclusive upper boundary); and a valid value is accepted after the
/// rejection (retry safety).
#[test]
fn set_pause_threshold_rejects_above_signer_count() {
    let (e, client, super_admin) = setup();
    add_signers(&e, &client, &super_admin, 1);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_set_pause_threshold(&super_admin, &2u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_THRESHOLD_EXCEEDS_SIGNERS)
    );
    assert_eq!(stored_threshold(&e, &client), 0);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // The inclusive upper boundary (== signer count) is accepted.
    client.set_pause_threshold(&super_admin, &1u32);
    assert_eq!(stored_threshold(&e, &client), 1);
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
}

/// Every value in `0..=signer_count` is a valid threshold.
#[test]
fn set_pause_threshold_accepts_every_value_up_to_signer_count() {
    let (e, client, super_admin) = setup();
    add_signers(&e, &client, &super_admin, 3);

    for threshold in 0u32..=3u32 {
        client.set_pause_threshold(&super_admin, &threshold);
        assert_eq!(stored_threshold(&e, &client), threshold);
    }
}

/// Re-setting the current value is a no-op: no new event and no epoch bump.
#[test]
fn set_pause_threshold_same_value_is_event_free() {
    let (e, client, super_admin) = setup();
    add_signers(&e, &client, &super_admin, 1);

    client.set_pause_threshold(&super_admin, &1u32);
    let epoch_after_set = client.get_config_epoch();
    let events_after_set = e.events().all().len();

    client.set_pause_threshold(&super_admin, &1u32);
    assert_eq!(client.get_config_epoch(), epoch_after_set);
    assert_eq!(e.events().all().len(), events_after_set);
}

/// `threshold == 0` restores the direct SuperAdmin path for both `pause` and
/// `unpause` (each returns `None` and flips the state immediately).
#[test]
fn set_pause_threshold_zero_restores_direct_mode() {
    let (e, client, super_admin) = setup();
    add_signers(&e, &client, &super_admin, 2);

    client.set_pause_threshold(&super_admin, &2u32);
    // Multisig mode: a direct SuperAdmin pause is rejected (not a signer).
    assert!(client.try_pause(&super_admin).is_err());
    assert!(!client.is_paused());

    // Back to direct mode.
    client.set_pause_threshold(&super_admin, &0u32);
    assert!(client.pause(&super_admin).is_none());
    assert!(client.is_paused());
    assert!(client.unpause(&super_admin).is_none());
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// Signer-removal clamp
// ---------------------------------------------------------------------------

/// Removing a signer clamps the threshold to the remaining signer count so the
/// surviving signers can still reach quorum. The clamp never raises the value.
#[test]
fn removing_a_signer_clamps_threshold_down() {
    let (e, client, super_admin) = setup();
    let signers = add_signers(&e, &client, &super_admin, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();
    client.set_pause_threshold(&super_admin, &2u32);
    assert_eq!(stored_threshold(&e, &client), 2);

    client.set_pause_signer(&super_admin, &s2, &false);
    assert_eq!(stored_threshold(&e, &client), 1);

    // The one remaining signer now meets quorum on their own.
    let id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

/// Setting the threshold never changes the paused state by itself.
#[test]
fn set_pause_threshold_does_not_change_pause_state() {
    let (e, client, super_admin) = setup();
    add_signers(&e, &client, &super_admin, 1);

    assert!(!client.is_paused());
    client.set_pause_threshold(&super_admin, &1u32);
    assert!(!client.is_paused());
    client.set_pause_threshold(&super_admin, &0u32);
    assert!(!client.is_paused());
}
