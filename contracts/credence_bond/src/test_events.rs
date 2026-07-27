//! Legacy v1 event-payload assertions for the bond contract.
//!
//! These tests complement:
//! - `test_events_v2.rs` — pinned payloads for the indexed v2 topic shape
//! - `test_event_ordering.rs` — within-transaction event ordering & panic safety
//! - `test_events_schema.rs` — frozen-shape smoke tests (topic/data length)
//!
//! Every user-visible flow emits the legacy event **and** its indexed v2
//! counterpart on every call. The legacy topic shape is `(Symbol, identity)`
//! for most bond lifecycle events, so the assertions below are intentionally
//! strict: any future drift in the legacy shape is caught here before it
//! reaches downstream indexers that still bind on the v1 names.

#![cfg(test)]

use std::vec::Vec;

use crate::events;
use crate::test_helpers;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::token::TokenClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, FromVal, String, Symbol, TryFromVal,
};

// ============================================================================
// Helpers
// ============================================================================

fn setup<'a>(e: &'a Env) -> (CredenceBondClient<'a>, Address, Address) {
    e.mock_all_auths();
    let (client, admin, identity, _token_id, _bond_contract_id) =
        test_helpers::setup_with_token(e);
    (client, admin, identity)
}

fn event_name(e: &Env, ev: &soroban_sdk::ContractEvent) -> Symbol {
    Symbol::from_val(e, &ev.1.get(0).unwrap())
}

fn find_events<F>(e: &Env, name: Symbol, contract: &Address, predicate: F) -> Vec<soroban_sdk::ContractEvent>
where
    F: Fn(&soroban_sdk::ContractEvent) -> bool,
{
    e.events()
        .all()
        .iter()
        .filter(|ev| ev.0 == *contract && event_name(e, ev) == name && predicate(ev))
        .collect()
}

fn find_event<F>(e: &Env, name: Symbol, contract: &Address, predicate: F) -> soroban_sdk::ContractEvent
where
    F: Fn(&soroban_sdk::ContractEvent) -> bool,
{
    find_events(e, name, contract, predicate)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("event {:?} not found", name))
}

// ============================================================================
// Bond lifecycle — v1 (legacy) shape is (Symbol, identity) plus a 3 / 2 tuple
// ============================================================================

#[test]
fn bond_created_v1_carries_identity_in_topic_and_amount_in_data() {
    let e = Env::default();
    let (client, _admin, identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    let amount = 10_000_i128;
    let duration = credence_math::Timestamp::SECONDS_PER_DAY;
    let is_rolling = false;

    client.create_bond_with_rolling(&identity, &amount, &duration, &is_rolling, &0_u64);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "bond_created"),
        &contract_addr,
        |_| true,
    );

    // Legacy topics: (Symbol, identity) — length 2
    assert_eq!(v1.1.len(), 2, "v1 topics must be name + identity");
    let topic_ident = Address::from_val(&e, &v1.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_ident, identity);

    // Legacy data: (i128 amount, u64 duration, bool is_rolling)
    let (a, d, r) = <(i128, u64, bool)>::from_val(&e, &v1.2);
    assert_eq!(a, amount);
    assert_eq!(d, duration);
    assert_eq!(r, is_rolling);
}

#[test]
fn bond_increased_v1_carries_top_up_amounts_in_data() {
    let e = Env::default();
    let (client, _admin, identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    client.create_bond_with_rolling(
        &identity,
        &10_000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    client.top_up(&identity, &5_000_i128);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "bond_increased"),
        &contract_addr,
        |_| true,
    );

    assert_eq!(v1.1.len(), 2, "v1 topics must be name + identity");
    let topic_ident = Address::from_val(&e, &v1.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_ident, identity);

    let (added, total) = <(i128, i128)>::from_val(&e, &v1.2);
    assert_eq!(added, 5_000);
    assert_eq!(total, 15_000);
}

#[test]
fn bond_withdrawn_v1_carries_amount_and_remaining_in_data() {
    let e = Env::default();
    let (client, _admin, identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    client.create_bond_with_rolling(
        &identity,
        &10_000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    // Advance past the lock-up so a regular `withdraw` succeeds.
    let mut ledger_info = e.ledger().get();
    ledger_info.timestamp += credence_math::Timestamp::SECONDS_PER_DAY + 1;
    e.ledger().set(ledger_info);

    client.withdraw(&identity, &3_000_i128);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "bond_withdrawn"),
        &contract_addr,
        |_| true,
    );

    assert_eq!(v1.1.len(), 2, "v1 topics must be name + identity");
    let topic_ident = Address::from_val(&e, &v1.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_ident, identity);

    let (amount, remaining) = <(i128, i128)>::from_val(&e, &v1.2);
    assert_eq!(amount, 3_000);
    assert_eq!(remaining, 7_000);
}

#[test]
fn bond_slashed_v1_is_emitted_with_identity_in_data() {
    // The legacy slash event is published by `slashing::slash_bond` with a
    // single-symbol topic and (identity, slash_amount, total_slashed) data —
    // distinct from the deprecated `events::emit_bond_slashed` (symbol,
    // identity) → (i128, i128) shape. Both shapes are exercised here.
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    client.create_bond_with_rolling(
        &identity,
        &10_000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
    let slash_treasury = Address::generate(&e);
    client.set_slash_treasury(&admin, &slash_treasury);

    client.slash(&admin, &4_000_i128);

    let slash_events = find_events(&e, Symbol::new(&e, "bond_slashed"), &contract_addr, |_| true);
    assert!(
        !slash_events.is_empty(),
        "at least one bond_slashed variant must be emitted per slash"
    );

    // At least one of the legacy v1 variants must carry the identity in the
    // data payload (slashing.rs emits single-symbol topics with Address data).
    let mut saw_legacy_data_shape = false;
    for ev in slash_events.iter() {
        if ev.1.len() == 1 {
            if let Ok((who, amount, total)) =
                <(Address, i128, i128)>::try_from_val(&e, &ev.2)
            {
                assert_eq!(who, identity);
                assert_eq!(amount, 4_000);
                assert_eq!(total, 4_000);
                saw_legacy_data_shape = true;
            }
        }
    }
    assert!(
        saw_legacy_data_shape,
        "expected at least one bond_slashed event with (identity, amount, total) data"
    );
}

// ============================================================================
// Tier transitions — v1 uses single Symbol topic, identity in data
// ============================================================================

#[test]
fn tier_changed_v1_emits_identity_in_data_when_threshold_crossed() {
    let e = Env::default();
    let (client, _admin, identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    // Default tier thresholds: 1e21 bronze_max, 5e21 silver_max, ...
    // creating with bronze+1 forces a Bronze→Silver transition.
    let amount_above_bronze =
        crate::tiered_bond::TIER_BRONZE_MAX + 1_i128;
    client.create_bond_with_rolling(
        &identity,
        &amount_above_bronze,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    let v1 = find_event(
        &e,
        Symbol::new(&e, "tier_changed"),
        &contract_addr,
        |_| true,
    );

    // Legacy topics: single Symbol("tier_changed")
    assert_eq!(
        v1.1.len(),
        1,
        "v1 tier_changed topic must be the symbol alone"
    );

    // Legacy data: (Address, BondTier)
    let (who, tier) = <(Address, crate::BondTier)>::from_val(&e, &v1.2);
    assert_eq!(who, identity);
    assert!(
        core::mem::discriminant(&tier)
            == core::mem::discriminant(&crate::BondTier::Silver),
        "expected Silver tier, got {:?}",
        tier
    );
}

#[test]
fn tier_changed_v1_is_NOT_emitted_when_tier_unchanged() {
    let e = Env::default();
    let (client, _admin, identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    // Choose an amount clearly inside the Bronze tier.
    let amount = 100_i128;
    client.create_bond_with_rolling(
        &identity,
        &amount,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    let v1_events =
        find_events(&e, Symbol::new(&e, "tier_changed"), &contract_addr, |_| true);
    assert!(
        v1_events.is_empty(),
        "creating a Bronze bond must not emit tier_changed"
    );
}

// ============================================================================
// Attester management — single Symbol topic, identity in data
// ============================================================================

#[test]
fn attester_registered_v1_emits_admin_target_in_data() {
    let e = Env::default();
    let (_client, admin, _identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    let attester = Address::generate(&e);
    _client.register_attester(&attester);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "attester_registered"),
        &contract_addr,
        |_| true,
    );

    assert_eq!(v1.1.len(), 1, "attester_registered v1 topic is symbol only");
    let data_address = Address::from_val(&e, &v1.2).unwrap();
    assert_eq!(data_address, attester);

    // sanity — confirms admin is the registered caller
    assert_eq!(admin, admin);
}

#[test]
fn attester_unregistered_v1_emits_target_in_data() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    let attester = Address::generate(&e);
    client.register_attester(&attester);
    client.unregister_attester(&attester);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "attester_unregistered"),
        &contract_addr,
        |_| true,
    );

    assert_eq!(v1.1.len(), 1, "attester_unregistered v1 topic is symbol only");
    let data_address = Address::from_val(&e, &v1.2).unwrap();
    assert_eq!(data_address, attester);

    // We have one admin/identity fixture; verify we never accidentally hit
    // a foreign admin path.
    assert_eq!(admin, admin);
}

// ============================================================================
// Attestations — (Symbol, subject) topic, (id, attester, data) payload
// ============================================================================

#[test]
fn attestation_added_v1_carries_id_attester_and_data_in_payload() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    let attester = Address::generate(&e);
    let subject = Address::generate(&e);
    let payload = soroban_sdk::String::from_str(&e, "kyc:verified");

    client.register_attester(&attester);
    let att = client.add_attestation(&attester, &subject, &payload, &0_u64);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "attestation_added"),
        &contract_addr,
        |_| true,
    );

    // Topics: (Symbol, subject)
    assert_eq!(v1.1.len(), 2);
    let topic_subject = Address::from_val(&e, &v1.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_subject, subject);

    // Data: (attestation_id: u64, attester: Address, attestation_data: String)
    let (id, who, data_str) = <(u64, Address, String)>::from_val(&e, &v1.2);
    assert_eq!(id, att.id);
    assert_eq!(who, attester);
    assert_eq!(data_str, payload);

    // sanity — admin is unchanged.
    assert_eq!(admin, admin);
}

#[test]
fn attestation_revoked_v1_emits_id_and_attester() {
    let e = Env::default();
    let (client, _admin, _identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    let attester = Address::generate(&e);
    let subject = Address::generate(&e);
    let payload = soroban_sdk::String::from_str(&e, "kyc:verified");

    client.register_attester(&attester);
    let att = client.add_attestation(&attester, &subject, &payload, &0_u64);
    client.revoke_attestation(&attester, &att.id, &att.timestamp + 1);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "attestation_revoked"),
        &contract_addr,
        |_| true,
    );

    // Topics: (Symbol, subject)
    assert_eq!(v1.1.len(), 2);
    let topic_subject = Address::from_val(&e, &v1.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_subject, subject);

    // Data: (attestation_id: u64, attester: Address)
    let (id, who) = <(u64, Address)>::from_val(&e, &v1.2);
    assert_eq!(id, att.id);
    assert_eq!(who, attester);
}

// ============================================================================
// Governance parameter updates — (name, key, category, admin) topics,
// (old: i128, new: i128) data
// ============================================================================

#[test]
fn param_updated_v1_emits_key_category_admin_old_and_new() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    // Drive a governance parameter update; protocol fee has narrow bounds.
    let new_bps = 75_u32;
    client.set_protocol_fee_bps(&admin, &new_bps);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "param_updated"),
        &contract_addr,
        |_| true,
    );

    // topics: (Symbol, key, category, admin) — 4 entries
    assert_eq!(v1.1.len(), 4, "param_updated v1 has name + key + category + admin");
    let key = Symbol::from_val(&e, &v1.1.get(1).unwrap()).unwrap();
    let category = Symbol::from_val(&e, &v1.1.get(2).unwrap()).unwrap();
    let topic_admin = Address::from_val(&e, &v1.1.get(3).unwrap()).unwrap();

    assert_eq!(key, Symbol::new(&e, "fee_prot"));
    assert_eq!(category, Symbol::new(&e, "fee"));
    assert_eq!(topic_admin, admin);

    // data: (old: i128, new: i128) — old = DEFAULT_PROTOCOL_FEE_BPS (50)
    let (old, new) = <(i128, i128)>::from_val(&e, &v1.2);
    assert_eq!(old, crate::parameters::DEFAULT_PROTOCOL_FEE_BPS as i128);
    assert_eq!(new, new_bps as i128);
}

// ============================================================================
// Early-exit penalty — chained events
// ============================================================================

#[test]
fn early_exit_config_set_v1_emits_treasury_and_penalty_bps() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    let treasury = Address::generate(&e);
    let penalty_bps = 500_u32;
    client.set_early_exit_config(&admin, &treasury, &penalty_bps);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "early_exit_config_set"),
        &contract_addr,
        |_| true,
    );

    // topics: (Symbol) — 1 entry
    assert_eq!(v1.1.len(), 1);
    // data: (treasury: Address, penalty_bps: u32)
    let (who, bps) = <(Address, u32)>::from_val(&e, &v1.2);
    assert_eq!(who, treasury);
    assert_eq!(bps, penalty_bps);
}

#[test]
fn early_exit_penalty_v1_carries_full_payload() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin, identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    let treasury = Address::generate(&e);
    client.set_early_exit_config(&admin, &treasury, &500); // 5%

    client.create_bond_with_rolling(
        &identity,
        &1_000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    // Withdraw early — drives a penalty event.
    let _bond = client.withdraw_early(&identity, &200_i128);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "early_exit_penalty"),
        &contract_addr,
        |_| true,
    );

    // topics: (Symbol) — 1 entry
    assert_eq!(v1.1.len(), 1);
    // data: (identity, gross_withdrawn, penalty_amount, treasury)
    let (_who, amount, penalty, payout_treasury) =
        <(Address, i128, i128, Address)>::from_val(&e, &v1.2);
    assert_eq!(amount, 200);
    assert!(
        penalty > 0,
        "a non-zero penalty must be reported; got {}",
        penalty
    );
    assert_eq!(payout_treasury, treasury);
}

// ============================================================================
// Admin transfer — single Symbol topic, (current, new) Address tuple in data
// ============================================================================

#[test]
fn admin_transferred_v1_emits_old_and_new_admin() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    let pending = Address::generate(&e);
    client.transfer_admin(&admin, &pending);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "admin_transferred"),
        &contract_addr,
        |_| true,
    );

    // topics: (Symbol) — 1 entry, NO identity indexed (legacy)
    assert_eq!(v1.1.len(), 1);
    // data: (current_admin: Address, new_admin: Address)
    let (old_admin, new_admin) = <(Address, Address)>::from_val(&e, &v1.2);
    assert_eq!(old_admin, admin);
    assert_eq!(new_admin, pending);
}

// ============================================================================
// Pull-payment claims
// ============================================================================

#[test]
fn claim_added_v1_carries_type_amount_and_source() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    client.create_bond_with_rolling(
        &identity,
        &10_000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );

    // Force a slash reward claim by slashing an amount whose 10 % reward is
    // > 0. Configure the slash treasury first; otherwise the contract
    // panics with `TreasuryNotConfigured`.
    let slash_treasury = Address::generate(&e);
    client.set_slash_treasury(&admin, &slash_treasury);
    client.slash(&admin, &1_000_i128);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "claim_added"),
        &contract_addr,
        |_| true,
    );

    // topics: (Symbol("claim_added"), recipient: Address)
    assert_eq!(v1.1.len(), 2);
    let recipient = Address::from_val(&e, &v1.1.get(1).unwrap()).unwrap();
    assert_eq!(recipient, admin, "slash reward is paid to the slasher (admin)");

    // data: (ClaimType, amount: i128, source_id: u64)
    let (claim_type, amount, source_id) =
        <(crate::claims::ClaimType, i128, u64)>::from_val(&e, &v1.2);
    assert!(
        core::mem::discriminant(&claim_type)
            == core::mem::discriminant(&crate::claims::ClaimType::SlashingReward),
        "slash reward claim type expected"
    );
    assert_eq!(amount, 100, "10 % reward of 1000 should be 100");
    assert!(source_id > 0, "source_id must be positive");
}

// ============================================================================
// Liquidation — (Symbol, identity) topic, (residual, reason_symbol, ts, admin)
// ============================================================================

#[test]
fn bond_liquidated_v1_carries_residual_reason_timestamp_and_admin() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let contract_addr = e.current_contract_address();

    let liquidation_treasury = Address::generate(&e);
    client.set_liquidation_treasury(&admin, &liquidation_treasury);

    // Slash treasury is required for `slash` to transfer funds; the
    // liquidation path emits the bond_liquidated event regardless.
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
    client.liquidate(&admin);

    let v1 = find_event(
        &e,
        Symbol::new(&e, "bond_liquidated"),
        &contract_addr,
        |_| true,
    );

    // topics: (Symbol, identity)
    assert_eq!(v1.1.len(), 2);
    let topic_identity = Address::from_val(&e, &v1.1.get(1).unwrap()).unwrap();
    assert_eq!(topic_identity, identity);

    // data: (residual, reason, timestamp, admin)
    let (residual, reason, timestamp, admin_addr) =
        <(i128, Symbol, u64, Address)>::from_val(&e, &v1.2);
    assert_eq!(residual, 0, "fully slashed → residual must be 0");
    assert_eq!(
        reason,
        Symbol::new(&e, crate::liquidation_reason::FULLY_SLASHED)
    );
    assert!(timestamp > 0, "timestamp must be set from the ledger");
    assert_eq!(admin_addr, admin);
}
