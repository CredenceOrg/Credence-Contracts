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
//! # Normalized comparison
//!
//! On the bond ingress paths the amount is first scaled to the normalized
//! 18-decimal format (see [`validate_leverage_for_token`] and
//! [`crate::normalization`]) and `MIN_BOND_AMOUNT` is expressed in those same
//! normalized units, so the ratio is dimensionless regardless of the bond
//! token's decimals (e.g. 7-decimal USDC) and no price-oracle read is
//! required. In phantom-balance mode (no token configured) the raw amount is
//! compared unchanged.

use crate::validation::MIN_BOND_AMOUNT;
use crate::{normalization, token_integration};
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
/// * `bond_amount` - The bond amount in 18-decimal normalized units; callers
///   holding raw token units go through [`validate_leverage_for_token`] first.
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

/// Normalized entry point for [`validate_leverage`].
///
/// When a bond token is configured, the raw token amount is scaled to the
/// normalized 18-decimal format (see [`crate::normalization::normalize`])
/// before the cap check, so a given `max_leverage` means the same value for
/// any token decimals. In phantom-balance mode (no token configured) the raw
/// amount is checked unchanged.
pub fn validate_leverage_for_token(e: &Env, bond_amount: i128, max_leverage: u32) {
    let normalized = if token_integration::has_token(e) {
        let token = token_integration::get_token(e);
        normalization::normalize(e, &token, bond_amount)
    } else {
        bond_amount
    };
    validate_leverage(e, normalized, max_leverage);
}
