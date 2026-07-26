#![cfg(test)]

extern crate std;

use crate::{
    test_helpers::setup_with_token, BatchTransferItem, CredenceBond, CredenceBondClient,
    MAX_BATCH_TRANSFER_SIZE,
};
use soroban_sdk::{
    testutils::Address as _,
    token::TokenClient,
    Address, Env, Vec,
};

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
#[should_panic(expected = "amount must be positive")]
fn test_batch_transfer_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, _contract_id) = setup(&env);

    let recipient = Address::generate(&env);
    let mut items = Vec::new(&env);
    items.push_back(make_item(&env, &recipient, 0));
    client.batch_transfer(&admin, &items);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_batch_transfer_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, _contract_id) = setup(&env);

    let recipient = Address::generate(&env);
    let mut items = Vec::new(&env);
    items.push_back(make_item(&env, &recipient, -100));
    client.batch_transfer(&admin, &items);
}

#[test]
#[should_panic(expected = "recipient cannot be the contract itself")]
fn test_batch_transfer_self_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _identity, _token_id, contract_id) = setup(&env);

    let mut items = Vec::new(&env);
    items.push_back(make_item(&env, &contract_id, 100));
    client.batch_transfer(&admin, &items);
}
