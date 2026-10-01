//! Boundary and recovery test coverage for `contracts/admin/src/pausable.rs`.
//!
//! This module fills the gaps left by `test_pausable.rs`,
//! `test_pause_failure_boundaries.rs`, and `test_concurrency_race_safety.rs`
//! by exercising:
//!
//! * **Full state-machine recovery** — pause → unpause → pause cycles returning
//!   the contract to each state without residual effects.
//! * **Redundant direct-path operations** — `do_pause` / `do_unpause` when the
//!   contract is already in the target state: idempotent, no event, no epoch.
//! * **Threshold = 0 recovery** — zeroing the threshold while signers remain
//!   re-activates the direct SuperAdmin path.
//! * **All-signers-removed boundary** — removing every signer clamps threshold
//!   to 0, re-enabling direct SuperAdmin control.
//! * **Signer-as-contract-address rejection** — `set_pause_signer` must refuse
//!   the contract's own address with a stable `InvalidAdminAddress` error.
//! * **Stale-epoch unpause proposal** — epoch staleness is symmetric: an unpause
//!   proposal is equally stale once the ledger crosses the epoch boundary.
//! * **Removed signer cannot approve** — a signer de-registered before calling
//!   `approve_pause_proposal` is rejected with `NotSigner`.
//! * **Existing approvals survive signer removal** — approvals already recorded
//!   before a signer is removed are not deleted; the proposal can still execute
//!   if the clamped threshold is met.
//! * **Epoch boundary: proposal at ledger sequence `PROPOSAL_EPOCH_SIZE - 1`**
//!   — a proposal created at the last sequence of an epoch expires the moment
//!   the ledger rolls into the next epoch.
//! * **Direct unpause while already unpaused** — no event, no epoch bump.
//! * **Threshold-zero with no signers** — a freshly initialised contract (no
//!   signers, no threshold) accepts a direct SuperAdmin pause/unpause.
//! * **`set_pause_signer` removes last signer → threshold clamps to 0 and
//!   contract still accepts direct pause** — regression for the clamping path.
//! * **Full multisig recovery round-trip** — propose pause, execute, propose
//!   unpause, execute; contract returns to clean unpaused state.
//!
//! ## Invariants asserted
//!
//! * A rejected or no-op call never advances `get_config_epoch`.
//! * A rejected or no-op call never emits an event.
//! * Successful state transitions advance the epoch exactly once.
//! * `PauseSignerCount` always equals the count of enabled `PauseSigner` keys.
//! * `PauseThreshold` never exceeds `PauseSignerCount`.

#![cfg(test)]

use crate::pausable::PROPOSAL_EPOCH_SIZE;
use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

// Wire-stable error discriminants (see `credence_errors::ContractError`).
const ERR_NOT_ADMIN: u32 = 100;
const ERR_NOT_SIGNER: u32 = 104;
const ERR_CONTRACT_PAUSED: u32 = 106;
const ERR_INVALID_ADMIN_ADDRESS: u32 = 110;
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

/// Assert `PauseSignerCount` == `expected` and equals the number of
/// `PauseSigner` entries that are actually `true` in instance storage.
fn assert_signer_count_invariant(
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
            "PauseSignerCount must equal the number of enabled PauseSigner entries"
        );
        assert_eq!(
            stored, expected,
            "expected {expected} enabled signers, got {stored}"
        );
    });
}

/// Assert `PauseThreshold` never exceeds `PauseSignerCount` in storage.
fn assert_threshold_never_exceeds_count(e: &Env, client: &AdminContractClient) {
    e.as_contract(&client.address, || {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        let threshold: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0);
        assert!(
            threshold <= count,
            "PauseThreshold ({threshold}) must never exceed PauseSignerCount ({count})"
        );
    });
}

// ---------------------------------------------------------------------------
// 1. Direct-path redundancy: already-paused / already-unpaused
// ---------------------------------------------------------------------------

/// Calling `pause` when the contract is already paused (direct mode) is a
/// no-op: no `paused` event is emitted and the epoch does not advance.
#[test]
fn direct_pause_while_already_paused_is_no_op() {
    let (_, client, super_admin) = setup();

    client.pause(&super_admin);
    assert!(client.is_paused());
    let epoch = client.get_config_epoch();
    let events = client.env().events().all().len();

    // Second pause: contract is already paused — idempotent no-op.
    let result = client.pause(&super_admin);
    assert_eq!(result, None, "direct-path pause always returns None");
    assert!(client.is_paused());
    assert_eq!(
        client.get_config_epoch(),
        epoch,
        "epoch must not advance on a no-op pause"
    );
    assert_eq!(
        client.env().events().all().len(),
        events,
        "no event must be emitted on a no-op pause"
    );
}

/// Calling `unpause` when the contract is already unpaused (direct mode) is a
/// no-op: no `unpaused` event is emitted and the epoch does not advance.
#[test]
fn direct_unpause_while_already_unpaused_is_no_op() {
    let (_, client, super_admin) = setup();

    assert!(!client.is_paused());
    let epoch = client.get_config_epoch();
    let events = client.env().events().all().len();

    let result = client.unpause(&super_admin);
    assert_eq!(result, None, "direct-path unpause always returns None");
    assert!(!client.is_paused());
    assert_eq!(
        client.get_config_epoch(),
        epoch,
        "epoch must not advance on a no-op unpause"
    );
    assert_eq!(
        client.env().events().all().len(),
        events,
        "no event must be emitted on a no-op unpause"
    );
}

// ---------------------------------------------------------------------------
// 2. Full state-machine recovery round-trips
// ---------------------------------------------------------------------------

/// Direct-path round-trip: pause → unpause → pause → unpause.
/// Each real transition advances the epoch exactly once and emits exactly one
/// event. The contract is fully operational after every unpause.
#[test]
fn direct_path_full_recovery_round_trip() {
    let (e, client, super_admin) = setup();

    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), 0);

    // Transition 1: unpaused → paused.
    client.pause(&super_admin);
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), 1);

    // While paused, write operations must be blocked.
    let new_admin = Address::generate(&e);
    assert!(client
        .try_add_admin(&super_admin, &new_admin, &AdminRole::Admin)
        .is_err());
    assert_eq!(
        client.get_config_epoch(),
        1,
        "rejected write must not bump epoch"
    );

    // Transition 2: paused → unpaused (recovery).
    client.unpause(&super_admin);
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), 2);

    // Write operations are unblocked after recovery.
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);

    // Transition 3: unpaused → paused again.
    client.pause(&super_admin);
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), 4); // add_admin bumped by 1

    // Transition 4: paused → unpaused (second recovery).
    client.unpause(&super_admin);
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), 5);

    // Full operations are available once more.
    let another = Address::generate(&e);
    client.add_admin(&super_admin, &another, &AdminRole::Operator);
    assert_eq!(client.get_admin_count(), 3);
}

/// Multisig round-trip: propose pause → approve → execute → propose unpause →
/// approve → execute. Contract returns to clean unpaused state with no orphaned
/// proposals.
#[test]
fn multisig_full_recovery_round_trip() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // --- Phase 1: pause ---
    let pause_id = client.pause(&s1).unwrap();
    assert!(!client.is_paused());
    client.approve_pause_proposal(&s2, &pause_id);
    client.execute_pause_proposal(&pause_id);
    assert!(client.is_paused());

    // Proposal must be consumed — cannot re-execute.
    let err = client
        .try_execute_pause_proposal(&pause_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_PROPOSAL_NOT_FOUND)
    );

    // --- Phase 2: unpause (recovery) ---
    let unpause_id = client.unpause(&s1).unwrap();
    assert_ne!(unpause_id, pause_id, "pause and unpause have distinct IDs");
    assert!(client.is_paused(), "still paused until executed");
    client.approve_pause_proposal(&s2, &unpause_id);
    client.execute_pause_proposal(&unpause_id);
    assert!(!client.is_paused());

    // Unpause proposal must also be consumed.
    let err = client
        .try_execute_pause_proposal(&unpause_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_PROPOSAL_NOT_FOUND)
    );

    // Contract is fully operational.
    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);
}

// ---------------------------------------------------------------------------
// 3. Threshold-zero recovery paths
// ---------------------------------------------------------------------------

/// Setting threshold back to 0 while signers are still registered re-enables
/// the direct SuperAdmin pause path. A SuperAdmin (not a signer) can then
/// pause/unpause immediately without a proposal.
#[test]
fn threshold_zero_recovery_re_enables_direct_path() {
    let (e, client, super_admin) = setup();
    let _signers = configure_signers(&e, &client, &super_admin, 2, 2);

    // While threshold == 2 a SuperAdmin cannot pause directly.
    let err = client.try_pause(&super_admin).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_SIGNER));
    assert!(!client.is_paused());

    // Lower threshold back to 0 → direct path re-enabled.
    client.set_pause_threshold(&super_admin, &0u32);

    let result = client.pause(&super_admin);
    assert_eq!(result, None, "direct path returns None");
    assert!(client.is_paused());

    // Recovery: direct unpause.
    let result = client.unpause(&super_admin);
    assert_eq!(result, None);
    assert!(!client.is_paused());

    // Signers are still registered; count invariant must hold.
    let s_vec = configure_signers(&e, &client, &super_admin, 0, 0); // just use empty
    e.as_contract(&client.address, || {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        // 2 signers were registered earlier and never removed.
        assert_eq!(count, 2);
        let threshold: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0);
        assert_eq!(threshold, 0);
    });
    let _ = s_vec;
}

/// Removing all registered signers one by one clamps the threshold at each
/// step and eventually sets it to 0, re-enabling the direct SuperAdmin path.
#[test]
fn removing_all_signers_clamps_threshold_to_zero_and_re_enables_direct_path() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 3, 3);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();
    let s3 = signers.get(2).unwrap();
    let all = [s1.clone(), s2.clone(), s3.clone()];

    // Remove s1: count → 2, threshold clamps from 3 → 2.
    client.set_pause_signer(&super_admin, &s1, &false);
    assert_signer_count_invariant(&e, &client, &all, 2);
    assert_threshold_never_exceeds_count(&e, &client);

    // Remove s2: count → 1, threshold clamps from 2 → 1.
    client.set_pause_signer(&super_admin, &s2, &false);
    assert_signer_count_invariant(&e, &client, &all, 1);
    assert_threshold_never_exceeds_count(&e, &client);

    // Remove s3 (last): count → 0, threshold clamps from 1 → 0.
    client.set_pause_signer(&super_admin, &s3, &false);
    assert_signer_count_invariant(&e, &client, &all, 0);
    assert_threshold_never_exceeds_count(&e, &client);

    // With threshold == 0 the direct SuperAdmin path is active.
    let result = client.pause(&super_admin);
    assert_eq!(
        result, None,
        "direct path returns None after all signers removed"
    );
    assert!(client.is_paused());

    // Recovery.
    client.unpause(&super_admin);
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// 4. Invalid-signer-address rejection (stable error code)
// ---------------------------------------------------------------------------

/// `set_pause_signer` must reject the contract's own address as a signer with
/// a stable `InvalidAdminAddress` (110) error, leave the signer count
/// unchanged, and not advance the epoch.
#[test]
fn set_pause_signer_rejects_contract_self_address() {
    let (e, client, super_admin) = setup();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_set_pause_signer(&super_admin, &client.address, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_ADMIN_ADDRESS),
        "contract self-address must be rejected with InvalidAdminAddress"
    );

    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // Signer count must be unchanged (0 valid signers).
    e.as_contract(&client.address, || {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        assert_eq!(count, 0);
    });
}

/// `set_pause_signer` must reject the all-zero Ed25519 address sentinel with
/// `InvalidAdminAddress` (110) and not advance the epoch.
#[test]
fn set_pause_signer_rejects_zero_address_with_stable_error() {
    let (e, client, super_admin) = setup();

    let zero = Address::from_string(&soroban_sdk::String::from_str(
        &e,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_set_pause_signer(&super_admin, &zero, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_ADMIN_ADDRESS)
    );

    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

// ---------------------------------------------------------------------------
// 5. Stale-epoch boundary for unpause proposals
// ---------------------------------------------------------------------------

/// Epoch staleness applies symmetrically to unpause proposals: a proposal
/// created in epoch N cannot be approved or executed once the ledger enters
/// epoch N+1, even if it has enough approvals.
#[test]
fn stale_epoch_unpause_proposal_cannot_execute() {
    let (e, client, super_admin) = setup();

    // Pause the contract first so there is something to unpause.
    client.pause(&super_admin);
    assert!(client.is_paused());

    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    // Create the unpause proposal at the last sequence of epoch 0.
    let epoch_boundary = u32::from(PROPOSAL_EPOCH_SIZE);
    e.ledger()
        .with_mut(|l| l.sequence_number = epoch_boundary - 1);
    let unpause_id = client.unpause(&s1).unwrap();

    let epoch_after_propose = client.get_config_epoch();

    // Advance into epoch 1 — the proposal is now stale.
    e.ledger().with_mut(|l| l.sequence_number = epoch_boundary);
    let events_before = e.events().all().len();

    let err = client
        .try_execute_pause_proposal(&unpause_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_STALE_ADMIN_EPOCH),
        "stale unpause proposal must be rejected with StaleAdminEpoch"
    );

    // Contract remains paused; no state changed.
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_after_propose);
    assert_eq!(e.events().all().len(), events_before);

    // Approval is also stale.
    let err = client
        .try_approve_pause_proposal(&s1, &unpause_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_STALE_ADMIN_EPOCH)
    );
    assert_eq!(e.events().all().len(), events_before);

    // Recovery: in the new epoch we can create a fresh unpause proposal.
    let new_unpause_id = client.unpause(&s1).unwrap();
    assert_ne!(
        new_unpause_id, unpause_id,
        "new epoch yields a new proposal id"
    );
    client.execute_pause_proposal(&new_unpause_id);
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// 6. Removed signer cannot approve; existing approvals survive
// ---------------------------------------------------------------------------

/// A signer that is de-registered before calling `approve_pause_proposal`
/// must be rejected with `NotSigner`. The rejection must not advance the epoch
/// or emit an event, and the proposal must remain live for remaining signers.
#[test]
fn removed_signer_cannot_approve_after_deregistration() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap();

    // De-register s2 before they approve.
    client.set_pause_signer(&super_admin, &s2, &false);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_approve_pause_proposal(&s2, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_NOT_SIGNER),
        "de-registered signer must be rejected with NotSigner"
    );

    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
    assert!(!client.is_paused());
}

/// Approvals recorded *before* a signer is removed are preserved. Once the
/// threshold is clamped down to the remaining signer count the existing
/// approvals can satisfy it and the proposal executes normally.
#[test]
fn existing_approvals_survive_signer_removal_and_can_satisfy_clamped_threshold() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Both signers approve the proposal while both are still registered.
    let id = client.pause(&s1).unwrap(); // s1 proposes + approves
    client.approve_pause_proposal(&s2, &id); // s2 approves

    // Now remove s2. Threshold clamps from 2 → 1.
    client.set_pause_signer(&super_admin, &s2, &false);

    // The 2 approvals (s1 + s2) that were recorded are still in storage.
    // With threshold now 1, the proposal meets the threshold and can execute.
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

/// A proposal with only 1 approval (s1) can still execute after threshold
/// clamps to 1 due to s2 removal — recovery for the "signers shrink" scenario.
#[test]
fn proposal_with_one_approval_executes_after_threshold_clamped_to_one() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap(); // s1 proposes + approves; threshold still 2

    // Before s2 approves, executing fails.
    assert!(client.try_execute_pause_proposal(&id).is_err());
    assert!(!client.is_paused());

    // Remove s2 → threshold clamps to 1. s1's single approval now sufficient.
    client.set_pause_signer(&super_admin, &s2, &false);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// 7. Epoch boundary: proposal created at last sequence of an epoch
// ---------------------------------------------------------------------------

/// A proposal created at sequence `PROPOSAL_EPOCH_SIZE - 1` (last slot of
/// epoch 0) becomes stale the instant the ledger advances to sequence
/// `PROPOSAL_EPOCH_SIZE` (first slot of epoch 1). Neither approval nor
/// execution is permitted in the new epoch.
#[test]
fn proposal_at_last_sequence_of_epoch_is_stale_in_next_epoch() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    // Last sequence of epoch 0.
    let last_seq_epoch0 = PROPOSAL_EPOCH_SIZE - 1;
    e.ledger().with_mut(|l| l.sequence_number = last_seq_epoch0);

    let id = client.pause(&s1).unwrap();
    assert_eq!(e.ledger().sequence(), last_seq_epoch0, "still in epoch 0");

    let epoch_after_propose = client.get_config_epoch();

    // Advance to first sequence of epoch 1 — the minimal staleness boundary.
    e.ledger()
        .with_mut(|l| l.sequence_number = PROPOSAL_EPOCH_SIZE);

    let err = client.try_execute_pause_proposal(&id).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_STALE_ADMIN_EPOCH)
    );
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_after_propose);

    let err = client
        .try_approve_pause_proposal(&s1, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_STALE_ADMIN_EPOCH)
    );
}

/// A proposal created in epoch 0 at sequence 0 remains valid throughout epoch
/// 0 (up to `PROPOSAL_EPOCH_SIZE - 1`). Specifically it must succeed at the
/// sequence immediately before the boundary.
#[test]
fn proposal_valid_throughout_its_epoch() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    // Propose at the beginning of epoch 0.
    e.ledger().with_mut(|l| l.sequence_number = 0);
    let id = client.pause(&s1).unwrap();

    // Execute at sequence PROPOSAL_EPOCH_SIZE - 1: still in epoch 0, must succeed.
    e.ledger()
        .with_mut(|l| l.sequence_number = PROPOSAL_EPOCH_SIZE - 1);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// 8. `require_not_paused` guards are consistently enforced
// ---------------------------------------------------------------------------

/// Every entrypoint guarded by `require_not_paused` must return
/// `ContractPaused` (106) when the contract is paused, and succeed once
/// unpaused. Tests cover `add_admin`, `remove_admin`, `update_admin_role`,
/// `deactivate_admin`, and `reactivate_admin`.
#[test]
fn all_require_not_paused_guards_enforce_consistently() {
    let (e, client, super_admin) = setup();
    let target = Address::generate(&e);
    client.add_admin(&super_admin, &target, &AdminRole::Admin);

    client.pause(&super_admin);
    assert!(client.is_paused());
    let epoch_paused = client.get_config_epoch();

    // add_admin is gated.
    let extra = Address::generate(&e);
    let err = client
        .try_add_admin(&super_admin, &extra, &AdminRole::Operator)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_CONTRACT_PAUSED)
    );
    assert_eq!(client.get_config_epoch(), epoch_paused);

    // remove_admin is gated.
    let err = client
        .try_remove_admin(&super_admin, &target)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_CONTRACT_PAUSED)
    );
    assert_eq!(client.get_config_epoch(), epoch_paused);

    // update_admin_role is gated.
    let err = client
        .try_update_admin_role(&super_admin, &target, &AdminRole::Operator)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_CONTRACT_PAUSED)
    );
    assert_eq!(client.get_config_epoch(), epoch_paused);

    // deactivate_admin is gated.
    let err = client
        .try_deactivate_admin(&super_admin, &target)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_CONTRACT_PAUSED)
    );
    assert_eq!(client.get_config_epoch(), epoch_paused);

    // Recovery: unpause and verify all entrypoints are unblocked.
    client.unpause(&super_admin);
    assert!(!client.is_paused());

    client.update_admin_role(&super_admin, &target, &AdminRole::Operator);
    client.deactivate_admin(&super_admin, &target);
    client.reactivate_admin(&super_admin, &target);
    client.remove_admin(&super_admin, &target);
    assert_eq!(client.get_admin_count(), 1);
}

// ---------------------------------------------------------------------------
// 9. Freshly initialised contract (no signers, threshold == 0)
// ---------------------------------------------------------------------------

/// A freshly initialised contract with no signers and threshold == 0 accepts
/// a direct SuperAdmin pause and unpause, returning `None` in both cases.
#[test]
fn fresh_contract_no_signers_accepts_direct_pause_unpause() {
    let (_, client, super_admin) = setup();

    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), 0);

    let r = client.pause(&super_admin);
    assert_eq!(r, None);
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), 1);

    let r = client.unpause(&super_admin);
    assert_eq!(r, None);
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), 2);
}

// ---------------------------------------------------------------------------
// 10. Signer count invariant across mixed add/remove/re-add sequences
// ---------------------------------------------------------------------------

/// `PauseSignerCount` and `PauseThreshold ≤ PauseSignerCount` must hold after
/// every operation in a sequence of adds, removes, idempotent no-ops, and
/// re-adds.
#[test]
fn signer_count_and_threshold_invariant_across_mixed_operations() {
    let (e, client, super_admin) = setup();

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let s3 = Address::generate(&e);
    let all = [s1.clone(), s2.clone(), s3.clone()];

    // Add s1 and s2, set threshold to 2.
    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin, &s2, &true);
    client.set_pause_threshold(&super_admin, &2u32);
    assert_signer_count_invariant(&e, &client, &all, 2);
    assert_threshold_never_exceeds_count(&e, &client);

    // Remove s2: threshold clamps from 2 → 1.
    client.set_pause_signer(&super_admin, &s2, &false);
    assert_signer_count_invariant(&e, &client, &all, 1);
    assert_threshold_never_exceeds_count(&e, &client);

    // Re-add s2: count → 2 again, threshold stays at 1.
    client.set_pause_signer(&super_admin, &s2, &true);
    assert_signer_count_invariant(&e, &client, &all, 2);
    assert_threshold_never_exceeds_count(&e, &client);

    // Add s3.
    client.set_pause_signer(&super_admin, &s3, &true);
    assert_signer_count_invariant(&e, &client, &all, 3);
    assert_threshold_never_exceeds_count(&e, &client);

    // Set threshold to 3 (== count).
    client.set_pause_threshold(&super_admin, &3u32);
    assert_threshold_never_exceeds_count(&e, &client);

    // Remove s1 and s2: threshold clamps to 1.
    client.set_pause_signer(&super_admin, &s1, &false);
    assert_signer_count_invariant(&e, &client, &all, 2);
    assert_threshold_never_exceeds_count(&e, &client);

    client.set_pause_signer(&super_admin, &s2, &false);
    assert_signer_count_invariant(&e, &client, &all, 1);
    assert_threshold_never_exceeds_count(&e, &client);

    // Only s3 remains; s3 can still pause with threshold == 1.
    let id = client.pause(&s3).unwrap();
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// 11. Recovery via threshold reduction (below-threshold → threshold lowered →
//     existing approvals now satisfy the new threshold)
// ---------------------------------------------------------------------------

/// If a proposal was created with a high threshold that has not yet been met,
/// lowering the threshold (by removing signers and letting it clamp) to a
/// value the existing approvals already satisfy allows the proposal to execute
/// — deterministic recovery without requiring additional approvals.
#[test]
fn lowering_threshold_allows_existing_approvals_to_satisfy_it() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 3, 3);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();
    let s3 = signers.get(2).unwrap();

    // s1 and s2 approve; s3 has not approved yet.
    let id = client.pause(&s1).unwrap(); // proposes + approves
    client.approve_pause_proposal(&s2, &id);

    // 2 approvals < threshold 3 → cannot execute yet.
    assert!(client.try_execute_pause_proposal(&id).is_err());
    assert!(!client.is_paused());

    // Remove s3 → count becomes 2, threshold clamps from 3 → 2.
    // Now 2 approvals == threshold 2 → proposal is executable.
    client.set_pause_signer(&super_admin, &s3, &false);

    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// 12. Non-SuperAdmin cannot use direct-path pause even when threshold is 0
// ---------------------------------------------------------------------------

/// Operator and Admin roles cannot call `pause` in direct mode (threshold 0).
/// A rejected call leaves no state, emits no event, and does not advance the
/// epoch. The correct role (SuperAdmin) succeeds immediately after.
#[test]
fn non_super_admin_cannot_use_direct_pause_path() {
    let (e, client, super_admin) = setup();
    // threshold is 0 by default

    let operator = Address::generate(&e);
    client.add_admin(&super_admin, &operator, &AdminRole::Operator);

    let epoch_after_add = client.get_config_epoch();
    let events_after_add = e.events().all().len();

    let err = client.try_pause(&operator).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_after_add);
    assert_eq!(e.events().all().len(), events_after_add);

    // SuperAdmin succeeds without any configuration change.
    client.pause(&super_admin);
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_after_add + 1);
}

// ---------------------------------------------------------------------------
// 13. Concurrent pause and unpause proposals in the same epoch are distinct
//     and independently executable — cross-epoch pair
// ---------------------------------------------------------------------------

/// Even in the same ledger epoch a pause and an unpause proposal can coexist
/// and execute independently. After executing the pause proposal the contract
/// is paused; after executing the unpause proposal it is unpaused again — full
/// round-trip in a single epoch.
#[test]
fn pause_and_unpause_proposals_coexist_and_execute_independently_in_same_epoch() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    let pause_id = client.pause(&s1).unwrap();
    let unpause_id = client.unpause(&s1).unwrap();

    assert_ne!(
        pause_id, unpause_id,
        "pause and unpause must derive distinct IDs even in the same epoch"
    );

    // Execute pause first.
    client.execute_pause_proposal(&pause_id);
    assert!(client.is_paused());

    // Execute unpause: recovery.
    client.execute_pause_proposal(&unpause_id);
    assert!(!client.is_paused());

    // Both proposals are consumed.
    assert!(client.try_execute_pause_proposal(&pause_id).is_err());
    assert!(client.try_execute_pause_proposal(&unpause_id).is_err());
}

// ---------------------------------------------------------------------------
// 14. Re-propose after stale epoch creates a new valid proposal
// ---------------------------------------------------------------------------

/// After a proposal goes stale (ledger crosses epoch boundary) a signer can
/// create a new proposal in the new epoch. The new proposal has a different ID
/// and can be approved and executed normally.
#[test]
fn re_propose_after_stale_epoch_creates_fresh_valid_proposal() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 1, 1);
    let s1 = signers.get(0).unwrap();

    // Create proposal in epoch 0.
    e.ledger().with_mut(|l| l.sequence_number = 0);
    let old_id = client.pause(&s1).unwrap();

    // Advance to epoch 1 — old proposal is stale.
    e.ledger()
        .with_mut(|l| l.sequence_number = PROPOSAL_EPOCH_SIZE);

    // Old proposal can no longer be executed.
    assert!(client.try_execute_pause_proposal(&old_id).is_err());

    // Re-propose in epoch 1.
    let new_id = client.pause(&s1).unwrap();
    assert_ne!(
        new_id, old_id,
        "new epoch must yield a different proposal ID"
    );

    // New proposal executes normally.
    client.execute_pause_proposal(&new_id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// 15. `approve_pause_proposal` by non-signer is rejected before state changes
// ---------------------------------------------------------------------------

/// A caller who is not a registered pause signer must be rejected with
/// `NotSigner` from `approve_pause_proposal`. No partial approval is recorded.
#[test]
fn non_signer_approve_is_rejected_with_no_state_change() {
    let (e, client, super_admin) = setup();
    let signers = configure_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers.get(0).unwrap();

    let id = client.pause(&s1).unwrap();

    let outsider = Address::generate(&e);
    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_approve_pause_proposal(&outsider, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_SIGNER));

    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
    assert!(!client.is_paused());

    // Approval count must remain 1 (only s1's implicit approval).
    e.as_contract(&client.address, || {
        let approvals: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseApprovalCount(id))
            .unwrap_or(0);
        assert_eq!(approvals, 1, "outsider approval must not have been counted");
    });
}
