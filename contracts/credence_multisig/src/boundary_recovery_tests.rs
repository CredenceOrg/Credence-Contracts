/// # Boundary and Recovery Test Suite — CredenceMultiSig
///
/// Covers scenarios that the primary test modules (`test_multisig`,
/// `test_access_control`, etc.) leave untested:
///
/// - Boundary: threshold tightly coupled to signer count (threshold == N)
/// - Boundary: never-expiring proposals (expires_at == 0)
/// - Boundary: `prune_expired_proposals` at the hard-cap iteration limit
/// - Permission: permissionless `execute_proposal` path succeeds for any caller
/// - Permission: `transfer_admin` is gated by the *current* admin, not the new one
/// - Duplicate: same op_hash blocked across two distinct Proposal objects
/// - State recovery: failed execute (insufficient sigs) does not mutate Proposal status
/// - State recovery: failed execute (expired) transitions status to Expired exactly once
/// - State recovery: failed execute (already-executed op_hash) does not change
///   the second proposal's status away from Pending
/// - State recovery: signer removal that drops threshold still rejects execute
///   when remaining active signatures < new threshold
/// - Invariant: proposal counter increments monotonically even across failures
extern crate std;

use crate::{ActionType, CredenceMultiSig, CredenceMultiSigClient, ProposalStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, String, Vec,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a contract with `n` signers at `threshold` and return both client
/// and the signer list.  `mock_all_auths` is enabled for the entire test.
fn setup_n(n: usize, threshold: u32) -> (Env, CredenceMultiSigClient<'static>, Address, Vec<Address>) {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let mut signers = Vec::new(&e);
    for _ in 0..n {
        signers.push_back(Address::generate(&e));
    }

    let cid = e.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(&e, &cid);
    client.initialize(&admin, &signers, &threshold);

    (e, client, admin, signers)
}

/// Unique 32-byte op-hash seed: avoids collision between tests sharing an Env.
fn op(seed: u8) -> [u8; 32] {
    let mut arr = [0u8; 32];
    arr[0] = seed;
    arr
}

fn submit(
    e: &Env,
    client: &CredenceMultiSigClient<'_>,
    proposer: &Address,
    expires_at: u64,
    op_seed: u8,
) -> u64 {
    client.submit_proposal(
        proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(e, "boundary test proposal"),
        &expires_at,
        &None,
        &BytesN::from_array(e, &op(op_seed)),
    )
}

// ---------------------------------------------------------------------------
// 1. Boundary — threshold tightly coupled to signer count (threshold == N)
// ---------------------------------------------------------------------------

#[test]
fn test_threshold_equals_signer_count_enforced_at_all_n() {
    // Invariant: when threshold == signer_count, ALL signers must sign.
    // Verify for a 5-signer, threshold-5 setup that exactly 4 signatures
    // are insufficient but 5 succeed.
    let (e, client, _admin, signers) = setup_n(5, 5);

    let pid = submit(&e, &client, &signers.get(0).unwrap(), 0, 0xA1);

    // Sign with only 4 of the 5 required signers.
    for i in 0..4 {
        client.sign_proposal(&signers.get(i).unwrap(), &pid);
    }

    // Execution must fail: effective signatures (4) < threshold (5).
    let err = client.try_execute_proposal(&pid);
    assert!(
        err.is_err(),
        // Invariant: 4 signatures < threshold of 5 must be rejected
        "execute must be rejected when effective_sigs < threshold"
    );

    // Proposal status is untouched — still Pending after a failed execute.
    // Invariant: a failed execute must not corrupt proposal status.
    assert_eq!(client.get_proposal(&pid).status, ProposalStatus::Pending);

    // Sign with the 5th signer.
    client.sign_proposal(&signers.get(4).unwrap(), &pid);

    // Now exactly threshold signatures are present — execution must succeed.
    client.execute_proposal(&pid);
    assert_eq!(client.get_proposal(&pid).status, ProposalStatus::Executed);
}

// ---------------------------------------------------------------------------
// 2. Boundary — never-expiring proposal (expires_at == 0)
// ---------------------------------------------------------------------------

#[test]
fn test_proposal_with_zero_expiry_never_expires() {
    // Invariant: expires_at == 0 disables the expiry check; the proposal
    // must remain executable regardless of how much ledger time advances.
    let (e, client, _admin, signers) = setup_n(2, 2);

    let pid = submit(&e, &client, &signers.get(0).unwrap(), 0 /* no expiry */, 0xA2);
    client.sign_proposal(&signers.get(0).unwrap(), &pid);
    client.sign_proposal(&signers.get(1).unwrap(), &pid);

    // Advance ledger timestamp far into the future.
    e.ledger().with_mut(|l| l.timestamp = u64::MAX / 2);

    // Execution must succeed even with a very large current timestamp.
    client.execute_proposal(&pid);
    assert_eq!(client.get_proposal(&pid).status, ProposalStatus::Executed);
}

// ---------------------------------------------------------------------------
// 3. Boundary — prune hard-cap: max_iter == MAX_ITER_HARD_CAP (200) is accepted
// ---------------------------------------------------------------------------

#[test]
fn test_prune_at_hard_cap_iter_succeeds_without_panic() {
    use crate::multisig::MAX_ITER_HARD_CAP;

    // Invariant: callers MAY pass max_iter == MAX_ITER_HARD_CAP; values above
    // it are silently clamped, not rejected.  Neither case should panic.
    let (e, client, _admin, signers) = setup_n(1, 1);

    e.ledger().with_mut(|l| l.timestamp = 1_000);

    // Submit enough expired proposals to keep the sweep busy.
    let proposer = signers.get(0).unwrap();
    for i in 0..10_u8 {
        submit(&e, &client, &proposer, 500 /* already expired */, i);
    }

    e.ledger().with_mut(|l| l.timestamp = 2_000);

    // Exact hard cap — must not panic.
    let pruned = client.prune_expired_proposals(&0, &MAX_ITER_HARD_CAP);
    // All 10 proposals submitted before `start_id=0` are eligible.
    assert_eq!(pruned, 10, "all 10 expired proposals must be pruned");

    // One above hard cap — must be silently clamped, also not panic.
    let pruned_capped = client.prune_expired_proposals(&0, &(MAX_ITER_HARD_CAP + 1));
    assert_eq!(
        pruned_capped, 0,
        // Invariant: already-pruned proposals are gone; a second pass prunes 0
        "re-pruning an already-empty range must return 0"
    );
}

// ---------------------------------------------------------------------------
// 4. Permission — permissionless execute path (any EOA can execute)
// ---------------------------------------------------------------------------

#[test]
fn test_execute_proposal_is_permissionless_after_threshold_met() {
    // Invariant: execute_proposal requires no auth — any account can trigger
    // it once the threshold is satisfied.
    let (e, client, _admin, signers) = setup_n(2, 2);

    let pid = submit(&e, &client, &signers.get(0).unwrap(), 0, 0xA3);
    client.sign_proposal(&signers.get(0).unwrap(), &pid);
    client.sign_proposal(&signers.get(1).unwrap(), &pid);

    // A completely unrelated address (not a signer, not the admin) executes.
    let outsider = Address::generate(&e);
    let _ = outsider; // The outsider isn't used in the call itself because
                      // execute_proposal takes no caller argument — it is
                      // truly permissionless.
    client.execute_proposal(&pid);
    assert_eq!(client.get_proposal(&pid).status, ProposalStatus::Executed);
}

// ---------------------------------------------------------------------------
// 5. Permission — transfer_admin: only the *current* admin, not the new one
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin_requires_current_admin_auth() {
    // Invariant: transfer_admin reads the stored admin and calls
    // require_auth() on it.  With mock_all_auths disabled, a call where the
    // stored admin has NOT authorised must fail.
    //
    // We re-create the contract *without* mock_all_auths so we can test
    // auth enforcement directly.
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;

    let e = Env::default();
    // Do NOT call mock_all_auths.

    let admin = Address::generate(&e);
    let mut init_signers = Vec::new(&e);
    init_signers.push_back(Address::generate(&e));

    let cid = e.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(&e, &cid);

    // Initialize: mock the admin auth for initialize only.
    e.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &cid,
            fn_name: "initialize",
            args: (&admin, &init_signers, &1_u32).into_val(&e),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &init_signers, &1);

    // Attempt transfer with NO auth mocked — stored admin has not authorised.
    let new_admin = Address::generate(&e);
    let err = client.try_transfer_admin(&new_admin);
    assert!(
        err.is_err(),
        // Invariant: transfer_admin must require auth from the stored admin
        "transfer_admin must fail when the stored admin has not authorised"
    );

    // Stored admin must be unchanged.
    assert_eq!(
        client.get_admin(),
        admin,
        "admin must be unchanged after a failed transfer_admin"
    );

    // Legitimate transfer: mock the stored admin authorising the call.
    e.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &cid,
            fn_name: "transfer_admin",
            args: (&new_admin,).into_val(&e),
            sub_invokes: &[],
        },
    }]);
    client.transfer_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

// ---------------------------------------------------------------------------
// 6. Duplicate — same op_hash across two distinct proposals is blocked
// ---------------------------------------------------------------------------

#[test]
fn test_op_hash_replay_across_different_proposal_objects_is_rejected() {
    // Invariant: DataKey::ExecutedOp is keyed on op_hash, not on proposal_id.
    // Executing a second proposal that carries the same op_hash must panic
    // with ProposalAlreadyExecuted (#604), even if the two proposals are
    // otherwise completely independent.
    let (e, client, admin, signers) = setup_n(2, 2);

    let shared_hash = BytesN::from_array(&e, &op(0xCC));

    // Proposal A: submit, sign, execute.
    let pid_a = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Proposal A"),
        &0_u64,
        &None,
        &shared_hash,
    );
    client.sign_proposal(&signers.get(0).unwrap(), &pid_a);
    client.sign_proposal(&signers.get(1).unwrap(), &pid_a);
    client.execute_proposal(&pid_a);

    // Proposal B: identical op_hash, different description.
    let pid_b = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "Proposal B — same op_hash"),
        &0_u64,
        &None,
        &shared_hash, // same hash → replay attempt
    );
    client.sign_proposal(&signers.get(0).unwrap(), &pid_b);
    client.sign_proposal(&signers.get(1).unwrap(), &pid_b);

    // Execute of B must be rejected.
    let err = client.try_execute_proposal(&pid_b);
    assert!(
        err.is_err(),
        // Invariant: op_hash uniqueness — the same operation cannot be
        // executed twice regardless of which proposal object carries it
        "second execute with identical op_hash must be rejected"
    );

    // Proposal B must remain Pending — its status must not have been mutated.
    // Invariant: a failed execute must not advance proposal B's status.
    assert_eq!(client.get_proposal(&pid_b).status, ProposalStatus::Pending);

    // The global op_hash guard is still set.
    assert!(
        client.is_operation_executed(&shared_hash),
        "is_operation_executed must return true for an executed op_hash"
    );

    let _ = (admin,); // admin not needed but kept for symmetry
}

// ---------------------------------------------------------------------------
// 7. State recovery — failed execute (insufficient sigs) preserves all state
// ---------------------------------------------------------------------------

#[test]
fn test_failed_execute_insufficient_sigs_preserves_proposal_and_signature_count() {
    // Invariant: when execute_proposal panics due to insufficient signatures
    // the Soroban VM rolls back ALL storage mutations atomically.
    // After the failed call:
    //   • proposal.status is still Pending
    //   • signature count is unchanged
    //   • the op_hash is NOT recorded in ExecutedOp
    let (e, client, _admin, signers) = setup_n(3, 3);

    let op_hash = BytesN::from_array(&e, &op(0xBB));
    let pid = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "needs 3 sigs"),
        &0_u64,
        &None,
        &op_hash,
    );

    // Sign with only 2 out of 3 required.
    client.sign_proposal(&signers.get(0).unwrap(), &pid);
    client.sign_proposal(&signers.get(1).unwrap(), &pid);

    // Attempt execute — must fail.
    let err = client.try_execute_proposal(&pid);
    assert!(err.is_err(), "execute with 2/3 signatures must fail");

    // Post-failure invariant checks.
    let proposal = client.get_proposal(&pid);
    assert_eq!(
        proposal.status,
        ProposalStatus::Pending,
        // Invariant: failed execute must not change proposal status
        "proposal status must remain Pending after failed execute"
    );

    assert_eq!(
        client.get_signature_count(&pid),
        2,
        // Invariant: signature count must be unchanged after failed execute
        "signature count must not be altered by a failed execute"
    );

    assert!(
        !client.is_operation_executed(&op_hash),
        // Invariant: the op_hash registry must not be written on failure
        "op_hash must not be marked executed after a failed execute"
    );

    // Providing the 3rd signature then succeeds.
    client.sign_proposal(&signers.get(2).unwrap(), &pid);
    client.execute_proposal(&pid);
    assert_eq!(client.get_proposal(&pid).status, ProposalStatus::Executed);
    assert!(client.is_operation_executed(&op_hash));
}

// ---------------------------------------------------------------------------
// 8. State recovery — failed execute (expired) marks status Expired exactly once
// ---------------------------------------------------------------------------

#[test]
fn test_failed_execute_expired_sets_status_to_expired_exactly_once_and_is_final() {
    // Invariant: `execute_proposal` on an expired proposal transitions the
    // proposal to ProposalStatus::Expired and then panics. Any subsequent
    // call targeting the same proposal must fail with ProposalAlreadyExecuted
    // (#604), i.e. the Expired state is terminal.
    let (e, client, _admin, signers) = setup_n(2, 2);

    e.ledger().with_mut(|l| l.timestamp = 1_000);

    let op_hash = BytesN::from_array(&e, &op(0xEE));
    let pid = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "expiring proposal"),
        &1_500_u64, // expires at ledger ts 1500
        &None,
        &op_hash,
    );

    client.sign_proposal(&signers.get(0).unwrap(), &pid);
    client.sign_proposal(&signers.get(1).unwrap(), &pid);

    // Advance time past expiry.
    e.ledger().with_mut(|l| l.timestamp = 2_000);

    // First attempted execute: must fail and mark the proposal as Expired.
    let err = client.try_execute_proposal(&pid);
    assert!(
        err.is_err(),
        "execute on an expired proposal must be rejected"
    );

    let proposal = client.get_proposal(&pid);
    assert_eq!(
        proposal.status,
        ProposalStatus::Expired,
        // Invariant: execute on an expired proposal must set status = Expired
        "expired proposal must transition to ProposalStatus::Expired"
    );

    // The op_hash must NOT have been recorded (proposal was not executed).
    assert!(
        !client.is_operation_executed(&op_hash),
        // Invariant: op_hash must not be written when a proposal expires
        "op_hash must not be marked executed for an expired proposal"
    );

    // Second execute attempt: must also fail because status is now Expired ≠ Pending.
    let err2 = client.try_execute_proposal(&pid);
    assert!(
        err2.is_err(),
        // Invariant: Expired is a terminal state — no further execution allowed
        "second execute on an Expired proposal must also be rejected"
    );

    // Status must not have changed (still Expired, not double-mutated).
    assert_eq!(client.get_proposal(&pid).status, ProposalStatus::Expired);
}

// ---------------------------------------------------------------------------
// 9. State recovery — signer removal adjusts threshold but may still block execute
// ---------------------------------------------------------------------------

#[test]
fn test_signer_removal_below_existing_sig_count_still_enforces_active_threshold() {
    // Scenario: 4 signers, threshold 3.  All 4 sign a proposal.  Admin then
    // removes 2 signers.  Threshold auto-adjusts to min(3, 2) == 2.
    // The 2 remaining active signers both signed, so effective_signatures == 2
    // which equals the new threshold → execute must SUCCEED.
    //
    // Invariant: `count_active_signatures` re-counts only current signers;
    // removed signers' stored signatures are not counted at execution time.
    let (e, client, admin, signers) = setup_n(4, 3);

    let op_hash = BytesN::from_array(&e, &op(0xDD));
    let pid = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "signer removal recovery"),
        &0_u64,
        &None,
        &op_hash,
    );

    // All 4 signers sign.
    for i in 0..4 {
        client.sign_proposal(&signers.get(i).unwrap(), &pid);
    }

    // Remove 2 of the original 4 signers.  Threshold auto-adjusts to 2.
    client.remove_signer(&admin, &signers.get(2).unwrap());
    client.remove_signer(&admin, &signers.get(3).unwrap());

    assert_eq!(
        client.get_signer_count(),
        2,
        "signer count must reflect removals"
    );
    assert_eq!(
        client.get_threshold(),
        2,
        // Invariant: threshold auto-adjusts downward to signer count when it
        // would otherwise exceed it
        "threshold must have been auto-adjusted to new signer count"
    );

    // Execute must succeed: 2 active signatures == threshold of 2.
    client.execute_proposal(&pid);
    assert_eq!(client.get_proposal(&pid).status, ProposalStatus::Executed);
}

#[test]
fn test_signer_removal_to_one_blocks_execute_when_that_one_hasnt_signed() {
    // Invariant: removing signers down to 1 means that single signer must
    // have signed; if they have not, execute must still be rejected.
    let (e, client, admin, signers) = setup_n(3, 2);

    let op_hash = BytesN::from_array(&e, &op(0xFF));
    let pid = client.submit_proposal(
        &signers.get(0).unwrap(),
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "last signer unsigned"),
        &0_u64,
        &None,
        &op_hash,
    );

    // Only signers[1] and signers[2] sign — signers[0] does NOT.
    client.sign_proposal(&signers.get(1).unwrap(), &pid);
    client.sign_proposal(&signers.get(2).unwrap(), &pid);

    // Remove signers[1] and signers[2].  Only signers[0] remains, threshold
    // auto-adjusts to 1.  signers[0] never signed.
    client.remove_signer(&admin, &signers.get(1).unwrap());
    client.remove_signer(&admin, &signers.get(2).unwrap());

    assert_eq!(client.get_signer_count(), 1);
    assert_eq!(client.get_threshold(), 1);

    // Execute must fail: the only active signer (signers[0]) never signed.
    let err = client.try_execute_proposal(&pid);
    assert!(
        err.is_err(),
        // Invariant: removed signers' signatures are not counted at execution
        "execute must be rejected when the only remaining signer hasn't signed"
    );

    // Proposal must remain Pending after the failed attempt.
    assert_eq!(client.get_proposal(&pid).status, ProposalStatus::Pending);
}

// ---------------------------------------------------------------------------
// 10. Invariant — proposal counter increments monotonically across failures
// ---------------------------------------------------------------------------

#[test]
fn test_proposal_counter_increments_even_when_subsequent_submits_fail() {
    // Invariant: the ProposalCounter is incremented before the proposal is
    // stored.  A successful submit returns the pre-increment counter value.
    // Consecutive successful submits must yield strictly increasing IDs.
    // (Failed submits are tested by verifying the next successful submit
    // continues from where it left off.)
    let (e, client, _admin, signers) = setup_n(1, 1);

    let proposer = signers.get(0).unwrap();

    let id0 = submit(&e, &client, &proposer, 0, 0x01);
    let id1 = submit(&e, &client, &proposer, 0, 0x02);
    let id2 = submit(&e, &client, &proposer, 0, 0x03);

    // Invariant: proposal IDs must be strictly monotonically increasing.
    assert!(id0 < id1, "proposal IDs must be strictly increasing");
    assert!(id1 < id2, "proposal IDs must be strictly increasing");

    // A failed submit (non-signer) must not consume a proposal ID.
    let non_signer = Address::generate(&e);
    let fail = client.try_submit_proposal(
        &non_signer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "unauthorized"),
        &0_u64,
        &None,
        &BytesN::from_array(&e, &op(0xF0)),
    );
    assert!(fail.is_err(), "non-signer submit must fail");

    // Next successful submit must pick up from id2 + 1 = 3.
    let id3 = submit(&e, &client, &proposer, 0, 0x04);
    assert_eq!(
        id3,
        id2 + 1,
        // Invariant: failed submits must not advance the ProposalCounter
        "ProposalCounter must not be advanced by a failed submit"
    );
}

// ---------------------------------------------------------------------------
// 11. Boundary — add signer raises count while keeping threshold valid
// ---------------------------------------------------------------------------

#[test]
fn test_add_signer_does_not_change_threshold_below_current_threshold() {
    // Invariant: adding a signer increases SignerCount but must not alter
    // Threshold when Threshold <= new SignerCount.
    let (e, client, admin, signers) = setup_n(3, 3);

    assert_eq!(client.get_threshold(), 3);

    let extra = Address::generate(&e);
    client.add_signer(&admin, &extra);

    assert_eq!(client.get_signer_count(), 4);
    assert_eq!(
        client.get_threshold(),
        3,
        // Invariant: adding a signer must not change a still-valid threshold
        "threshold must remain 3 after adding a 4th signer"
    );
}

// ---------------------------------------------------------------------------
// 12. Boundary — set_threshold to signer_count is the maximum allowed value
// ---------------------------------------------------------------------------

#[test]
fn test_set_threshold_to_signer_count_is_accepted() {
    // Invariant: threshold == signer_count is valid; threshold == signer_count + 1 is not.
    let (e, client, admin, signers) = setup_n(3, 1);
    let n = client.get_signer_count();

    // Setting threshold == n must succeed.
    client.set_threshold(&admin, &n);
    assert_eq!(client.get_threshold(), n);

    // Setting threshold == n + 1 must fail.
    let err = client.try_set_threshold(&admin, &(n + 1));
    assert!(
        err.is_err(),
        // Invariant: threshold > signer_count must be rejected
        "threshold above signer_count must be rejected"
    );

    // Threshold must be unchanged after the failed attempt.
    assert_eq!(
        client.get_threshold(),
        n,
        // Invariant: failed set_threshold must not alter current threshold
        "threshold must remain n after a rejected set_threshold call"
    );

    let _ = signers; // suppress unused warning
    let _ = e;
}

// ---------------------------------------------------------------------------
// 13. Permission — reject on uninitialized contract
// ---------------------------------------------------------------------------

#[test]
fn test_submit_proposal_panics_on_uninitialized_contract() {
    // Invariant: all mutating entrypoints that reach require_signer or
    // require_admin call `get(&DataKey::Admin)` which returns
    // ContractError::NotInitialized (#1) when the contract hasn't been set up.
    let e = Env::default();
    e.mock_all_auths();
    let cid = e.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(&e, &cid);

    let proposer = Address::generate(&e);
    let err = client.try_submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "uninit"),
        &0_u64,
        &None,
        &BytesN::from_array(&e, &op(0x11)),
    );
    assert!(
        err.is_err(),
        // Invariant: submit_proposal on an uninitialized contract must be rejected
        "submit_proposal must be rejected for an uninitialized contract"
    );
}

// ---------------------------------------------------------------------------
// 14. Duplicate — double-sign after proposal re-opens (sign→remove→add back)
// ---------------------------------------------------------------------------

#[test]
fn test_removed_and_readded_signer_cannot_double_sign() {
    // Invariant: a signer who already signed a proposal, gets removed, and
    // then gets added back retains their original on-chain signature entry.
    // Attempting to sign again must still panic with AlreadyActive (#405).
    let (e, client, admin, signers) = setup_n(3, 2);

    let signer_a = signers.get(0).unwrap();
    let pid = submit(&e, &client, &signer_a, 0, 0x55);

    // signer_a signs.
    client.sign_proposal(&signer_a, &pid);
    assert!(client.has_signed(&pid, &signer_a));

    // Admin removes signer_a then adds them back.
    client.remove_signer(&admin, &signer_a);
    client.add_signer(&admin, &signer_a);

    // signer_a is active again but their stored signature entry persists.
    // A second sign attempt must be rejected.
    let err = client.try_sign_proposal(&signer_a, &pid);
    assert!(
        err.is_err(),
        // Invariant: signing twice (even after remove+readd) must be rejected
        "double-sign after remove+readd must be rejected"
    );

    // Signature count must be 1, not 2.
    assert_eq!(
        client.get_signature_count(&pid),
        1,
        "signature count must not increase after a rejected double-sign"
    );
}

// ---------------------------------------------------------------------------
// 15. Boundary — empty proposal description is rejected at submit time
// ---------------------------------------------------------------------------

#[test]
fn test_empty_description_rejected_at_submit_without_side_effects() {
    // Invariant: submit_proposal panics immediately on empty description
    // before allocating a new proposal ID, so the ProposalCounter must
    // not advance on this failure.
    let (e, client, _admin, signers) = setup_n(1, 1);
    let proposer = signers.get(0).unwrap();

    let id0 = submit(&e, &client, &proposer, 0, 0xAA);

    // Attempt with empty description.
    let err = client.try_submit_proposal(
        &proposer,
        &ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, ""), // empty — must be rejected
        &0_u64,
        &None,
        &BytesN::from_array(&e, &op(0xAB)),
    );
    assert!(err.is_err(), "empty description must cause submit to fail");

    // Next successful submit must be id0 + 1, proving the counter was not
    // advanced by the failed attempt.
    let id1 = submit(&e, &client, &proposer, 0, 0xAC);
    assert_eq!(
        id1,
        id0 + 1,
        // Invariant: failed submit must not advance ProposalCounter
        "ProposalCounter must not be advanced after a rejected submit"
    );
}
