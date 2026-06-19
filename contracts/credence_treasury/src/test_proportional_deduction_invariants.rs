//! Property and adversarial tests for the treasury's proportional deduction invariants.
//!
//! Tracks and asserts the core accounting identity:
//! `BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds) == TotalBalance`
//! across mixed sequences of deposits, withdrawals, slippage scenarios, min liquidity limits,
//! and full drains.

#![cfg(test)]

use crate::{CredenceTreasury, CredenceTreasuryClient, CumulativeAmount, FundSource};
use proptest::prelude::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, TryFromVal};

// --- Mock Taxed Token for Slippage Testing ---
// This token simulates a tax/fee (slippage) on transfer.
#[contract]
pub struct TaxedToken;

#[contractimpl]
impl TaxedToken {
    pub fn initialize(e: Env, admin: Address, tax_rate_bps: i128) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "admin"), &admin);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "tax"), &tax_rate_bps);
    }

    pub fn mint(e: Env, to: Address, amount: i128) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&Symbol::new(&e, "admin"))
            .unwrap();
        admin.require_auth();
        let balance_key = (Symbol::new(&e, "balance"), to.clone());
        let balance: i128 = e.storage().persistent().get(&balance_key).unwrap_or(0);
        e.storage()
            .persistent()
            .set(&balance_key, &(balance + amount));
    }

    pub fn balance(e: Env, id: Address) -> i128 {
        let balance_key = (Symbol::new(&e, "balance"), id);
        e.storage().persistent().get(&balance_key).unwrap_or(0)
    }

    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let tax_rate: i128 = e.storage().instance().get(&Symbol::new(&e, "tax")).unwrap();
        let tax = (amount * tax_rate) / 10000;
        let actual_amount = amount - tax;

        let from_key = (Symbol::new(&e, "balance"), from);
        let to_key = (Symbol::new(&e, "balance"), to);

        let from_balance: i128 = e.storage().persistent().get(&from_key).unwrap_or(0);
        let to_balance: i128 = e.storage().persistent().get(&to_key).unwrap_or(0);

        if from_balance < amount {
            panic!("insufficient balance");
        }

        e.storage()
            .persistent()
            .set(&from_key, &(from_balance - amount));
        e.storage()
            .persistent()
            .set(&to_key, &(to_balance + actual_amount));
    }
}

// --- Environment Helpers ---

fn setup_env(e: &Env) -> (CredenceTreasuryClient<'_>, Address, Address) {
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);

    let token_admin = Address::generate(e);
    let token_id = e.register_stellar_asset_contract(token_admin.clone());

    e.mock_all_auths();
    client.initialize(&admin, &token_id);

    // Mint some initial tokens to the admin so they can deposit
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);
    stellar_client.mint(&admin, &(i128::MAX / 2));

    (client, admin, token_id)
}

fn setup_taxed_env(e: &Env, tax_bps: i128) -> (CredenceTreasuryClient<'_>, Address, Address) {
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);

    let token_id = e.register(TaxedToken, ());
    let token_client = TaxedTokenClient::new(e, &token_id);
    token_client.initialize(&admin, &tax_bps);

    e.mock_all_auths();
    client.initialize(&admin, &token_id);

    token_client.mint(&admin, &(i128::MAX / 2));

    (client, admin, token_id)
}

/// Find the most recent `treasury_withdrawal_executed` event in a pre-fetched events Vec.
///
/// Matches by data shape `(Address, i128, i128)` which is unique to
/// `treasury_withdrawal_executed` — no other treasury or token event has this layout.
fn find_withdrawal_executed_event_in(
    e: &Env,
    events: &soroban_sdk::Vec<(
        Address,
        soroban_sdk::Vec<soroban_sdk::Val>,
        soroban_sdk::Val,
    )>,
) -> Option<(Address, i128, i128)> {
    events
        .iter()
        .rev()
        .find_map(|(_, _topics, data)| <(Address, i128, i128)>::try_from_val(e, &data).ok())
}

// --- Adversarial Unit Tests ---

/// Exercises `execute_withdrawal` over structured sequences of deposits to both sources,
/// asserting the core identity:
/// `BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds) == TotalBalance`
/// holds after every withdrawal, and that the executed event matches details.
#[test]
fn test_withdrawal_identity_under_mixed_deposits() {
    let e = Env::default();
    let (client, admin, token_id) = setup_env(&e);
    let token_client = soroban_sdk::token::TokenClient::new(&e, &token_id);

    // Mix deposits to both sources
    client.receive_fee(&admin, &10_000, &FundSource::ProtocolFee);
    client.receive_fee(&admin, &20_000, &FundSource::SlashedFunds);
    client.receive_fee(&admin, &5_000, &FundSource::ProtocolFee);
    client.receive_fee(&admin, &15_000, &FundSource::SlashedFunds);

    assert_eq!(client.get_balance(), 50_000);
    assert_eq!(
        client.get_balance_by_source(&FundSource::ProtocolFee),
        15_000
    );
    assert_eq!(
        client.get_balance_by_source(&FundSource::SlashedFunds),
        35_000
    );

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    // Sequence of withdrawals
    let withdrawals = [1_000_i128, 5_500, 12_000, 30_000];
    for &amount in withdrawals.iter() {
        let id = client.propose_withdrawal(&signer, &recipient, &amount);
        client.approve_withdrawal(&signer, &id);

        let recipient_bal_before = token_client.balance(&recipient);
        client.execute_withdrawal(&id, &0);

        // Capture events immediately after execute_withdrawal, before any other calls
        // that might affect the events buffer. The most recent (Address,i128,i128)
        // data event is uniquely the treasury_withdrawal_executed event.
        let events = e.events().all();
        let exec_event = find_withdrawal_executed_event_in(&e, &events);
        let (event_recipient, min_out, act_out) =
            exec_event.expect("treasury_withdrawal_executed event not found");

        let recipient_bal_after = token_client.balance(&recipient);
        let actual_amount = recipient_bal_after - recipient_bal_before;

        // Core identity check
        let total = client.get_balance();
        let protocol = client.get_balance_by_source(&FundSource::ProtocolFee);
        let slashed = client.get_balance_by_source(&FundSource::SlashedFunds);
        assert_eq!(protocol + slashed, total);
        assert!(protocol >= 0);
        assert!(slashed >= 0);

        // Event validation: Assert `treasury_withdrawal_executed` emits (recipient, min_amount_out, actual_amount)
        assert_eq!(event_recipient, recipient);
        assert_eq!(min_out, 0);
        assert_eq!(act_out, actual_amount);
    }
}

/// Verifies that any withdrawal attempting to breach the `MinLiquidity` floor reverts.
#[test]
#[should_panic(expected = "Error(Contract, #602)")]
fn test_min_liquidity_floor_reverts() {
    let e = Env::default();
    let (client, admin, _token_id) = setup_env(&e);

    client.receive_fee(&admin, &10_000, &FundSource::ProtocolFee);
    client.set_min_liquidity(&admin, &3_000);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    // Total 10,000. Floor is 3,000. Maximum withdrawable is 7,000.
    // Try to withdraw 7,001. Should panic.
    let id = client.propose_withdrawal(&signer, &recipient, &7_001);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);
}

/// Verifies that the slippage guard correctly reverts when `actual_amount < min_amount_out`.
#[test]
#[should_panic(expected = "Error(Contract, #602)")]
fn test_slippage_guard_reverts() {
    let e = Env::default();
    // Use TaxedToken with 2% tax (200 basis points)
    let (client, admin, token_id) = setup_taxed_env(&e, 200);
    let token_client = TaxedTokenClient::new(&e, &token_id);

    let deposit_amount = 10_000_i128;
    client.receive_fee(&admin, &deposit_amount, &FundSource::ProtocolFee);
    // TaxedToken takes 2% on transfer, so the contract needs extra tokens to execute the full transfer.
    token_client.mint(&client.address, &deposit_amount);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let amount = 5_000_i128;
    let id = client.propose_withdrawal(&signer, &recipient, &amount);
    client.approve_withdrawal(&signer, &id);

    // 2% tax on 5,000 means recipient receives 4,900.
    // If we require min_amount_out = 4,901, it must revert.
    client.execute_withdrawal(&id, &4_901);
}

/// Covers the full-drain shortcut (amount == total) and single-source-only treasuries.
#[test]
fn test_full_drain_shortcut_and_single_source() {
    let e = Env::default();
    let (client, admin, token_id) = setup_env(&e);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    // Test case 1: Single source (ProtocolFee only, SlashedFunds is zero)
    client.receive_fee(&admin, &10_000, &FundSource::ProtocolFee);
    assert_eq!(
        client.get_balance_by_source(&FundSource::ProtocolFee),
        10_000
    );
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 0);

    let id = client.propose_withdrawal(&signer, &recipient, &4_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    let protocol = client.get_balance_by_source(&FundSource::ProtocolFee);
    let slashed = client.get_balance_by_source(&FundSource::SlashedFunds);
    let total = client.get_balance();
    assert_eq!(protocol, 6_000);
    assert_eq!(slashed, 0);
    assert_eq!(total, 6_000);
    assert_eq!(protocol + slashed, total);

    // Drain the rest to zero (hits amount == total shortcut)
    let id2 = client.propose_withdrawal(&signer, &recipient, &6_000);
    client.approve_withdrawal(&signer, &id2);
    client.execute_withdrawal(&id2, &0);

    let protocol = client.get_balance_by_source(&FundSource::ProtocolFee);
    let slashed = client.get_balance_by_source(&FundSource::SlashedFunds);
    let total = client.get_balance();
    assert_eq!(protocol, 0);
    assert_eq!(slashed, 0);
    assert_eq!(total, 0);
    assert_eq!(protocol + slashed, total);

    // Test case 2: Single source (SlashedFunds only, ProtocolFee is zero)
    client.receive_fee(&admin, &15_000, &FundSource::SlashedFunds);
    let protocol = client.get_balance_by_source(&FundSource::ProtocolFee);
    let slashed = client.get_balance_by_source(&FundSource::SlashedFunds);
    let total = client.get_balance();
    assert_eq!(protocol, 0);
    assert_eq!(slashed, 15_000);
    assert_eq!(total, 15_000);
    assert_eq!(protocol + slashed, total);

    let id3 = client.propose_withdrawal(&signer, &recipient, &5_000);
    client.approve_withdrawal(&signer, &id3);
    client.execute_withdrawal(&id3, &0);

    let protocol = client.get_balance_by_source(&FundSource::ProtocolFee);
    let slashed = client.get_balance_by_source(&FundSource::SlashedFunds);
    let total = client.get_balance();
    assert_eq!(protocol, 0);
    assert_eq!(slashed, 10_000);
    assert_eq!(total, 10_000);
    assert_eq!(protocol + slashed, total);

    // Full drain of slashed funds (hits amount == total shortcut)
    let id4 = client.propose_withdrawal(&signer, &recipient, &10_000);
    client.approve_withdrawal(&signer, &id4);
    client.execute_withdrawal(&id4, &0);

    let protocol = client.get_balance_by_source(&FundSource::ProtocolFee);
    let slashed = client.get_balance_by_source(&FundSource::SlashedFunds);
    let total = client.get_balance();
    assert_eq!(protocol, 0);
    assert_eq!(slashed, 0);
    assert_eq!(total, 0);
    assert_eq!(protocol + slashed, total);
}

/// Asserts that `CumulativeReceived` only counts deposits and is unaffected by withdrawals.
#[test]
fn test_cumulative_received_unaffected_by_withdrawal() {
    let e = Env::default();
    let (client, admin, _token_id) = setup_env(&e);

    client.receive_fee(&admin, &6_000, &FundSource::ProtocolFee);
    client.receive_fee(&admin, &4_000, &FundSource::SlashedFunds);

    let cum_before = client.get_cumulative_received();
    let total_before = (u128::from(cum_before.rollovers) * ((i128::MAX as u128) + 1))
        + cum_before.remainder as u128;
    assert_eq!(total_before, 10_000);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let id = client.propose_withdrawal(&signer, &recipient, &3_000);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    let cum_after = client.get_cumulative_received();
    let total_after =
        (u128::from(cum_after.rollovers) * ((i128::MAX as u128) + 1)) + cum_after.remainder as u128;

    // Withdrawal must not increment CumulativeReceived
    assert_eq!(total_after, total_before);
    assert_eq!(client.get_balance(), 7_000);
}

/// Verifies that `treasury_withdrawal_executed` correctly encodes a non-zero
/// `min_amount_out` in the event payload.
#[test]
fn test_withdrawal_event_encodes_nonzero_min_amount_out() {
    let e = Env::default();
    let (client, admin, token_id) = setup_env(&e);
    let token_client = soroban_sdk::token::TokenClient::new(&e, &token_id);

    client.receive_fee(&admin, &10_000, &FundSource::ProtocolFee);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let min_out: i128 = 500;
    let id = client.propose_withdrawal(&signer, &recipient, &2_000);
    client.approve_withdrawal(&signer, &id);

    let bal_before = token_client.balance(&recipient);
    client.execute_withdrawal(&id, &min_out);

    let events = e.events().all();
    let (event_recipient, event_min_out, event_actual) =
        find_withdrawal_executed_event_in(&e, &events)
            .expect("treasury_withdrawal_executed event not found");

    let actual_amount = token_client.balance(&recipient) - bal_before;

    assert_eq!(event_recipient, recipient);
    assert_eq!(event_min_out, min_out); // non-zero min_amount_out is correctly emitted
    assert_eq!(event_actual, actual_amount);
    assert!(event_actual >= min_out); // sanity: withdrawal succeeded, so guard was satisfied
}

// --- Property Fuzz Testing via Proptest ---

#[derive(Clone, Debug)]
enum Action {
    DepositProtocol(i128),
    DepositSlashed(i128),
    Withdraw(i128),
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        (1_i128..=10_000_i128).prop_map(Action::DepositProtocol),
        (1_i128..=10_000_i128).prop_map(Action::DepositSlashed),
        (1_i128..=10_000_i128).prop_map(Action::Withdraw),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Core property: across random sequences of deposits and withdrawals,
    /// the identity `BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds) == TotalBalance`
    /// always holds, no source balance goes negative, and CumulativeReceived increases monotonically.
    #[test]
    fn test_proportional_deduction_invariants_proptest(
        actions in prop::collection::vec(action_strategy(), 1..=20)
    ) {
        let e = Env::default();
        let (client, admin, token_id) = setup_env(&e);
        let token_client = soroban_sdk::token::TokenClient::new(&e, &token_id);

        let signer = Address::generate(&e);
        let recipient = Address::generate(&e);
        client.add_signer(&signer);
        client.set_threshold(&1);

        let mut expected_protocol = 0_i128;
        let mut expected_slashed = 0_i128;
        let mut expected_cum_total = 0_u128;

        for action in actions {
            match action {
                Action::DepositProtocol(amount) => {
                    client.receive_fee(&admin, &amount, &FundSource::ProtocolFee);
                    expected_protocol += amount;
                    expected_cum_total += amount as u128;
                }
                Action::DepositSlashed(amount) => {
                    client.receive_fee(&admin, &amount, &FundSource::SlashedFunds);
                    expected_slashed += amount;
                    expected_cum_total += amount as u128;
                }
                Action::Withdraw(amount) => {
                    let total = client.get_balance();
                    if amount <= total {
                        let id = client.propose_withdrawal(&signer, &recipient, &amount);
                        client.approve_withdrawal(&signer, &id);

                        let recipient_bal_before = token_client.balance(&recipient);
                        client.execute_withdrawal(&id, &0);
                        let recipient_bal_after = token_client.balance(&recipient);
                        let actual_withdrawn = recipient_bal_after - recipient_bal_before;

                        // Calculate expected proportional deductions locally
                        let prev_total = expected_protocol + expected_slashed;
                        let local_protocol_deduction = if expected_protocol == 0 || actual_withdrawn == 0 {
                            0
                        } else if actual_withdrawn == prev_total {
                            expected_protocol
                        } else {
                            let num = (expected_protocol as u128) * (actual_withdrawn as u128);
                            let den = prev_total as u128;
                            (num / den) as i128
                        };

                        let local_slashed_deduction = actual_withdrawn - local_protocol_deduction;

                        expected_protocol -= local_protocol_deduction;
                        expected_slashed -= local_slashed_deduction;
                    }
                }
            }

            // Invariant assertions
            let total = client.get_balance();
            let protocol = client.get_balance_by_source(&FundSource::ProtocolFee);
            let slashed = client.get_balance_by_source(&FundSource::SlashedFunds);

            prop_assert_eq!(protocol + slashed, total);
            prop_assert!(protocol >= 0);
            prop_assert!(slashed >= 0);
            prop_assert_eq!(protocol, expected_protocol);
            prop_assert_eq!(slashed, expected_slashed);

            let cum_received = client.get_cumulative_received();
            let actual_cum_total = (u128::from(cum_received.rollovers) * ((i128::MAX as u128) + 1))
                + u128::try_from(cum_received.remainder).unwrap();
            prop_assert_eq!(actual_cum_total, expected_cum_total);
        }
    }
}
