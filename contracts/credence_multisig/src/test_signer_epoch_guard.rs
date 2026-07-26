//! Signer-epoch guard tests (issue #839).
//!
//! Locks same / off-by-one / ancient boundaries for
//! `require_matching_signer_epoch` on the multisig pause path.

#![cfg(test)]

use crate::pausable::PROPOSAL_EPOCH_SIZE;
use crate::{CredenceMultiSig, CredenceMultiSigClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Vec};

fn setup() -> (Env, CredenceMultiSigClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register_contract(None, CredenceMultiSig);
    let client = CredenceMultiSigClient::new(&e, &contract_id);
    let mut signers = Vec::new(&e);
    signers.push_back(Address::generate(&e));
    client.initialize(&admin, &signers, &1);
    (e, client, admin)
}

fn add_pause_signers(
    e: &Env,
    client: &CredenceMultiSigClient,
    admin: &Address,
    n: usize,
    threshold: u32,
) -> Vec<Address> {
    let mut signers = Vec::new(e);
    for _ in 0..n {
        let s = Address::generate(e);
        client.set_pause_signer(admin, &s, &true);
        signers.push_back(s);
    }
    client.set_pause_threshold(admin, &threshold);
    signers
}

#[test]
fn test_signer_epoch_guard_same_epoch_passes() {
    let (env, client, admin) = setup();
    let signers = add_pause_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &id);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

#[test]
fn test_signer_epoch_guard_off_by_one_epoch_fails() {
    let (env, client, admin) = setup();
    let signers = add_pause_signers(&env, &client, &admin, 2, 2);
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
    assert_eq!(err, soroban_sdk::Error::from_contract_error(515)); // StaleSignerEpoch
}

#[test]
fn test_signer_epoch_guard_ancient_epoch_fails() {
    let (env, client, admin) = setup();
    let signers = add_pause_signers(&env, &client, &admin, 2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    let id = client.pause(&s1).unwrap();

    env.ledger().with_mut(|l| {
        l.sequence_number += 10 * u32::from(PROPOSAL_EPOCH_SIZE);
    });

    let res = client.try_approve_pause_proposal(&s2, &id);
    assert!(res.is_err(), "ancient epoch approval must fail");
    let err = res.unwrap_err().unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(515)); // StaleSignerEpoch
}
