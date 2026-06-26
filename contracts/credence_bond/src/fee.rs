// ============================================================================
// FILE: contracts/credence_bond/src/fee.rs
//
// Drop this new module into contracts/credence_bond/src/ and add
//   pub mod fee;
// to contracts/credence_bond/src/lib.rs
//
// All fee-related constants and the validated setter live here.
// ============================================================================

use soroban_sdk::{contracttype, symbol_short, Address, Env};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Basis-points denominator: 10 000 bps = 100 %.
pub const BPS_DENOMINATOR: u32 = 10_000;

/// Hard ceiling for protocol fees: 10 % (1 000 bps).
/// No admin call can push the fee above this value.
pub const MAX_FEE_BPS: u32 = 1_000;

/// Sensible default applied at contract initialisation: 2 % (200 bps).
pub const DEFAULT_FEE_BPS: u32 = 200;

// ---------------------------------------------------------------------------
// Storage key
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub enum FeeKey {
    ProtocolFeeBps,
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Dedicated error code returned when a proposed fee exceeds MAX_FEE_BPS.
/// Wire this into your contract's top-level error enum with a unique discriminant.
///
/// Example in errors.rs / your existing error type:
///
/// ```rust
/// #[contracterror]
/// #[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// #[repr(u32)]
/// pub enum CredenceError {
///     // … existing variants …
///     FeeBpsExceedsMaximum = 42,   // pick an unused discriminant
/// }
/// ```
pub const FEE_EXCEEDS_MAX_ERROR_CODE: u32 = 42;

// ---------------------------------------------------------------------------
// Read helper
// ---------------------------------------------------------------------------

/// Returns the current protocol fee in basis points.
/// Falls back to DEFAULT_FEE_BPS if the key has never been written
/// (e.g. on contracts deployed before this change).
pub fn get_protocol_fee_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&FeeKey::ProtocolFeeBps)
        .unwrap_or(DEFAULT_FEE_BPS)
}

// ---------------------------------------------------------------------------
// Validated setter
// ---------------------------------------------------------------------------

/// Updates the protocol fee after validating:
///   1. Caller is the stored admin.
///   2. `new_fee_bps` ≤ MAX_FEE_BPS (hard cap, in basis points).
///
/// Emits a `fee_updated` event containing `(previous_bps, new_bps)` so
/// off-chain indexers can track every change without re-reading storage.
///
/// # Panics
/// Panics with `FEE_EXCEEDS_MAX_ERROR_CODE` if `new_fee_bps > MAX_FEE_BPS`.
///
/// # Units
/// Both `previous_bps` and `new_bps` are in **basis points** (bps).
/// 100 bps = 1 %.  Maximum accepted value is MAX_FEE_BPS (1 000 bps = 10 %).
pub fn set_protocol_fee_bps(env: &Env, admin: &Address, new_fee_bps: u32) {
    // ── Auth ─────────────────────────────────────────────────────────────
    admin.require_auth();

    // ── Range check ──────────────────────────────────────────────────────
    if new_fee_bps > MAX_FEE_BPS {
        panic!(
            "CredenceError::FeeBpsExceedsMaximum ({}): proposed {} bps > max {} bps",
            FEE_EXCEEDS_MAX_ERROR_CODE, new_fee_bps, MAX_FEE_BPS
        );
    }

    // ── Read previous value before overwrite ──────────────────────────────
    let previous_bps = get_protocol_fee_bps(env);

    // ── Persist ──────────────────────────────────────────────────────────
    env.storage()
        .instance()
        .set(&FeeKey::ProtocolFeeBps, &new_fee_bps);

    // ── Emit event: (previous_bps, new_bps) ──────────────────────────────
    // Topic:  symbol "fee_updated"
    // Data:   (previous_bps: u32, new_bps: u32)  — both in bps
    env.events()
        .publish((symbol_short!("fee_upd"),), (previous_bps, new_fee_bps));
}
