//! Tests for same-ledger collateral increase vs slashing guard (#169).

use crate::test_helpers;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::Env;

#[test]
#[should_panic(expected = "slash blocked: collateral increased in this ledger")]
fn test_slash_same_ledger_after_increase_bond_rejected() {
    let e = Env::default();
    let (client, admin, identity, _token, _id) = test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);
    client.top_up(&1_000_i128);
    client.slash(&admin, &100_i128);
}

#[test]
fn test_slash_next_ledger_after_increase_bond_allowed() {
    let e = Env::default();
    let (client, admin, identity, _token, _id) = test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);
    client.top_up(&1_000_i128);
    test_helpers::advance_ledger_sequence(&e);
    let bond = client.slash(&admin, &100_i128);
    assert_eq!(bond.slashed_amount, 100);
    assert_eq!(bond.bonded_amount, 11_000);
}

#[test]
#[should_panic(expected = "slash blocked: collateral increased in this ledger")]
fn test_slash_same_ledger_after_top_up_rejected() {
    let e = Env::default();
    let (client, admin, identity, _token, _id) = test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.top_up(&5_000_i128);
    client.slash(&admin, &100_i128);
}

#[test]
fn test_slash_next_ledger_after_create_bond_allowed() {
    let e = Env::default();
    let (client, admin, identity, _token, _id) = test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    let bond = client.slash(&admin, &200_i128);
    assert_eq!(bond.slashed_amount, 200);
}

#[test]
fn test_withdraw_unaffected_after_create_same_ledger() {
    let e = Env::default();
    let (client, _admin, identity, _token, _id) = test_helpers::setup_with_token(&e);
    let duration = 86_400_u64;
    client.create_bond_with_rolling(&identity, &10_000_i128, &duration, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp += duration + 1);
    let bond = client.withdraw(&1_000_i128);
    assert_eq!(bond.bonded_amount, 9_000);
}
