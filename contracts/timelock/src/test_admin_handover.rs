use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal,
};

fn setup_env() -> (Env, TimelockContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, client, admin)
}

fn propose_admin_at(
    env: &Env,
    client: &TimelockContractClient<'static>,
    new_admin: &Address,
    timestamp: u64,
) -> PendingAdmin {
    env.ledger().with_mut(|li| li.timestamp = timestamp);
    client.propose_admin(new_admin);
    client.get_pending_admin().unwrap()
}

fn assert_contract_error<T, I: std::fmt::Debug>(
    result: Result<Result<T, soroban_sdk::Error>, Result<soroban_sdk::Error, I>>,
    expected: ContractError,
) {
    match result {
        Err(Ok(error)) => assert_eq!(
            error,
            soroban_sdk::Error::from_contract_error(expected as u32)
        ),
        _ => panic!("expected contract error {expected:?}"),
    }
}

#[test]
fn accept_without_proposal_fails_and_keeps_admin() {
    let (_env, client, admin) = setup_env();
    assert_eq!(client.get_pending_admin(), None);

    assert_contract_error(client.try_accept_admin(), ContractError::NoPendingAdmin);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn cancel_without_proposal_fails_and_keeps_admin() {
    let (_env, client, admin) = setup_env();
    assert_eq!(client.get_pending_admin(), None);

    assert_contract_error(
        client.try_cancel_admin_proposal(),
        ContractError::NoPendingAdmin,
    );
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn wrong_acceptor_rejected_and_keeps_admin_and_pending() {
    let env = Env::default();
    let old_admin = Address::generate(&env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    client.initialize(&old_admin);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    let third_party = Address::generate(&env);
    let pending = propose_admin_at(&env, &client, &new_admin, 1_000);

    env.ledger().with_mut(|li| li.timestamp = pending.eta);

    // Current admin cannot accept on behalf of the proposed admin.
    env.mock_auths(&[MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_accept_admin().is_err());
    assert_eq!(client.get_admin(), old_admin);
    assert_eq!(client.get_pending_admin(), Some(pending.clone()));

    // Unrelated third party cannot accept either.
    env.mock_auths(&[MockAuth {
        address: &third_party,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_accept_admin().is_err());
    assert_eq!(client.get_admin(), old_admin);
    assert_eq!(client.get_pending_admin(), Some(pending));
}

#[test]
fn accept_after_cancel_fails_and_keeps_admin() {
    let (env, client, old_admin) = setup_env();
    let new_admin = Address::generate(&env);
    let pending = propose_admin_at(&env, &client, &new_admin, 1_000);

    client.cancel_admin_proposal();
    assert_eq!(client.get_pending_admin(), None);

    // Second cancel fails with NoPendingAdmin.
    assert_contract_error(
        client.try_cancel_admin_proposal(),
        ContractError::NoPendingAdmin,
    );
    assert_eq!(client.get_admin(), old_admin);

    // Accepting after cancellation fails even once the original eta passes.
    env.ledger().with_mut(|li| li.timestamp = pending.eta);
    assert_contract_error(client.try_accept_admin(), ContractError::NoPendingAdmin);
    assert_eq!(client.get_admin(), old_admin);
}

#[test]
fn unauthorized_cancel_rejected_and_keeps_proposal() {
    let env = Env::default();
    let old_admin = Address::generate(&env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    client.initialize(&old_admin);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    let pending = propose_admin_at(&env, &client, &new_admin, 1_000);

    // Proposed admin cannot cancel the proposal.
    env.mock_auths(&[MockAuth {
        address: &new_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "cancel_admin_proposal",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_cancel_admin_proposal().is_err());
    assert_eq!(client.get_admin(), old_admin);
    assert_eq!(client.get_pending_admin(), Some(pending));
}

#[test]
fn double_propose_overwrites_previous_candidate_and_resets_window() {
    let env = Env::default();
    let old_admin = Address::generate(&env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    client.initialize(&old_admin);

    env.mock_all_auths();
    let first_candidate = Address::generate(&env);
    let second_candidate = Address::generate(&env);

    let first_pending = propose_admin_at(&env, &client, &first_candidate, 1_000);
    let second_pending = propose_admin_at(&env, &client, &second_candidate, 2_000);

    assert_eq!(client.get_pending_admin(), Some(second_pending.clone()));
    assert_eq!(second_pending.address, second_candidate);
    assert!(second_pending.eta > first_pending.eta);

    // At first_pending.eta, second_candidate is not yet ready.
    env.ledger().with_mut(|li| li.timestamp = first_pending.eta);
    assert_contract_error(client.try_accept_admin(), ContractError::TimelockNotReady);
    assert_eq!(client.get_admin(), old_admin);

    // At second_pending.eta, first_candidate cannot accept.
    env.ledger()
        .with_mut(|li| li.timestamp = second_pending.eta);
    env.mock_auths(&[MockAuth {
        address: &first_candidate,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_accept_admin().is_err());
    assert_eq!(client.get_admin(), old_admin);

    // second_candidate can accept at second_pending.eta.
    env.mock_auths(&[MockAuth {
        address: &second_candidate,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.accept_admin();
    assert_eq!(client.get_admin(), second_candidate);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn invalid_proposals_fail_and_keep_admin() {
    let (env, client, old_admin) = setup_env();

    assert_contract_error(
        client.try_propose_admin(&old_admin),
        ContractError::AdminUnchanged,
    );
    assert_eq!(client.get_admin(), old_admin);
    assert_eq!(client.get_pending_admin(), None);

    let contract_address = client.address.clone();
    assert_contract_error(
        client.try_propose_admin(&contract_address),
        ContractError::InvalidAdminAddress,
    );
    assert_eq!(client.get_admin(), old_admin);
    assert_eq!(client.get_pending_admin(), None);

    // Non-admin cannot propose a new admin.
    let attacker = Address::generate(&env);
    let candidate = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "propose_admin",
            args: (candidate.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_propose_admin(&candidate).is_err());
    assert_eq!(client.get_admin(), old_admin);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn accept_timing_boundaries_and_post_handover_invariants() {
    let (env, client, old_admin) = setup_env();
    let new_admin = Address::generate(&env);
    let pending = propose_admin_at(&env, &client, &new_admin, 1_000);

    // One second before eta fails with TimelockNotReady.
    env.ledger().with_mut(|li| li.timestamp = pending.eta - 1);
    assert_contract_error(client.try_accept_admin(), ContractError::TimelockNotReady);
    assert_eq!(client.get_admin(), old_admin);

    // One second after expires_at fails with SignatureExpired.
    env.ledger()
        .with_mut(|li| li.timestamp = pending.expires_at + 1);
    assert_contract_error(client.try_accept_admin(), ContractError::SignatureExpired);
    assert_eq!(client.get_admin(), old_admin);

    // Accepting at exact expires_at succeeds.
    env.ledger()
        .with_mut(|li| li.timestamp = pending.expires_at);
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);
    assert_eq!(client.get_pending_admin(), None);

    // Replaying accept_admin after completion fails with NoPendingAdmin.
    assert_contract_error(client.try_accept_admin(), ContractError::NoPendingAdmin);
    assert_eq!(client.get_admin(), new_admin);

    // Old admin can no longer propose a new admin after handover.
    let next_candidate = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &client.address,
            fn_name: "propose_admin",
            args: (next_candidate.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_propose_admin(&next_candidate).is_err());
    assert_eq!(client.get_admin(), new_admin);
    assert_eq!(client.get_pending_admin(), None);
}
