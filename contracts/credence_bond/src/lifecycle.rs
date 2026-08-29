//! # Bond lifecycle state-transition matrix (issue #1273)
//!
//! Every bond lifecycle entrypoint (`create_bond`, `top_up`,
//! `extend_duration`, `withdraw`, `withdraw_early`, `request_withdrawal`,
//! `renew_if_rolling`, the cooldown trio, `slash_bond`, `withdraw_bond`,
//! `liquidate`) is required to enforce a *legal* transition from the bond's
//! current lifecycle stage to the stage the operation produces.
//!
//! The lifecycle is derived entirely from the stored [`IdentityBond`]:
//!
//! | Stage  | `active` | Meaning                                                    |
//! |--------|----------|------------------------------------------------------------|
//! | `None` | n/a      | No `DataKey::Bond` stored for the identity.                |
//! | `Active` | `true` | Bond is live; amount/duration can be mutated, slashable.   |
//! | `Withdrawn` | `false` | Closed via `withdraw_bond`.                               |
//! | `Liquidated` | `false` | Closed via `liquidate` (or fully-slashed + expired).       |
//!
//! ## Legal transition matrix
//!
//! | From          | Allowed operations                                       |
//! |---------------|----------------------------------------------------------|
//! | `None`        | `create_bond`                                            |
//! | `Active`      | `top_up`, `extend_duration`, `withdraw`, `withdraw_early`, `request_withdrawal`, `renew_if_rolling`, `request_cooldown_withdrawal`, `execute_cooldown_withdrawal`, `cancel_cooldown`, `slash_bond`, `withdraw_bond`, `liquidate` |
//! | `Withdrawn`   | (terminal — no mutating operation is legal)              |
//! | `Liquidated`  | (terminal — no mutating operation is legal)              |
//!
//! `require_bond_active` centralizes the "may this bond still be mutated"
//! pre-condition. It must run immediately after the bond is loaded and before
//! any storage write, token transfer, or external callback, so a rejected or
//! failed operation never leaves partial state.

use credence_errors::ContractError;
use soroban_sdk::{panic_with_error, Env};

use crate::IdentityBond;

/// Read the lifecycle stage of an identity's bond from storage.
///
/// `None` when no bond is stored; otherwise derived from `bond.active`.
pub fn stage_of(_e: &Env, bond: &IdentityBond) -> BondStage {
    if bond.active {
        BondStage::Active
    } else {
        BondStage::Closed
    }
}

/// The lifecycle stage of a bond.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BondStage {
    /// No bond exists yet (before `create_bond`).
    None,
    /// Bond is active and may be mutated / slashed / withdrawn / liquidated.
    Active,
    /// Bond is closed (withdrawn or liquidated); terminal.
    Closed,
}

/// Central pre-condition: the loaded bond must be active for any mutating
/// lifecycle operation to proceed.
///
/// # Panics
/// - [`ContractError::BondNotActive`] when `bond.active == false`.
pub fn require_bond_active(e: &Env, bond: &IdentityBond) {
    if !bond.active {
        panic_with_error!(e, ContractError::BondNotActive);
    }
}
