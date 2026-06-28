//! Comprehensive tests for the Credence Treasury contract.
//! Covers: initialization, fees, depositors, multi-sig (signers, threshold,
//! propose/approve/execute), fund source tracking, events, and security.
//! Also tests emergency rescue functionality for stuck native tokens.

use crate::{CredenceTreasury, CredenceTreasuryClient, CumulativeAmount, FundSource};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

const CUMULATIVE_SEGMENT: u128 = (i128::MAX as u128) + 1;

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

#[test]
fn test_initialize() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    assert_eq!(client.get_admin(), _admin);
    assert_eq!(client.get_balance(), 0);
    assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 0);
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 0);
    assert_eq!(
        client.get_cumulative_received(),
        CumulativeAmount {
            rollovers: 0,
            remainder: 0,
        }
    );
    assert_eq!(client.get_threshold(), 0);
    assert_eq!(client.get_min_liquidity(), 0);
}

fn counter_to_u128(counter: &CumulativeAmount) -> u128 {
    (u128::from(counter.rollovers) * CUMULATIVE_SEGMENT)
        + u128::try_from(counter.remainder).expect("remainder should be non-negative")
}

fn withdraw_all(
    e: &Env,
    client: &CredenceTreasuryClient<'_>,
    _token_id: &Address,
    amount: i128,
) -> (Address, Address, u64) {
    let signer = Address::generate(e);
    let recipient = Address::generate(e);

    // Note: Treasury is assumed to be already funded via receive_fee in the calling test
    client.add_signer(&signer);
    client.set_threshold(&1);
    let proposal_id = client.propose_withdrawal(&signer, &recipient, &amount);
    client.approve_withdrawal(&signer, &proposal_id);
    client.execute_withdrawal(&proposal_id, &0);
    (signer, recipient, proposal_id)
}

#[test]
fn test_receive_fee_as_admin() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 1000);
    assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 1000);
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 0);
    client.receive_fee(&admin, &500, &FundSource::SlashedFunds);
    assert_eq!(client.get_balance(), 1500);
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 500);
}

#[test]
#[should_panic(expected = "Error(Contract, #700)")]
fn test_receive_fee_overflow_panics() {
    let e = Env::default();
    let (client, admin, token_id) = setup(&e);
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);

    // Give admin enough tokens to reach exactly i128::MAX
    // Setup already gave them i128::MAX / 2
    stellar_client.mint(&admin, &(i128::MAX - (i128::MAX / 2)));

    client.receive_fee(&admin, &i128::MAX, &FundSource::ProtocolFee);

    // For the second deposit, we need 1 more token, but we can't have more than i128::MAX balance in one account easily.
    // Actually, we can just mint 1 more to the admin's balance if it's not already MAX.
    // Wait, i128::MAX is the absolute limit for a single account balance in most token implementations.
    // But we can just use a different account for the second deposit!
    let admin2 = Address::generate(&e);
    client.add_depositor(&admin2);
    stellar_client.mint(&admin2, &1);

    client.receive_fee(&admin2, &1, &FundSource::ProtocolFee);
}

// Tests for emergency rescue functionality
#[test]
fn test_rescue_native_success() {
    let e = Env::default();
    let (client, admin, token_id) = setup(&e);
    let recipient = Address::generate(&e);
    let contract_id = client.address.clone();

    // Deposit 1000 into treasury accounting
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);

    // Mint 300 extra directly to the contract (simulating stuck/airdropped tokens)
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
    stellar_client.mint(&contract_id, &300);

    // Actual balance = 1300, accounted = 1000, excess = 300
    client.rescue_native(&admin, &recipient, &300);

    // Recipient received the rescued tokens
    let token_client = soroban_sdk::token::TokenClient::new(&e, &token_id);
    assert_eq!(token_client.balance(&recipient), 300);
    // Accounted balance unchanged
    assert_eq!(client.get_balance(), 1000);
}

#[test]
fn test_rescue_native_partial_excess() {
    let e = Env::default();
    let (client, admin, token_id) = setup(&e);
    let recipient = Address::generate(&e);
    let contract_id = client.address.clone();

    client.receive_fee(&admin, &500, &FundSource::ProtocolFee);

    // Mint 200 excess directly to contract
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
    stellar_client.mint(&contract_id, &200);

    // Rescue only part of the excess
    client.rescue_native(&admin, &recipient, &100);

    let token_client = soroban_sdk::token::TokenClient::new(&e, &token_id);
    assert_eq!(token_client.balance(&recipient), 100);
    assert_eq!(client.get_balance(), 500);
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_rescue_native_unauthorized() {
    let e = Env::default();
    let (client, _admin, token_id) = setup(&e);
    let recipient = Address::generate(&e);
    let unauthorized = Address::generate(&e);
    let contract_id = client.address.clone();

    // Mint excess so the balance check is not the rejecting guard
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
    stellar_client.mint(&contract_id, &500);

    client.rescue_native(&unauthorized, &recipient, &500);
}

#[test]
#[should_panic(expected = "Error(Contract, #600)")]
fn test_rescue_native_zero_amount() {
    let e = Env::default();
    let (client, admin, token_id) = setup(&e);
    let recipient = Address::generate(&e);
    let contract_id = client.address.clone();

    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
    stellar_client.mint(&contract_id, &500);

    client.rescue_native(&admin, &recipient, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #602)")]
fn test_rescue_native_exceeds_excess() {
    let e = Env::default();
    let (client, admin, token_id) = setup(&e);
    let recipient = Address::generate(&e);
    let contract_id = client.address.clone();

    // 1000 accounted, 100 excess → total actual = 1100
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
    stellar_client.mint(&contract_id, &100);

    // Try to rescue 200 — more than 100 excess
    client.rescue_native(&admin, &recipient, &200);
}

#[test]
#[should_panic(expected = "Error(Contract, #602)")]
fn test_rescue_native_zero_excess_rejected() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    let recipient = Address::generate(&e);

    // 1000 accounted, no extra tokens → excess = 0
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);

    // Any rescue amount > 0 must fail
    client.rescue_native(&admin, &recipient, &1);
}

#[test]
#[should_panic(expected = "Error(Contract, #602)")]
fn test_rescue_native_cannot_drain_accounted_funds() {
    let e = Env::default();
    let (client, admin, token_id) = setup(&e);
    let recipient = Address::generate(&e);
    let contract_id = client.address.clone();

    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    // Only 50 tokens excess
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
    stellar_client.mint(&contract_id, &50);

    // Attempt to rescue the full accounted amount — must be rejected
    client.rescue_native(&admin, &recipient, &1000);
}

#[test]
fn test_receive_fee_as_depositor() {
    let e = Env::default();
    let (client, _admin, token_id) = setup(&e);
    let depositor = Address::generate(&e);

    // Give depositor tokens
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
    stellar_client.mint(&depositor, &2000);

    client.add_depositor(&depositor);
    client.receive_fee(&depositor, &2000, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 2000);
    assert!(client.is_depositor(&depositor));
    client.remove_depositor(&depositor);
    assert!(!client.is_depositor(&depositor));
}

#[test]
#[should_panic(expected = "Error(Contract, #105)")]
fn test_receive_fee_unauthorized() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    let other = Address::generate(&e);
    client.receive_fee(&other, &100, &FundSource::ProtocolFee);
}

#[test]
#[should_panic(expected = "Error(Contract, #600)")]
fn test_receive_fee_zero_amount() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &0, &FundSource::ProtocolFee);
}

#[test]
#[should_panic(expected = "Error(Contract, #600)")]
fn test_receive_fee_negative_amount() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &-100, &FundSource::ProtocolFee);
}

#[test]
fn test_add_remove_signer_and_threshold() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    assert!(client.is_signer(&s1));
    assert!(client.is_signer(&s2));
    client.set_threshold(&2);
    assert_eq!(client.get_threshold(), 2);
    client.remove_signer(&s1);
    assert!(!client.is_signer(&s1));
    assert_eq!(client.get_threshold(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #601)")]
fn test_set_threshold_exceeds_signers() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    let s1 = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&3);
}

#[test]
fn test_propose_approve_execute_withdrawal() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &10_000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&2);
    let id = client.propose_withdrawal(&s1, &recipient, &3000);
    let prop = client.get_proposal(&id);
    assert_eq!(prop.recipient, recipient);
    assert_eq!(prop.amount, 3000);
    assert!(!prop.executed);
    assert_eq!(client.get_approval_count(&id), 0);
    client.approve_withdrawal(&s1, &id);
    assert!(client.has_approved(&id, &s1));
    assert_eq!(client.get_approval_count(&id), 1);
    client.approve_withdrawal(&s2, &id);
    assert_eq!(client.get_approval_count(&id), 2);
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 7000);
    let prop2 = client.get_proposal(&id);
    assert!(prop2.executed);
}

#[test]
fn test_withdrawal_reduces_available_source_balances_proportionally() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &100, &FundSource::ProtocolFee);
    client.receive_fee(&admin, &200, &FundSource::SlashedFunds);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&signer, &recipient, &150);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    assert_eq!(client.get_balance(), 150);
    assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 50);
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 100);

    let cumulative_total = client.get_cumulative_received();
    assert_eq!(counter_to_u128(&cumulative_total), 300);
}

#[test]
#[should_panic(expected = "Error(Contract, #104)")]
fn test_propose_withdrawal_non_signer() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let other = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.propose_withdrawal(&other, &recipient, &500);
}

#[test]
#[should_panic(expected = "Error(Contract, #600)")]
fn test_propose_withdrawal_zero_amount() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    client.propose_withdrawal(&s1, &recipient, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #602)")]
fn test_propose_withdrawal_exceeds_balance() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &100, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    client.propose_withdrawal(&s1, &recipient, &200);
}

#[test]
#[should_panic(expected = "Error(Contract, #104)")]
fn test_approve_withdrawal_non_signer() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let other = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&other, &id);
}

#[test]
fn test_double_approve_is_noop() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&s1, &id);
    client.approve_withdrawal(&s1, &id);
    assert_eq!(client.get_approval_count(&id), 1);
    client.execute_withdrawal(&id, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #605)")]
fn test_execute_without_threshold() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&2);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #604)")]
fn test_execute_twice_fails() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
    client.execute_withdrawal(&id, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #603)")]
fn test_get_proposal_invalid_id() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    let _ = client.get_proposal(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #604)")]
fn test_approve_after_execute_fails() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
    client.approve_withdrawal(&s2, &id);
}

#[test]
fn test_fund_source_tracking() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &100, &FundSource::ProtocolFee);
    client.receive_fee(&admin, &200, &FundSource::SlashedFunds);
    client.receive_fee(&admin, &50, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 350);
    assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 150);
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 200);
    assert_eq!(
        counter_to_u128(&client.get_cumulative_by_source(&FundSource::ProtocolFee)),
        150
    );
    assert_eq!(
        counter_to_u128(&client.get_cumulative_by_source(&FundSource::SlashedFunds)),
        200
    );
}

#[test]
fn test_multiple_proposals() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    client.receive_fee(&admin, &5000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&2);
    let id1 = client.propose_withdrawal(&s1, &r1, &1000);
    let id2 = client.propose_withdrawal(&s2, &r2, &2000);
    assert_ne!(id1, id2);
    client.approve_withdrawal(&s1, &id1);
    client.approve_withdrawal(&s2, &id1);
    client.execute_withdrawal(&id1, &0);
    assert_eq!(client.get_balance(), 4000);
    client.approve_withdrawal(&s1, &id2);
    client.approve_withdrawal(&s2, &id2);
    client.execute_withdrawal(&id2, &0);
    assert_eq!(client.get_balance(), 2000);
}

#[test]
fn test_remove_signer_caps_threshold() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&2);
    client.remove_signer(&s2);
    assert_eq!(client.get_threshold(), 1);
}

#[test]
fn test_add_signer_idempotent() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    let s1 = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s1);
    assert!(client.is_signer(&s1));
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_get_admin_uninitialized() {
    let e = Env::default();
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(&e, &contract_id);
    let _ = client.get_admin();
}

#[test]
fn test_get_approval_count_nonexistent_proposal() {
    let e = Env::default();
    let (client, _admin, _token) = setup(&e);
    assert_eq!(client.get_approval_count(&99), 0);
}

// ── Slippage bound tests (issue #124) ────────────────────────────────────────

/// Helper: set up a funded treasury with one signer and a ready-to-execute proposal.
fn setup_ready_proposal(amount: i128) -> (Env, CredenceTreasuryClient<'static>, u64) {
    let e = Env::default();
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(&e, &contract_id);
    let admin = Address::generate(&e);

    let token_admin = Address::generate(&e);
    let token_id = e.register_stellar_asset_contract(token_admin.clone());
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);

    e.mock_all_auths();
    client.initialize(&admin, &token_id);

    // Give admin tokens and deposit
    stellar_client.mint(&admin, &amount);
    client.receive_fee(&admin, &amount, &FundSource::ProtocolFee);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&signer, &recipient, &amount);
    client.approve_withdrawal(&signer, &id);
    (e, client, id)
}

#[test]
fn test_execute_withdrawal_at_exact_min_amount_out_succeeds() {
    // min_amount_out == proposal.amount → should succeed (boundary condition).
    let (_e, client, id) = setup_ready_proposal(500);
    client.execute_withdrawal(&id, &500);
    assert_eq!(client.get_balance(), 0);
    assert!(client.get_proposal(&id).executed);
}

#[test]
fn test_execute_withdrawal_min_amount_out_zero_succeeds() {
    // min_amount_out == 0 → no slippage check, always succeeds.
    let (_e, client, id) = setup_ready_proposal(500);
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 0);
}

#[test]
fn test_execute_withdrawal_min_amount_out_below_proposal_succeeds() {
    // min_amount_out < proposal.amount → caller accepts any amount above threshold.
    let (_e, client, id) = setup_ready_proposal(1000);
    client.execute_withdrawal(&id, &999);
    assert_eq!(client.get_balance(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #609)")]
fn test_execute_withdrawal_slippage_reverts_when_below_min() {
    // min_amount_out > proposal.amount → must revert.
    let (_e, client, id) = setup_ready_proposal(500);
    client.execute_withdrawal(&id, &501);
}

#[test]
#[should_panic(expected = "Error(Contract, #609)")]
fn test_execute_withdrawal_slippage_reverts_adversarial_large_min() {
    // Adversarial: caller sets an unreachably high min_amount_out.
    let (_e, client, id) = setup_ready_proposal(100);
    client.execute_withdrawal(&id, &i128::MAX);
}

#[test]
fn test_cumulative_protocol_fee_rollover_survives_large_claim_cycle() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);

    // Give admin enough tokens to reach exactly i128::MAX
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &_token);
    stellar_client.mint(&admin, &(i128::MAX - (i128::MAX / 2)));

    client.receive_fee(&admin, &i128::MAX, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), i128::MAX);
    assert_eq!(
        client.get_balance_by_source(&FundSource::ProtocolFee),
        i128::MAX
    );
    assert_eq!(
        client.get_cumulative_by_source(&FundSource::ProtocolFee),
        CumulativeAmount {
            rollovers: 0,
            remainder: i128::MAX,
        }
    );

    let _ = withdraw_all(&e, &client, &_token, i128::MAX);
    assert_eq!(client.get_balance(), 0);
    assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 0);

    // Mint tokens for the next deposit
    stellar_client.mint(&admin, &10);
    client.receive_fee(&admin, &10, &FundSource::ProtocolFee);

    assert_eq!(client.get_balance(), 10);
    assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 10);
    assert_eq!(
        client.get_cumulative_by_source(&FundSource::ProtocolFee),
        CumulativeAmount {
            rollovers: 1,
            remainder: 9,
        }
    );
    assert_eq!(
        client.get_cumulative_received(),
        CumulativeAmount {
            rollovers: 1,
            remainder: 9,
        }
    );
}

#[test]
fn test_cumulative_fees_reconcile_after_repeated_high_rate_claims() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    let burst = i128::MAX / 2;
    let mut expected_cumulative = 0_u128;

    for _ in 0..3 {
        // Mint tokens for this burst
        let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &_token);
        stellar_client.mint(&admin, &burst);

        client.receive_fee(&admin, &burst, &FundSource::ProtocolFee);
        expected_cumulative += u128::try_from(burst).expect("burst should fit");
        let _ = withdraw_all(&e, &client, &_token, burst);
        assert_eq!(client.get_balance(), 0);
        assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 0);
    }

    let cumulative_protocol = client.get_cumulative_by_source(&FundSource::ProtocolFee);
    let cumulative_total = client.get_cumulative_received();

    assert_eq!(counter_to_u128(&cumulative_protocol), expected_cumulative);
    assert_eq!(counter_to_u128(&cumulative_total), expected_cumulative);
    assert!(cumulative_protocol.rollovers >= 1);
}

// ─── Proposal expiry tests ─────────────────────────────────────────────────

fn advance(e: &Env, secs: u64) {
    let info = e.ledger().get();
    e.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: info.timestamp + secs,
        ..info
    });
}

#[test]
fn test_proposal_ttl_default_and_set_get() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);

    // Default TTL is 7 days
    assert_eq!(client.get_proposal_ttl(), 7 * 24 * 60 * 60);

    // Admin can update
    client.set_proposal_ttl(&admin, &3600);
    assert_eq!(client.get_proposal_ttl(), 3600);

    // TTL=0 means no expiry
    client.set_proposal_ttl(&admin, &0);
    assert_eq!(client.get_proposal_ttl(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #608)")]
fn test_approve_withdrawal_after_expiry_rejected() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);

    client.add_signer(&signer);
    client.set_threshold(&1);
    client.set_proposal_ttl(&admin, &3600); // 1 hour TTL

    // Fund the treasury
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);

    let id = client.propose_withdrawal(&signer, &recipient, &500);

    // Advance past the 1 hour TTL
    advance(&e, 3601);

    client.approve_withdrawal(&signer, &id);
}

#[test]
#[should_panic(expected = "Error(Contract, #608)")]
fn test_execute_withdrawal_after_expiry_rejected() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);

    client.add_signer(&signer);
    client.set_threshold(&1);
    client.set_proposal_ttl(&admin, &3600);

    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);

    let id = client.propose_withdrawal(&signer, &recipient, &500);
    client.approve_withdrawal(&signer, &id);

    // Advance past the TTL
    advance(&e, 3601);

    client.execute_withdrawal(&id, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #608)")]
fn test_execute_exactly_at_expiry_rejected() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);

    client.add_signer(&signer);
    client.set_threshold(&1);
    client.set_proposal_ttl(&admin, &3600);

    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);

    let id = client.propose_withdrawal(&signer, &recipient, &500);
    client.approve_withdrawal(&signer, &id);

    // Advance exactly to the expiry timestamp
    advance(&e, 3600);
    let proposal = client.get_proposal(&id);
    assert!(e.ledger().timestamp() >= proposal.expires_at);

    client.execute_withdrawal(&id, &0);
}

#[test]
fn test_propose_expire_and_repropose_succeeds() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);

    client.add_signer(&signer);
    client.set_threshold(&1);
    client.set_proposal_ttl(&admin, &3600);

    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);

    // First proposal — let it expire
    let id1 = client.propose_withdrawal(&signer, &recipient, &500);
    advance(&e, 3601);

    // The old proposal is expired; verify by getting it (expired but still stored)
    let stale = client.get_proposal(&id1);
    assert!(e.ledger().timestamp() >= stale.expires_at);

    // Re-propose with a fresh proposal and execute successfully
    let id2 = client.propose_withdrawal(&signer, &recipient, &500);
    client.approve_withdrawal(&signer, &id2);
    client.execute_withdrawal(&id2, &0);
}

#[test]
fn test_ttl_zero_never_expires() {
    let e = Env::default();
    let (client, admin, _token) = setup(&e);
    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);

    client.add_signer(&signer);
    client.set_threshold(&1);
    client.set_proposal_ttl(&admin, &0); // No expiry

    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);

    let id = client.propose_withdrawal(&signer, &recipient, &500);

    // Advance a huge amount of time
    advance(&e, 365 * 24 * 3600); // 1 year

    // Still executable because TTL=0 means no expiry
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);
}
