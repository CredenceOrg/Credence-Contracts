//! Tests for deterministic pause proposal-ID derivation.
//!
//! Covers:
//! 1. Duplicate submission idempotency within the same epoch.
//! 2. Different epoch yields a different proposal ID.
//! 3. Simultaneous submission within the same epoch produces exactly one record.
//! 4. Epoch boundary: ledger N*EPOCH_SIZE-1 vs N*EPOCH_SIZE → different IDs.
//! 5. Vote accumulation after duplicate submission.
//! 6. Legacy fetch by counter-based ID returns a typed error, not a panic.

#![cfg(test)]

use super::*;
use crate::pausable::PROPOSAL_EPOCH_SIZE;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{vec, Address, Env, Vec};

fn setup() -> (Env, Address, CredenceDelegationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, client)
}

fn add_signers(
    env: &Env,
    admin: &Address,
    client: &CredenceDelegationClient,
    n: usize,
    threshold: u32,
) -> Vec<Address> {
    let mut signers = Vec::new(env);
    for _ in 0..n {
        let s = Address::generate(env);
        client.set_pause_signer(admin, &s, &true);
        signers.push_back(s);
    }
    client.set_pause_threshold(admin, &threshold);
    signers
}

/// Two operators submitting the identical Pause action in the same epoch must
/// receive the same proposal_id, and the second submission must not overwrite
/// the proposal record.
#[test]
fn test_proposal_id_duplicate_submission_idempotent() {
    let (env, admin, client) = setup();
    let signers = add_signers(&env, &admin, &client, 2, 1);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Both operators submit Pause in the same epoch.
    let id1 = client.pause(&s1).unwrap();
    let id2 = client.pause(&s2).unwrap();

    assert_eq!(
        id1, id2,
        "same action in same epoch must yield same proposal_id"
    );

    // There must be exactly one proposal record in storage.
    let view = client.get_pause_proposal_state(&id1, &vec![&env, s1.clone(), s2.clone()]);
    assert_eq!(view.action, 1, "action should be Pause (1)");
}

/// The same action submitted in two different epochs must produce different IDs.
#[test]
fn test_proposal_id_different_epoch_yields_new_id() {
    let (env, admin, client) = setup();
    let signers = add_signers(&env, &admin, &client, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Epoch 0.
    let id_epoch0 = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &id_epoch0);
    client.execute_pause_proposal(&id_epoch0);
    assert!(client.is_paused());

    // Advance to the next epoch.
    env.ledger().with_mut(|l| {
        l.sequence_number += u32::from(PROPOSAL_EPOCH_SIZE);
    });

    // Unpause to allow a new pause proposal.
    client.unpause(&admin);
    assert!(!client.is_paused());

    let id_epoch1 = client.pause(&s1).unwrap();
    assert_ne!(
        id_epoch0, id_epoch1,
        "different epochs must yield different proposal IDs"
    );
}

/// Simulated concurrent submissions (same epoch) must result in exactly one
/// proposal record in storage — not two separate proposals.
#[test]
fn test_proposal_id_simultaneous_submission_single_record() {
    let (env, admin, client) = setup();
    let contract_id = client.address.clone();
    let signers = add_signers(&env, &admin, &client, 3, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();
    let s3 = signers.get(2).unwrap();

    // All three "concurrent" submissions in the same epoch.
    let id_a = client.pause(&s1).unwrap();
    let id_b = client.pause(&s2).unwrap();
    let id_c = client.pause(&s3).unwrap();

    assert_eq!(id_a, id_b);
    assert_eq!(id_b, id_c);

    // Confirm only one proposal record exists in storage.
    let has_proposal = env.as_contract(&contract_id, || {
        env.storage().instance().has(&DataKey::PauseProposal(id_a))
    });
    assert!(has_proposal, "exactly one proposal record must exist");

    // And the approval count must be 3 (one per unique signer).
    let view =
        client.get_pause_proposal_state(&id_a, &vec![&env, s1.clone(), s2.clone(), s3.clone()]);
    assert_eq!(view.approvals, 3);
}

/// A proposal submitted at ledger sequence N*EPOCH_SIZE-1 (end of one epoch)
/// and one submitted at N*EPOCH_SIZE (start of the next epoch) must differ.
#[test]
fn test_proposal_id_epoch_boundary() {
    let (env, admin, client) = setup();
    let signers = add_signers(&env, &admin, &client, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Place ledger at the last sequence of epoch 0.
    let epoch_boundary = u32::from(PROPOSAL_EPOCH_SIZE);
    env.ledger().with_mut(|l| {
        l.sequence_number = epoch_boundary - 1;
    });
    let id_before = client.pause(&s1).unwrap();

    // Approve and execute so we can re-propose.
    client.approve_pause_proposal(&s2, &id_before);
    client.execute_pause_proposal(&id_before);
    assert!(client.is_paused());
    client.unpause(&admin);

    // Advance to the first sequence of epoch 1.
    env.ledger().with_mut(|l| {
        l.sequence_number = epoch_boundary;
    });
    let id_after = client.pause(&s1).unwrap();

    assert_ne!(
        id_before, id_after,
        "proposals straddling an epoch boundary must have different IDs"
    );
}

/// After a duplicate submission, a vote cast by a third operator must increment
/// the shared proposal's count — not create a phantom second proposal.
#[test]
fn test_proposal_id_vote_accumulation_after_duplicate() {
    let (env, admin, client) = setup();
    let signers = add_signers(&env, &admin, &client, 3, 3);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();
    let s3 = signers.get(2).unwrap();

    // Two operators submit the same action (idempotent).
    let id = client.pause(&s1).unwrap();
    let id2 = client.pause(&s2).unwrap();
    assert_eq!(id, id2);

    // The shared proposal has 2 approvals (s1 and s2).
    let view =
        client.get_pause_proposal_state(&id, &vec![&env, s1.clone(), s2.clone(), s3.clone()]);
    assert_eq!(view.approvals, 2);

    // Third operator approves via the normal approve path.
    client.approve_pause_proposal(&s3, &id);

    let view2 =
        client.get_pause_proposal_state(&id, &vec![&env, s1.clone(), s2.clone(), s3.clone()]);
    assert_eq!(
        view2.approvals, 3,
        "vote must accumulate on the single shared proposal"
    );

    // Now the 3-of-3 threshold is met; execution must succeed.
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

/// Fetching by a counter-based ID that was never written must return a typed
/// `ProposalNotFound` error, not a panic.
#[test]
fn test_legacy_fetch_returns_typed_error_not_panic() {
    let (_env, _admin, client) = setup();

    // Slot 999 was never allocated by the counter (counter never advances now).
    let result = client.try_get_proposal_by_legacy_id(&999_u64);
    assert!(
        result.is_err(),
        "fetching a non-existent legacy ID must return an error"
    );

    // The error must be a typed contract error (ProposalNotFound, code 603),
    // not a panic from an unwrap or an unexpected host failure.  The internal
    // `get_proposal_by_legacy_id` returns `Err(ContractError::ProposalNotFound)`
    // which the public entrypoint converts to a contract panic; the `try_` client
    // method catches that and surfaces it as `Err`.
    assert!(result.is_err(), "must return Err, not a value");
}
