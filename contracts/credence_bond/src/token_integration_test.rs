use crate::test_helpers;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env, String};

#[test]
fn test_set_usdc_token_and_network() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let token = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let network = String::from_str(&e, "testnet");
    client.set_usdc_token(&admin, &token, &network);

    assert_eq!(client.get_usdc_token(), token);
    assert_eq!(client.get_usdc_network(), Some(network));
}

#[test]
fn test_get_usdc_network_none_when_unset() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    assert_eq!(client.get_usdc_network(), None);
}

#[test]
#[should_panic(expected = "unsupported stellar network")]
fn test_set_usdc_token_rejects_unknown_network() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let token = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let network = String::from_str(&e, "futurenet");
    client.set_usdc_token(&admin, &token, &network);
}

#[test]
fn test_create_bond_moves_tokens_into_contract() {
    let e = Env::default();
    let (client, _admin, identity, token_id, bond_contract_id) = test_helpers::setup_with_token(&e);

    let token_client = TokenClient::new(&e, &token_id);
    let balance_before = token_client.balance(&identity);
    let contract_before = token_client.balance(&bond_contract_id);

    let amount = 2_500_i128;
    client.create_bond(&identity, &amount, &86400_u64, &false, &0_u64);

    let balance_after = token_client.balance(&identity);
    let contract_after = token_client.balance(&bond_contract_id);

    assert_eq!(balance_before - balance_after, amount);
    assert_eq!(contract_after - contract_before, amount);
}

#[test]
#[should_panic(expected = "insufficient token allowance")]
fn test_create_bond_without_approval_panics() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);
    client.initialize(&admin);

    let token_id = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let stellar_asset = StellarAssetClient::new(&e, &token_id);
    stellar_asset.set_authorized(&identity, &true);
    stellar_asset.mint(&identity, &10_000_i128);

    client.set_token(&admin, &token_id);
    client.create_bond(&identity, &1_000_i128, &86400_u64, &false, &0_u64);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_token_rejects_non_admin() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let attacker = Address::generate(&e);
    client.initialize(&admin);

    let token = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    client.set_token(&attacker, &token);
}

#[test]
#[should_panic(expected = "token not set")]
fn test_get_usdc_token_without_configuration_panics() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);
    let _ = client.get_usdc_token();
}

#[test]
#[should_panic(expected = "insufficient token allowance")]
fn test_top_up_requires_remaining_allowance() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let identity = Address::generate(&e);
    client.initialize(&admin);

    let token_id = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let stellar_asset = StellarAssetClient::new(&e, &token_id);
    stellar_asset.set_authorized(&identity, &true);
    stellar_asset.mint(&identity, &10_000_i128);

    let token_client = TokenClient::new(&e, &token_id);
    let expiration = e.ledger().sequence().saturating_add(10_000);
    token_client.approve(&identity, &contract_id, &1_000_i128, &expiration);

    client.set_token(&admin, &token_id);
    client.create_bond(&identity, &1_000_i128, &86400_u64, &false, &0_u64);
    client.top_up(&1_i128);
}

#[test]
fn test_withdraw_transfers_tokens_back_to_identity() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, token_id, bond_contract_id) = test_helpers::setup_with_token(&e);

    client.create_bond(&identity, &1_000_i128, &86400_u64, &false, &0_u64);

    let token_client = TokenClient::new(&e, &token_id);
    let identity_before = token_client.balance(&identity);
    let contract_before = token_client.balance(&bond_contract_id);

    e.ledger().with_mut(|li| li.timestamp = 87_401);
    client.withdraw_bond(&400_i128);

    let identity_after = token_client.balance(&identity);
    let contract_after = token_client.balance(&bond_contract_id);

    assert_eq!(identity_after - identity_before, 400);
    assert_eq!(contract_before - contract_after, 400);
}

#[test]
#[should_panic(expected = "amount must be non-negative")]
fn test_top_up_negative_amount_panics() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(&e);
    client.create_bond(&identity, &1_000_i128, &86400_u64, &false, &0_u64);
    client.top_up(&-1_i128);
}

#[test]
#[should_panic(expected = "amount must be non-negative")]
fn test_withdraw_negative_amount_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(&e);
    client.create_bond(&identity, &1_000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87_401);
    client.withdraw_bond(&-1_i128);
}
