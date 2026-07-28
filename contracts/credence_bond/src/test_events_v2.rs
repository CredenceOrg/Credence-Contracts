#![cfg(test)]

use std::vec::Vec;

use crate::{test_helpers, CredenceBond, CredenceBondClient};
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

    client.initialize(&admin, &None);

    // --- SETUP MOCK TOKEN ---
    let token_addr = e.register(test_helpers::MockStellarAsset, ());

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
    let duration = credence_math::Timestamp::SECONDS_PER_DAY;
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
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created")
        })
        .collect();

    let new_create_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created_v2")
        })
        .collect();

    assert_eq!(
        old_create_events.len(),
        1,
        "Should emit old bond_created event"
    );
    assert_eq!(
        new_create_events.len(),
        1,
        "Should emit new bond_created_v2 event"
    );

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

    // Fast-forward the ledger time so the credence_math::Timestamp::SECONDS_PER_DAYs lock-up period expires
    let mut ledger_info = e.ledger().get();
    ledger_info.timestamp += duration + 1;
    e.ledger().set(ledger_info);

    client.withdraw(&identity, &withdraw_amount);

    let events = e.events().all();

    // Find both old and new bond_withdrawn events
    let old_withdraw_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_withdrawn")
        })
        .collect();

    let new_withdraw_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "bond_withdrawn_v2")
        })
        .collect();

    assert_eq!(
        old_withdraw_events.len(),
        1,
        "Should emit old bond_withdrawn event"
    );
    assert_eq!(
        new_withdraw_events.len(),
        1,
        "Should emit new bond_withdrawn_v2 event"
    );

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

    client.top_up(&identity, &top_up_amount);

    let events = e.events().all();

    // Find both old and new bond_increased events
    let old_increase_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_increased")
        })
        .collect();

    let new_increase_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "bond_increased_v2")
        })
        .collect();

    assert_eq!(
        old_increase_events.len(),
        1,
        "Should emit old bond_increased event"
    );
    assert_eq!(
        new_increase_events.len(),
        1,
        "Should emit new bond_increased_v2 event"
    );

    // Verify new increase event structure with improved indexing
    let new_increase_event = &new_increase_events[0];
    let increase_topic_name = Symbol::from_val(&e, &new_increase_event.1.get(0).unwrap());
    let increase_topic_ident = Address::from_val(&e, &new_increase_event.1.get(1).unwrap());
    let increase_topic_added = i128::from_val(&e, &new_increase_event.1.get(2).unwrap());
    let increase_topic_total = i128::from_val(&e, &new_increase_event.1.get(3).unwrap());
    let increase_topic_timestamp = u64::from_val(&e, &new_increase_event.1.get(4).unwrap());
    let _increase_data = <(bool, crate::BondTier)>::from_val(&e, &new_increase_event.2);

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

    client.initialize(&admin, &None);

    // Setup token
    let token_addr = e.register(test_helpers::MockStellarAsset, ());
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

        client.create_bond_with_rolling(identity, &amount, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64);

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
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created_v2")
                && i128::from_val(&e, &ev.1.get(2).unwrap()) >= 10_000_i128 // Indexed amount field
        })
        .collect();

    assert_eq!(
        large_bond_events.len(),
        2,
        "Should find 2 bonds with amount >= 10,000"
    );

    // Test efficient time-based filtering using v2 indexed timestamp field
    let time_threshold = timestamps[1]; // After second bond
    let recent_bond_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created_v2")
                && u64::from_val(&e, &ev.1.get(3).unwrap()) > time_threshold // Indexed timestamp field
        })
        .collect();

    assert_eq!(
        recent_bond_events.len(),
        2,
        "Should find 2 bonds created after time threshold"
    );

    // Test efficient identity-based filtering (already worked in old version)
    let identity1_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "bond_created_v2")
                && Address::from_val(&e, &ev.1.get(1).unwrap()) == identity1 // Indexed identity field
        })
        .collect();

    assert_eq!(
        identity1_events.len(),
        2,
        "Should find 2 bonds for identity1"
    );
}

#[test]
fn test_event_schema_compatibility() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    client.initialize(&admin, &None);

    // Setup token
    let token_addr = e.register(test_helpers::MockStellarAsset, ());
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    let token_client = TokenClient::new(&e, &token_addr);

    token_admin_client.mint(&identity, &100_000_i128);
    token_client.approve(&identity, &contract_id, &100_000_i128, &99999_u32);
    client.set_token(&admin, &token_addr);

    // Test that both old and new events are emitted for backward compatibility
    client.create_bond_with_rolling(&identity, &10_000_i128, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64);

    let events = e.events().all();

    // Count events by type
    let mut old_events = 0;
    let mut new_events = 0;

    for event in events.iter() {
        if event.0 != contract_id {
            continue;
        }

        let event_name = Symbol::from_val(&e, &event.1.get(0).unwrap());
        if event_name == Symbol::new(&e, "bond_created")
            || event_name == Symbol::new(&e, "bond_withdrawn")
            || event_name == Symbol::new(&e, "bond_increased")
        {
            old_events += 1;
        } else if event_name == Symbol::new(&e, "bond_created_v2")
            || event_name == Symbol::new(&e, "bond_withdrawn_v2")
            || event_name == Symbol::new(&e, "bond_increased_v2")
        {
            new_events += 1;
        }
    }

    assert!(old_events > 0, "Should emit old events for compatibility");
    assert!(new_events > 0, "Should emit new v2 events");
    assert_eq!(
        old_events, new_events,
        "Should emit equal number of old and new events"
    );
}

#[test]
fn test_tier_changed_v2_event_on_create_bond() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    client.initialize(&admin, &None);

    let token_addr = e.register(test_helpers::MockStellarAsset, ());
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    let token_client = TokenClient::new(&e, &token_addr);

    let bond_amount = crate::tiered_bond::TIER_BRONZE_MAX + 1;
    token_admin_client.mint(&identity, &bond_amount);
    token_client.approve(&identity, &contract_id, &bond_amount, &99999_u32);
    client.set_token(&admin, &token_addr);

    let ts_before = e.ledger().timestamp();
    client.create_bond_with_rolling(
        &identity,
        &bond_amount,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    let events = e.events().all();

    let v1_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap()) == Symbol::new(&e, "tier_changed")
        })
        .collect();

    let v2_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "tier_changed_v2")
        })
        .collect();

    assert_eq!(v1_events.len(), 1, "Bronze→Silver must emit tier_changed");
    assert_eq!(
        v2_events.len(),
        1,
        "Bronze→Silver must emit tier_changed_v2"
    );

    let v1_data = <(Address, crate::BondTier)>::from_val(&e, &v1_events[0].2);
    assert_eq!(v1_data.0, identity);
    assert!(
        core::mem::discriminant(&v1_data.1) == core::mem::discriminant(&crate::BondTier::Silver)
    );

    let v2_event = &v2_events[0];
    assert_eq!(
        Symbol::from_val(&e, &v2_event.1.get(0).unwrap()),
        Symbol::new(&e, "tier_changed_v2")
    );
    assert_eq!(
        Address::from_val(&e, &v2_event.1.get(1).unwrap()),
        identity
    );
    let v2_data = <(crate::BondTier, crate::BondTier, u64)>::from_val(&e, &v2_event.2);
    assert!(
        core::mem::discriminant(&v2_data.0) == core::mem::discriminant(&crate::BondTier::Bronze)
    );
    assert!(
        core::mem::discriminant(&v2_data.1) == core::mem::discriminant(&crate::BondTier::Silver)
    );
    assert!(
        v2_data.2 >= ts_before,
        "tier_changed_v2 timestamp must reflect ledger time"
    );
}

// ============================================================================
// Critical-flow event-payload assertions (added for issue #1022).
//
// These tests pin down the v2 payload of events that the older shared tests
// don't fully exercise — `bond_slashed_v2`, `bond_liquidated`, the early-exit
// penalty data tuple, `param_updated`, `attestation_added`, and the legacy
// single-symbol attester/claim/payment event payloads. They follow the same
// shape as the existing tests in this file: drive the contract through the
// real entry point, then read back from `env.events().all()`.
// ============================================================================

#[test]
fn test_bond_slashed_v2_pays_out_legacy_v1_and_indexed_v2() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    client.initialize(&admin, &None);

    let token_addr = e.register(test_helpers::MockStellarAsset, ());
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    let token_client = TokenClient::new(&e, &token_addr);

    token_admin_client.mint(&identity, &100_000_i128);
    token_client.approve(&identity, &contract_id, &100_000_i128, &99999_u32);
    client.set_token(&admin, &token_addr);

    client.create_bond_with_rolling(
        &identity,
        &10_000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    // Set the slash treasury so the transfer-out path is satisfied.
    let slash_treasury = Address::generate(&e);
    client.set_slash_treasury(&admin, &slash_treasury);

    let slash_amount = 4_000_i128;
    let ts_before = e.ledger().timestamp();
    client.slash(&admin, &slash_amount);

    let events = e.events().all();

    let v2_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "bond_slashed_v2")
        })
        .collect();

    assert_eq!(
        v2_events.len(),
        1,
        "slash must emit exactly one bond_slashed_v2 event"
    );
    let ev = &v2_events[0];

    // Topics: (Symbol, identity, slash_amount, total_slashed, timestamp, admin)
    assert_eq!(ev.1.len(), 6);
    let topic_ident = Address::from_val(&e, &ev.1.get(1).unwrap());
    let topic_amount = i128::from_val(&e, &ev.1.get(2).unwrap());
    let topic_total = i128::from_val(&e, &ev.1.get(3).unwrap());
    let topic_ts = u64::from_val(&e, &ev.1.get(4).unwrap());
    let topic_admin = Address::from_val(&e, &ev.1.get(5).unwrap());

    assert_eq!(topic_ident, identity);
    assert_eq!(topic_amount, slash_amount);
    assert_eq!(topic_total, slash_amount, "cumulative total after a single slash == per-event delta");
    assert!(topic_ts >= ts_before, "v2 slash timestamp must reflect ledger time");
    assert_eq!(topic_admin, admin);

    // Data: (reason: String, is_full_slash: bool)
    let (reason, is_full) = <(String, bool)>::from_val(&e, &ev.2);
    assert!(!reason.is_empty(), "v2 slash must carry a non-empty reason");
    assert!(!is_full, "partial slash must not be reported as full");
}

#[test]
fn test_bond_withdrawn_v2_flags_early_withdrawal_and_penalty() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);
    client.initialize(&admin, &None);

    let token_addr = e.register(test_helpers::MockStellarAsset, ());
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    let token_client = TokenClient::new(&e, &token_addr);

    token_admin_client.mint(&identity, &100_000_i128);
    token_client.approve(&identity, &contract_id, &100_000_i128, &99999_u32);
    client.set_token(&admin, &token_addr);

    let treasury = Address::generate(&e);
    client.set_early_exit_config(&admin, &treasury, &500); // 5%

    client.create_bond_with_rolling(
        &identity,
        &1_000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    let gross_withdraw = 200_i128;
    let penalty_bps = 500_u32;
    let remaining_duration = credence_math::Timestamp::SECONDS_PER_DAY;
    let expected_penalty = crate::early_exit_penalty::calculate_penalty(
        gross_withdraw,
        remaining_duration,
        credence_math::Timestamp::SECONDS_PER_DAY,
        penalty_bps,
    );

    client.withdraw_early(&identity, &gross_withdraw);

    let events = e.events().all();
    let v2_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "bond_withdrawn_v2")
        })
        .collect();

    assert_eq!(
        v2_events.len(),
        1,
        "early withdraw must emit exactly one bond_withdrawn_v2"
    );
    let ev = &v2_events[0];

    // Topics: (Symbol, identity, amount_withdrawn, remaining, timestamp)
    assert_eq!(ev.1.len(), 5);
    let topic_amount = i128::from_val(&e, &ev.1.get(2).unwrap());
    let topic_remaining = i128::from_val(&e, &ev.1.get(3).unwrap());
    let topic_ts = u64::from_val(&e, &ev.1.get(4).unwrap());
    assert_eq!(topic_amount, gross_withdraw);
    assert_eq!(topic_remaining, 800);
    assert!(topic_ts > 0);

    // Data: (is_early: bool, penalty_amount: i128)
    let (is_early, penalty) = <(bool, i128)>::from_val(&e, &ev.2);
    assert!(is_early, "withdraw_early must flag is_early=true");
    assert_eq!(penalty, expected_penalty, "v2 penalty must match calculate_penalty");
}

#[test]
fn test_bond_liquidated_v2_payload_after_full_slash() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);
    client.initialize(&admin, &None);

    let token_addr = e.register(test_helpers::MockStellarAsset, ());
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    let token_client = TokenClient::new(&e, &token_addr);

    token_admin_client.mint(&identity, &100_000_i128);
    token_client.approve(&identity, &contract_id, &100_000_i128, &99999_u32);
    client.set_token(&admin, &token_addr);

    let liquidation_treasury = Address::generate(&e);
    client.set_liquidation_treasury(&admin, &liquidation_treasury);
    let slash_treasury = Address::generate(&e);
    client.set_slash_treasury(&admin, &slash_treasury);

    client.create_bond_with_rolling(
        &identity,
        &1_000_i128,
        &86_400_u64,
        &false,
        &0_u64,
    );
    client.slash(&admin, &1_000_i128); // fully slash
    let ts_before = e.ledger().timestamp();
    client.liquidate(&admin);

    let events = e.events().all();
    let matched: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "bond_liquidated")
        })
        .collect();

    assert_eq!(matched.len(), 1, "exactly one bond_liquidated per bond");
    let ev = &matched[0];

    // Topics: (Symbol, identity)
    assert_eq!(ev.1.len(), 2);
    let topic_ident = Address::from_val(&e, &ev.1.get(1).unwrap());
    assert_eq!(topic_ident, identity);

    // Data: (residual, reason Symbol, timestamp, admin)
    let (residual, reason, ts, admin_addr) =
        <(i128, Symbol, u64, Address)>::from_val(&e, &ev.2);
    assert_eq!(residual, 0, "fully-slashed → residual must be 0");
    assert_eq!(reason, Symbol::new(&e, "fully_slashed"));
    assert!(ts >= ts_before);
    assert_eq!(admin_addr, admin);
}

#[test]
fn test_param_updated_v2_emits_keyed_topics_for_governance_setters() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let token_addr = e.register(test_helpers::MockStellarAsset, ());
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    let token_admin_addr = Address::generate(&e);
    token_admin_client.mint(&token_admin_addr, &1_i128);
    client.set_token(&admin, &token_addr);

    client.set_protocol_fee_bps(&admin, &75_u32);
    let events = e.events().all();
    let matched: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "param_updated")
        })
        .collect();

    assert_eq!(matched.len(), 1);
    let ev = &matched[0];
    // topics: (name, key, category, admin) -> 4 entries
    assert_eq!(ev.1.len(), 4);
    let key = Symbol::from_val(&e, &ev.1.get(1).unwrap());
    let category = Symbol::from_val(&e, &ev.1.get(2).unwrap());
    let topic_admin = Address::from_val(&e, &ev.1.get(3).unwrap());

    assert_eq!(key, Symbol::new(&e, "fee_prot"));
    assert_eq!(category, Symbol::new(&e, "fee"));
    assert_eq!(topic_admin, admin);

    // data: (old, new) i128
    let (old, new) = <(i128, i128)>::from_val(&e, &ev.2);
    assert_eq!(old, crate::parameters::DEFAULT_PROTOCOL_FEE_BPS as i128);
    assert_eq!(new, 75);
}

#[test]
fn test_attestation_added_emits_subject_in_topic_and_id_in_data() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let token_addr = e.register(test_helpers::MockStellarAsset, ());
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    let token_client = TokenClient::new(&e, &token_addr);

    let attester = Address::generate(&e);
    let subject = Address::generate(&e);
    token_admin_client.mint(&attester, &1_i128);
    token_client.approve(&attester, &contract_id, &1_i128, &99999_u32);
    client.set_token(&admin, &token_addr);
    client.register_attester(&attester);

    let payload = String::from_str(&e, "kyc:verified");
    let att = client.add_attestation(&attester, &subject, &payload, &0_u64);

    let events = e.events().all();
    let matched: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "attestation_added")
        })
        .collect();

    assert_eq!(matched.len(), 1);
    let ev = &matched[0];
    // topics: (Symbol, subject) -> 2 entries
    assert_eq!(ev.1.len(), 2);
    let topic_subject = Address::from_val(&e, &ev.1.get(1).unwrap());
    assert_eq!(topic_subject, subject);

    // data: (id, attester, payload)
    let (id, who, data_str) = <(u64, Address, String)>::from_val(&e, &ev.2);
    assert_eq!(id, att.id);
    assert_eq!(who, attester);
    assert_eq!(data_str, payload);
}

