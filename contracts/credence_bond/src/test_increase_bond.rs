#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, IntoVal, Symbol, TryFromVal, Vec};

fn setup<'a>(
    e: &'a Env,
) -> (
    CredenceBondClient<'a>,
    Address,
    Address,
    soroban_sdk::token::Client<'a>,
) {
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);

    let admin = Address::generate(e);
    let identity = Address::generate(e);

    client.initialize(&admin);

    let token_admin = Address::generate(e);
    let token_id = e
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);
    let token_client = soroban_sdk::token::Client::new(e, &token_id);

    token_admin_client.mint(&identity, &10_000_000_000_i128);
    client.set_token(&admin, &token_id);
    client.set_bond_token(&admin, &token_id);

    (client, contract_id, identity, token_client)
}

#[test]
fn test_increase_bond_success_transfers_and_updates_storage() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // Approve enough for both create_bond (1000) and increase_bond (500)
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    let before_user = token_client.balance(&identity);
    let before_contract = token_client.balance(&contract_id);

    let updated = client.increase_bond(&identity, &500_i128);

    assert_eq!(updated.bonded_amount, 1500);
    assert_eq!(token_client.balance(&identity), before_user - 500);
    assert_eq!(token_client.balance(&contract_id), before_contract + 500);
}

#[test]
#[should_panic(expected = "token not set")]
fn test_increase_bond_fails_without_token_configuration() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);
    client.initialize(&admin);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    client.increase_bond(&identity, &10_i128);
}

#[test]
#[should_panic(expected = "not bond owner")]
fn test_increase_bond_fails_for_non_owner() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    let stranger = Address::generate(&e);

    // Approve for create_bond (1000) and increase_bond (500)
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);
    token_client.approve(&stranger, &contract_id, &500_i128, &1000_u32);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    client.increase_bond(&stranger, &500_i128);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_increase_bond_rejects_zero_amount() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // Approve for create_bond
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.increase_bond(&identity, &0_i128);
}

#[test]
#[should_panic(expected = "bond increase caused overflow")]
fn test_increase_bond_overflow_protection() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // First create a bond with a normal amount
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);
    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    // Now try to increase by i128::MAX - this should cause overflow
    token_client.approve(&identity, &contract_id, &i128::MAX, &1000_u32);

    client.increase_bond(&identity, &i128::MAX);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_increase_bond_fails_without_allowance() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // Approve for create_bond only
    token_client.approve(&identity, &contract_id, &1000_i128, &1000_u32);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    // No approval for increase_bond - should fail
    client.increase_bond(&identity, &500_i128);
}

#[test]
fn test_increase_bond_emits_event() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // Approve for create_bond (1000) and increase_bond (250)
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    let _ = client.increase_bond(&identity, &250_i128);

    let events = e.events().all();
    assert!(!events.is_empty());

    let expected_topics = Vec::from_array(
        &e,
        [
            Symbol::new(&e, "bond_increased").into_val(&e),
            identity.clone().into_val(&e),
        ],
    );
    let expected_data = (250_i128, 1000_i128, 1250_i128);

    let found = events.iter().any(|evt| {
        if evt.1 != expected_topics {
            return false;
        }
        <(i128, i128, i128)>::try_from_val(&e, &evt.2)
            .map(|data| data == expected_data)
            .unwrap_or(false)
    });

    assert!(found, "expected bond_increased event not found");
}

#[test]
fn test_increase_bond_preserves_other_fields() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // Approve for create_bond (1000) and increase_bond (150)
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);

    let original = client.create_bond(&identity, &1000_i128, &86400_u64, &true, &7200_u64);

    let updated = client.increase_bond(&identity, &150_i128);

    assert_eq!(updated.identity, original.identity);
    assert_eq!(updated.bond_start, original.bond_start);
    assert_eq!(updated.bond_duration, original.bond_duration);
    assert_eq!(updated.slashed_amount, original.slashed_amount);
    assert_eq!(updated.active, original.active);
    assert_eq!(updated.is_rolling, original.is_rolling);
    assert_eq!(
        updated.withdrawal_requested_at,
        original.withdrawal_requested_at
    );
    assert_eq!(
        updated.notice_period_duration,
        original.notice_period_duration
    );
    assert_eq!(updated.bonded_amount, 1150_i128);
}
