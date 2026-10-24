//! Accounting tests for the two treasury defects fixed under issue #1632:
//!
//! 1. `set_token` must be rejected while the treasury holds a non-zero
//!    `TotalBalance`, otherwise the new token's balance cannot back the
//!    recorded books.
//! 2. Every inflow and outflow must measure the **treasury's own** token
//!    balance before and after the transfer, not the counterparty's delta.
//!    For fee-on-transfer tokens the two differ, and booking the counterparty
//!    delta lets `TotalBalance` drift above the tokens the contract actually
//!    holds (phantom funds).
//!
//! Invariants asserted after every step:
//! - `token.balance(contract) == get_balance()`
//! - `get_balance() == get_balance_by_source(ProtocolFee) + get_balance_by_source(SlashedFunds)`

#![cfg(test)]

extern crate std;

use crate::test_slippage_adversarial::{TaxedToken, TaxedTokenClient};
use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env};

fn setup_taxed(e: &Env) -> (CredenceTreasuryClient<'_>, Address, Address, TaxedTokenClient<'_>) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);

    let token_id = e.register(TaxedToken, ());
    let token_client = TaxedTokenClient::new(e, &token_id);
    token_client.initialize(&admin);

    client.initialize(&admin, &token_id);
    token_client.mint(&admin, &(i128::MAX / 4));

    (client, admin, token_id, token_client)
}

fn setup_standard(e: &Env) -> (CredenceTreasuryClient<'_>, Address, Address, token::Client<'_>) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);

    let token_id = e.register_stellar_asset_contract_v2(admin.clone());
    let token_client = token::Client::new(e, &token_id.address());

    client.initialize(&admin, &token_id.address());
    token::StellarAssetClient::new(e, &token_id.address()).mint(&admin, &(i128::MAX / 4));

    (client, admin, token_id.address(), token_client)
}

fn assert_balance_invariants(client: &CredenceTreasuryClient<'_>, token_client: &TaxedTokenClient<'_>) {
    let contract_balance = token_client.balance(&client.address);
    let total = client.get_balance();
    let sum = client.get_balance_by_source(&FundSource::ProtocolFee)
        + client.get_balance_by_source(&FundSource::SlashedFunds);
    assert_eq!(contract_balance, total, "token balance must equal booked total");
    assert_eq!(sum, total, "per-source sum must equal booked total");
}

// ---------------------------------------------------------------------------
// set_token guard
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #612)")]
fn set_token_rejected_with_nonzero_balance() {
    let e = Env::default();
    let (client, admin, _token_id, _taxed) = setup_taxed(&e);

    client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);
    assert!(client.get_balance() > 0);

    let new_token = Address::generate(&e);
    client.set_token(&admin, &new_token);
}

#[test]
fn set_token_allowed_when_empty() {
    let e = Env::default();
    let (client, admin, _token_id, _taxed) = setup_taxed(&e);

    assert_eq!(client.get_balance(), 0);
    let new_token = Address::generate(&e);
    client.set_token(&admin, &new_token);
    assert_eq!(client.get_token(), new_token);
}

// ---------------------------------------------------------------------------
// Inflow books the treasury's received delta
// ---------------------------------------------------------------------------

#[test]
fn fee_on_transfer_inflow_books_received_amount() {
    let e = Env::default();
    let (client, admin, _token_id, token_client) = setup_taxed(&e);

    client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);

    // 1% is burned in flight, so the treasury receives 990 and must book 990.
    assert_eq!(token_client.balance(&client.address), 990);
    assert_eq!(client.get_balance(), 990);
    assert_balance_invariants(&client, &token_client);
}

// ---------------------------------------------------------------------------
// Outflow books the treasury's debit
// ---------------------------------------------------------------------------

#[test]
fn fee_on_transfer_outflow_books_treasury_debit() {
    let e = Env::default();
    let (client, admin, _token_id, token_client) = setup_taxed(&e);

    client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 990);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let id = client.propose_withdrawal(&signer, &recipient, &500);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);

    // Treasury debited the full 500 even though the recipient received 495.
    assert_eq!(token_client.balance(&recipient), 495);
    assert_eq!(token_client.balance(&client.address), 490);
    assert_eq!(client.get_balance(), 490);
    assert_balance_invariants(&client, &token_client);
}

#[test]
fn settle_books_treasury_debit() {
    let e = Env::default();
    let (client, admin, _token_id, token_client) = setup_taxed(&e);

    client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 990);

    let destination = Address::generate(&e);
    client.register_corridor(&admin, &destination);

    let returned = client.settle(&admin, &destination, &500);

    // Returned value is the destination's received delta (slippage view),
    // while the treasury books its full debit of 500.
    assert_eq!(returned, 495);
    assert_eq!(token_client.balance(&destination), 495);
    assert_eq!(token_client.balance(&client.address), 490);
    assert_eq!(client.get_balance(), 490);
    assert_balance_invariants(&client, &token_client);
}

// ---------------------------------------------------------------------------
// End-to-end invariant across a mixed sequence
// ---------------------------------------------------------------------------

#[test]
fn balance_invariant_after_sequence() {
    let e = Env::default();
    let (client, admin, _token_id, token_client) = setup_taxed(&e);

    // receive
    client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);
    assert_balance_invariants(&client, &token_client);

    // receive (slashed)
    client.receive_fee(&admin, &2_000, &FundSource::SlashedFunds);
    assert_balance_invariants(&client, &token_client);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    // withdraw
    let id = client.propose_withdrawal(&signer, &recipient, &300);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &0);
    assert_balance_invariants(&client, &token_client);

    // settle
    let destination = Address::generate(&e);
    client.register_corridor(&admin, &destination);
    client.settle(&admin, &destination, &400);
    assert_balance_invariants(&client, &token_client);

    // receive again
    client.receive_fee(&admin, &500, &FundSource::ProtocolFee);
    assert_balance_invariants(&client, &token_client);
}

// ---------------------------------------------------------------------------
// Standard token: deltas agree, so behaviour is unchanged
// ---------------------------------------------------------------------------

#[test]
fn standard_token_sequence_keeps_books_equal_to_holdings() {
    let e = Env::default();
    let (client, admin, _token_id, token_client) = setup_standard(&e);

    client.receive_fee(&admin, &1_000, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 1_000);
    assert_eq!(token_client.balance(&client.address), 1_000);
    assert_eq!(
        client.get_balance_by_source(&FundSource::ProtocolFee),
        1_000
    );

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let id = client.propose_withdrawal(&signer, &recipient, &400);
    client.approve_withdrawal(&signer, &id);
    client.execute_withdrawal(&id, &400);

    assert_eq!(client.get_balance(), 600);
    assert_eq!(token_client.balance(&client.address), 600);
    assert_eq!(token_client.balance(&recipient), 400);
}
