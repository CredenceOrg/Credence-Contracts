#![cfg(test)]

extern crate std;

use crate::{
    test_helpers::setup_with_token, BatchTransferItem, CredenceBond, CredenceBondClient,
    MAX_BATCH_TRANSFER_SIZE,
};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env, Symbol, TryFromVal, Vec,
};
use credence_errors::ContractError;
use soroban_sdk::{testutils::Address as _, token::TokenClient, Address, Env, Vec};

fn setup(env: &Env) -> (CredenceBondClient, Address, Address, Address, Address) {
    let (client, admin, identity, token_id, contract_id) = setup_with_token(env);

    // Fund the contract by transferring tokens from identity
    let token = TokenClient::new(env, &token_id);
    token.transfer(&identity, &contract_id, &10_000_000);

    (client, admin, identity, token_id, contract_id)
}

fn make_item(env: &Env, recipient: &Address, amount: i128) -> BatchTransferItem {
    BatchTransferItem {
        recipient: recipient.clone(),
        amount,
    }
}

#[test]
fn test_batch_transfer_updates_addressed_bonds_only_and_preserves_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, token_id, contract_id) = setup(&env);
    let asset = StellarAssetClient::new(&env, &token_id);
    let token = TokenClient::new(&env, &token_id);

    let identity1 = Address::generate(&env);
    let identity2 = Address::generate(&env);
    let unaddressed = Address::generate(&env);
    for identity in [&identity1, &identity2, &unaddressed] {
        asset.mint(identity, &100_000);
        token.approve(identity, &contract_id, &100_000, &10_000);
    }

    client.create_bond(&identity1, &10_000, &86_400, &false, &0);
    client.create_bond(&identity2, &20_000, &86_400, &false, &0);
    client.create_bond(&unaddressed, &30_000, &86_400, &false, &0);

    let mut items = Vec::new(&env);
    items.push_back(make_item(&env, &identity1, 1_000));
    items.push_back(make_item(&env, &identity2, 2_000));

    let count = client.batch_transfer(&admin, &items);
    assert_eq!(count, 2);
    assert_eq!(client.get_identity_state(&identity1).bonded_amount, 9_000);
    assert_eq!(client.get_identity_state(&identity2).bonded_amount, 18_000);
    assert_eq!(client.get_identity_state(&unaddressed).bonded_amount, 30_000);

    let all_events = env.events().all();
    let expected_topic = Symbol::new(&env, "batch_transfer");
    let event = all_events
        .iter()
        .find(|ev| {
            ev.0 == contract_id
                && ev.1.get(0)
                    .and_then(|v| Symbol::try_from_val(&env, &v).ok())
                    .map(|topic| topic == expected_topic)
                    .unwrap_or(false)
        })
        .expect("batch_transfer event must exist");
    let (event_count, event_total): (u32, i128) =
        soroban_sdk::TryFromVal::try_from_val(&env, &event.2).unwrap();
    assert_eq!(event_count, 2);
    assert_eq!(event_total, 3_000);

    let topic_admin =
        Address::try_from_val(&env, &event.1.get(1).unwrap()).expect("topic admin");
    assert_eq!(topic_admin, admin);
}

#[test]
fn test_batch_transfer_multiple_recipients() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, token_id, contract_id) = setup(&env);

    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let recipient3 = Address::generate(&env);

    let mut items = Vec::new(&env);
    items.push_back(make_item(&env, &recipient1, 1000));
    items.push_back(make_item(&env, &recipient2, 2000));
    items.push_back(make_item(&env, &recipient3, 3000));

    let token = TokenClient::new(&env, &token_id);
    let balance_before = token.balance(&contract_id);

    let count = client.batch_transfer(&admin, &items);

    assert_eq!(count, 3);
    assert_eq!(balance_before - token.balance(&contract_id), 6000);
    assert_eq!(token.balance(&recipient1), 1000);
    assert_eq!(token.balance(&recipient2), 2000);
    assert_eq!(token.balance(&recipient3), 3000);
}

#[test]
fn test_batch_transfer_single_recipient() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, _contract_id) = setup(&env);

    let recipient = Address::generate(&env);
    let mut items = Vec::new(&env);
    items.push_back(make_item(&env, &recipient, 5000));

    let count = client.batch_transfer(&admin, &items);

    assert_eq!(count, 1);
}

#[test]
fn test_batch_transfer_at_max_size_is_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, _contract_id) = setup(&env);

    let recipient = Address::generate(&env);
    let mut items = Vec::new(&env);
    for _ in 0..MAX_BATCH_TRANSFER_SIZE {
        items.push_back(make_item(&env, &recipient, 1));
    }

    let count = client.batch_transfer(&admin, &items);
    assert_eq!(count, MAX_BATCH_TRANSFER_SIZE);
}

#[test]
#[should_panic(expected = "Error(Contract, #202)")]
fn test_batch_transfer_empty_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, _contract_id) = setup(&env);

    let items = Vec::new(&env);
    client.batch_transfer(&admin, &items);
}

#[test]
#[should_panic(expected = "Error(Contract, #201)")]
fn test_batch_transfer_excessive_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, _contract_id) = setup(&env);

    let mut items = Vec::new(&env);
    let recipient = Address::generate(&env);
    for _ in 0..MAX_BATCH_TRANSFER_SIZE + 1 {
        items.push_back(make_item(&env, &recipient, 1));
    }
    client.batch_transfer(&admin, &items);
}

#[test]
fn test_batch_transfer_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, _contract_id) = setup(&env);

    let recipient = Address::generate(&env);
    let mut items = Vec::new(&env);
    items.push_back(make_item(&env, &recipient, 0));
    assert_eq!(
        client.try_batch_transfer(&admin, &items),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            ContractError::AmountMustBePositive as u32
        )))
    );
}

#[test]
fn test_batch_transfer_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, _contract_id) = setup(&env);

    let recipient = Address::generate(&env);
    let mut items = Vec::new(&env);
    items.push_back(make_item(&env, &recipient, -100));
    assert_eq!(
        client.try_batch_transfer(&admin, &items),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            ContractError::AmountMustBePositive as u32
        )))
    );
}

#[test]
fn test_batch_transfer_self_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, contract_id) = setup(&env);

    let mut items = Vec::new(&env);
    items.push_back(make_item(&env, &contract_id, 100));
    assert_eq!(
        client.try_batch_transfer(&admin, &items),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            ContractError::SelfTransferNotAllowed as u32
        )))
    );
}
