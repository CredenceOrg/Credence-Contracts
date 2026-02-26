//! Shared test helpers for fixed_duration_bond tests.

use crate::{FixedDurationBond, FixedDurationBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env};

/// Default mint: large enough for all test scenarios.
pub const DEFAULT_MINT: i128 = 100_000_000_000_000;

/// One day in seconds.
pub const ONE_DAY: u64 = 86_400;
/// One week in seconds.
pub const ONE_WEEK: u64 = 604_800;

/// Full environment setup: deploys contract + token, mints to `owner`, approves contract.
/// Returns `(client, admin, owner, token_address, contract_id)`.
pub fn setup(
    e: &Env,
) -> (
    FixedDurationBondClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    setup_with_mint(e, DEFAULT_MINT)
}

pub fn setup_with_mint(
    e: &Env,
    mint_amount: i128,
) -> (
    FixedDurationBondClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    e.mock_all_auths();

    let contract_id = e.register(FixedDurationBond, ());
    let client = FixedDurationBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let owner = Address::generate(e);

    let stellar_asset = e
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let asset_admin = StellarAssetClient::new(e, &stellar_asset);
    asset_admin.set_authorized(&owner, &true);
    asset_admin.mint(&owner, &mint_amount);

    let token = TokenClient::new(e, &stellar_asset);
    let expiry_ledger = e.ledger().sequence().saturating_add(10_000);
    token.approve(&owner, &contract_id, &mint_amount, &expiry_ledger);

    client.initialize(&admin, &stellar_asset);

    (client, admin, owner, stellar_asset, contract_id)
}
