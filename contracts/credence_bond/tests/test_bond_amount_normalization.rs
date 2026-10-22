//! Integration coverage for issue #1612: bond bounds and leverage checks must
//! run on the 18-decimal normalized amount, not the raw token units.
//!
//! The library links without `cfg(test)` here, so these tests exercise the
//! production `MIN_BOND_AMOUNT` / `MAX_BOND_AMOUNT` constants against a real
//! 7-decimal Stellar Asset Contract (like USDC).
#![cfg(test)]

use credence_bond::soroban_sdk::testutils::Address as _;
use credence_bond::soroban_sdk::token::{StellarAssetClient, TokenClient};
use credence_bond::soroban_sdk::{Address, Env, Vec};
use credence_bond::{CredenceBond, CredenceBondClient};

/// 1 token in the 7-decimal units used by Stellar Asset Contracts.
const ONE_TOKEN: i128 = 10_000_000;
/// Production maximum bond: 100M tokens.
const MAX_TOKENS: i128 = 100_000_000;

/// Bond contract wired to a real 7-decimal SAC (USDC-like).
fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address) {
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);

    let admin = Address::generate(e);
    client.initialize(&admin, &None);

    let token_id = e.register_stellar_asset_contract_v2(admin.clone()).address();
    let mut accepted = Vec::new(e);
    accepted.push_back(token_id.clone());
    client.set_accepted_tokens(&admin, &accepted);
    client.set_token(&admin, &token_id);

    (client, admin, contract_id, token_id)
}

/// Mint `amount` 7-decimal units to `who` and approve the bond contract.
fn fund(e: &Env, token_id: &Address, who: &Address, amount: i128, spender: &Address) {
    StellarAssetClient::new(e, token_id).mint(who, &amount);
    TokenClient::new(e, token_id).approve(who, spender, &i128::MAX, &99999_u32);
}

#[test]
fn create_bond_accepts_one_token_at_7_decimals() {
    let e = Env::default();
    let (client, _admin, contract_id, token_id) = setup(&e);

    let bob = Address::generate(&e);
    fund(&e, &token_id, &bob, 1_000_000 * ONE_TOKEN, &contract_id);

    // 1 token == the 18-decimal normalized minimum.
    let bond = client.create_bond(&bob, &ONE_TOKEN, &86_400_u64, &false, &0_u64);

    assert!(bond.active);
    assert_eq!(bond.bonded_amount, ONE_TOKEN);
    assert_eq!(
        TokenClient::new(&e, &token_id).balance(&bob),
        1_000_000 * ONE_TOKEN - ONE_TOKEN
    );
}

#[test]
fn create_bond_rejects_below_one_token_at_7_decimals() {
    let e = Env::default();
    let (client, _admin, contract_id, token_id) = setup(&e);

    let bob = Address::generate(&e);
    let funded = 1_000_000 * ONE_TOKEN;
    fund(&e, &token_id, &bob, funded, &contract_id);

    assert!(client
        .try_create_bond(&bob, &(ONE_TOKEN - 1), &86_400_u64, &false, &0_u64)
        .is_err());
    // The rejected bond never transfers tokens.
    assert_eq!(TokenClient::new(&e, &token_id).balance(&bob), funded);
}

#[test]
fn create_bond_rejects_above_max_at_7_decimals() {
    let e = Env::default();
    let (client, _admin, contract_id, token_id) = setup(&e);

    let bob = Address::generate(&e);
    let amount = MAX_TOKENS * ONE_TOKEN + 1;
    fund(&e, &token_id, &bob, amount, &contract_id);

    assert!(client
        .try_create_bond(&bob, &amount, &86_400_u64, &false, &0_u64)
        .is_err());
    assert_eq!(TokenClient::new(&e, &token_id).balance(&bob), amount);
}

#[test]
fn top_up_bounds_are_normalized() {
    let e = Env::default();
    let (client, _admin, contract_id, token_id) = setup(&e);

    let bob = Address::generate(&e);
    fund(&e, &token_id, &bob, 100_000_002 * ONE_TOKEN, &contract_id);

    // 1-token bond at 7 decimals is the production minimum.
    let bond = client.create_bond(&bob, &ONE_TOKEN, &86_400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, ONE_TOKEN);

    // A second 1-token top-up must survive the normalized bounds check.
    let bond = client.top_up(&bob, &ONE_TOKEN);
    assert_eq!(bond.bonded_amount, 2 * ONE_TOKEN);

    // Pushing the total past the 100M-token cap reverts.
    assert!(client.try_top_up(&bob, &(MAX_TOKENS * ONE_TOKEN)).is_err());
}

#[test]
fn leverage_uses_normalized_amount() {
    let e = Env::default();
    let (client, admin, contract_id, token_id) = setup(&e);
    client.set_max_leverage(&admin, &10_u32);

    let bob = Address::generate(&e);
    let carol = Address::generate(&e);
    fund(&e, &token_id, &bob, 11 * ONE_TOKEN, &contract_id);
    fund(&e, &token_id, &carol, 11 * ONE_TOKEN, &contract_id);

    // 10 tokens -> leverage 10x == cap: accepted.
    let bond = client.create_bond(&bob, &(10 * ONE_TOKEN), &86_400_u64, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 10 * ONE_TOKEN);

    // 11 tokens -> leverage 11x > cap: rejected with LeverageExceeded.
    assert!(client
        .try_create_bond(&carol, &(11 * ONE_TOKEN), &86_400_u64, &false, &0_u64)
        .is_err());
}
