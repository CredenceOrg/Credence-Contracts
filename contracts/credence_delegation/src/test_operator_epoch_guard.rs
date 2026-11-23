//! Operator-epoch guard tests for delegation (issue #1044).
//!
//! Regression coverage for same-ledger, off-by-one, and ancient governance
//! epoch transitions. Verifies that `require_matching_operator_epoch` correctly
//! rejects stale pause proposals in the delegation contract.
//!
//! The delegation contract uses hash-derived proposal IDs computed from
//! `(action, epoch)` where `epoch = ledger_sequence / PROPOSAL_EPOCH_SIZE`.
//! A proposal submitted in epoch N cannot be approved or executed in epoch N+1.

#![cfg(test)]

use crate::pausable::PROPOSAL_EPOCH_SIZE;
use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

fn setup() -> (Env, CredenceDelegationClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    client.initialize(&admin);
    (e, client, admin)
}

fn add_signers(
    e: &Env,
    client: &CredenceDelegationClient,
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

/// Same-epoch: proposal and approval in the same epoch must succeed.
#[test]
fn test_operator_epoch_guard_same_epoch_passes() {
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
fn test_operator_epoch_guard_off_by_one_epoch_fails() {
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
    assert!(
        res.is_err(),
        "off-by-one epoch approval must fail in delegation"
    );
    let err = res.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ContractError::StaleEpoch as u32)
    );
}

/// Ancient: proposal many epochs in the past → reject.
#[test]
fn test_operator_epoch_guard_ancient_epoch_fails() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap();

    env.ledger().with_mut(|l| {
        l.sequence_number += 10 * u32::from(PROPOSAL_EPOCH_SIZE);
    });

    let res = client.try_approve_pause_proposal(&s2, &id);
    assert!(
        res.is_err(),
        "ancient epoch approval must fail in delegation"
    );
    let err = res.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ContractError::StaleEpoch as u32)
    );
}

/// Stale execution: proposal approved in epoch N, execute in epoch N+1 → reject.
#[test]
fn test_operator_epoch_guard_rejects_stale_execution() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &id);

    env.ledger().with_mut(|l| {
        l.sequence_number += u32::from(PROPOSAL_EPOCH_SIZE);
    });

    let res = client.try_execute_pause_proposal(&id);
    assert!(
        res.is_err(),
        "stale proposal execution must fail in delegation"
    );
    let err = res.unwrap_err().unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ContractError::StaleEpoch as u32)
    );
    assert!(!client.is_paused());
}

/// Same-epoch unpause: proposal and approval within same epoch must work.
#[test]
fn test_operator_epoch_guard_unpause_same_epoch_passes() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // First pause
    let pause_id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pause_id);
    client.execute_pause_proposal(&pause_id);
    assert!(client.is_paused());

    // Then unpause in the same epoch
    let unpause_id = client.unpause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &unpause_id);
    client.execute_pause_proposal(&unpause_id);
    assert!(!client.is_paused());
}

/// Off-by-one unpause: unpause proposal crosses epoch boundary → reject.
#[test]
fn test_operator_epoch_guard_unpause_off_by_one_fails() {
    let (env, client, admin) = setup();
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Pause first
    client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &0); // approve prior pause proposal
                                            // ... actually pause directly since it was admin in setup
                                            // Let's re-setup with proper pause
    drop(signers);
    let signers = add_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let pause_id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pause_id);
    client.execute_pause_proposal(&pause_id);
    assert!(client.is_paused());

    // Now cross epoch boundary for unpause
    let epoch_boundary = u32::from(PROPOSAL_EPOCH_SIZE);
    env.ledger().with_mut(|l| {
        l.sequence_number = epoch_boundary - 1;
    });

    let unpause_id = client.unpause(&s1).unwrap();

    env.ledger().with_mut(|l| {
        l.sequence_number = epoch_boundary;
    });

    let res = client.try_approve_pause_proposal(&s2, &unpause_id);
    assert!(
        res.is_err(),
        "off-by-one epoch unpause approval must fail in delegation"
    );
}
