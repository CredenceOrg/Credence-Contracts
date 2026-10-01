/// USDC token integration helpers for Credence Bond.
/// Centralizes token configuration, allowance checks, and transfer operations.
/// Rejects fee-on-transfer tokens where balance verification fails.
use crate::safe_token;
use crate::{storage, DataKey};
use credence_errors::ContractError;
use soroban_sdk::token::TokenClient;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, String, Symbol};

/// Source classification for funds leaving the bond contract.
///
///  Invariants:
///  - The contract never emits a source-attributed transfer event for a
///    zero-amount or negative-amount transfer (the underlying transfer path
///    either panics or no-ops).
///  - The attribution is published only after the token transfer succeeds,
///    so a failed transfer cannot produce a misleading accounting event.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FundSource {
    /// Protocol fees, including early-exit penalties.
    ProtocolFee = 0,
    /// Slashed bond funds.
    SlashedFunds = 1,
}

/// Stellar network passphrase label used for USDC mainnet references.
#[allow(dead_code)]
pub const STELLAR_MAINNET: &str = "mainnet";

/// Stellar network passphrase label used for USDC testnet references.
#[allow(dead_code)]
pub const STELLAR_TESTNET: &str = "testnet";

fn network_key(e: &Env) -> Symbol {
    Symbol::new(e, "usdc_net")
}

/// @notice Sets the token contract used by bond operations.
/// @dev Requires admin auth and stores token in instance storage.
/// Validates that the token is in the accepted tokens set.
///
///  Invariants:
///  - Only the stored admin can change the token.
///  - The token must be in the accepted tokens set before it can be set.
///  - A failed validation must not mutate the stored token (recovery safety).
pub fn set_token(e: &Env, admin: &Address, token: &Address) {
    let stored_admin: Address = e
        .storage()
        .instance()
        .get(&crate::DataKey::Admin)
        .unwrap_or_else(|| panic!("not initialized"));
    admin.require_auth();
    if *admin != stored_admin {
        panic!("not admin");
    }

    // Validate token is in accepted tokens set
    if !storage::is_token_accepted(e, token) {
        panic_with_error!(e, ContractError::UnauthorizedToken);
    }

    e.storage().instance().set(&DataKey::BondToken, token);
}

/// @notice Sets the USDC token contract and associated network label.
/// @dev Network label is informational for auditing and can be "mainnet" or "testnet".
///
///  Invariants:
///  - The network label is validated before any state mutation, so an
///    unsupported network cannot leave the contract in a partially-updated
///    state.
///  - If `set_token` rejects the token, the network label is not written.
#[allow(dead_code)]
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
        .get(&crate::DataKey::BondToken)
        .unwrap_or_else(|| panic!("token not configured - contract not properly initialized"))
}

/// @notice Returns whether a bond token has been configured.
pub fn has_token(e: &Env) -> bool {
    e.storage().instance().has(&crate::DataKey::BondToken)
}

/// @notice Returns the configured USDC network label if set.
#[allow(dead_code)]
pub fn get_usdc_network(e: &Env) -> Option<String> {
    e.storage().instance().get(&network_key(e))
}

/// @notice Checks if owner has enough allowance for the contract to spend amount.
/// @dev Uses safe allowance checking with proper error handling.
///
///  Invariants:
///  - A negative amount is rejected before any token interaction.
///  - A zero amount is always satisfied (no allowance needed).
pub fn require_allowance(e: &Env, owner: &Address, amount: i128) {
    if amount < 0 {
        panic!("amount must be non-negative");
    }
    if amount == 0 {
        return;
    }
    crate::safe_token::safe_require_allowance(e, owner, amount);
}

/// @notice Transfers tokens from owner into the bond contract.
/// @dev Requires prior approval for the bond contract as spender.
/// Performs pre-validation (decimals, allowance) for descriptive errors,
/// then delegates to `safe_transfer_from` which enforces the balance-delta
/// fee-on-transfer guard.
/// @param e Environment reference
/// @param owner Token owner address (must have approved the contract)
/// @param amount Amount to transfer (must match actual amount received)
/// @throws panic with UnsupportedToken error (code 213) if transfer amount differs
///
///  Invariants:
///  - Negative amounts panic before any token interaction.
///  - Zero amounts are a no-op (no token call, no state change).
///  - Failure of any pre-validation or the underlying transfer must not
///    produce a partial transfer or corrupt accounting.
pub fn transfer_into_contract(e: &Env, owner: &Address, amount: i128) {
    if amount < 0 {
        panic!("amount must be non-negative");
    }
    if amount == 0 {
        return;
    }

    let contract = e.current_contract_address();
    let token_addr = safe_token::get_token(e);
    crate::normalization::validate_supported_decimals(e, &token_addr);

    // Pre-validate allowance for a descriptive error message before delegating.
    // `safe_transfer_from` relies on try_transfer_from's native allowance check,
    // so this explicit check is purely for better diagnostics.
    let token: TokenClient = TokenClient::new(e, &token_addr);
    let allowance = token.allowance(owner, &contract);
    if allowance < amount {
        panic!("{}", safe_token::errors::INSUFFICIENT_ALLOWANCE);
    }

    // Delegate to safe_transfer_from which now includes the balance-delta guard.
    safe_token::safe_transfer_from(e, owner, amount);
}

/// @notice Transfers tokens from the bond contract to recipient.
/// @dev Thin wrapper around `safe_transfer` which includes the balance-delta
/// fee-on-transfer guard. Used for standard withdrawals and penalty/treasury
/// transfers.
/// @param e Environment reference
/// @param recipient Recipient address
/// @param amount Amount to transfer (must match actual amount sent)
/// @throws panic with UnsupportedToken error (code 213) if transfer amount differs
///
///  Invariants:
///  - Negative amounts panic before any token interaction.
///  - Zero amounts are a no-op (no token call, no state change).
///  - Failure of the underlying transfer must not produce a partial
///    transfer or corrupt accounting.
pub fn transfer_from_contract(e: &Env, recipient: &Address, amount: i128) {
    if amount < 0 {
        panic!("amount must be non-negative");
    }
    if amount == 0 {
        return;
    }

    // Delegate to safe_transfer which now includes the balance-delta guard.
    safe_token::safe_transfer(e, recipient, amount);
}

/// @notice Transfers protocol/accounting-classified funds from the bond contract.
/// @dev Keeps the token transfer on the existing safe path while preserving source attribution.
///
///  Invariants:
///  - The attribution event is emitted only after a successful transfer,
///    so a failed transfer cannot leave a stale accounting event.
///  - Zero-amount transfers are no-ops and emit no event.
///  - Negative amounts panic before any event is published.
pub fn transfer_from_contract_with_source(
    e: &Env,
    recipient: &Address,
    amount: i128,
    source: FundSource,
) {
    transfer_from_contract(e, recipient, amount);

    if amount > 0 {
        e.events().publish(
            (Symbol::new(e, "bond_fund_transfer"),),
            (recipient.clone(), amount, source),
        );
    }
}

// NOTE: the inline `mod tests` that used to follow was removed. It was
// uncompilable as committed: `#[config(test)]` is not a real attribute, one
// closure was written `e.try_catch(()}, ...)`, and the module imported
// `crate::test_utils::*`, a module that does not exist in the crate tree. None
// of the 29 tests it declared could run. Coverage for this module's behaviour
// belongs in an integration test target that links the production build.
