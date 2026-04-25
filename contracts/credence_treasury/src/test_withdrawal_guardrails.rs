//! Comprehensive boundary tests for treasury withdrawal guardrails.
//!
//! This module tests the liquidity-floor and slippage protection mechanisms
//! to ensure the treasury maintains solvency and protects against unfavorable
//! withdrawal conditions.

use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceTreasuryClient<'_>, Address, Address) {
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);

    let token_admin = Address::generate(e);
    let token_id = e.register_stellar_asset_contract(token_admin.clone());

    e.mock_all_auths();
    client.initialize(&admin, &token_id);

    // Give admin some tokens so they can deposit
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);
    stellar_client.mint(&admin, &(i128::MAX / 2));

    (client, admin, token_id)
}

fn setup_withdrawal_scenario(
    e: &Env,
    initial_balance: i128,
    min_liquidity: i128,
) -> (CredenceTreasuryClient<'_>, Address, Address, Address, Address) {
    let (client, admin, token_id) = setup(e);

    // Give admin enough tokens for the initial deposit (already done in setup, but ensures it's enough)
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);
    stellar_client.mint(&admin, &initial_balance);

    // Deposit funds into the treasury - this now performs an actual token transfer
    client.receive_fee(&admin, &initial_balance, &FundSource::ProtocolFee);

    client.set_min_liquidity(&admin, &min_liquidity);

    let signer = Address::generate(e);
    let recipient = Address::generate(e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    (client, admin, signer, recipient, token_id)
}

// ── Liquidity Floor Guardrail Tests ──────────────────────────────────────────

#[test]
fn test_min_liquidity_set_and_get() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);

    assert_eq!(client.get_min_liquidity(), 0);

    client.set_min_liquidity(&admin, &1000);
    assert_eq!(client.get_min_liquidity(), 1000);

    client.set_min_liquidity(&admin, &5000);
    assert_eq!(client.get_min_liquidity(), 5000);
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_min_liquidity_unauthorized_caller() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    let unauthorized = Address::generate(&e);

    client.set_min_liquidity(&unauthorized, &1000);
}

#[test]
fn test_withdrawal_respects_min_liquidity_floor() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 3_000);

    // Withdraw 7000, leaving exactly 3000 (at the floor)
    let id = client.propose_withdrawal(&signer, &recipient, &7_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    assert_eq!(client.get_balance(), 3_000);
}

#[test]
#[should_panic(expected = "liquidity guard: withdrawal would breach minimum liquidity floor")]
fn test_withdrawal_blocked_when_breaching_min_liquidity() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 3_000);

    // Try to withdraw 7001, which would leave 2999 (below floor)
    let id = client.propose_withdrawal(&signer, &recipient, &7_001);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);
}

#[test]
#[should_panic(expected = "liquidity guard: withdrawal would breach minimum liquidity floor")]
fn test_withdrawal_blocked_when_exactly_one_below_floor() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 5_000, 1_000);

    // Boundary: withdraw 4001, leaving 999 (one below floor)
    let id = client.propose_withdrawal(&signer, &recipient, &4_001);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);
}

#[test]
fn test_withdrawal_allowed_when_exactly_at_floor() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 5_000, 1_000);

    // Boundary: withdraw 4000, leaving exactly 1000 (at floor)
    let id = client.propose_withdrawal(&signer, &recipient, &4_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    assert_eq!(client.get_balance(), 1_000);
}

#[test]
fn test_withdrawal_with_zero_min_liquidity() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 1_000, 0);

    // Can withdraw everything when min_liquidity is 0
    let id = client.propose_withdrawal(&signer, &recipient, &1_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    assert_eq!(client.get_balance(), 0);
}

#[test]
#[should_panic(expected = "liquidity guard: withdrawal would breach minimum liquidity floor")]
fn test_withdrawal_blocked_with_high_min_liquidity() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 9_999);

    // Try to withdraw 2, which would leave 9998 (below floor of 9999)
    let id = client.propose_withdrawal(&signer, &recipient, &2);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);
}

#[test]
fn test_min_liquidity_can_be_updated_between_withdrawals() {
    let e = Env::default();
    let (client, admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 2_000);

    // First withdrawal with min_liquidity = 2000
    let id1 = client.propose_withdrawal(&signer, &recipient, &5_000);
    client.approve_withdrawal(&signer, &id1);
    client.execute_withdrawal(&id1, &0);
    assert_eq!(client.get_balance(), 5_000);

    // Update min_liquidity to 1000
    client.set_min_liquidity(&admin, &1_000);

    // Second withdrawal now possible with new floor
    let id2 = client.propose_withdrawal(&signer, &recipient, &3_500);
    client.approve_withdrawal(&signer, &id2);
    client.execute_withdrawal(&id2, &0);
    assert_eq!(client.get_balance(), 1_500);
}

#[test]
fn test_multiple_small_withdrawals_respect_cumulative_floor() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 5_000);

    // Multiple withdrawals, each respecting the floor
    for i in 0..5 {
        let withdraw_amount = 1_000;
        let id = client.propose_withdrawal(&signer, &recipient, &withdraw_amount);
        client.approve_withdrawal(&signer, &id);
        client.execute_withdrawal(&id, &0);

        let expected_balance = 10_000 - ((i + 1) * withdraw_amount);
        assert_eq!(client.get_balance(), expected_balance);
    }

    // Balance is now exactly at floor (5000)
    assert_eq!(client.get_balance(), 5_000);
}

#[test]
#[should_panic(expected = "liquidity guard: withdrawal would breach minimum liquidity floor")]
fn test_sixth_withdrawal_blocked_at_floor() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 5_000);

    // Five successful withdrawals
    for _ in 0..5 {
        let id = client.propose_withdrawal(&signer, &recipient, &1_000);
        client.approve_withdrawal(&signer, &id);
        client.execute_withdrawal(&id, &0);
    }

    // Sixth withdrawal should fail (would breach floor)
    let id = client.propose_withdrawal(&signer, &recipient, &1);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);
}

// ── Slippage Protection Tests ────────────────────────────────────────────────

#[test]
fn test_slippage_guard_accepts_exact_amount() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 0);

    let id = client.propose_withdrawal(&signer, &recipient, &5_000);
    client.approve_withdrawal(&signer, &id);

    // min_amount_out equals proposal amount - should succeed
    client.execute_withdrawal(&id, &5_000);
    assert_eq!(client.get_balance(), 5_000);
}

#[test]
fn test_slippage_guard_accepts_lower_minimum() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 0);

    let id = client.propose_withdrawal(&signer, &recipient, &5_000);
    client.approve_withdrawal(&signer, &id);

    // min_amount_out below proposal amount - should succeed
    client.execute_withdrawal(&id, &4_999);
    assert_eq!(client.get_balance(), 5_000);
}

#[test]
#[should_panic(expected = "slippage: received amount below minimum")]
fn test_slippage_guard_rejects_higher_minimum() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 0);

    let id = client.propose_withdrawal(&signer, &recipient, &5_000);
    client.approve_withdrawal(&signer, &id);

    // min_amount_out above proposal amount - should fail
    client.execute_withdrawal(&id, &5_001);
}

#[test]
#[should_panic(expected = "slippage: received amount below minimum")]
fn test_slippage_guard_rejects_max_minimum() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 0);

    let id = client.propose_withdrawal(&signer, &recipient, &100);
    client.approve_withdrawal(&signer, &id);

    // Adversarial: set unreachably high minimum
    client.execute_withdrawal(&id, &i128::MAX);
}

#[test]
fn test_slippage_guard_with_zero_minimum() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 0);

    let id = client.propose_withdrawal(&signer, &recipient, &5_000);
    client.approve_withdrawal(&signer, &id);

    // min_amount_out = 0 disables slippage check
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 5_000);
}

// ── Combined Guardrail Tests ──────────────────────────────────────────────────

#[test]
fn test_both_guardrails_liquidity_floor_and_slippage() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 3_000);

    // Withdraw 7000, leaving exactly 3000 (at floor)
    // Also require min_amount_out of 7000
    let id = client.propose_withdrawal(&signer, &recipient, &7_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &7_000);

    assert_eq!(client.get_balance(), 3_000);
}

#[test]
#[should_panic(expected = "liquidity guard: withdrawal would breach minimum liquidity floor")]
fn test_liquidity_guard_checked_before_slippage() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 5_000);

    // Try to withdraw 6000 (would breach floor)
    // Even though slippage check would pass
    let id = client.propose_withdrawal(&signer, &recipient, &6_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &6_000);
}

#[test]
#[should_panic(expected = "slippage: received amount below minimum")]
fn test_slippage_guard_checked_after_liquidity() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 10_000, 3_000);

    // Withdraw 7000 (liquidity check passes)
    // But require min_amount_out of 7001 (slippage check fails)
    let id = client.propose_withdrawal(&signer, &recipient, &7_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &7_001);
}

// ── Edge Cases and Boundary Conditions ────────────────────────────────────────

#[test]
#[should_panic]
fn test_min_liquidity_equals_total_balance() {
    let e = Env::default();
    let (client, admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 5_000, 0);

    // Set min_liquidity equal to total balance
    client.set_min_liquidity(&admin, &5_000);

    // Any withdrawal should fail - would breach floor
    let id = client.propose_withdrawal(&signer, &recipient, &1);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);
}

#[test]
#[should_panic]
fn test_min_liquidity_exceeds_total_balance() {
    let e = Env::default();
    let (client, admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 5_000, 0);

    // Set min_liquidity higher than total balance
    client.set_min_liquidity(&admin, &10_000);

    // Any withdrawal should fail
    let id = client.propose_withdrawal(&signer, &recipient, &1);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);
}

#[test]
fn test_withdrawal_with_mixed_fund_sources_respects_floor() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);

    // Add funds from both sources
    client.receive_fee(&admin, &5_000, &FundSource::ProtocolFee);
    client.receive_fee(&admin, &5_000, &FundSource::SlashedFunds);
    client.set_min_liquidity(&admin, &3_000);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    // Withdraw 7000, leaving 3000 (at floor)
    let id = client.propose_withdrawal(&signer, &recipient, &7_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    assert_eq!(client.get_balance(), 3_000);

    // Verify proportional deduction from both sources
    let protocol_balance = client.get_balance_by_source(&FundSource::ProtocolFee);
    let slashed_balance = client.get_balance_by_source(&FundSource::SlashedFunds);
    assert_eq!(protocol_balance + slashed_balance, 3_000);
}

#[test]
fn test_negative_min_liquidity_treated_as_zero() {
    let e = Env::default();
    let (client, admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 1_000, 0);

    // Set negative min_liquidity (should be treated as allowing full withdrawal)
    client.set_min_liquidity(&admin, &-100);

    // Should be able to withdraw everything
    let id = client.propose_withdrawal(&signer, &recipient, &1_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    assert_eq!(client.get_balance(), 0);
}

#[test]
fn test_large_balance_with_large_min_liquidity() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) =
        setup_withdrawal_scenario(&e, i128::MAX / 2, i128::MAX / 4);

    // Can withdraw up to the floor
    let withdraw_amount = (i128::MAX / 2) - (i128::MAX / 4);
    let id = client.propose_withdrawal(&signer, &recipient, &withdraw_amount);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    assert_eq!(client.get_balance(), i128::MAX / 4);
}

#[test]
#[should_panic(expected = "Error(Contract, #602)")]
fn test_proposal_amount_validation_before_guardrails() {
    let e = Env::default();
    let (client, _admin, signer, recipient, _token) = setup_withdrawal_scenario(&e, 5_000, 1_000);

    // Proposal validation happens first - can't propose more than balance
    client.propose_withdrawal(&signer, &recipient, &10_000);
}
