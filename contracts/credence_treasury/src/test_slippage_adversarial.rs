#![cfg(test)]

use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol};

// --- Mock Taxed Token ---
// This token simulates "slippage" by taking a fee on every transfer.
#[contract]
pub struct TaxedToken;

#[contractimpl]
impl TaxedToken {
    pub fn initialize(e: Env, admin: Address) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "admin"), &admin);
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "tax"), &100_i128); // 1% tax (basis points: 100/10000)
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

        // The tax is "lost" or burned in this simple mock to simulate slippage
    }
}

// --- Test Suite ---

fn setup_adversarial(e: &Env) -> (CredenceTreasuryClient<'_>, Address, Address) {
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);

    let token_id = e.register(TaxedToken, ());
    let token_client = TaxedTokenClient::new(e, &token_id);
    token_client.initialize(&admin);

    e.mock_all_auths();
    client.initialize(&admin, &token_id);

    // Give admin tokens so they can deposit
    token_client.mint(&admin, &(i128::MAX / 2));

    (client, admin, token_id)
}

#[test]
#[should_panic(expected = "Error(Contract, #609)")]
fn test_withdrawal_fails_when_tax_causes_slippage() {
    let e = Env::default();
    let (client, admin, _token_id) = setup_adversarial(&e);

    // The treasury books only what it actually received: 1% is burned in
    // flight, so a 10,000 request credits 9,900.
    let amount = 10_000_i128;
    client.receive_fee(&admin, &amount, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 9_900);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let id = client.propose_withdrawal(&signer, &recipient, &9_900);
    client.approve_withdrawal(&signer, &id);

    // 1% tax on 9,900 is 99, so the recipient receives 9,801 while the
    // treasury is debited the full 9,900. A 9,802 floor must revert.
    client.execute_withdrawal(&id, &9_802);
}

#[test]
#[should_panic(expected = "Error(Contract, #609)")]
fn test_slippage_revert_with_taxed_token() {
    let e = Env::default();
    let (client, admin, _token_id) = setup_adversarial(&e);

    let amount = 10_000_i128;
    client.receive_fee(&admin, &amount, &FundSource::ProtocolFee);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let id = client.propose_withdrawal(&signer, &recipient, &9_900);
    client.approve_withdrawal(&signer, &id);

    // Tax is 1%. Requested 9,900. Delivered 9,801.
    // Minimum 9,802 -> Revert.
    client.execute_withdrawal(&id, &9_802);
}

#[test]
fn test_slippage_succeeds_at_threshold_with_taxed_token() {
    let e = Env::default();
    let (client, admin, token_id) = setup_adversarial(&e);
    let token_client = TaxedTokenClient::new(&e, &token_id);

    let amount = 10_000_i128;
    client.receive_fee(&admin, &amount, &FundSource::ProtocolFee);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let id = client.propose_withdrawal(&signer, &recipient, &9_900);
    client.approve_withdrawal(&signer, &id);

    // Tax is 1%. Requested 9,900. Delivered 9,801.
    // Minimum 9,801 -> Success.
    client.execute_withdrawal(&id, &9_801);

    // All 9,900 booked units left the treasury; the recipient got 9,801.
    assert_eq!(client.get_balance(), 0);
    assert_eq!(token_client.balance(&recipient), 9_801);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
fn test_slippage_succeeds_well_below_threshold_with_taxed_token() {
    let e = Env::default();
    let (client, admin, token_id) = setup_adversarial(&e);
    let token_client = TaxedTokenClient::new(&e, &token_id);

    let amount = 10_000_i128;
    client.receive_fee(&admin, &amount, &FundSource::ProtocolFee);

    let signer = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let id = client.propose_withdrawal(&signer, &recipient, &9_900);
    client.approve_withdrawal(&signer, &id);

    // Minimum 5,000 -> Success.
    client.execute_withdrawal(&id, &5_000);

    // Treasury-side accounting: the full 9,900 debit is booked, leaving zero.
    assert_eq!(client.get_balance(), 0);
    assert_eq!(token_client.balance(&client.address), 0);
}
