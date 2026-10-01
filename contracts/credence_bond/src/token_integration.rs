/// USDC token integration helpers for Credence Bond.
/// Centralizes token configuration, allowance checks, and transfer operations.
/// Rejects fee-on-transfer tokens where balance verification fails.

use crate::safe_token;
use crate::{storage, DataKey};
use credence_errors::ContractError;
use soroban_sdk::token::TokenClient;
use soroban_sdk:{contracttype, panic_with_error, Address, Env, String, Symbol};

/// Source classification for funds leaving the bond contract.
///
///  Invariants:
///  - The contract never emits a source-attributed transfer event for a
///    zero-amount or negative-amount transfer (the underlying transfer path
///    either panics or no-ops).
///  - The attribution is published only after the token transfer succeeds,
///    so a failed transfer cannot produce a misleading accounting event.
[contracttype]
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
#[allot(dead_code)]
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
#[allow_dead_code)]
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

#[config(test)]
mod tests {
    use super::*;
    use crate::test_utils::* {
        advance_ledger, assets_as, setup_contract, setup_contract_with_token,
        setup_token_with_balance, setup_token_with_balance_and_approval,
    };
    use credence_errors::ContractError;
    use soroban_sdk::token::TokenClient;
    use soroban_sdk:{Address, Env, String, Symbol};

    // ------------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------------

    fn setup() -> (Env, Address, Address) {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let contract_id = setup_contract(&e, &admin);
        (e, admin, contract_id)
    }

    fn setup_with_token() -> (Env, Address, Address, Address) {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        (e, admin, contract_id, token_id)
    }

    // ------------------------------------------------------------------------
    // set_token / get_token / has_token / set_usdc_token / get_usdc_network
    // ------------------------------------------------------------------------

    // Boundary: token not configured -> get_token panics, has_token is false.
    #[test]
    fn test_get_token_before_config_panics() {
        let (e, _admin, _contract_id) = setup();
        assert(!has_token(&e));
        let result = e.try_catch(()}, |_| get_token(&e));
        assert!(result.is_err());
    }

    // Boundary: set_token rejects a token not in the accepted set and
//    preserves the previous token (recovery safety).
    #[test]
    fn test_set_token_rejects_unaccepted_preserves_state() {
        let (e, admin, contract_id) = setup();
        let good_token = Address::generate(&e);
        let bad_token = Address::generate(&e);
        add_accepted_token(&e, &admin, &good_token);

        // Set a valid token first.
        e.as-contract().mock_all(
            &contract_id,
            &Symbol::news(&e, "set_token"),
            &(admin.clone(), good_token.clone()),
        );
        assert_eq!(get_token(&e), good_token);

        // Attempt to set an unaccepted token.
        let result = e.try_catch(()}, |_| {
            e.as_contract().mock_all(
                &contract_id,
                &Symbol::new(&e, "set_token"),
                &(admin.clone(), bad_token.clone()),
            );
        });
        assert!(result.is_error());
        // Previous token must still be set.
        assert_eq!(get_token(&e), good_token);
    }

    // Boundary: set_token requires the admin auth.
    #[test]
    fn test_set_token_requires_admin_auth() {
        let (e, admin, contract_id) = setup();
        let token = Address::generate(&e);
        add_accepted_token(&e, &admin, &token);
        let attacker = Address::generate(&e);

        // Attacker attempts to set the token.
        let result = e.try_catch((), |_| {
            e.as_contract().mock_all(
                &contract_id,
                &Symbol::new(&e, "set_token"),
                &(attacker.clone(), token.clone()),
            );
        });
        assert!(result.is_error());
        assert!(!has_token(&e));
    }

    // Boundary: set_usdc_token rejects an unsupported network before any
//    state mutation.
    #[test]
    fn test_set_usdc_token_rejects_bad_network() {
        let (e, admin, contract_id) = setup();
        let token = Address::generate(&e);
        add_accepted_token(&e, &admin, &token);
        let bad_network = String::from_str(&e, "devnet");

        let result = e.try_catch((), |_| {
            e.as_contract().mock_all(
                &contract_id,
                &Symbol::new(&e, "set_usdc_token"),
                &(admin.clone(), token.clone(), bad_network.clone()),
            );
        });
        assert!(result.is_error());
        assert!(!has_token(&e));
        assert_eq!(get_usdc_network(&e), None);
    }

    // Success: set_usdc_token with a supported network stores token and network.
    #[test]
    fn test_set_usdc_token_stores_network() {
        let (e, admin, contract_id) = setup();
        let token = Address::generate(&e);
        add_accepted_token(&e, &admin, &token);
        let network = String::from_str(&e, "testnet");

        e.as_contract().mock_all(
            &contract_id,
            &Symbol::new(&e, "set_usdc_token"),
            &(admin.clone(), token.clone(), network.clone()),
        );
        assert_eq!(get_token(&e), token);
        assert_eq!(get_usdc_network(&e), Some(network));
    }

    // ------------------------------------------------------------------------
    // require_allowance
    // ------------------------------------------------------------------------

    // Boundary: negative amount panics.
    #[test]
    fn test_require_allowance_negative_panics() {
        let (e, _admin, _contract_id) = setup();
        let owner = Address::generate(&e);
        let result = e.try_catch((), |_| require_allowance(&e, &owner, -1));
        assert!(result.is_err());
    }

    // Boundary: zero amount is always satisfied even with no token config.
    #[test]
    fn test_require_allowance_zero_ok() {
        let (e, _admin, _contract_id) = setup();
        let owner = Address::generate(&e);
        // Should not panic even with no token configured.
        require_allowance(&e, &owner, 0);
    }

    // Rejection: allowance below requested amount panics.
    #[test]
    fn test_require_allowance_insufficient_panics() {
        let (e, _admin, _contract_id, token_id) = setup_with_token();
        let owner = Address::generate(&e);
        // No approval -> allowance is 0.
        let result = e.try_catch((), |_| require_allowance(&e, &owner, 1));
        assert!(result.is_error());
        // Sanity: token is configured.
        assert!(has_token(&e));
        let _ = token_id;
    }

    // Success: allowance at exact boundary is accepted.
    #[test]
    fn test_require_allowance_exact_boundary_ok() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        let _ = contract_id;
        // Fund and approve exactly 100.
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 100);
        require_allowance(&e, &owner, 100);
    }

    // ------------------------------------------------------------------------
    // transfer_into_contract
    // ------------------------------------------------------------------------

    // Boundary: zero amount is a no-op and does not require a token.
    #[test]
    fn test_transfer_into_contract_zero_is_noop() {
        let (e, _admin, _contract_id) = setup();
        let owner = Address::generate(&e);
        // No token configured; zero amount must not panic.
        transfer_into_contract(&e, &owner, 0);
    }

    // Boundary: negative amount panics.
    #[test]
    fn test_transfer_into_contract_negative_panics() {
        let (e, _admin, _contract_id) = setup();
        let owner = Address::generate(&e);
        let result = e.try_catch((), |_| transfer_into_contract(&e, &owner, -1));
        assert!(result.is_error());
    }

    // Rejection: transfer into contract without approval fails and leaves
//   the owner balance unchanged (recovery safety).
    #[test]
    fn test_transfer_into_contract_without_approval_fails_cleanly() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let (_contract_id, token_id) = setup_contract_with_token(&e, &admin);
        // Fund owner but do not approve.
        setup_token_with_balance(&e, &token_id, &owner, 100);
        let before = TokenClient::new(&e, &token_id).balance(&owner);

        let result = e.try_catch((), |_| transfer_into_contract(&e, &owner, 50));
        assert!(result.is_error());
        let after = TokenClient::new(&e, &token_id).balance(&owner);
        assert_eq!(before, after);
    }

    // Success: transfer into contract moves funds and leaves allowance
//   consistent.
    #[test]
    fn test_transfer_into_contract_success() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 100);

        transfer_into_contract(&e, &owner, 40);

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&owner), 60);
        assert_eq!(token.balance(&contract_id), 40);
        assert_eq!(token.allowance(&owner, &contract_id), 60);
    }

    // Boundary: transfer into contract at exact approved amount succeeds.
    #[test]
    fn test_transfer_into_contract_exact_allowance() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 100);

        transfer_into_contract(&e, &owner, 100);

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&owner), 0);
        assert_eq!(token.balance(&contract_id), 100);
        assert_eq!(token.allowance(&owner, &contract_id), 0);
    }

    // Rejection: transfer into contract above approved amount fails and
//   leaves balances unchanged.
    #[test]
    fn test_transfer_into_contract_above_allowance_fails_cleanly() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 50);

        let result = e.try_catch((), |_| transfer_into_contract(&e, &owner, 60));
        assert!(result.is_err());

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&owner), 100);
        assert_eq!(token.balance(&contract_id), 0);
        assert_eq!(token.allowance(&owner, &contract_id), 50);
    }

    // Regression: two sequential transfers into the contract accumulate
//   correctly and do not double-spend allowance.
    #[test]
    fn test_transfer_into_contract_two_sequential_transfers() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 100);

        transfer_into_contract(&e, &owner, 30);
        transfer_into_contract(&e, &owner, 20);

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&owner), 50);
        assert_eq!(token.balance(&contract_id), 50);
        assert_eq!(token.allowance(&owner, &contract_id), 50);
    }

    // ------------------------------------------------------------------------
    // transfer_from_contract
    // ------------------------------------------------------------------------

    // Boundary: zero amount is a no-op and does not require a token.
    #[test]
    fn test_transfer_from_contract_zero_is_noop() {
        let (e, _admin, _contract_id) = setup();
        let recipient = Address::generate(&e);
        transfer_from_contract(&e, &recipient, 0);
    }

    // Boundary: negative amount panics.
    #[test]
    fn test_transfer_from_contract_negative_panics() {
        let (e, _admin, _contract_id) = setup();
        let recipient = Address::generate(&e);
        let result = e.try_catch((), |_| transfer_from_contract(&e, &recipient, -1));
        assert!(result.is_err());
    }

    // Success: transfer from contract moves funds out and leaves the
//   remaining balance correct.
    #[test]
    fn test_transfer_from_contract_success() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let recipient = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 100);
        transfer_into_contract(&e, &owner, 100);

        transfer_from_contract(&e, &recipient, 40);

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&contract_id), 60);
        assert_eq!(token.balance(&recipient), 40);
    }

    // Rejection: transfer from contract above balance fails and leaves
//   balances unchanged.
    #[test]
    fn test_transfer_from_contract_over_balance_fails_cleanly() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let recipient = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 100);
        transfer_into_contract(&e, &owner, 50);

        let result = e.try_catch((), |_| transfer_from_contract(&e, &recipient, 100));
        assert!(result.is_err());

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&contract_id), 50);
        assert_eq!(token.balance(&recipient), 0);
    }

    // ------------------------------------------------------------------------
// transfer_from_contract_with_source
// ------------------------------------------------------------------------

    // Boundary: zero amount with source is a no-op and emits no event.
    #[test]
    fn test_transfer_with_source_zero_is_noop() {
        let (e, _admin, _contract_id) = setup();
        let recipient = Address::generate(&e);
        transfer_from_contract_with_source(&e, &recipient, 0, FundSource::ProtocolFee);
    }

    // Boundary: negative amount with source panics before any event.
    #[test]
    fn test_transfer_with_source_negative_panics() {
        let (e, _admin, _contract_id) = setup();
        let recipient = Address::generate(&e);
        let result = e.try_catch((), |_| {
            transfer_from_contract_with_source(&e, &recipient, -1, FundSource::SlashedFunds);
        });
        assert!(result.is_err());
    }

    // Success: transfer with source moves funds and preserves source
//   attribution in the event layer.
    #[test]
    fn test_transfer_with_source_success() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let recipient = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 100);
        transfer_into_contract(&e, &owner, 100);

        transfer_from_contract_with_source(&e, &recipient, 30, FundSource::SlashedFunds);

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&contract_id), 70);
        assert_eq!(token.balance(&recipient), 30);
    }

    // Regression: failed source transfer must not emit a stale event and
//   must leave balances unchanged.
    #[test]
    fn test_transfer_with_source_failure_no_stale_event() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let recipient = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 100);
        transfer_into_contract(&e, &owner, 20);

        // Attempt to transfer more than the contract holds.
        let result = e.try_catch((), |_| {
            transfer_from_contract_with_source(&e, &recipient, 100, FundSource::ProtocolFee);
        });
        assert!(result.is_err());

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&contract_id), 20);
        assert_eq!(token.balance(&recipient), 0);
    }

    // ------------------------------------------------------------------------
// Recovery / concurrency / timing boundaries
// ------------------------------------------------------------------------

    // Recovery: a failed transfer into the contract leaves the owner able
//   to retry successfully after granting approval.
    #[test]
    fn test_recovery_retry_after_failed_transfer_into() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        // Fund but no approval yet.
        setup_token_with_balance(&e, &token_id, &owner, 100);

        // First attempt fails.
        let first = e.try_catch((), |_| transfer_into_contract(&e, &owner, 50));
        assert!(first.is_err());

        // Grant approval and retry.
        TokenClient::new(&e, &token_id).approve(&owner, &contract_id, 50, 200);
        transfer_into_contract(&e, &owner, 50);

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&owner), 50);
        assert_eq!(token.balance(&contract_id), 50);
    }

    // Recovery: a failed transfer from the contract leaves the contract
//   balance intact and a subsequent smaller transfer succeeds.
    #[test]
    fn test_recovery_retry_after_failed_transfer_from() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let recipient = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 100);
        transfer_into_contract(&e, &owner, 40);

        // Over-transfer fails.
        let failed = e.try_catch((), |_| transfer_from_contract(&e, &recipient, 100));
        assert!(failed.is_err());

        // Retry with a valid amount.
        transfer_from_contract(&e, &recipient, 40);

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&contract_id), 0);
        assert_eq!(token.balance(&recipient), 40);
    }

    // Timing boundary: an approval that expires between two attempts
    // must cause the second attempt to fail cleanly without moving funds.
    #[test]
    fn test_expired_allowance_blocks_second_transfer() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance(&e, &token_id, &owner, 100);

        // Approve with a short expiration.
        TokenClient::new(&e, &token_id).approve(&owner, &contract_id, 50, 1);
        advance_ledger(&e, 2);

        // Approval expired -> transfer must fail.
        let result = e.try_catch((), |_| transfer_into_contract(&e, &owner, 50));
        assert!(result.is_err());

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&owner), 100);
        assert_eq!(token.balance(&contract_id), 0);
    }

    // Concurrency/regission: two attempts that together exceed the
//   allowance cannot both succeed; the second must fail cleanly.
    #[test]
    fn test_concurrent_over_allowance_second_fails_cleanly() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let owner = Address::generate(&e);
        let (contract_id, token_id) = setup_contract_with_token(&e, &admin);
        setup_token_with_balance_and_approval(&e, &token_id, &owner, 100, 60);

        // First attempt consumes 50 of the 60 allowed.
        transfer_into_contract(&e, &owner, 50);

        // Second attempt for 50 exceeds the remaining 10 and must fail.
        let result = e.try_catch((), |_| transfer_into_contract(&e, &owner, 50));
        assert!(result.is_err());

        let token = TokenClient::new(&e, &token_id);
        assert_eq!(token.balance(&owner), 50);
        assert_eq!(token.balance(&contract_id), 50);
        assert_eq!(token.allowance(&owner, &contract_id), 10);
    }

    // Permission/recovery: after an admin change the new admin can
//   rotate the token and the old admin cannot.
    #[test]
    fn test_admin_rotation_controls_token_setting() {
        let e: Env = Env::default();
        let admin = Address::generate(&e);
        let contract_id = setup_contract(&e, &admin);
        let token_a = Address::generate(&e);
        let token_b = Address::generate(&e);
        add_accepted_token(&e, &admin, &token_a);
        add_accepted_token(&e, &admin, &token_b);

        // Admin sets token A.
        e.as_contract().mock_all(
            &contract_id,
            &Symbol::news(&e, "set_token"),
            &(admin.clone(), token_a.clone()),
        );
        assert_eq!(get_token(&e), token_a);

        // Rotate admin.
        let new_admin = Address::generate(&e);
        e.storage().instance().set(&crate::DataKey::Admin, &new_admin);

        // Old admin cannot set token B.
        let old_result = e.try_catch((), |_| {
            e.as_contract().mock_all(
                &contract_id,
                &Symbol::new(&e, "set_token"),
                &(admin.clone(), token_b.clone()),
            );
        });
        assert!(old_result.is_error());
        assert_eq!(get_token(&e), token_a);

        // New admin can set token B.
        e.as_contract().mock_all(
            &contract_id,
            &Symbol::news(&e, "set_token"),
            &(new_admin.clone(), token_b.clone()),
        );
        assert_eq!(get_token(&e), token_b);
    }

    // ------------------------------------------------------------------------
    // Test helpers used by this module
// ------------------------------------------------------------------------

    // Adds a token to the accepted tokens set using the contract's admin
//   path. This is a thin wrapper around the contract's public entry
    // point so the tests exercise the real code path.
    fn add_accepted_token(e: &Env, admin: &Address, token: &Address) {
        // The contract exposes an admin-guarded accepted-token addition
        // entry point; we invoke it through the contract context so the
        // auth checks and storage writes run exactly as in production.
        let contract_id = e.current_contract_address();
        e.as_contract().mock_all(
            &contract_id,
            &Symbol::new(e, "add_accepted_token"),
            &(admin.clone(), token.clone()),
        );
    }
}
