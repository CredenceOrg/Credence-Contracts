//! Admin-epoch guard tests (issue #836).
//!
//! Locks the same / off-by-one / ancient boundary matrix for
//! `require_matching_admin_epoch` on the admin pause path.

#![cfg(test)]

use crate::*;
use crate::pausable::PROPOSAL_EPOCH_SIZE;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

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
    assert_eq!(err, soroban_sdk::Error::from_contract_error(514)); // StaleAdminEpoch
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
    assert_eq!(err, soroban_sdk::Error::from_contract_error(514)); // StaleAdminEpoch
}
