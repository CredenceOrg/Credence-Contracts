//! Deterministic failure-boundary coverage for the admin `unpause` entry point.
//!
//! [`AdminContract::unpause`] (declared in `lib.rs`, implemented in
//! [`crate::pausable`]) mirrors `pause` with two modes:
//!
//! * **direct** — when no signer threshold is configured (`threshold == 0`) a
//!   SuperAdmin unpauses immediately and `unpause` returns `None`;
//! * **proposal** — when a threshold is configured only a registered pause
//!   signer may initiate the unpause and `unpause` returns `Some(proposal_id)`.
//!
//! Coverage spans the input classes that matter for a privileged toggle:
//!
//! * **valid**     — a legitimate caller flips (or proposes) the state once;
//! * **invalid**   — missing authority is rejected with a stable error and
//!                   leaves no state, event, or epoch advance;
//! * **duplicate** — a repeated/retried call is an idempotent no-op;
//! * **boundary**  — already-unpaused toggles, stale-epoch proposals, and
//!                   proposal lifecycle edges.
//!
//! Shared invariant (see the retry contract at the top of `lib.rs`): a rejected,
//! stale, repeated, or no-op operation never advances
//! [`AdminContract::get_config_epoch`] and never leaves partial state behind,
//! while a committed mutation advances it exactly once.

#![cfg(test)]

use crate::pausable::PROPOSAL_EPOCH_SIZE;
use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

// Wire-stable error discriminants (`credence_errors::ContractError`).
const ERR_NOT_ADMIN: u32 = 100;
const ERR_NOT_SIGNER: u32 = 104;
const ERR_STALE_ADMIN_EPOCH: u32 = 514;
const ERR_PROPOSAL_NOT_FOUND: u32 = 603;
const ERR_INSUFFICIENT_APPROVALS: u32 = 605;

fn setup() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, super_admin)
}

/// Register `n` fresh pause signers and set `threshold`, returning them in order.
fn configure_signers(
    e: &Env,
    client: &AdminContractClient,
    admin: &Address,
    n: usize,
    threshold: u32,
) -> soroban_sdk::Vec<Address> {
    let mut signers = soroban_sdk::Vec::new(e);
    for _ in 0..n {
        let s = Address::generate(e);
        client.set_pause_signer(admin, &s, &true);
        signers.push_back(s);
    }
    client.set_pause_threshold(admin, &threshold);
    signers
}

// ---------------------------------------------------------------------------
// Direct mode (threshold == 0)
// ---------------------------------------------------------------------------

/// Direct unpause is SuperAdmin-only. Lesser roles and strangers are rejected
/// with `NotAdmin`, the contract stays paused, and the epoch does not move; the
/// SuperAdmin can still unpause afterwards (retry safety).
#[test]
fn unpause_direct_requires_super_admin_role() {
    let (e, client, super_admin) = setup();
    client.pause(&super_admin);
    assert!(client.is_paused());

    let operator = Address::generate(&e);
    client.add_admin(&super_admin, &operator, &AdminRole::Operator);
    let mid = Address::generate(&e);
    client.add_admin(&super_admin, &mid, &AdminRole::Admin);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    for caller in [operator, mid, Address::generate(&e)] {
        let err = client.try_unpause(&caller).unwrap_err().unwrap();
        assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));
    }

    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // Recovery: the SuperAdmin can still unpause.
    assert!(client.unpause(&super_admin).is_none());
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
}

/// Unpausing an already-unpaused contract is an idempotent no-op: no event and
/// no epoch advance, so a retried transaction cannot desynchronise indexers.
#[test]
fn unpause_direct_is_a_noop_when_already_unpaused() {
    let (e, client, super_admin) = setup();
    assert!(!client.is_paused());

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    assert!(client.unpause(&super_admin).is_none());

    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

// ---------------------------------------------------------------------------
// Proposal mode (threshold > 0)
// ---------------------------------------------------------------------------

/// With a threshold configured, being SuperAdmin is not sufficient: only a
/// registered pause signer may initiate an unpause. Rejections leave no
/// proposal, event, or epoch change, and a registered signer can still propose.
#[test]
fn unpause_multisig_requires_registered_signer() {
    let (e, client, super_admin) = setup();
    client.pause(&super_admin);
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client.try_unpause(&super_admin).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_SIGNER));

    let err = client
        .try_unpause(&Address::generate(&e))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_SIGNER));

    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // Recovery: a registered signer proposes and completes the unpause.
    let id = client.unpause(&s1).unwrap();
    assert!(client.is_paused());
    client.execute_pause_proposal(&id);
    assert!(!client.is_paused());
}

/// The full multisig unpause lifecycle: propose (which records the caller's
/// approval), collect quorum, execute.
#[test]
fn unpause_multisig_full_flow() {
    let (e, client, super_admin) = setup();
    client.pause(&super_admin);
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.unpause(&s1).unwrap();
    assert!(client.is_paused());

    let err = client.try_execute_pause_proposal(&id).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INSUFFICIENT_APPROVALS)
    );
    assert!(client.is_paused());

    client.approve_pause_proposal(&s2, &id);
    client.execute_pause_proposal(&id);
    assert!(!client.is_paused());
}

/// A duplicated approval from the same signer is an event-free no-op.
#[test]
fn unpause_duplicate_approval_is_event_free() {
    let (e, client, super_admin) = setup();
    client.pause(&super_admin);
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.unpause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &id);

    let epoch_after_first = client.get_config_epoch();
    let events_after_first = e.events().all().len();

    client.approve_pause_proposal(&s2, &id);
    assert_eq!(client.get_config_epoch(), epoch_after_first);
    assert_eq!(e.events().all().len(), events_after_first);

    client.execute_pause_proposal(&id);
    assert!(!client.is_paused());
}

/// Executing an unpause proposal while already unpaused consumes the proposal
/// exactly once and advances the epoch exactly once, but emits no `unpaused`
/// event because the state transition is itself a no-op.
#[test]
fn redundant_unpause_execution_consumes_proposal_without_event() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    assert!(!client.is_paused());
    let id = client.unpause(&s1).unwrap();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    client.execute_pause_proposal(&id);

    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    assert_eq!(e.events().all().len(), events_before);

    // Consumed exactly once; replay is rejected.
    assert!(client.try_execute_pause_proposal(&id).is_err());
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
}

// ---------------------------------------------------------------------------
// Error / stale / not-found boundaries
// ---------------------------------------------------------------------------

/// Approving/executing an id that was never proposed is rejected with
/// `ProposalNotFound` before any state is touched.
#[test]
fn unpause_unknown_proposal_is_rejected() {
    let (e, client, super_admin) = setup();
    client.pause(&super_admin);
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();
    let unknown_id = 98_765_u64;

    let err = client
        .try_approve_pause_proposal(&s1, &unknown_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_PROPOSAL_NOT_FOUND)
    );

    let err = client
        .try_execute_pause_proposal(&unknown_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_PROPOSAL_NOT_FOUND)
    );

    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// An unpause proposal goes stale once the ledger crosses into the next epoch:
/// approval and execution both fail with `StaleAdminEpoch`, emit nothing, and
/// leave the proposal record intact (no partial cleanup).
#[test]
fn stale_epoch_unpause_proposal_cannot_execute() {
    let (e, client, super_admin) = setup();
    client.pause(&super_admin);
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    let epoch_boundary = u32::from(PROPOSAL_EPOCH_SIZE);
    e.ledger()
        .with_mut(|l| l.sequence_number = epoch_boundary - 1);
    let id = client.unpause(&s1).unwrap();

    let epoch_after_propose = client.get_config_epoch();
    e.ledger().with_mut(|l| l.sequence_number = epoch_boundary);
    let events_before = e.events().all().len();

    let err = client.try_execute_pause_proposal(&id).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_STALE_ADMIN_EPOCH)
    );
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_after_propose);
    assert_eq!(e.events().all().len(), events_before);

    let err = client
        .try_approve_pause_proposal(&s1, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_STALE_ADMIN_EPOCH)
    );
    assert_eq!(e.events().all().len(), events_before);
}
