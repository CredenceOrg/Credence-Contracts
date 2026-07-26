//! Leverage Validation Module
//!
//! Enforces a configurable cap on the leverage a bonding position may carry.
//!
//! # Leverage definition
//!
//! ```text
//! leverage = bond_amount / MIN_BOND_AMOUNT   (integer division, rounds down)
//! ```
//!
//! A bond of exactly `MIN_BOND_AMOUNT` carries leverage 1×.  A bond of
//! `10 × MIN_BOND_AMOUNT` carries leverage 10×.  The guard rejects any bond
//! whose computed leverage exceeds `max_leverage`.
//!
//! # Decimal-agnostic design
//!
//! Both `bond_amount` and `MIN_BOND_AMOUNT` are expressed in the same token's
//! raw units (e.g. 6-decimal USDC or 18-decimal ERC-20 equivalents), so the
//! ratio is always dimensionless and no price-oracle read is required.

use crate::validation::MIN_BOND_AMOUNT;
use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Env};

/// Division helper: returns `ContractError::Overflow` instead of panicking.
#[inline]
fn checked_div_leverage(e: &Env, a: i128, b: i128) -> i128 {
    a.checked_div(b)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow))
}

/// Validates that `bond_amount` does not exceed the leverage cap.
///
/// # Arguments
/// * `e` - The Soroban environment
/// * `bond_amount` - The raw token amount being bonded (before fees).
/// * `max_leverage` - Maximum allowed leverage multiplier, as stored in the
///   `MaxLeverage` parameter (see `parameters::get_max_leverage`).
///
/// # Panics
/// Panics with `ContractError::LeverageExceeded` when
/// `bond_amount / MIN_BOND_AMOUNT > max_leverage`.
///
/// Non-positive amounts are left to `validate_bond_amount` (called earlier in
/// the open-position flow) and are treated as leverage 0 here, which always
/// passes.
pub fn validate_leverage(e: &Env, bond_amount: i128, max_leverage: u32) {
    if bond_amount <= 0 {
        return;
    }
    let leverage = checked_div_leverage(e, bond_amount, MIN_BOND_AMOUNT);
    if leverage > max_leverage as i128 {
        panic_with_error!(e, ContractError::LeverageExceeded);
    }
}
