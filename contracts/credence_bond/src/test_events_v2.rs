#![cfg(test)]

use std::vec::Vec;

use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, FromVal, Symbol,
};

#[test]
fn test_v2_event_indexing_improvements() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
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

    // --- Test bond_created_v2 event with improved indexing ---
    let initial_amount = 10_000_i128;
    let duration = 86400_u64;
    let is_rolling = false;
    let notice_period = 0_u64;
    let bond_start = e.ledger().timestamp();

    client.create_bond_with_rolling(
        &identity,
        &initial_amount,
        &duration,
        &is_rolling,
        &notice_period,
    );

    let events = e.events().all();
    
    // Find both old and new bond_created events
    let old_create_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id && 
            Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created")
        })
        .collect();
    
    let new_create_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id && 
            Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created_v2")
        })
        .collect();

    assert_eq!(old_create_events.len(), 1, "Should emit old bond_created event");
    assert_eq!(new_create_events.len(), 1, "Should emit new bond_created_v2 event");

    // Verify old event structure (backward compatibility)
    let old_event = &old_create_events[0];
    let old_topic_name = Symbol::from_val(&e, &old_event.1.get(0).unwrap());
    let old_topic_ident = Address::from_val(&e, &old_event.1.get(1).unwrap());
    let old_data = <(i128, u64, bool)>::from_val(&e, &old_event.2);

    assert_eq!(old_topic_name, Symbol::new(&e, "bond_created"));
    assert_eq!(old_topic_ident, identity);
    assert_eq!(old_data, (initial_amount, duration, is_rolling));

    // Verify new event structure with improved indexing
    let new_event = &new_create_events[0];
    let new_topic_name = Symbol::from_val(&e, &new_event.1.get(0).unwrap());
    let new_topic_ident = Address::from_val(&e, &new_event.1.get(1).unwrap());
    let new_topic_amount = i128::from_val(&e, &new_event.1.get(2).unwrap());
    let new_topic_timestamp = u64::from_val(&e, &new_event.1.get(3).unwrap());
    let new_data = <(u64, bool, u64)>::from_val(&e, &new_event.2);

    assert_eq!(new_topic_name, Symbol::new(&e, "bond_created_v2"));
    assert_eq!(new_topic_ident, identity);
    assert_eq!(new_topic_amount, initial_amount); // Now indexed!
    assert_eq!(new_topic_timestamp, bond_start); // Now indexed!
    assert_eq!(new_data, (duration, is_rolling, bond_start + duration));

    // --- Test bond_withdrawn_v2 event with improved indexing ---
    let withdraw_amount = 3_000_i128;
    let expected_remaining = 7_000_i128;

    // Fast-forward the ledger time so the 86400s lock-up period expires
    let mut ledger_info = e.ledger().get();
    ledger_info.timestamp += duration + 1;
    e.ledger().set(ledger_info);

    client.withdraw(&withdraw_amount);

    let events = e.events().all();
    
    // Find both old and new bond_withdrawn events
    let old_withdraw_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id && 
            Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_withdrawn")
        })
        .collect();
    
    let new_withdraw_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id && 
            Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_withdrawn_v2")
        })
        .collect();

    assert_eq!(old_withdraw_events.len(), 1, "Should emit old bond_withdrawn event");
    assert_eq!(new_withdraw_events.len(), 1, "Should emit new bond_withdrawn_v2 event");

    // Verify new withdraw event structure with improved indexing
    let new_withdraw_event = &new_withdraw_events[0];
    let withdraw_topic_name = Symbol::from_val(&e, &new_withdraw_event.1.get(0).unwrap());
    let withdraw_topic_ident = Address::from_val(&e, &new_withdraw_event.1.get(1).unwrap());
    let withdraw_topic_amount = i128::from_val(&e, &new_withdraw_event.1.get(2).unwrap());
    let withdraw_topic_remaining = i128::from_val(&e, &new_withdraw_event.1.get(3).unwrap());
    let withdraw_topic_timestamp = u64::from_val(&e, &new_withdraw_event.1.get(4).unwrap());
    let withdraw_data = <(bool, i128)>::from_val(&e, &new_withdraw_event.2);

    assert_eq!(withdraw_topic_name, Symbol::new(&e, "bond_withdrawn_v2"));
    assert_eq!(withdraw_topic_ident, identity);
    assert_eq!(withdraw_topic_amount, withdraw_amount); // Now indexed!
    assert_eq!(withdraw_topic_remaining, expected_remaining); // Now indexed!
    assert!(withdraw_topic_timestamp > 0); // Now indexed!
    assert_eq!(withdraw_data, (false, 0)); // Not early withdrawal, no penalty

    // --- Test bond_increased_v2 event with improved indexing ---
    let top_up_amount = 5_000_i128;
    let expected_total_after_top_up = 12_000_i128;

    client.top_up(&top_up_amount);

    let events = e.events().all();
    
    // Find both old and new bond_increased events
    let old_increase_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id && 
            Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_increased")
        })
        .collect();
    
    let new_increase_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id && 
            Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_increased_v2")
        })
        .collect();

    assert_eq!(old_increase_events.len(), 1, "Should emit old bond_increased event");
    assert_eq!(new_increase_events.len(), 1, "Should emit new bond_increased_v2 event");

    // Verify new increase event structure with improved indexing
    let new_increase_event = &new_increase_events[0];
    let increase_topic_name = Symbol::from_val(&e, &new_increase_event.1.get(0).unwrap());
    let increase_topic_ident = Address::from_val(&e, &new_increase_event.1.get(1).unwrap());
    let increase_topic_added = i128::from_val(&e, &new_increase_event.1.get(2).unwrap());
    let increase_topic_total = i128::from_val(&e, &new_increase_event.1.get(3).unwrap());
    let increase_topic_timestamp = u64::from_val(&e, &new_increase_event.1.get(4).unwrap());
    let increase_data = <(bool, crate::BondTier)>::from_val(&e, &new_increase_event.2);

    assert_eq!(increase_topic_name, Symbol::new(&e, "bond_increased_v2"));
    assert_eq!(increase_topic_ident, identity);
    assert_eq!(increase_topic_added, top_up_amount); // Now indexed!
    assert_eq!(increase_topic_total, expected_total_after_top_up); // Now indexed!
    assert!(increase_topic_timestamp > 0); // Now indexed!
    // tier_changed and new_tier in data depend on threshold configuration
}

#[test]
fn test_event_indexing_query_efficiency() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity1 = Address::generate(&e);
    let identity2 = Address::generate(&e);

    client.initialize(&admin);

    // Setup token
    let token_admin = Address::generate(&e);
    let token_addr = e.register_stellar_asset_contract_v2(token_admin).address();
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    let token_client = TokenClient::new(&e, &token_addr);
    
    token_admin_client.mint(&identity1, &100_000_i128);
    token_admin_client.mint(&identity2, &100_000_i128);
    token_client.approve(&identity1, &contract_id, &100_000_i128, &99999_u32);
    token_client.approve(&identity2, &contract_id, &100_000_i128, &99999_u32);
    client.set_token(&admin, &token_addr);

    // Create multiple bonds with different amounts to test amount-based queries
    let amounts = [1_000_i128, 5_000_i128, 10_000_i128, 25_000_i128];
    let mut timestamps = Vec::new();

    for (i, &amount) in amounts.iter().enumerate() {
        let identity = if i % 2 == 0 { &identity1 } else { &identity2 };
        timestamps.push(e.ledger().timestamp());
        
        client.create_bond_with_rolling(identity, &amount, &86400_u64, &false, &0_u64);
        
        // Advance time for uniqueness
        let mut ledger_info = e.ledger().get();
        ledger_info.timestamp += 1000;
        e.ledger().set(ledger_info);
    }

    let events = e.events().all();

    // Test efficient amount-based filtering using v2 indexed fields
    let large_bond_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id && 
            Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created_v2") &&
            i128::from_val(&e, &ev.1.get(2).unwrap()) >= 10_000_i128 // Indexed amount field
        })
        .collect();

    assert_eq!(large_bond_events.len(), 2, "Should find 2 bonds with amount >= 10,000");

    // Test efficient time-based filtering using v2 indexed timestamp field
    let time_threshold = timestamps[1]; // After second bond
    let recent_bond_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id && 
            Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created_v2") &&
            u64::from_val(&e, &ev.1.get(3).unwrap()) > time_threshold // Indexed timestamp field
        })
        .collect();

    assert_eq!(recent_bond_events.len(), 2, "Should find 2 bonds created after time threshold");

    // Test efficient identity-based filtering (already worked in old version)
    let identity1_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id && 
            Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created_v2") &&
            Address::from_val(&e, &ev.1.get(1).unwrap()) == identity1 // Indexed identity field
        })
        .collect();

    assert_eq!(identity1_events.len(), 2, "Should find 2 bonds for identity1");
}

#[test]
fn test_event_schema_compatibility() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    client.initialize(&admin);

    // Setup token
    let token_admin = Address::generate(&e);
    let token_addr = e.register_stellar_asset_contract_v2(token_admin).address();
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    let token_client = TokenClient::new(&e, &token_addr);
    
    token_admin_client.mint(&identity, &100_000_i128);
    token_client.approve(&identity, &contract_id, &100_000_i128, &99999_u32);
    client.set_token(&admin, &token_addr);

    // Test that both old and new events are emitted for backward compatibility
    client.create_bond_with_rolling(&identity, &10_000_i128, &86400_u64, &false, &0_u64);

    let events = e.events().all();
    
    // Count events by type
    let mut old_events = 0;
    let mut new_events = 0;

    for event in events.iter() {
        if event.0 != contract_id {
            continue;
        }
        
        let event_name = Symbol::from_val(&e, &event.1.get(0).unwrap());
        if event_name == Symbol::new(&e, "bond_created") || 
           event_name == Symbol::new(&e, "bond_withdrawn") ||
           event_name == Symbol::new(&e, "bond_increased") {
            old_events += 1;
        } else if event_name == Symbol::new(&e, "bond_created_v2") || 
                  event_name == Symbol::new(&e, "bond_withdrawn_v2") ||
                  event_name == Symbol::new(&e, "bond_increased_v2") {
            new_events += 1;
        }
    }

    assert!(old_events > 0, "Should emit old events for compatibility");
    assert!(new_events > 0, "Should emit new v2 events");
    assert_eq!(old_events, new_events, "Should emit equal number of old and new events");
}
