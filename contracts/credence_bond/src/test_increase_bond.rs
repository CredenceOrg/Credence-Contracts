#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger};
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

    client.initialize(&admin, &None);

    let token_id = e.register(test_helpers::MockStellarAsset, ());
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

    // Approve enough for both create_bond (1000) and top_up (500)
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    let before_user = token_client.balance(&identity);
    let before_contract = token_client.balance(&contract_id);

    let updated = client.top_up(&identity, &500_i128);

    assert_eq!(updated.bonded_amount, 1500);
    assert_eq!(token_client.balance(&identity), before_user - 500);
    assert_eq!(token_client.balance(&contract_id), before_contract + 500);
}

#[test]
#[should_panic(expected = "no bond")]
fn test_increase_bond_fails_without_token_configuration() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);
    client.top_up(&identity, &10_i128);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_increase_bond_fails_for_non_owner() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    let stranger = Address::generate(&e);

    // Approve only enough for create_bond from the real bond owner.
    // Stranger allowance must not be used for the bond owner's top-up.
    token_client.approve(&identity, &contract_id, &1000_i128, &1000_u32);
    token_client.approve(&stranger, &contract_id, &500_i128, &1000_u32);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    client.top_up(&identity, &500_i128);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_increase_bond_rejects_zero_amount() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // Approve for create_bond
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    client.top_up(&identity, &0_i128);
}

#[test]
#[should_panic(expected = "bond increase caused overflow")]
fn test_increase_bond_overflow_protection() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // First create a bond with a normal amount
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);
    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    // Now try to increase by i128::MAX - this should cause overflow
    token_client.approve(&identity, &contract_id, &i128::MAX, &1000_u32);

    client.top_up(&identity, &i128::MAX);
}

#[test]
#[should_panic(expected = "HostError")]
fn test_increase_bond_fails_without_allowance() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // Approve for create_bond only
    token_client.approve(&identity, &contract_id, &1000_i128, &1000_u32);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    // No approval for top_up - should fail
    client.top_up(&identity, &500_i128);
}

#[test]
fn test_increase_bond_emits_event() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    // Approve for create_bond (1000) and top_up (250)
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    let _ = client.top_up(&identity, &250_i128);

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

    // Approve for create_bond (1000) and top_up (150)
    token_client.approve(&identity, &contract_id, &2000_i128, &1000_u32);

    let original =
        client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &true, &7200_u64);

    let updated = client.top_up(&identity, &150_i128);

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

// ── lifecycle edge-cases (issue #284) ────────────────────────────────────────

/// top-up preserves bond_start and bond_duration (time fields must not change).
#[test]
fn test_increase_bond_preserves_time_fields() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let (client, contract_id, identity, token_client) = setup(&e);

    token_client.approve(&identity, &contract_id, &3_000_i128, &1_000_u32);
    let original =
        client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);

    // Advance time before top-up
    e.ledger().with_mut(|li| li.timestamp = 2_000_000);
    let updated = client.top_up(&identity, &500_i128);

    assert_eq!(
        updated.bond_start, original.bond_start,
        "bond_start must not change on top-up"
    );
    assert_eq!(
        updated.bond_duration, original.bond_duration,
        "bond_duration must not change on top-up"
    );
}

/// top-up on a rolling bond preserves is_rolling and notice_period_duration.
#[test]
fn test_increase_bond_preserves_rolling_fields() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    token_client.approve(&identity, &contract_id, &3_000_i128, &1_000_u32);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &true, &3_600_u64);

    let updated = client.top_up(&identity, &500_i128);

    assert!(
        updated.is_rolling,
        "is_rolling must be preserved after top-up"
    );
    assert_eq!(
        updated.notice_period_duration, 3_600,
        "notice_period_duration must be preserved"
    );
}

/// top-up respects supply cap — pushing total over cap must panic.
#[test]
#[should_panic(expected = "supply cap exceeded")]
fn test_increase_bond_respects_supply_cap() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    token_client.approve(&identity, &contract_id, &5_000_i128, &1_000_u32);
    let admin = soroban_sdk::Address::generate(&e);
    // Re-initialize with admin to set cap — use mock_all_auths already active
    client.set_supply_cap(&admin, &1_200_i128);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    // total=1_000, cap=1_200 → top-up of 300 would push to 1_300 > 1_200
    client.top_up(&identity, &300_i128);
}

/// top-up of exactly 1 (minimum positive) is accepted.
#[test]
fn test_increase_bond_minimum_positive_amount_accepted() {
    let e = Env::default();
    let (client, contract_id, identity, token_client) = setup(&e);

    token_client.approve(&identity, &contract_id, &2_000_i128, &1_000_u32);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    let updated = client.top_up(&identity, &1_i128);
    assert_eq!(updated.bonded_amount, 1_001);
}

/// slashed_amount is unchanged after a top-up.
#[test]
fn test_increase_bond_does_not_clear_slashed_amount() {
    use crate::test_helpers;
    let e = Env::default();
    let (client, admin, identity, _token_id, contract_id) = test_helpers::setup_with_token(&e);

    let token_client = soroban_sdk::token::Client::new(&e, &_token_id);
    let expiry = e.ledger().sequence().saturating_add(10_000);
    token_client.approve(&identity, &contract_id, &5_000_i128, &expiry);

    client.create_bond_with_rolling(&identity, &2_000_i128, &86_400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &500);

    let before = client.get_identity_state();
    assert_eq!(before.slashed_amount, 500);

    client.top_up(&identity, &1_000_i128);
    let after = client.get_identity_state();
    assert_eq!(
        after.slashed_amount, 500,
        "top-up must not clear slashed_amount"
    );
    assert_eq!(after.bonded_amount, 3_000);
}
