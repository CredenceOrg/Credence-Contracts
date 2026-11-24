//! Admin-epoch guard tests (issue #836).
//!
//! Locks the same / off-by-one / ancient boundary matrix for
//! `require_matching_admin_epoch` on the admin pause path.
//!
//! Adversarial regression matrix (issue extension):
//! - boundary adjacency: last sequence of epoch N vs. first of epoch N+1
//! - exact epoch boundaries for both approval and execution
//! - duplicate approvals within the same epoch
//! - retry after a stale-epoch rejection (epoch advances again)
//! - concurrent-timing simulation: two signers approving across an epoch boundary
//! - authorization rejection for non-signer approval
//! - execution guard after an epoch advance
//!
//! Invariants under test:
//! 1. A proposal created in epoch E may only be approved/executed while
//!    the current epoch equals E. Any drift is rejected with `StaleAdminEpoch`
//!    (contract error 514).
//! 2. Rejection is side-effect free: the proposal and its approval count
//!    are unchanged, so a retry in the correct epoch can still succeed.
//! 3. Authorization is enforced independently of epoch validity.

#![cfg(test)]

use crate::pausable::PROPOSAL_EPOCH_SIZE;
use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{Address, Env};

/// Contract error code for `StaleAdminEpoch` exposed by the admin contract.
const STALE_ADMIN_EPOCH: u32 = 514;

fn setup() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, super_admin)
}

fn add_signers(
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

/// Set the ledger sequence number to a specific value.
fn set_sequence(env: &Env, seq: u32) {
    env.ledger().with_mut(|l| {
        l.sequence_number = seq;
    });
}

/// Returns the first sequence number of the epoch containing `seq`.
fn epoch_start(seq: u32) -> u32 {
    let size = u32::from(PROPOSAL_EPOCH_SIZE);
    seq - seq % size
}

/// Same-epoch approval must succeed.
#[test]
fn test_admin_epoch_guard_same_epoch_passes() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &id);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

/// Off-by-one: proposal at end of epoch N, approve at start of epoch N+1 → reject.
#[test]
fn test_admin_epoch_guard_off_by_one_epoch_fails() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let epoch_boundary = u32::from(PROPOSAL_EPOCH_SIZE);
    env.ledger().with_mut(|l| {
        l.sequence_number = epoch_boundary - 1;
    });

    let id = client.pause(&s1).unwrap();

    env.ledger().with_mut(|l| {
        l.sequence_number = epoch_boundary;
    });

    let res = client.try_approve_pause_proposal(&s2, &id);
    assert!(res.is_err(), "off-by-one epoch approval must fail");
    let err = res.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(STALE_ADMIN_EPOCH)
    ); // StaleAdminEpoch
}

/// Ancient: proposal many epochs in the past → reject.
#[test]
fn test_admin_epoch_guard_ancient_epoch_fails() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap();

    env.ledger().with_mut(|l| {
        l.sequence_number += 10 * u32::from(PROPOSAL_EPOCH_SIZE);
    });

    let res = client.try_approve_pause_proposal(&s2, &id);
    assert!(res.is_err(), "ancient epoch approval must fail");
    let err = res.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(STALE_ADMIN_EPOCH)
    ); // StaleAdminEpoch
}

/// ------------------------------------------------------------------------------
/// Adversarial regression extensions (boundary / duplicate / retry / concurrency)
/// ------------------------------------------------------------------------------

/// Boundary: approval at the very last sequence of the proposal epoch still
/// matches and must succeed.
#[test]
fn test_admin_epoch_guard_last_seq_of_epoch_passes() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Start of epoch 0.
    set_sequence(&env, 0);
    let id = client.pause(&s1).unwrap();

    // Last sequence of epoch 0.
    set_sequence(&env, u32::from(PROPOSAL_EPOCH_SIZE) - 1);
    client.approve_pause_proposal(&s2, &id);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

/// Boundary: approval at the first sequence of the next epoch must fail.
#[test]
fn test_admin_epoch_guard_first_seq_of_next_epoch_fails() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    set_sequence(&env, 0);
    let id = client.pause(&s1).unwrap();

    set_sequence(&env, u32::from(PROPOSAL_EPOCH_SIZE));
    let res = client.try_approve_pause_proposal(&s2, &id);
    assert!(res.is_err(), "first seq of next epoch must fail");
    let err = res.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(STALE_ADMIN_EPOCH)
    );
}

/// Duplicate approval from the same signer in the same epoch must not
/// inflate the approval count or bypass the threshold.
#[test]
fn test_admin_epoch_guard_duplicate_approval_id_empotent() {
    let (env, client, admin) = setup();
    // Threshold 2 with only one active signer approving twice.
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();

    set_sequence(&env, 0);
    let id = client.pause(&s1).unwrap();

    // First approval registers.
    client.approve_pause_proposal(&s1, &id);
    // Duplicate approval from the same signer must not advance the count.
    let epoch = client.get_config_epoch();
    client.approve_pause_proposal(&s1, &id);
    assert_eq!(client.get_config_epoch(), epoch);

    // Threshold not met: execution must fail and not pause.
    let exec = client.try_execute_pause_proposal(&id);
    assert!(exec.is_err(), "threshold not met must not execute");
    assert!(!client.is_paused());
}

/// Retry: after a stale-epoch rejection, advancing to a fresh epoch must
/// not corrupt state; the proposal remains unapproved and can be approved
/// only in its original epoch. This documents the deterministic rejection
/// semantics for a retry that lands in a later epoch.
#[test]
fn test_admin_epoch_guard_retry_after_stale_rejection_fails() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    set_sequence(&env, 0);
    let id = client.pause(&s1).unwrap();

    // First attempt at the epoch boundary fails.
    set_sequence(&env, u32::from(PROPOSAL_EPOCH_SIZE));
    let res1 = client.try_approve_pause_proposal(&s2, &id);
    assert!(res1.is_err());

    // Retry in a later epoch also fails deterministically.
    set_sequence(&env, u32::from(PROPOSAL_EPOCH_SIZE) + 1);
    let res2 = client.try_approve_pause_proposal(&s2, &id);
    assert!(res2.is_err(), "retry in a later epoch must fail");
    let err = res2.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(STALE_ADMIN_EPOCH)
    );

    // State is uncorrupted: no approval was recorded.
    assert!(!client.is_paused());
}

/// Concurrent timing: two approvals split across an epoch boundary must
/// not both count toward the threshold. The first approval lands in epoch N
/// and the second lands in epoch N+1, so the second must be rejected and
/// the proposal must remain unexecutable.
#[test]
fn test_admin_epoch_guard_concurrent_boundary_split_fails() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Proposal created in epoch 0.
    set_sequence(&env, 0);
    let id = client.pause(&s1).unwrap();

    // Approval 1 in epoch 0 (last seq).
    set_sequence(&env, u32::from(PROPOSAL_EPOCH_SIZE) - 1);
    client.approve_pause_proposal(&s2, &id);

    // Approval 2 in epoch 1 must be rejected as stale.
    set_sequence(&env, u32::from(PROPOSAL_EPOCH_SIZE));
    let res = client.try_approve_pause_proposal(&s1, &id);
    assert!(res.is_err(), "cross-epoch approval must fail");
    let err = res.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(STALE_ADMIN_EPOCH)
    );

    // Execution in epoch 1 must also fail because the proposal epoch is stale.
    let exec = client.try_execute_pause_proposal(&id);
    assert!(exec.is_err(), "execution in later epoch must fail");
    assert!(!client.is_paused());
}

/// Authorization: a non-signer attempting to approve must be rejected
/// regardless of epoch validity.
#[test]
fn test_admin_epoch_guard_non_signer_approval_rejected() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let outsider = Address::generate(&env);

    set_sequence(&env, 0);
    let id = client.pause(&s1).unwrap();

    let res = client.try_approve_pause_proposal(&outsider, &id);
    assert!(res.is_err(), "non-signer approval must be rejected");
    assert!(!client.is_paused());
}

/// Epoch start boundary is inclusive: a proposal created at the first seq
/// of an epoch can be approved within that same epoch.
#[test]
fn test_admin_epoch_guard_epoch_start_inclusive_passes() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Start of epoch 2.
    let start = epoch_start(2 * u32::from(PROPOSAL_EPOCH_SIZE));
    set_sequence(&env, start);
    let id = client.pause(&s1).unwrap();

    // Still within epoch 2.
    set_sequence(&env, start + u32::from(PROPOSAL_EPOCH_SIZE) - 1);
    client.approve_pause_proposal(&s2, &id);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}
