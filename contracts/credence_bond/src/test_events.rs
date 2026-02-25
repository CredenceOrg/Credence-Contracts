#![cfg(test)]

use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, FromVal, Symbol,
};

#[test]
fn test_lifecycle_event_emissions() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    client.initialize(&admin);

    // --- SETUP MOCK TOKEN ---
    let token_admin = Address::generate(&e);
    let token_addr = e.register_stellar_asset_contract_v2(token_admin).address();

    // 1. Mint 100,000 tokens to the identity so they have funds to bond
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    token_admin_client.mint(&identity, &100_000_i128);

    // 2. APPROVE the contract to spend the identity's tokens (Fixes "not enough allowance")
    let token_client = TokenClient::new(&e, &token_addr);
    token_client.approve(&identity, &contract_id, &100_000_i128, &99999_u32);

    // 3. Tell the CredenceBond contract which token to use
    client.set_token(&admin, &token_addr);

    // --- 1. Test Create Bond Event ---
    let initial_amount = 10_000_i128;
    let duration = 86400_u64;
    let is_rolling = false;
    let notice_period = 0_u64;

    client.create_bond(
        &identity,
        &initial_amount,
        &duration,
        &is_rolling,
        &notice_period,
    );

    // Filter events to only capture those emitted by our contract (ignore token transfers)
    let events = e.events().all();
    let create_event = events
        .into_iter()
        .rev()
        .find(|ev| ev.0 == contract_id)
        .unwrap();

    // Decode Topics
    let topic_name = Symbol::from_val(&e, &create_event.1.get(0).unwrap());
    let topic_ident = Address::from_val(&e, &create_event.1.get(1).unwrap());

    assert_eq!(topic_name, Symbol::new(&e, "bond_created"));
    assert_eq!(topic_ident, identity.clone());

    // Decode Data
    let create_data = <(i128, u64, bool)>::from_val(&e, &create_event.2);
    assert_eq!(create_data, (initial_amount, duration, is_rolling));

    // --- 2. Test Top Up Event (Increase) ---
    let top_up_amount = 5_000_i128;
    let expected_total_after_top_up = 15_000_i128;

    client.top_up(&top_up_amount);

    let events = e.events().all();
    let top_up_event = events
        .into_iter()
        .rev()
        .find(|ev| ev.0 == contract_id)
        .unwrap();

    // Decode Topics
    let topic_name = Symbol::from_val(&e, &top_up_event.1.get(0).unwrap());
    let topic_ident = Address::from_val(&e, &top_up_event.1.get(1).unwrap());

    assert_eq!(topic_name, Symbol::new(&e, "bond_increased"));
    assert_eq!(topic_ident, identity.clone());

    // Decode Data
    let top_up_data = <(i128, i128)>::from_val(&e, &top_up_event.2);
    assert_eq!(top_up_data, (top_up_amount, expected_total_after_top_up));

    // --- 3. Test Withdraw Event ---
    let withdraw_amount = 3_000_i128;
    // Current bonded = 15,000. After withdrawing 3,000, expected remaining = 12,000.
    let expected_remaining_bonded = 12_000_i128;

    // Fast-forward the ledger time so the 86400s lock-up period expires
    let mut ledger_info = e.ledger().get();
    ledger_info.timestamp += duration + 1;
    e.ledger().set(ledger_info);

    client.withdraw(&withdraw_amount);

    let events = e.events().all();
    let withdraw_event = events
        .into_iter()
        .rev()
        .find(|ev| ev.0 == contract_id)
        .unwrap();

    // Decode Topics
    let topic_name = Symbol::from_val(&e, &withdraw_event.1.get(0).unwrap());
    let topic_ident = Address::from_val(&e, &withdraw_event.1.get(1).unwrap());

    assert_eq!(topic_name, Symbol::new(&e, "bond_withdrawn"));
    assert_eq!(topic_ident, identity);

    // Decode Data
    let withdraw_data = <(i128, i128)>::from_val(&e, &withdraw_event.2);
    assert_eq!(withdraw_data, (withdraw_amount, expected_remaining_bonded));
}
