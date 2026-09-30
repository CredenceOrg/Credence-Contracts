// accrual_helper_template.rs
//
// Generic, copy-pasteable accrual helper template for lending contracts.
// Adapt storage keys, checked-arithmetic helpers and interest math to your
// target contract. This file is a template and not tied into any crate by
// default.

/// Template: Idempotent accrual helper
///
/// Usage:
/// - Add the relevant storage keys to your contract's `DataKey` enum
///   (e.g., `LastAccrualTimestamp`, `InterestRatePerSecond`, `TotalDebt`, `TotalReserves`).
/// - Adapt the checked arithmetic helpers (`checked_add_i128`, `checked_mul_i128`) to
///   the ones your contract uses (or import from `credence_math`).
/// - Call `ensure_accrued(&e)` at the start of any public `borrow/repay` entry
///   points to guarantee state is fresh before principal-mutating operations.

use soroban_sdk::Env;

// NOTE: This module is a template. Replace DataKey references with the ones used
// in your contract. The code below intentionally uses comments/pseudocode to avoid
// compile-time coupling; replace the comments with the real calls when adapting.

pub module accrual_helper_template {
    use super::*;

    /// Ensure interest is accrued up to the current ledger timestamp.
    ///
    /// Properties:
    /// - Idempotent and cheap when no accrual is necessary (compares timestamps).
    /// - Panics (reverts) on unrecoverable arithmetic/configuration errors.
    ///
    /// Adaptation checklist (before using in production):
    /// - Replace `DataKey::...` storage keys with your contract's `DataKey` enum.
    /// - Use your checked arithmetic helpers (e.g. `checked_add_i128`).
    /// - Implement the interest calculation that fits your model (per-second rate,
    ///   index-based, or other accrual model).
    pub fn ensure_accrued(e: &Env) {
        // PSEUDOCODE - replace with concrete code in your contract.
        // let now: u64 = e.ledger().timestamp();
        // let last: u64 = e.storage().instance().get(&DataKey::LastAccrualTimestamp).unwrap_or(0_u64);
        // if now <= last {
        //     // Already up-to-date; short-circuit to keep gas low.
        //     return;
        // }
        // let elapsed = now.saturating_sub(last);

        // Read interest-rate/config and principal totals needed for accrual.
        // let rate_per_second: i128 = e.storage().instance().get(&DataKey::InterestRatePerSecond).unwrap_or_else(|| panic!("interest rate not configured"));
        // let total_debt: i128 = e.storage().instance().get(&DataKey::TotalDebt).unwrap_or(0_i128);

        // Compute accrued interest using checked arithmetic. Example for simple
        // continuous-ish model: interest = total_debt * rate_per_second * elapsed.
        // let interest = checked_mul_i128(total_debt, rate_per_second, "interest mul overflow");
        // let interest = checked_mul_i128(interest, elapsed as i128, "interest mul overflow");

        // Update total debt (and reserves if fees apply).
        // let new_total_debt = checked_add_i128(total_debt, interest, "debt overflow");
        // e.storage().instance().set(&DataKey::TotalDebt, &new_total_debt);

        // Optionally split some portion of interest into reserves/fees.
        // let fee = compute_fee(interest);
        // let prev_reserves: i128 = e.storage().instance().get(&DataKey::TotalReserves).unwrap_or(0_i128);
        // e.storage().instance().set(&DataKey::TotalReserves, &checked_add_i128(prev_reserves, fee, "reserve overflow"));

        // Finally, update last accrual timestamp.
        // e.storage().instance().set(&DataKey::LastAccrualTimestamp, &now);

        // NOTE: All of the above must use your contract's concrete helpers and types.
    }

    // Example tiny helper signatures you might implement in a lending contract.
    // fn checked_add_i128(a: i128, b: i128, err_msg: &str) -> i128 { ... }
    // fn checked_mul_i128(a: i128, b: i128, err_msg: &str) -> i128 { ... }
}
