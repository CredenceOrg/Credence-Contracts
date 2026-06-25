use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct MockStellarAsset;

#[contractimpl]
impl MockStellarAsset {
    pub fn decimals(_e: Env) -> u32 {
        18
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
    pub fn approve(_e: Env, _from: Address, _spender: Address, _amount: i128, _expiration: u32) {
        // no-op
    }
    pub fn mint(e: Env, to: Address, amount: i128) {
        let current = Self::balance(e.clone(), to.clone());
        e.storage().instance().set(&to, &(current + amount));
    }
    pub fn set_authorized(_e: Env, _id: Address, _auth: bool) {
        // no-op
    }
}

/// Advance ledger sequence (test utility). Slashing is rejected in the same ledger as the last
/// collateral increase; call this after `create_bond` / `top_up` / `increase_bond` when a test
/// needs an immediate slash in the following ledger.
pub fn advance_ledger_sequence(e: &Env) {
    let mut info = e.ledger().get();
    info.sequence_number = info.sequence_number.saturating_add(1);
    e.ledger().set(info);
}

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
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let identity = Address::generate(e);

    client.initialize(&admin, &None);

    let stellar_asset = e.register(MockStellarAsset, ());
    let stellar_client = StellarAssetClient::new(e, &stellar_asset);
    stellar_client.set_authorized(&identity, &true);
    stellar_client.mint(&identity, &mint_amount);

    let token_client = TokenClient::new(e, &stellar_asset);
    let expiration = e.ledger().sequence().saturating_add(10000);
    token_client.approve(&identity, &contract_id, &mint_amount, &expiration);

    e.as_contract(&contract_id, || {
        e.storage()
            .instance()
            .set(&crate::DataKey::BondToken, &stellar_asset);
    });

    (client, admin, identity, stellar_asset, contract_id)
}
