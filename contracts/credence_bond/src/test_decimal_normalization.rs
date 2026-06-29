#![cfg(test)]

//! Comprehensive tests for decimal normalization across different token configurations.
//! Tests verify that the bond contract correctly handles tokens with 6, 8, 18, and 24 decimals.

use crate::{BondTier, CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

#[contract]
pub struct MockDecimalToken;

#[contractimpl]
impl MockDecimalToken {
    pub fn decimals(e: Env) -> u32 {
        e.storage()
            .instance()
            .get(&Symbol::new(&e, "decimals"))
            .unwrap_or(18)
    }
    pub fn balance(e: Env, id: Address) -> i128 {
        e.storage()
            .instance()
            .get(&id)
            .unwrap_or(10_000_000_000_000_000_000_000_000_000_i128)
    }
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        let from_bal = Self::balance(e.clone(), from.clone());
        let to_bal = Self::balance(e.clone(), to.clone());
        e.storage().instance().set(&from, &(from_bal - amount));
        e.storage().instance().set(&to, &(to_bal + amount));
    }
    pub fn transfer_from(e: Env, _spender: Address, from: Address, to: Address, amount: i128) {
        Self::transfer(e, from, to, amount);
    }
    pub fn allowance(_e: Env, _from: Address, _spender: Address) -> i128 {
        i128::MAX
    }
}

fn setup_token_with_decimals(
    e: &Env,
    decimals: u32,
) -> (CredenceBondClient<'_>, Address, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(&e);
    let identity = Address::generate(e);

    client.initialize(&admin, &None);

    let token_id = e.register(MockDecimalToken, ());
    e.as_contract(&token_id, || {
        e.storage()
            .instance()
            .set(&Symbol::new(e, "decimals"), &decimals);
    });

    client.set_token(&admin, &token_id);
    (client, admin, identity, token_id)
}

#[test]
fn test_6_decimal_bond_creation() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_token_with_decimals(&e, 6);

    let native_amount = 1_000_000_000_i128;
    let bond = client.create_bond_with_rolling(&identity, &native_amount, &86400, &false, &0);

    let expected_normalized = 1_000_000_000_000_000_000_000_i128;
    assert_eq!(bond.bonded_amount, expected_normalized);
    assert_eq!(client.get_tier(), BondTier::Silver);
}

#[test]
fn test_6_decimal_withdrawal() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_token_with_decimals(&e, 6);

    let native_amount = 1_000_000_000_i128;
    client.create_bond_with_rolling(&identity, &native_amount, &86400, &false, &0);

    e.ledger().with_mut(|l| l.timestamp = 100_000);

    let withdraw_amount_normalized = 400_000_000_000_000_000_000_i128;
    let bond = client.withdraw_bond(&withdraw_amount_normalized);

    let expected_remaining = 600_000_000_000_000_000_000_i128;
    assert_eq!(bond.bonded_amount, expected_remaining);
}

#[test]
fn test_8_decimal_bond_creation() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_token_with_decimals(&e, 8);

    let native_amount = 100_000_000_000_i128;
    let bond = client.create_bond_with_rolling(&identity, &native_amount, &86400, &false, &0);

    let expected_normalized = 1_000_000_000_000_000_000_000_i128;
    assert_eq!(bond.bonded_amount, expected_normalized);
    assert_eq!(client.get_tier(), BondTier::Silver);
}

#[test]
fn test_18_decimal_bond_creation() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_token_with_decimals(&e, 18);

    let native_amount = 1_000_000_000_000_000_000_000_i128;
    let bond = client.create_bond_with_rolling(&identity, &native_amount, &86400, &false, &0);

    let expected_normalized = 1_000_000_000_000_000_000_000_i128;
    assert_eq!(bond.bonded_amount, expected_normalized);
    assert_eq!(client.get_tier(), BondTier::Silver);
}

#[test]
fn test_invariant_preservation_across_decimals() {
    let decimals_list = [6, 8, 18];

    for decimals in decimals_list {
        let e = Env::default();
        let (client, _admin, identity, _token) = setup_token_with_decimals(&e, decimals);

        let native_amount = match decimals {
            6 => 1_000_000_000_i128,
            8 => 100_000_000_000_i128,
            18 => 1_000_000_000_000_000_000_000_i128,
            _ => panic!("unsupported decimals"),
        };

        let bond = client.create_bond_with_rolling(&identity, &native_amount, &86400, &false, &0);

        let expected_normalized = 1_000_000_000_000_000_000_000_i128;
        assert_eq!(
            bond.bonded_amount, expected_normalized,
            "Failed for {} decimals",
            decimals
        );
        assert_eq!(
            client.get_tier(),
            BondTier::Silver,
            "Tier mismatch for {} decimals",
            decimals
        );
    }
}

#[test]
fn test_minimum_bond_amount_6_decimals() {
    let e = Env::default();
    let (client, _admin, identity, _token) = setup_token_with_decimals(&e, 6);

    let e1 = Env::default();
    let (client1, _admin1, identity1, _token1) = setup_token_with_decimals(&e1, 18);

    // Min bond is 1 token normalized = 10^18
    // For 6 decimals: 10^18 / 10^12 = 10^6 = 1,000,000
    let min_native_6 = 1_000_000_i128;
    let bond = client.create_bond_with_rolling(&identity, &min_native_6, &86400, &false, &0);
    assert_eq!(bond.bonded_amount, 1_000_000_000_000_000_000_i128);

    // For 18 decimals: 10^18
    let min_native_18 = 1_000_000_000_000_000_000_i128;
    let bond1 = client1.create_bond_with_rolling(&identity1, &min_native_18, &86400, &false, &0);
    assert_eq!(bond1.bonded_amount, 1_000_000_000_000_000_000_i128);
}

#[test]
fn test_tier_boundaries_with_different_decimals() {
    // Test Bronze -> Silver boundary (1000 tokens)
    for decimals in [6, 8, 18] {
        let e = Env::default();
        let (client, _admin, identity, _token) = setup_token_with_decimals(&e, decimals);

        let native_1000_tokens = match decimals {
            6 => 1_000_000_000_i128,
            8 => 100_000_000_000_i128,
            18 => 1_000_000_000_000_000_000_000_i128,
            _ => panic!("unsupported"),
        };

        let bond =
            client.create_bond_with_rolling(&identity, &native_1000_tokens, &86400, &false, &0);
        assert_eq!(
            client.get_tier(),
            BondTier::Silver,
            "1000 tokens should be Silver tier for {} decimals",
            decimals
        );
        assert_eq!(bond.bonded_amount, 1_000_000_000_000_000_000_000_i128);
    }
}
