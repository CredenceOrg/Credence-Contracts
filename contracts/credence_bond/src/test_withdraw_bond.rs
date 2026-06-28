//! Comprehensive tests for withdraw_bond functionality.
//! Covers: lock-up enforcement, cooldown (notice period), partial withdrawals,
//! insufficient balance, slashing interaction, and edge cases.

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::TokenClient;
use soroban_sdk::{contract, contractimpl, Address, Env};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn setup_with_token(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    test_helpers::setup_with_token(e)
}

mod failing_withdraw_callback {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    pub struct FailingWithdrawCallback;

    #[contractimpl]
    impl FailingWithdrawCallback {
        pub fn on_withdraw(_e: Env, _amount: i128) {
            panic!("callback failed");
        }
    }
}

#[test]
fn test_withdraw_bond_callback_failure_reverts_state() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity, token_id, bond_contract_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let callback_id = e.register(failing_withdraw_callback::FailingWithdrawCallback, ());
    client.set_callback(&admin, &callback_id);

    let before_bond = client.get_identity_state();
    let token_client = TokenClient::new(&e, &token_id);
    let before_identity_balance = token_client.balance(&identity);
    let before_contract_balance = token_client.balance(&bond_contract_id);

    let result = catch_unwind(AssertUnwindSafe(|| {
        client.withdraw_bond(&500);
    }));
    assert!(result.is_err());

    let after_bond = client.get_identity_state();
    assert_eq!(after_bond.bonded_amount, before_bond.bonded_amount);
    assert_eq!(after_bond.slashed_amount, before_bond.slashed_amount);
    assert_eq!(token_client.balance(&identity), before_identity_balance);
    assert_eq!(
        token_client.balance(&bond_contract_id),
        before_contract_balance
    );
}
#[test]
fn test_withdraw_bond_after_lockup_non_rolling() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    e.ledger().with_mut(|li| li.timestamp = 87401);
    let bond = client.withdraw_bond(&500);
    assert_eq!(bond.bonded_amount, 500);
}

#[test]
#[should_panic(expected = "lock-up period not elapsed; use withdraw_early")]
fn test_withdraw_bond_before_lockup_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    e.ledger().with_mut(|li| li.timestamp = 44200);
    client.withdraw_bond(&500);
}

#[test]
#[should_panic(expected = "cooldown window not elapsed; request_withdrawal first")]
fn test_withdraw_bond_rolling_before_notice_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    e.ledger().with_mut(|li| li.timestamp = 1101);

    client.withdraw_bond(&500);
}

#[test]
#[should_panic(expected = "cooldown window not elapsed; request_withdrawal first")]
fn test_withdraw_bond_rolling_before_cooldown_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    client.request_withdrawal(&identity);
    e.ledger().with_mut(|li| li.timestamp = 1005);

    client.withdraw_bond(&500);
}

#[test]
fn test_withdraw_bond_rolling_after_cooldown() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &true, &10_u64);
    client.request_withdrawal(&identity);
    e.ledger().with_mut(|li| li.timestamp = 1011);

    let bond = client.withdraw_bond(&500);
    assert_eq!(bond.bonded_amount, 500);
}

#[test]
fn test_withdraw_bond_partial_withdrawal() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw_bond(&300);
    assert_eq!(bond.bonded_amount, 700);
    let bond = client.withdraw_bond(&200);
    assert_eq!(bond.bonded_amount, 500);
    let bond = client.withdraw_bond(&500);
    assert_eq!(bond.bonded_amount, 0);
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdraw_bond_insufficient_balance() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    client.withdraw_bond(&1001);
}

#[test]
fn test_withdraw_bond_after_slash() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &400);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw_bond(&600);
    assert_eq!(bond.bonded_amount, 400);
    assert_eq!(bond.slashed_amount, 400);
}

#[test]
fn test_withdraw_bond_zero_amount() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw_bond(&0);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_withdraw_bond_full_withdrawal() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, token_id, bond_contract_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw_bond(&1000);
    assert_eq!(bond.bonded_amount, 0);

    let token_client = TokenClient::new(&e, &token_id);
    let balance = token_client.balance(&bond_contract_id);
    assert_eq!(balance, 0);
}

#[test]
fn test_withdraw_alias_calls_withdraw_bond() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87401);

    let bond = client.withdraw(&identity, &500);
    assert_eq!(bond.bonded_amount, 500);
}

// ── lifecycle edge-cases (issue #284) ────────────────────────────────────────

/// Withdraw at exactly the lockup boundary (bond_start + duration) succeeds.
#[test]
fn test_withdraw_bond_at_exact_lockup_boundary() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    // bond_start=1_000, duration=86_400 → end=87_400; at exactly end it should succeed
    e.ledger().with_mut(|li| li.timestamp = 87_400);
    let bond = client.withdraw_bond(&500);
    assert_eq!(bond.bonded_amount, 500);
}

/// Withdraw one second before lockup boundary must panic.
#[test]
#[should_panic(expected = "lock-up period not elapsed; use withdraw_early")]
fn test_withdraw_bond_one_second_before_lockup_boundary_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    // end = 87_400; one second before = 87_399
    e.ledger().with_mut(|li| li.timestamp = 87_399);
    client.withdraw_bond(&500);
}

/// withdraw_bond decrements total supply by the withdrawn amount.
#[test]
fn test_withdraw_bond_decrements_total_supply() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    assert_eq!(client.get_total_supply(), 1_000);

    e.ledger().with_mut(|li| li.timestamp = 87_401);
    client.withdraw_bond(&400);
    assert_eq!(client.get_total_supply(), 600);

    client.withdraw_bond(&600);
    assert_eq!(client.get_total_supply(), 0);
}

/// withdraw_bond with amount=0 is a no-op (bonded_amount unchanged).
#[test]
fn test_withdraw_bond_zero_amount_is_noop() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87_401);

    let bond = client.withdraw_bond(&0);
    assert_eq!(
        bond.bonded_amount, 1_000,
        "zero-amount withdraw must be a no-op"
    );
    assert_eq!(client.get_total_supply(), 1_000);
}

/// withdraw_bond with negative amount must panic.
#[test]
#[should_panic(expected = "amount must be non-negative")]
fn test_withdraw_bond_negative_amount_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 87_401);
    client.withdraw_bond(&(-1_i128));
}

/// withdraw_early before lockup succeeds and applies penalty.
#[test]
fn test_withdraw_early_before_lockup_applies_penalty() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    let treasury = soroban_sdk::Address::generate(&e);
    client.set_early_exit_config(&admin, &treasury, &500_u32); // 5% penalty

    client.create_bond_with_rolling(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);
    // Still within lockup
    e.ledger().with_mut(|li| li.timestamp = 44_200);
    let bond = client.withdraw_early(&identity, &5_000_i128);
    // bonded_amount reduced by 5_000
    assert_eq!(bond.bonded_amount, 5_000);
}

/// withdraw_early after lockup expires must panic (use withdraw instead).
#[test]
#[should_panic(expected = "use withdraw for post lock-up")]
fn test_withdraw_early_after_lockup_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    let treasury = soroban_sdk::Address::generate(&e);
    client.set_early_exit_config(&admin, &treasury, &500_u32);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    // Past lockup end
    e.ledger().with_mut(|li| li.timestamp = 90_000);
    client.withdraw_early(&identity, &500_i128);
}

/// withdraw_early with no early-exit config set must panic.
#[test]
#[should_panic(expected = "early exit config not set")]
fn test_withdraw_early_without_config_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    // Still within lockup, but no early exit config
    e.ledger().with_mut(|li| li.timestamp = 44_200);
    client.withdraw_early(&identity, &500_i128);
}

/// withdraw_early with amount exceeding available balance must panic.
#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_withdraw_early_insufficient_balance_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    let treasury = soroban_sdk::Address::generate(&e);
    client.set_early_exit_config(&admin, &treasury, &500_u32);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 44_200);
    client.withdraw_early(&identity, &9_999_i128); // more than bonded
}

/// withdraw_bond on a rolling bond at exact cooldown boundary succeeds.
#[test]
fn test_withdraw_bond_rolling_at_exact_cooldown_boundary() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    // notice_period = 100 seconds
    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &true, &100_u64);
    client.request_withdrawal(&identity);
    // requested_at = 1_000; can withdraw at 1_000 + 100 = 1_100
    e.ledger().with_mut(|li| li.timestamp = 1_100);
    let bond = client.withdraw_bond(&500);
    assert_eq!(bond.bonded_amount, 500);
}

/// withdraw_bond on a rolling bond one second before cooldown boundary must panic.
#[test]
#[should_panic(expected = "cooldown window not elapsed; request_withdrawal first")]
fn test_withdraw_bond_rolling_one_second_before_cooldown_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, _token_id, _bond_id) = setup_with_token(&e);

    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &true, &100_u64);
    client.request_withdrawal(&identity);
    // one second before cooldown expires
    e.ledger().with_mut(|li| li.timestamp = 1_099);
    client.withdraw_bond(&500);
}
