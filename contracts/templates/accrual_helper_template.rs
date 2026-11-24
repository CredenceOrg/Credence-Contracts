// accrual_helper_template.rs
//
// Generic, copy-pasteable accrual helper template for lending contracts.
// Adapt storage keys, checked-arithmetic helpers and interest math to your
// target contract.
//
// This file is compile-checked via `contracts/templates/tests/template_compile.rs`.

use soroban_sdk::Env;

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
#[allow(dead_code)]
pub fn ensure_accrued(e: &Env) {
    let now: u64 = e.ledger().timestamp();
    // Replace DataKey::LastAccrualTimestamp with your contract's key.
    // let last: u64 = e.storage().instance().get(&DataKey::LastAccrualTimestamp).unwrap_or(0_u64);
    let last: u64 = 0_u64;
    if now <= last {
        // Already up-to-date; short-circuit to keep gas low.
        return;
    }
    let _elapsed = now.saturating_sub(last);

    // Read interest-rate/config and principal totals needed for accrual.
    // let rate_per_second: i128 = ...;
    // let total_debt: i128 = ...;

    // Compute accrued interest using checked arithmetic. Example for a simple
    // continuous model: interest = total_debt * rate_per_second * elapsed.
    // let interest = checked_mul_i128(total_debt, rate_per_second, "interest mul overflow");
    // let interest = checked_mul_i128(interest, elapsed as i128, "elapsed mul overflow");

    // Update total debt and optionally split fees into reserves.
    // e.storage().instance().set(&DataKey::TotalDebt, &new_total_debt);
    // e.storage().instance().set(&DataKey::LastAccrualTimestamp, &now);
}

/// Example checked addition helper (adapt to use `panic_with_error!` in production).
#[allow(dead_code)]
pub fn checked_add_i128(a: i128, b: i128, _err_msg: &str) -> i128 {
    a.checked_add(b).expect(_err_msg)
}

/// Example checked multiplication helper.
#[allow(dead_code)]
pub fn checked_mul_i128(a: i128, b: i128, _err_msg: &str) -> i128 {
    a.checked_mul(b).expect(_err_msg)
}
