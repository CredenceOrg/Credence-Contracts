//! Deterministic failure-boundary and recovery coverage for the delegation
//! `pausable` module (issue #1363).
//!
//! `contracts/credence_delegation/src/pausable.rs` backs the multisig pause
//! switch for the delegation contract. This suite pins the boundaries the
//! module's own invariants depend on:
//!
//! * **authorization** — admin-only config, signer-only proposals;
//! * **validation** — threshold within `0..=signer_count`, no zero threshold
//!   while signers exist;
//! * **no-lockout invariant** — enabling the first signer auto-raises the
//!   threshold to 1, removing the last signer clears it, and the admin can
//!   always unpause;
//! * **stale epoch** — a proposal bound to a previous epoch cannot be approved
//!   or executed;
//! * **recovery** — every rejected/no-op path leaves state consistent and the
//!   next valid call succeeds.
//!
//! Wire-stable error codes are asserted exactly so a change to the error
//! mapping is caught by `cargo test`.

#![cfg(test)]

use crate::pausable::PROPOSAL_EPOCH_SIZE;
use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

// Wire-stable discriminants (`credence_errors::ContractError`).
const ERR_NOT_INITIALIZED: u32 = 1;
const ERR_NOT_ADMIN: u32 = 100;
const ERR_NOT_SIGNER: u32 = 104;
const ERR_CONTRACT_PAUSED: u32 = 106;
const ERR_INVALID_PAUSE_ACTION: u32 = 107;
const ERR_STALE_EPOCH: u32 = 513;
const ERR_THRESHOLD_EXCEEDS_SIGNERS: u32 = 601;
const ERR_PROPOSAL_NOT_FOUND: u32 = 603;
const ERR_INSUFFICIENT_APPROVALS: u32 = 605;

fn setup() -> (Env, Address, CredenceDelegationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, client)
}

fn stored_threshold(env: &Env, client: &CredenceDelegationClient) -> u32 {
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0)
    })
}

fn stored_signer_count(env: &Env, client: &CredenceDelegationClient) -> u32 {
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0)
    })
}

// ---------------------------------------------------------------------------
// set_pause_threshold — validation boundaries
// ---------------------------------------------------------------------------

#[test]
fn set_pause_threshold_requires_admin() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true);

    let stranger = Address::generate(&env);
    let err = client
        .try_set_pause_threshold(&stranger, &1u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));
    assert_eq!(stored_threshold(&env, &client), 1);

    // Recovery: the admin can still set it.
    client.set_pause_threshold(&admin, &1u32);
    assert_eq!(stored_threshold(&env, &client), 1);
}

#[test]
fn set_pause_threshold_rejects_above_signer_count_and_recovers() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true); // auto threshold = 1

    let err = client
        .try_set_pause_threshold(&admin, &2u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_THRESHOLD_EXCEEDS_SIGNERS)
    );
    // Rejected call left the previous threshold intact.
    assert_eq!(stored_threshold(&env, &client), 1);

    // The inclusive upper boundary (== signer count) is accepted.
    client.set_pause_threshold(&admin, &1u32);
    assert_eq!(stored_threshold(&env, &client), 1);
}

#[test]
fn set_pause_threshold_exact_count_boundary() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);

    client.set_pause_threshold(&admin, &2u32);
    assert_eq!(stored_threshold(&env, &client), 2);

    let err = client
        .try_set_pause_threshold(&admin, &3u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_THRESHOLD_EXCEEDS_SIGNERS)
    );
    assert_eq!(stored_threshold(&env, &client), 2);
}

#[test]
fn set_pause_threshold_zero_with_signers_is_rejected() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true); // threshold = 1

    // A zero threshold while signers exist would make multisig unreachable.
    let err = client
        .try_set_pause_threshold(&admin, &0u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_PAUSE_ACTION)
    );
    assert_eq!(stored_threshold(&env, &client), 1);
}

#[test]
fn set_pause_threshold_zero_without_signers_is_allowed() {
    let (env, admin, client) = setup();
    // No signers: count = 0, so a zero threshold is valid (direct admin mode).
    client.set_pause_threshold(&admin, &0u32);
    assert_eq!(stored_threshold(&env, &client), 0);
}

// ---------------------------------------------------------------------------
// set_pause_signer — no-lockout invariant and count integrity
// ---------------------------------------------------------------------------

#[test]
fn first_signer_auto_sets_threshold_to_one() {
    let (env, admin, client) = setup();
    assert_eq!(stored_threshold(&env, &client), 0);

    let s1 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true);

    // No-lockout: enabling a signer raises a zero threshold to 1...
    assert_eq!(stored_threshold(&env, &client), 1);
    assert_eq!(stored_signer_count(&env, &client), 1);
    // ...which switches `pause` into proposal mode (returns Some(id)).
    assert!(client.pause(&s1).is_some());
    assert!(!client.is_paused());
}

#[test]
fn removing_last_signer_resets_threshold_to_zero() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true);

    client.set_pause_signer(&admin, &s1, &false);

    // Clamped back to the (now zero) signer count, so the admin regains the
    // direct toggle and is never locked out.
    assert_eq!(stored_threshold(&env, &client), 0);
    assert_eq!(stored_signer_count(&env, &client), 0);
    assert!(client.pause(&admin).is_none());
    assert!(client.is_paused());
}

#[test]
fn set_pause_signer_requires_admin() {
    let (env, admin, client) = setup();
    let signer = Address::generate(&env);

    let stranger = Address::generate(&env);
    let err = client
        .try_set_pause_signer(&stranger, &signer, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));
    assert_eq!(stored_signer_count(&env, &client), 0);

    client.set_pause_signer(&admin, &signer, &true);
    assert_eq!(stored_signer_count(&env, &client), 1);
}

#[test]
fn duplicate_signer_enable_preserves_count_invariant() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s1, &true); // duplicate enable
    assert_eq!(stored_signer_count(&env, &client), 1);

    client.set_pause_signer(&admin, &s2, &true);
    assert_eq!(stored_signer_count(&env, &client), 2);

    client.set_pause_signer(&admin, &s2, &false);
    client.set_pause_signer(&admin, &s2, &false); // duplicate disable
    assert_eq!(stored_signer_count(&env, &client), 1);

    client.set_pause_signer(&admin, &s1, &false);
    assert_eq!(stored_signer_count(&env, &client), 0);
}

// ---------------------------------------------------------------------------
// pause / unpause — authorization, paused gating, recovery
// ---------------------------------------------------------------------------

#[test]
fn direct_pause_and_unpause_require_admin() {
    let (env, admin, client) = setup();
    let stranger = Address::generate(&env);

    let err = client.try_pause(&stranger).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    let err = client.try_unpause(&stranger).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    assert!(!client.is_paused());
    assert!(client.pause(&admin).is_none());
    assert!(client.is_paused());
}

#[test]
fn proposal_requires_registered_signer() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true); // threshold = 1

    // Being admin is not enough in multisig mode; only signers may propose.
    let err = client.try_pause(&admin).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_SIGNER));

    let stranger = Address::generate(&env);
    let err = client.try_pause(&stranger).unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_SIGNER));

    assert!(!client.is_paused());
    // Recovery: a registered signer can still propose.
    assert!(client.pause(&s1).is_some());
}

#[test]
fn governance_mutations_are_rejected_while_paused_and_recover() {
    let (env, admin, client) = setup();
    client.pause(&admin); // direct pause (threshold 0)
    assert!(client.is_paused());

    let signer = Address::generate(&env);
    let err = client
        .try_set_pause_signer(&admin, &signer, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_CONTRACT_PAUSED)
    );

    let err = client
        .try_set_pause_threshold(&admin, &0u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_CONTRACT_PAUSED)
    );

    // Nothing was mutated by the rejected calls.
    assert_eq!(stored_signer_count(&env, &client), 0);

    // Recovery: unpause, then the same calls succeed.
    client.unpause(&admin);
    assert!(!client.is_paused());
    client.set_pause_signer(&admin, &signer, &true);
    assert_eq!(stored_signer_count(&env, &client), 1);
}

// ---------------------------------------------------------------------------
// Proposal lifecycle boundaries
// ---------------------------------------------------------------------------

#[test]
fn execute_below_threshold_is_rejected_then_recovers() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let id = client.pause(&s1).unwrap(); // proposer's approval recorded

    let err = client.try_execute_pause_proposal(&id).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INSUFFICIENT_APPROVALS)
    );
    assert!(!client.is_paused());

    // The proposal is still live: the missing approval completes it.
    client.approve_pause_proposal(&s2, &id);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

#[test]
fn unknown_proposal_is_not_found() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true);
    let unknown_id = 42_424_u64;

    let err = client
        .try_execute_pause_proposal(&unknown_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_PROPOSAL_NOT_FOUND)
    );

    let err = client
        .try_approve_pause_proposal(&s1, &unknown_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_PROPOSAL_NOT_FOUND)
    );

    assert!(!client.is_paused());
}

#[test]
fn stale_epoch_proposal_cannot_be_executed_or_approved() {
    let (env, admin, client) = setup();
    let s1 = Address::generate(&env);
    client.set_pause_signer(&admin, &s1, &true); // threshold = 1

    // End of epoch 0: derive_proposal_id uses (sequence - 1) / EPOCH_SIZE.
    env.ledger()
        .with_mut(|l| l.sequence_number = PROPOSAL_EPOCH_SIZE);
    let id = client.pause(&s1).unwrap();
    assert!(!client.is_paused());

    // Cross into epoch 1: the same action now derives a different id.
    env.ledger()
        .with_mut(|l| l.sequence_number = PROPOSAL_EPOCH_SIZE + 1);

    let err = client.try_execute_pause_proposal(&id).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_STALE_EPOCH)
    );

    let err = client
        .try_approve_pause_proposal(&s1, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_STALE_EPOCH)
    );

    assert!(
        !client.is_paused(),
        "a stale proposal must not change state"
    );
}

#[test]
fn uninitialized_contract_rejects_governance_calls() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&env, &contract_id);
    let caller = Address::generate(&env);

    // No admin stored -> fail closed with NotInitialized (not a silent success).
    let err = client.try_pause(&caller).unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_NOT_INITIALIZED)
    );

    let err = client
        .try_set_pause_signer(&caller, &caller, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_NOT_INITIALIZED)
    );
    assert!(!client.is_paused());
}
