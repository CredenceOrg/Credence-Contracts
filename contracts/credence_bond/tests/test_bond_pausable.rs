//! Integration coverage for issue #1042: emergency pause gating on bond mutations.
#![cfg(test)]

use credence_bond::soroban_sdk::testutils::Address as _;
use credence_bond::soroban_sdk::{Address, Bytes, Env, IntoVal, String, Symbol, Val, Vec};
use credence_bond::{CredenceBond, CredenceBondClient, DataKey};
use credence_errors::ContractError;

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin, &None);
    (client, admin)
}

fn setup_with_bond(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin) = setup(e);
    let identity = Address::generate(e);
    // Bond creation enforces a large minimum amount (1e18).
    let amount = 1_000_000_000_000_000_000_i128;
    client.create_bond(&identity, &amount, &3600_u64, &false, &0_u64);
    let treasury = Address::generate(e);
    client.set_slash_treasury(&admin, &treasury);
    (client, admin, identity)
}

fn expect_contract_error(
    env: &Env,
    contract: &Address,
    func: &str,
    args: Vec<Val>,
) -> ContractError {
    match env.try_invoke_contract::<(), ContractError>(contract, &Symbol::new(env, func), args) {
        Err(Ok(error)) => error,
        Err(Err(invoke)) => panic!("{func} failed without a ContractError: {invoke:?}"),
        Ok(_) => panic!("{func} unexpectedly succeeded"),
    }
}

#[test]
fn mutating_entrypoints_revert_when_paused() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e);
    let stranger = Address::generate(&e);

    assert!(!client.is_paused());
    client.pause(&admin);
    assert!(client.is_paused());

    assert!(client
        .try_create_bond(
            &stranger,
            &1_000_000_000_000_000_000_i128,
            &3600_u64,
            &false,
            &0_u64
        )
        .is_err());
    assert!(client
        .try_top_up(&identity, &1_000_000_000_000_000_000_i128)
        .is_err());
    assert!(client
        .try_withdraw(&identity, &1_000_000_000_000_000_000_i128)
        .is_err());
    assert!(client
        .try_withdraw_early(&identity, &1_000_000_000_000_000_000_i128)
        .is_err());
    assert!(client.try_request_withdrawal(&identity).is_err());
    assert!(client.try_renew_if_rolling(&identity).is_err());
    assert!(client.try_withdraw_bond(&identity).is_err());
    assert!(client
        .try_slash_bond(&admin, &identity, &100_i128, &Bytes::new(&e))
        .is_err());
    assert!(client.try_collect_fees(&admin, &Bytes::new(&e)).is_err());
}

#[test]
fn views_remain_callable_while_paused() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.pause(&admin);
    assert!(client.is_paused());

    let random_addr = Address::generate(&e);
    let _ = client.version();
    let _ = client.describe_config();
    let _ = client.describe_bond(&random_addr);
    let _ = client.get_nonce(&random_addr);
    let _ = client.get_weight_config();
    let _ = client.is_paused();
}

#[test]
fn pause_during_active_lockup_then_unpause() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e);

    client.pause(&admin);
    assert!(client
        .try_top_up(&identity, &1_000_000_000_000_000_000_i128)
        .is_err());
    assert!(client
        .try_withdraw_early(&identity, &1_000_000_000_000_000_000_i128)
        .is_err());
    let _ = client.get_identity_state(&identity);

    client.unpause(&admin);
    assert!(!client.is_paused());
    assert!(client
        .try_top_up(&identity, &1_000_000_000_000_000_000_i128)
        .is_ok());
}

#[test]
fn multisig_pause_and_unpause_flow() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();
    assert!(!client.is_paused());
    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());

    let pid2 = client.unpause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pid2);
    client.execute_pause_proposal(&pid2);
    assert!(!client.is_paused());
}

#[test]
fn pause_management_errors_are_typed() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let signer = Address::generate(&e);
    client.set_pause_signer(&admin, &signer, &true);

    let mut args = Vec::new(&e);
    args.push_back(admin.clone().into_val(&e));
    args.push_back(2_u32.into_val(&e));
    assert_eq!(
        expect_contract_error(&e, &client.address, "set_pause_threshold", args),
        ContractError::ThresholdExceedsSigners
    );

    client.set_pause_threshold(&admin, &1_u32);
    let stranger = Address::generate(&e);
    let mut args = Vec::new(&e);
    args.push_back(stranger.into_val(&e));
    assert_eq!(
        expect_contract_error(&e, &client.address, "pause", args),
        ContractError::NotSigner
    );

    let mut args = Vec::new(&e);
    args.push_back(signer.clone().into_val(&e));
    args.push_back(99_u64.into_val(&e));
    assert_eq!(
        expect_contract_error(&e, &client.address, "approve_pause_proposal", args),
        ContractError::ProposalNotFound
    );

    let mut args = Vec::new(&e);
    args.push_back(99_u64.into_val(&e));
    assert_eq!(
        expect_contract_error(&e, &client.address, "execute_pause_proposal", args),
        ContractError::ProposalNotFound
    );
}

#[test]
fn unpause_without_stored_admin_returns_typed_not_initialized() {
    let e = Env::default();
    let (client, caller) = setup(&e);
    e.as_contract(&client.address, || {
        e.storage().instance().set(&DataKey::PauseThreshold, &1_u32);
        e.storage().instance().remove(&DataKey::Admin);
    });

    let mut args = Vec::new(&e);
    args.push_back(caller.into_val(&e));
    assert_eq!(
        expect_contract_error(&e, &client.address, "unpause", args),
        ContractError::NotInitialized
    );
}

#[test]
fn executing_pause_proposal_without_quorum_returns_typed_error() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let signer1 = Address::generate(&e);
    let signer2 = Address::generate(&e);
    client.set_pause_signer(&admin, &signer1, &true);
    client.set_pause_signer(&admin, &signer2, &true);
    client.set_pause_threshold(&admin, &2_u32);
    let proposal_id = client.pause(&signer1).unwrap();

    let mut args = Vec::new(&e);
    args.push_back(proposal_id.into_val(&e));
    assert_eq!(
        expect_contract_error(&e, &client.address, "execute_pause_proposal", args),
        ContractError::InsufficientApprovals
    );
}

#[test]
fn executing_pause_proposal_with_invalid_action_returns_typed_error() {
    let e = Env::default();
    let (client, _) = setup(&e);
    e.as_contract(&client.address, || {
        e.storage()
            .instance()
            .set(&DataKey::PauseProposal(7), &3_u32);
    });

    let mut args = Vec::new(&e);
    args.push_back(7_u64.into_val(&e));
    assert_eq!(
        expect_contract_error(&e, &client.address, "execute_pause_proposal", args),
        ContractError::InvalidPauseAction
    );
}

#[test]
fn pause_management_remains_available_while_paused() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.pause(&admin);
    assert!(client.is_paused());

    let signer = Address::generate(&e);
    client.set_pause_signer(&admin, &signer, &true);
    client.set_pause_threshold(&admin, &1u32);
    client.unpause(&admin);
    assert!(!client.is_paused());
}

#[test]
fn attestation_mutations_blocked_when_paused() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let contract_id = Address::generate(&e);
    let attester = Address::generate(&e);
    client.register_attester(&attester);

    client.pause(&admin);
    assert!(client
        .try_add_attestation(
            &attester,
            &Address::generate(&e),
            &String::from_str(&e, "kyc"),
            &contract_id,
            &0_u64,
            &0_u64,
        )
        .is_err());
    assert!(client
        .try_add_attestation_batch(&attester, &Vec::new(&e))
        .is_err());
}
