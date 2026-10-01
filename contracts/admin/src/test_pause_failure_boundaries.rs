//! Deterministic failure-boundary coverage for the admin pause entry point.
//!
//! [`AdminContract::pause`] (declared in `lib.rs`, implemented in
//! [`crate::pausable`]) has two execution modes:
//!
//! * **direct** — when no signer threshold is configured (`threshold == 0`) a
//!   SuperAdmin toggles the state immediately and `pause` returns `None`;
//! * **proposal** — when a threshold is configured `pause` may only be
//!   initiated by a registered pause signer and returns `Some(proposal_id)`.
//!
//! Both paths must behave deterministically for every input class:
//!
//! * **valid**     — a legitimate caller flips (or proposes) the state once;
//! * **invalid**   — a caller without the required authority is rejected with a
//!                   stable error and leaves no state, event, or epoch advance;
//! * **duplicate** — a retried/duplicated call is an idempotent no-op that emits
//!                   no event and does not move the config epoch;
//! * **boundary**  — threshold == signer count, threshold clamping on signer
//!                   removal, redundant execution, and stale-epoch proposals.
//!
//! The shared invariant is the retry contract documented at the top of
//! `lib.rs`: a rejected, stale, repeated, or failed privileged operation never
//! advances [`AdminContract::get_config_epoch`] and never leaves partial state
//! behind, while a committed mutation advances it exactly once.

#![cfg(test)]

use crate::pausable::PROPOSAL_EPOCH_SIZE;
use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

// Wire-stable error discriminants (`credence_errors::ContractError`).
const ERR_NOT_ADMIN: u32 = 100;
const ERR_NOT_SIGNER: u32 = 104;
const ERR_STALE_ADMIN_EPOCH: u32 = 514;
const ERR_THRESHOLD_EXCEEDS_SIGNERS: u32 = 601;
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

/// Register `n` fresh pause signers and set `threshold`, returning the signer
/// addresses in registration order.
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

/// Assert the signer-count storage invariant from docs/pause-signer-invariant.md
/// directly against contract storage, and that it equals `expected`.
fn assert_pause_signer_invariant(
    e: &Env,
    client: &AdminContractClient,
    addrs: &[Address],
    expected: u32,
) {
    e.as_contract(&client.address, || {
        let mut counted: u32 = 0;
        for a in addrs.iter() {
            let enabled: bool = e
                .storage()
                .instance()
                .get(&DataKey::PauseSigner(a.clone()))
                .unwrap_or(false);
            if enabled {
                counted += 1;
            }
        }
        let stored: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        assert_eq!(
            stored, counted,
            "PauseSignerCount must equal the number of enabled signers"
        );
        assert_eq!(stored, expected);
    });
}

// ---------------------------------------------------------------------------
// Authorization / permission boundaries
// ---------------------------------------------------------------------------

/// Direct pausing (threshold == 0) is SuperAdmin-only: lesser admin roles and
/// non-admins are rejected with `NotAdmin`, leave the contract unpaused, and do
/// not advance the config epoch.
#[test]
fn pause_direct_requires_super_admin_role() {
    let (e, client, super_admin) = setup();

    let operator = Address::generate(&e);
    client.add_admin(&super_admin, &operator, &AdminRole::Operator);
    let mid = Address::generate(&e);
    client.add_admin(&super_admin, &mid, &AdminRole::Admin);

    let epoch_before = client.get_config_epoch();

    let err = client.try_pause(&operator).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    let err = client.try_pause(&mid).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    let stranger = Address::generate(&e);
    let err = client.try_pause(&stranger).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before);
}

/// When a threshold is configured, `pause` may only be initiated by a
/// registered pause signer — being a SuperAdmin is not sufficient. A rejected
/// attempt creates no proposal, emits nothing, advances no epoch, and a
/// subsequent valid attempt still works (retry safety).
#[test]
fn pause_multisig_requires_registered_signer() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client.try_pause(&super_admin).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_SIGNER));

    let stranger = Address::generate(&e);
    let err = client.try_pause(&stranger).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_SIGNER));

    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // A registered signer can still propose afterwards.
    let id = client.pause(&s1).unwrap();
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Validation / boundary cases
// ---------------------------------------------------------------------------

/// Threshold validation boundaries: above the signer count is rejected as
/// `ThresholdExceedsSigners` with no state change, exactly the signer count is
/// accepted, re-setting the current value is an event-free no-op, and `0`
/// returns to the direct SuperAdmin toggle.
#[test]
fn set_pause_threshold_boundaries() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);

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
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // Exactly the signer count is the inclusive upper boundary.
    client.set_pause_threshold(&super_admin, &1u32);
    assert_eq!(client.get_config_epoch(), epoch_before + 1);

    // Re-setting the same value is a no-op: no event, no epoch bump.
    let epoch_after_set = client.get_config_epoch();
    let events_after_set = e.events().all().len();
    client.set_pause_threshold(&super_admin, &1u32);
    assert_eq!(client.get_config_epoch(), epoch_after_set);
    assert_eq!(e.events().all().len(), events_after_set);

    // Zero is a legal boundary that returns to the direct SuperAdmin path.
    client.set_pause_threshold(&super_admin, &0u32);
    assert!(client.pause(&super_admin).is_none());
    assert!(client.is_paused());
}

/// Removing a signer clamps the threshold to the remaining signer count, so the
/// surviving signers can still reach quorum instead of the contract becoming
/// unpauseable.
#[test]
fn removing_signer_clamps_threshold_to_remaining_count() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    client.set_pause_signer(&super_admin, &s2, &false);

    let id = client.pause(&s1).unwrap();
    // s1's single approval now meets the clamped threshold.
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Error / "not found" states
// ---------------------------------------------------------------------------

/// Approving or executing an id that was never proposed is rejected with
/// `ProposalNotFound` before any state is touched.
#[test]
fn unknown_proposal_is_rejected() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();
    let unknown_id = 12_345_u64;

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

    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// Below-threshold execution is rejected as `InsufficientApprovals` without
/// committing anything; the live proposal keeps its approvals and completes
/// once the threshold is met.
#[test]
fn insufficient_approvals_rejected_but_recoverable() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap();
    let epoch_after_propose = client.get_config_epoch();

    let err = client
        .try_execute_pause_proposal(&id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INSUFFICIENT_APPROVALS)
    );
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_after_propose);

    // The proposal is still live: collecting the missing approval completes it.
    client.approve_pause_proposal(&s2, &id);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Stale-epoch boundary
// ---------------------------------------------------------------------------

/// A proposal id is bound to the ledger epoch it was derived in. Once the
/// ledger crosses into the next epoch the proposal goes stale: approval and
/// execution both fail with `StaleAdminEpoch`, no `paused` event is emitted, the
/// epoch does not move, and the proposal record survives (no partial cleanup).
#[test]
fn stale_epoch_proposal_cannot_execute() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    let epoch_boundary = u32::from(PROPOSAL_EPOCH_SIZE);
    e.ledger().with_mut(|l| l.sequence_number = epoch_boundary - 1);
    let id = client.pause(&s1).unwrap();

    let epoch_after_propose = client.get_config_epoch();
    e.ledger().with_mut(|l| l.sequence_number = epoch_boundary);
    let events_before = e.events().all().len();

    // The proposal met the threshold in its own epoch; only staleness blocks it.
    let err = client
        .try_execute_pause_proposal(&id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_STALE_ADMIN_EPOCH)
    );

    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_after_propose);
    assert_eq!(e.events().all().len(), events_before);

    // Approval is stale too, and the attempt leaves no partial approval behind.
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

// ---------------------------------------------------------------------------
// Duplicate / idempotency boundaries
// ---------------------------------------------------------------------------

/// A repeated approval from the same signer is an idempotent no-op: no event,
/// no epoch bump. This is what makes a retried approval safe to replay.
#[test]
fn duplicate_approval_is_event_free() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap();

    let events_before = e.events().all().len();
    client.approve_pause_proposal(&s2, &id);
    let epoch_after_first = client.get_config_epoch();
    let events_after_first = e.events().all().len();
    assert!(events_after_first > events_before, "first approval emits");

    // Duplicate approval: no new event, no epoch bump.
    client.approve_pause_proposal(&s2, &id);
    assert_eq!(client.get_config_epoch(), epoch_after_first);
    assert_eq!(e.events().all().len(), events_after_first);

    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

/// `set_pause_signer` is idempotent: re-enabling a registered signer or
/// disabling an unregistered one emits no event and does not advance the epoch,
/// while a real transition still emits and bumps exactly once.
#[test]
fn set_pause_signer_duplicate_is_event_free() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    client.set_pause_signer(&super_admin, &s1, &true);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    let stranger = Address::generate(&e);
    client.set_pause_signer(&super_admin, &stranger, &false);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    client.set_pause_signer(&super_admin, &stranger, &true);
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    assert_eq!(e.events().all().len(), events_before + 1);
}

// ---------------------------------------------------------------------------
// Concurrency / deterministic derivation
// ---------------------------------------------------------------------------

/// Proposal ids are derived deterministically from `(action, epoch)`, so two
/// signers proposing in the same epoch converge on a single proposal whose
/// approvals accumulate — concurrent callers cannot fork the governance state.
#[test]
fn concurrent_proposals_in_same_epoch_converge() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id1 = client.pause(&s1).unwrap();
    let id2 = client.pause(&s2).unwrap();
    assert_eq!(id1, id2, "same epoch must derive a single proposal id");
    assert!(!client.is_paused());

    // Both approvals are attached to the one proposal, meeting the threshold.
    client.execute_pause_proposal(&id1);
    assert!(client.is_paused());
    assert!(client.try_execute_pause_proposal(&id1).is_err());
}

/// The proposal-id preimage includes the action, so a Pause and an Unpause
/// proposal created in the same epoch are distinct and independently executable.
#[test]
fn pause_and_unpause_proposals_are_distinct() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    let pause_id = client.pause(&s1).unwrap();
    let unpause_id = client.unpause(&s1).unwrap();
    assert_ne!(pause_id, unpause_id);

    client.execute_pause_proposal(&pause_id);
    assert!(client.is_paused());

    client.execute_pause_proposal(&unpause_id);
    assert!(!client.is_paused());
}

/// Executing a pause proposal while the contract is already paused consumes the
/// proposal exactly once and advances the epoch exactly once, but emits no
/// `paused` event because the state transition itself is a no-op.
#[test]
fn redundant_pause_execution_consumes_proposal_without_event() {
    let (e, client, super_admin) = setup();

    // Direct pause first, then enable multisig while paused.
    client.pause(&super_admin);
    assert!(client.is_paused());

    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();
    let id = client.pause(&s1).unwrap();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    client.execute_pause_proposal(&id);

    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    assert_eq!(e.events().all().len(), events_before);

    // The proposal is consumed and cannot be replayed.
    assert!(client.try_execute_pause_proposal(&id).is_err());
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
}

// ---------------------------------------------------------------------------
// Storage invariant
// ---------------------------------------------------------------------------

/// The stored `PauseSignerCount` must always equal the number of enabled
/// `PauseSigner` entries (see docs/pause-signer-invariant.md), across
/// idempotent add/remove/no-op sequences.
#[test]
fn pause_signer_count_invariant_holds_across_edits() {
    let (e, client, super_admin) = setup();

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let s3 = Address::generate(&e);
    let all = [s1.clone(), s2.clone(), s3.clone()];

    client.set_pause_signer(&super_admin, &s1, &true);
    assert_pause_signer_invariant(&e, &client, &all, 1);

    client.set_pause_signer(&super_admin, &s1, &true); // duplicate enable
    assert_pause_signer_invariant(&e, &client, &all, 1);

    client.set_pause_signer(&super_admin, &s2, &true);
    assert_pause_signer_invariant(&e, &client, &all, 2);

    client.set_pause_signer(&super_admin, &s2, &false);
    assert_pause_signer_invariant(&e, &client, &all, 1);

    client.set_pause_signer(&super_admin, &s2, &false); // duplicate disable
    assert_pause_signer_invariant(&e, &client, &all, 1);

    client.set_pause_signer(&super_admin, &s3, &false); // never enabled
    assert_pause_signer_invariant(&e, &client, &all, 1);

    client.set_pause_signer(&super_admin, &s1, &false);
    client.set_pause_signer(&super_admin, &s3, &true);
    assert_pause_signer_invariant(&e, &client, &all, 1);
}

// ---------------------------------------------------------------------------
// set_pause_threshold — dedicated failure-boundary coverage
// ---------------------------------------------------------------------------

/// Only a SuperAdmin may call `set_pause_threshold`. Lesser roles and
/// non-admins are rejected with `NotAdmin`; the threshold and epoch are
/// unchanged after each rejected attempt.
#[test]
fn set_pause_threshold_requires_super_admin() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);

    let operator = Address::generate(&e);
    client.add_admin(&super_admin, &operator, &AdminRole::Operator);
    let mid = Address::generate(&e);
    client.add_admin(&super_admin, &mid, &AdminRole::Admin);
    let stranger = Address::generate(&e);

    let epoch_before = client.get_config_epoch();

    for caller in [&operator, &mid, &stranger] {
        let err = client
            .try_set_pause_threshold(caller, &1u32)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));
    }

    // Threshold remains at its initial value (0); epoch is untouched.
    assert_eq!(client.get_config_epoch(), epoch_before);
}

/// When there are no registered signers any threshold > 0 is rejected as
/// `ThresholdExceedsSigners`. Threshold 0 is always valid regardless of signer
/// count and does not advance the epoch when it is already 0.
#[test]
fn set_pause_threshold_zero_signers() {
    let (e, client, super_admin) = setup();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    // Any positive threshold is invalid with 0 signers.
    let err = client
        .try_set_pause_threshold(&super_admin, &1u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_THRESHOLD_EXCEEDS_SIGNERS)
    );
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // Threshold 0 when it is already 0 is an idempotent no-op.
    client.set_pause_threshold(&super_admin, &0u32);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// `set_pause_threshold` emits `pause_threshold_set` exactly once per real
/// change and never on a no-op re-set, matching the epoch-bump contract.
#[test]
fn set_pause_threshold_emits_event_on_change_only() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);

    let events_before = e.events().all().len();
    let epoch_before = client.get_config_epoch();

    // Real change: 0 → 1. One event, one epoch bump.
    client.set_pause_threshold(&super_admin, &1u32);
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    assert_eq!(e.events().all().len(), events_before + 1);

    // No-op re-set: 1 → 1. No event, no epoch bump.
    let epoch_after = client.get_config_epoch();
    let events_after = e.events().all().len();
    client.set_pause_threshold(&super_admin, &1u32);
    assert_eq!(client.get_config_epoch(), epoch_after);
    assert_eq!(e.events().all().len(), events_after);
}
