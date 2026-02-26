//! Shared test helpers for credence_bond tests.
//! Provides token setup for tests that need create_bond, top_up, withdraw, etc.

#![cfg(test)]

use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env};

/// Default mint amount for tests (covers tier thresholds and most scenarios).
const DEFAULT_MINT: i128 = 100_000_000_000_000_000;

/// Mint amount for tests needing i128::MAX (e.g. overflow tests).
const MAX_MINT: i128 = i128::MAX;

/// Setup bond contract with Stellar Asset token.
/// Mints `mint_amount` to identity and approves contract.
/// Returns (client, admin, identity, token_address, bond_contract_id).
pub fn setup_with_token(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    setup_with_token_mint(e, DEFAULT_MINT)
}

/// Setup with max mint for overflow/edge case tests.
pub fn setup_with_max_mint(
    e: &Env,
) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    setup_with_token_mint(e, MAX_MINT)
}

/// Setup with custom mint amount (for tests needing very large amounts).
pub fn setup_with_token_mint(
    e: &Env,
    mint_amount: i128,
) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let identity = Address::generate(e);

    client.initialize(&admin);

    let stellar_asset = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let stellar_client = StellarAssetClient::new(e, &stellar_asset);
    stellar_client.set_authorized(&identity, &true);
    stellar_client.mint(&identity, &mint_amount);

    let token_client = TokenClient::new(e, &stellar_asset);
    let expiration = e.ledger().sequence().saturating_add(10000);
    token_client.approve(&identity, &contract_id, &mint_amount, &expiration);

    client.set_token(&admin, &stellar_asset);

    (client, admin, identity, stellar_asset, contract_id)
}
