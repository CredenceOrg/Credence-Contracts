//! Interface for flashloan receivers.
/// Contracts that wish to receive flashloans from the Credence Treasury must implement this trait.

use soroban_sdk::{contractclient, Address, Bytes, Env, Symbol};

/// @notice Defines the magic value returned on successful flashloan execution.
pub const FLASH_LOAN_SUCCESS: &str = "FLASH_LOAN_SUCCESS";

/// @title  FlashLoanModule
/// @notice Interface for a flashloan receiver contract.
///
/// # Invariants
/// - `on_flash_loan` MUST return `Symbol(FLASH_LOAN_SUCCESS)` for the loan to be
///   considered successful. Any other symbol (or a panic) must be treated by the
///   treasury as a failed repayment and trigger a recovery path.
/// - `on_flash_loan` MUST NOT mutate the caller's state in a way that leaves the
///   treasury in an inconsistent state if the return value is not the magic value.
/// - The callback MUST be idempotent with respect to a given `initiator`/`token`/
///   `amount` triple for the duration of a single flashloan execution.
///
/// # Boundary and recovery considerations
/// - `amount` and `fee` are i128. Callers MUST reject negative values and must not
///   overflow when computing `amount + fee`.
/// - Zero-amount loans are allowed but the callback still MUST return the magic
///   value and MUST not panic.
/// - `data` may be empty; receivers MUST not assume a minimum length.
/// - Recovery: the treasury is responsible for reverting the transfer and any
///   state changes if the callback does not return the magic value. Receivers
///   MUST NOT attempt to compensate the treasury themselves.
#[contractclient(name = "FlashLoanReceiverClient")]
pub trait FlashLoanReceiver {
    /// @notice Callback invoked by the treasury after transferring the loan amount.
    /// @param  initiator The address that initiated the flashloan.
    /// @param  token     The address of the token being loaned.
    /// @param  amount    The amount of tokens loaned.
    /// @param  fee       The fee amount required to be repaid along with the principal.
    /// @param  data      Arbitrary data passed by the initiator.
    /// @return A symbol that must match `FLASH_LOAN_SUCCESS` for the loan to be considered successful.
    fn on_flash_loan(
        e: Env,
        initiator: Address,
        token: Address,
        amount: i128,
        fee: i128,
        data: Bytes,
    ) -> Symbol;
}

/// @dev Module of pure helpers used by the treasury to validate receiver
/// responses and to enforce boundary invariants. These functions are deterministic
/// and side-effect free so they can be exercised in focused tests without a live
/// treasury contract.
pub mod validation {
    use super::FLASH_LOAN_SUCCESS;
    use soroban_sdk::{Env, Symbol};

    /// @dev Returns the canonical success symbol for the given environment.
    pub fn success_symbol(e: &Env) -> Symbol {
        Symbol::new(e, FLASH_LOAN_SUCCESS)
    }

    /// @dev Returns true iff `response` is the magic success symbol.
    /// Any other symbol (including an empty one) must be treated as a failure.
    pub fn is_success(e: &Env, response: &Symbol) -> bool {
        *response == success_symbol(e)
    }

    /// @dev Validates the numeric parameters of a flashloan callback.
    /// Returns `true` iff `amount` and `fee` are non-negative and their sum
    /// does not overflow `i128`. This is the boundary check the treasury applies
    /// before dispatching to a receiver.
    pub fn valid_amounts(amount: i128, fee: i128) -> bool {
        if amount < 0 || fee < 0 {
            return false;
        }
        amount.checked_add(fee).is_some()
    }

    /// @dev Returns the total repayment due (`amount + fee`) or `None` if the
    /// combination is invalid or overflows. Callers MUST treat `None` as a
    /// rejection and must not proceed with the loan.
    pub fn total_repayment(amount: i128, fee: i128) -> Option<i128> {
        if !valid_amounts(amount, fee) {
            return None;
        }
        amount.checked_add(fee)
    }
}
