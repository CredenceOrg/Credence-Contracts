#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::{Address, Env, IntoVal};

struct Fixture {
    env: Env,
    client: BountyEscrowClient<'static>,
    admin: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000);

    let admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_addr = token_contract.address();

    let contract_id = env.register(BountyEscrow, ());
    let client = BountyEscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    Fixture {
        env,
        client,
        admin,
    }
}

fn contract_error(e: ContractError) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(e as u32)
}

#[test]
fn test_admin_transfer_flow() {
    let f = setup();
    let new_admin = Address::generate(&f.env);

    assert_eq!(f.client.get_admin(), f.admin);
    assert_eq!(f.client.get_pending_admin(), None);

    // Step 1: Propose new admin
    f.client.propose_admin(&new_admin);
    assert_eq!(f.client.get_pending_admin(), Some(new_admin.clone()));
    assert_eq!(f.client.get_admin(), f.admin);

    // Step 2: Proposed admin accepts
    f.client.accept_admin();
    assert_eq!(f.client.get_admin(), new_admin);
    assert_eq!(f.client.get_pending_admin(), None);
}

#[test]
fn test_wrong_acceptor_rejected() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_addr = token_contract.address();
    let contract_id = env.register(BountyEscrow, ());
    let client = BountyEscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.propose_admin(&new_admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    // Old admin cannot accept on behalf of proposed admin
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_accept_admin().is_err());
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    // Unrelated third party cannot accept either
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_accept_admin().is_err());
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin));
}

#[test]
fn test_cancel_admin_proposal() {
    let f = setup();
    let new_admin = Address::generate(&f.env);

    f.client.propose_admin(&new_admin);
    assert_eq!(f.client.get_pending_admin(), Some(new_admin.clone()));

    // Admin cancels proposal
    f.client.cancel_admin_proposal();
    assert_eq!(f.client.get_pending_admin(), None);
    assert_eq!(f.client.get_admin(), f.admin);

    // Accept fails after cancellation
    assert_eq!(
        f.client.try_accept_admin(),
        Err(Ok(contract_error(ContractError::NoPendingAdmin)))
    );

    // Second cancel also fails
    assert_eq!(
        f.client.try_cancel_admin_proposal(),
        Err(Ok(contract_error(ContractError::NoPendingAdmin)))
    );
}

#[test]
fn test_unauthorized_cancel_rejected() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_addr = token_contract.address();
    let contract_id = env.register(BountyEscrow, ());
    let client = BountyEscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &token_addr);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.propose_admin(&new_admin);

    // Proposed admin cannot cancel the proposal
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
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    // Unrelated stranger cannot cancel either
    env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "cancel_admin_proposal",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_cancel_admin_proposal().is_err());
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin));
}

#[test]
fn test_replace_admin_proposal() {
    let f = setup();
    let candidate1 = Address::generate(&f.env);
    let candidate2 = Address::generate(&f.env);

    f.client.propose_admin(&candidate1);
    assert_eq!(f.client.get_pending_admin(), Some(candidate1));

    // Overwrite proposal with candidate 2
    f.client.propose_admin(&candidate2);
    assert_eq!(f.client.get_pending_admin(), Some(candidate2.clone()));

    // Candidate 2 accepts
    f.client.accept_admin();
    assert_eq!(f.client.get_admin(), candidate2);
    assert_eq!(f.client.get_pending_admin(), None);
}

#[test]
fn test_propose_self_or_contract_rejected() {
    let f = setup();

    assert_eq!(
        f.client.try_propose_admin(&f.admin),
        Err(Ok(contract_error(ContractError::AdminUnchanged)))
    );

    assert_eq!(
        f.client.try_propose_admin(&f.client.address),
        Err(Ok(contract_error(ContractError::InvalidAdminAddress)))
    );
}

#[test]
fn test_accept_or_cancel_without_proposal_fails() {
    let f = setup();

    assert_eq!(
        f.client.try_accept_admin(),
        Err(Ok(contract_error(ContractError::NoPendingAdmin)))
    );

    assert_eq!(
        f.client.try_cancel_admin_proposal(),
        Err(Ok(contract_error(ContractError::NoPendingAdmin)))
    );
}
