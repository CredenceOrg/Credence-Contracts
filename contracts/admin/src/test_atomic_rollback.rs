//! Regression tests for atomic ownership-transfer failure handling.
//!
//! These use the generated contract client (`try_accept_ownership`) so the
//! assertions exercise Soroban's transaction boundary rather than a direct
//! Rust call. A rejected acceptance must retain the current owner, proposal,
//! and event stream exactly as they were before the attempted transaction.

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

fn setup() -> (Env, Address, AdminContractClient<'static>, Address, Address) {
    let env = Env::default();
    let contract_id = env.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let candidate = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&owner, &1, &100);
    client.add_admin(&owner, &candidate, &AdminRole::SuperAdmin);
    client.transfer_ownership(&owner, &candidate);
    env.ledger()
        .with_mut(|ledger| ledger.timestamp += OWNERSHIP_TRANSFER_TIMELOCK);

    (env, contract_id, client, owner, candidate)
}

#[test]
fn rejected_acceptance_rolls_back_when_candidate_is_deactivated() {
    let (env, contract_id, client, owner, candidate) = setup();
    // Failure injection at the acceptance boundary: a candidate can become
    // inactive through a future administrative recovery path. We write that
    // terminal state directly because peer SuperAdmins cannot deactivate one
    // another through the public permission model.
    env.as_contract(&contract_id, || {
        let mut info: AdminInfo = env
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(candidate.clone()))
            .unwrap();
        info.active = false;
        env.storage()
            .instance()
            .set(&DataKey::AdminInfo(candidate.clone()), &info);
    });

    let events_before = env.events().all().len();
    assert!(client.try_accept_ownership(&candidate).is_err());

    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.get_pending_owner(), Some(candidate));
    assert_eq!(env.events().all().len(), events_before);
}

#[test]
fn rejected_acceptance_rolls_back_when_candidate_is_suspended() {
    let (env, _contract_id, client, owner, candidate) = setup();
    let suspension_end = env.ledger().timestamp() + 1;
    client.suspend_admin(&owner, &candidate, &suspension_end);

    let events_before = env.events().all().len();
    assert!(client.try_accept_ownership(&candidate).is_err());

    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.get_pending_owner(), Some(candidate));
    assert_eq!(env.events().all().len(), events_before);
}
