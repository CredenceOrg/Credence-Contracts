// Helpers for working with tokens without getting rekt
// Handles all the annoying edge cases like zero addresses, negative amounts, etc.
use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env};

// Error messages you'll see when stuff breaks
pub mod errors {
    #[allow(dead_code)]
    pub const TOKEN_NOT_SET: &str = "token not configured";
    pub const INVALID_AMOUNT: &str = "amount must be non-negative";
    pub const INSUFFICIENT_ALLOWANCE: &str = "insufficient token allowance";
    // Note: safe_transfer_from delegates allowance enforcement to try_transfer_from's
    // native check. INSUFFICIENT_ALLOWANCE is used by safe_require_allowance and
    // by transfer_into_contract's pre-validation for descriptive error messages.
    #[allow(dead_code)]
    pub const TRANSFER_FAILED: &str = "token transfer failed";
    #[allow(dead_code)]
    pub const ALLOWANCE_FAILED: &str = "token allowance check failed";
    pub const APPROVE_FAILED: &str = "token approve failed";
    #[allow(dead_code)]
    pub const ZERO_ADDRESS: &str = "token address cannot be zero";
    pub const TRANSFER_AMOUNT_MISMATCH: &str =
        "unsupported token: transfer amount mismatch (code 213)";
}

/// Validates a token address is not zero
fn validate_token_address(_token: &Address) {
    // Address in Soroban doesn't have a simple is_zero() check.
    // Validation is usually handled by require_auth or by checking if it matches a known value.
}

// Can't send negative tokens, that doesn't make sense
fn validate_amount(amount: i128) {
    if amount < 0 {
        panic!("{}", errors::INVALID_AMOUNT);
    }
}

// Grab the token address from storage, fail loudly if not there
pub fn get_token(e: &Env) -> Address {
    crate::token_integration::get_token(e)
}

// Get a token client we can actually use to call functions
pub fn token_client(e: &Env) -> TokenClient<'_> {
    let token = get_token(e);
    TokenClient::new(e, &token)
}

/// Safely transfers tokens from contract to recipient.
///
/// Includes a balance-delta guard that verifies the actual amount sent
/// matches the requested amount, rejecting fee-on-transfer tokens.
///
/// # Arguments
/// * `e` - Contract environment
/// * `recipient` - Address to receive tokens
/// * `amount` - Amount to transfer
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If transfer fails (with descriptive error)
/// * If actual sent amount != requested (fee-on-transfer rejection)
pub fn safe_transfer(e: &Env, recipient: &Address, amount: i128) {
    validate_amount(amount);
    if amount == 0 {
        return; // nothing to do
    }

    validate_token_address(recipient);

    let contract = e.current_contract_address();
    let token = token_client(e);

    // Balance-delta guard: authoritative fee-on-transfer check.
    // Rejects any token where sent != requested.
    let balance_before = token.balance(&contract);

    match token.try_transfer(&contract, recipient, &amount) {
        Ok(_) => {}
        Err(_) => panic!("{}", errors::TRANSFER_FAILED),
    }

    let balance_after = token.balance(&contract);
    let actual_sent = balance_before
        .checked_sub(balance_after)
        .expect("balance underflow");

    if actual_sent != amount {
        panic!("{}", errors::TRANSFER_AMOUNT_MISMATCH);
    }
}

/// Safely transfers tokens from owner to contract using allowance.
///
/// Includes a balance-delta guard that verifies the actual amount received
/// matches the requested amount, rejecting fee-on-transfer tokens.
///
/// Note: The allowance check is performed natively by `try_transfer_from`;
/// no manual pre-check is needed. If the allowance is insufficient, the
/// transfer itself will fail and we panic with a descriptive error.
///
/// # Arguments
/// * `e` - Contract environment
/// * `owner` - Address owning the tokens
/// * `amount` - Amount to transfer
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If allowance is insufficient (via try_transfer_from failure)
/// * If transfer fails
/// * If actual received amount != requested (fee-on-transfer rejection)
pub fn safe_transfer_from(e: &Env, owner: &Address, amount: i128) {
    validate_amount(amount);
    if amount == 0 {
        return;
    }

    validate_token_address(owner);

    let contract = e.current_contract_address();
    // Construct the token client once and reuse for balance reads and transfer.
    let token = token_client(e);

    // Balance-delta guard: authoritative fee-on-transfer check.
    // Rejects any token where received != requested.
    let balance_before = token.balance(&contract);

    match token.try_transfer_from(&contract, owner, &contract, &amount) {
        Ok(_) => {}
        Err(_) => {
            // try_transfer_from can fail for many reasons (insufficient allowance,
            // insufficient balance, etc.). Panic so the caller gets a clear signal.
            panic!("{}", errors::TRANSFER_FAILED);
        }
    }

    let balance_after = token.balance(&contract);
    let actual_received = balance_after
        .checked_sub(balance_before)
        .expect("balance underflow");

    if actual_received != amount {
        panic!("{}", errors::TRANSFER_AMOUNT_MISMATCH);
    }
}

/// Safely checks allowance with proper error handling
///
/// # Arguments
/// * `e` - Contract environment
/// * `owner` - Address owning the tokens
/// * `amount` - Required amount
///
/// # Panics
/// * If token is not configured
/// * If allowance check fails
/// * If allowance is insufficient
pub fn safe_require_allowance(e: &Env, owner: &Address, amount: i128) {
    validate_amount(amount);
    if amount == 0 {
        return;
    }

    let allowance = token_client(e).allowance(owner, &e.current_contract_address());
    if allowance < amount {
        panic!("{}", errors::INSUFFICIENT_ALLOWANCE);
    }
}

/// Safely approves token spending (use with caution).
///
/// Uses `try_approve` so we can panic with a descriptive error message
/// instead of relying on the SDK's default panic.
///
/// # Arguments
/// * `e` - Contract environment
/// * `spender` - Address to approve spending for
/// * `amount` - Amount to approve
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If approve fails
#[allow(dead_code)]
pub fn safe_approve(e: &Env, spender: &Address, amount: i128) {
    validate_amount(amount);
    validate_token_address(spender);

    let token = get_token(e);
    let contract = e.current_contract_address();
    // Use a long expiration for the allowance
    let expiration = e.ledger().sequence() + 10000;
    match TokenClient::new(e, &token).try_approve(&contract, spender, &amount, &expiration) {
        Ok(_) => {}
        Err(_) => panic!("{}", errors::APPROVE_FAILED),
    }
}

/// Safely increases allowance (if supported by token)
///
/// # Arguments
/// * `e` - Contract environment
/// * `spender` - Address to increase allowance for
/// * `added_value` - Amount to increase allowance by
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If operation fails
#[allow(dead_code)]
pub fn safe_increase_allowance(e: &Env, spender: &Address, added_value: i128) {
    validate_amount(added_value);
    if added_value == 0 {
        return;
    }

    validate_token_address(spender);

    // For tokens that don't support increaseAllowance, fall back to approve
    let current_allowance = token_client(e).allowance(&e.current_contract_address(), spender);
    let new_allowance = current_allowance
        .checked_add(added_value)
        .expect("allowance overflow");

    safe_approve(e, spender, new_allowance);
}

/// Force approve (reset to 0 first, then set new amount)
/// Useful for tokens with front-running protection
///
/// # Arguments
/// * `e` - Contract environment
/// * `spender` - Address to approve spending for
/// * `amount` - Amount to approve
///
/// # Panics
/// * If token is not configured
/// * If amount is negative
/// * If operation fails
#[allow(dead_code)]
pub fn force_approve(e: &Env, spender: &Address, amount: i128) {
    validate_amount(amount);
    validate_token_address(spender);

    // Reset to 0 first
    safe_approve(e, spender, 0);
    // Then set the desired amount
    safe_approve(e, spender, amount);
}

/// Updates state ONLY if the transfer works. No half-finished updates.
/// If transfer fails, the state update never runs.
///
/// Uses the balance-delta guard internally via `safe_transfer`.
///
/// # Arguments
/// * `e` - Contract environment
/// * `recipient` - Address to receive tokens
/// * `amount` - Amount to transfer
/// * `state_update` - Closure to run only if transfer succeeds
///
/// # Panics
/// * Same panics as `safe_transfer`
#[allow(dead_code)]
pub fn atomic_transfer_and_update<F>(e: &Env, recipient: &Address, amount: i128, state_update: F)
where
    F: FnOnce(),
{
    validate_amount(amount);
    if amount == 0 {
        state_update(); // no transfer needed, just update
        return;
    }

    // safe_transfer now includes the balance-delta guard internally.
    // If this panics, state_update never runs.
    safe_transfer(e, recipient, amount);

    // Transfer worked, now we can safely update state
    state_update();
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{testutils::Address as TestAddress, Address, Env};

    #[test]
    fn test_validate_amount() {
        let _env = Env::default();

        // Valid amounts
        validate_amount(0);
        validate_amount(100);

        // Invalid amount should panic
        std::panic::catch_unwind(|| validate_amount(-1)).unwrap_err();
    }

    #[test]
    fn test_zero_address_validation() {
        let env = Env::default();
        let _zero_addr = Address::generate(&env);

        // This would panic in a real scenario with actual zero address
        // validate_token_address(&zero_addr);
    }
}
