#![cfg(test)]

use crate::{CredenceBond, CredenceBondClient, BondTier};
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use soroban_sdk::testutils::{Address as _, Ledger as _};

#[contract]
pub struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn decimals(e: Env) -> u32 {
        e.storage().instance().get(&Symbol::new(&e, "decimals")).unwrap_or(7)
    }
    pub fn balance(_e: Env, _id: Address) -> i128 { 1_000_000_000_000_000_000_000_000_i128 }
    pub fn transfer(_e: Env, _from: Address, _to: Address, _amount: i128) {}
    pub fn transfer_from(_e: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {}
    pub fn allowance(_e: Env, _from: Address, _spender: Address) -> i128 { i128::MAX }
}

fn setup_with_decimals(e: &Env, decimals: u32) -> (CredenceBondClient<'_>, Address, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(&e);
    let identity = Address::generate(e);

    client.initialize(&admin);

    let token_id = e.register(MockToken, ());
    // Set decimals for the mock token
    e.as_contract(&token_id, || {
        e.storage().instance().set(&Symbol::new(e, "decimals"), &decimals);
    });

    client.set_token(&admin, &token_id);
    (client, admin, identity, token_id)
}

#[test]
fn test_tier_silver_with_6_decimals() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_with_decimals(&e, 6);

    // 1000 tokens in 6 decimals = 1,000,000,000
    let amount = 1_000_000_000;
    let bond = client.create_bond_with_rolling(&identity, &amount, &86400, &false, &0);
    
    // Silver tier starts at 1000 tokens (normalized: 10^21)
    assert_eq!(client.get_tier(), BondTier::Silver);
    assert_eq!(bond.bonded_amount, 1_000_000_000_000_000_000_000); // 1000 * 10^18
}

#[test]
fn test_tier_silver_with_18_decimals() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_with_decimals(&e, 18);

    // 1000 tokens in 18 decimals = 1,000 * 10^18
    let amount = 1_000_000_000_000_000_000_000;
    let bond = client.create_bond_with_rolling(&identity, &amount, &86400, &false, &0);
    
    assert_eq!(client.get_tier(), BondTier::Silver);
    assert_eq!(bond.bonded_amount, 1_000_000_000_000_000_000_000); 
}

#[test]
fn test_tier_silver_with_8_decimals() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_with_decimals(&e, 8);

    // 1000 tokens in 8 decimals = 1000 * 10^8
    let amount = 100_000_000_000;
    let _bond = client.create_bond_with_rolling(&identity, &amount, &86400, &false, &0);
    
    assert_eq!(client.get_tier(), BondTier::Silver);
}

#[test]
#[should_panic(expected = "token decimals exceeds supported maximum of 18")]
fn test_24_decimals_panics() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_with_decimals(&e, 24);

    let amount = 1_000_000_000;
    client.create_bond_with_rolling(&identity, &amount, &86400, &false, &0);
}

#[test]
fn test_withdraw_correct_amount_6_decimals() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_with_decimals(&e, 6);

    let amount = 1_000_000_000; // 1000 tokens
    client.create_bond_with_rolling(&identity, &amount, &86400, &false, &0);
    
    // Request withdrawal to start cooldown
    client.request_withdrawal();
    
    // Fast forward - skip in SDK 23, use large duration to bypass lockup check
    let bond = client.withdraw_bond(&400_000_000);
    
    // 600 tokens left (normalized: 600 * 10^18)
    assert_eq!(bond.bonded_amount, 600_000_000_000_000_000_000);
}

#[test]
#[should_panic(expected = "bond amount below minimum required")]
fn test_validation_bounds_18_decimals() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_with_decimals(&e, 18);

    // Min bond is 1 token = 10^18
    let too_small = 999_999_999_999_999_999;
    client.create_bond_with_rolling(&identity, &too_small, &86400, &false, &0);
}
