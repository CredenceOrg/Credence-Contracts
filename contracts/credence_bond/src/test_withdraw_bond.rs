//! Comprehensive unit tests for bond withdrawal flows.
//! Scenarios covered:
//! - successful withdrawal
//! - partial withdrawal
//! - insufficient balance rejection
//! - early-withdraw path rejection after lock-up
//! - cooldown/notice-period enforcement helper behavior

use crate::{rolling_bond, CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_withdraw_bond_successful() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);

    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);
    let bond = client.withdraw(&1000_i128);

    assert_eq!(bond.bonded_amount, 0);
    assert_eq!(bond.slashed_amount, 0);
}

#[test]
fn test_withdraw_bond_partial_withdrawal() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);

    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);
    let bond = client.withdraw(&400_i128);

    assert_eq!(bond.bonded_amount, 600);
    assert_eq!(bond.slashed_amount, 0);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdraw_bond_insufficient_balance() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);

    client.create_bond(&identity, &500_i128, &100_u64, &false, &0_u64);
    client.withdraw(&501_i128);
}

#[test]
#[should_panic(expected = "use withdraw for post lock-up")]
fn test_withdraw_bond_early_withdrawal_rejection() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);

    let (client, admin) = setup(&e);
    let treasury = Address::generate(&e);
    let identity = Address::generate(&e);

    // Configure penalty so early withdraw path is active.
    client.set_early_exit_config(&admin, &treasury, &500);
    client.create_bond(&identity, &1000_i128, &100_u64, &false, &0_u64);

    // Advance past lock-up and ensure early path is rejected.
    e.ledger().with_mut(|li| li.timestamp = 1_101);
    client.withdraw_early(&100_i128);
}

#[test]
fn test_withdraw_bond_cooldown_enforcement_helper() {
    let requested_at = 1_000_u64;
    let notice = 50_u64;

    // During cooldown.
    assert!(!rolling_bond::can_withdraw_after_notice(
        1_049,
        requested_at,
        notice
    ));
    // Exactly at cooldown end.
    assert!(rolling_bond::can_withdraw_after_notice(
        1_050,
        requested_at,
        notice
    ));
    // Well after cooldown.
    assert!(rolling_bond::can_withdraw_after_notice(
        1_500,
        requested_at,
        notice
    ));
}

#[test]
fn test_withdraw_bond_cooldown_requires_request() {
    // If no withdrawal request was made, cooldown must not pass.
    assert!(!rolling_bond::can_withdraw_after_notice(2_000, 0, 30));
}

#[test]
fn test_withdraw_bond_exact_available_after_slash() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let identity = Address::generate(&e);

    client.create_bond(&identity, &1_000_i128, &100_u64, &false, &0_u64);
    client.slash(&admin, &250_i128);

    let bond = client.withdraw(&750_i128);
    assert_eq!(bond.bonded_amount, 250);
    assert_eq!(bond.slashed_amount, 250);
}
