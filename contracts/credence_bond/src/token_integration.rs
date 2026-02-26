//! USDC token integration helpers for Credence Bond.
//! Centralizes token configuration, allowance checks, and transfer operations.

use crate::DataKey;
use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env, String, Symbol};

/// Stellar network passphrase label used for USDC mainnet references.
pub const STELLAR_MAINNET: &str = "mainnet";

/// Stellar network passphrase label used for USDC testnet references.
pub const STELLAR_TESTNET: &str = "testnet";

fn network_key(e: &Env) -> Symbol {
    Symbol::new(e, "usdc_net")
}

fn token_client(e: &Env) -> TokenClient<'_> {
    let token = get_token(e);
    TokenClient::new(e, &token)
}

/// @notice Sets the token contract used by bond operations.
/// @dev Requires admin auth and stores token in instance storage.
pub fn set_token(e: &Env, admin: &Address, token: &Address) {
    let stored_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic!("not initialized"));
    admin.require_auth();
    if *admin != stored_admin {
        panic!("not admin");
    }
    e.storage().instance().set(&DataKey::Token, token);
}

/// @notice Sets the USDC token contract and associated network label.
/// @dev Network label is informational for auditing and can be "mainnet" or "testnet".
pub fn set_usdc_token(e: &Env, admin: &Address, token: &Address, network: &String) {
    if *network != String::from_str(e, STELLAR_MAINNET)
        && *network != String::from_str(e, STELLAR_TESTNET)
    {
        panic!("unsupported stellar network");
    }
    set_token(e, admin, token);
    e.storage().instance().set(&network_key(e), network);
    e.events().publish(
        (Symbol::new(e, "usdc_token_set"),),
        (token.clone(), network.clone()),
    );
}

/// @notice Returns the configured token address.
/// @dev Panics if token has not been configured.
pub fn get_token(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Token)
        .unwrap_or_else(|| panic!("token not set"))
}

/// @notice Returns the configured USDC network label if set.
pub fn get_usdc_network(e: &Env) -> Option<String> {
    e.storage().instance().get(&network_key(e))
}

/// @notice Checks if owner has enough allowance for the contract to spend amount.
/// @dev Uses token allowance(owner, spender) where spender is the bond contract.
pub fn require_allowance(e: &Env, owner: &Address, amount: i128) {
    if amount < 0 {
        panic!("amount must be non-negative");
    }
    if amount == 0 {
        return;
    }
    let contract = e.current_contract_address();
    let allowance = token_client(e).allowance(owner, &contract);
    if allowance < amount {
        panic!("insufficient token allowance");
    }
}

/// @notice Transfers tokens from owner into the bond contract.
/// @dev Requires prior approval for the bond contract as spender.
pub fn transfer_into_contract(e: &Env, owner: &Address, amount: i128) {
    if amount < 0 {
        panic!("amount must be non-negative");
    }
    if amount == 0 {
        return;
    }

    require_allowance(e, owner, amount);
    let contract = e.current_contract_address();
    token_client(e).transfer_from(&contract, owner, &contract, &amount);
}

/// @notice Transfers tokens from the bond contract to recipient.
/// @dev Used for standard withdrawals and penalty/treasury transfers.
pub fn transfer_from_contract(e: &Env, recipient: &Address, amount: i128) {
    if amount < 0 {
        panic!("amount must be non-negative");
    }
    if amount == 0 {
        return;
    }

    let contract = e.current_contract_address();
    token_client(e).transfer(&contract, recipient, &amount);
}
