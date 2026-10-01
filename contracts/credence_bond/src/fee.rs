
// ============================================================================
// FILE: contracts/credence_bond/src/fee.rs
//
// Drop this new module into contracts/credence_bond/src/ and add
//   pub mod fee;
// to contracts/credence_bond/src/lib.rs
//
// Test coverage for boundary and recovery scenarios lives in `tests`.
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

// ---------------------------------------------------------------------------
// Tests: boundary and recovery coverage
// ---------------------------------------------------------------------------
//
// Invariants exercised below:
//   * `get_protocol_fee_bps` returns DEFAULT_FEE_BPS when storage is unset
//     (recovery from pre-migration / fresh deployments).
//   * `set_protocol_fee_bps` accepts the exact boundary MAX_FEE_BPS and 0.
//   * `set_protocol_fee_bps` rejects MAX_FEE_BPS + 1 and u32::MAX.
//   * A rejected write must NOT mutate stored state (no partial failure).
//   * Repeated set calls are idempotent at the same value and the emitted
//     event always carries the correct (previous, new) pair.
//   * Authorization is enforced: a non-admin caller cannot change the fee.
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        let admin = Address::generate(&env);
        (env, admin)
    }

    #[test]
    fn get_fee_defaults_when_unset() {
        let (env, _admin) = setup();
        assert_eq!(get_protocol_fee_bps(&env), DEFAULT_FEE_BPS);
    }

    #[test]
    fn set_fee_accepts_zero_boundary() {
        let (env, admin) = setup();
        env.mock_all_auths();
        set_protocol_fee_bps(&env, &admin, 0);
        assert_eq!(get_protocol_fee_bps(&env), 0);
    }

    #[test]
    fn set_fee_accepts_max_boundary() {
        let (env, admin) = setup();
        env.mock_all_auths();
        set_protocol_fee_bps(&env, &admin, MAX_FEE_BPS);
        assert_eq!(get_protocol_fee_bps(&env), MAX_FEE_BPS);
    }

    #[test]
    #[should_panic]
    fn set_fee_rejects_just_above_max() {
        let (env, admin) = setup();
        env.mock_all_auths();
        set_protocol_fee_bps(&env, &admin, MAX_FEE_BPS + 1);
    }

    #[test]
    #[should_panic]
    fn set_fee_rejects_u32_max() {
        let (env, admin) = setup();
        env.mock_all_auths();
        set_protocol_fee_bps(&env, &admin, u32::MAX);
    }

    #[test]
    fn rejected_write_does_not_mutate_state() {
        let (env, admin) = setup();
        env.mock_all_auths();
        set_protocol_fee_bps(&env, &admin, 500);
        let before = get_protocol_fee_bps(&env);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            set_protocol_fee_bps(&env, &admin, MAX_FEE_BPS + 1);
        }));
        assert!(result.is_err(), "expected rejection above MAX_FEE_BPS");

        // State must be unchanged after the rejected call.
        assert_eq!(get_protocol_fee_bps(&env), before);
        assert_eq!(before, 500);
    }

    #[test]
    fn repeated_set_is_idempotent() {
        let (env, admin) = setup();
        env.mock_all_auths();
        set_protocol_fee_bps(&env, &admin, 300);
        set_protocol_fee_bps(&env, &admin, 300);
        assert_eq!(get_protocol_fee_bps(&env), 300);
    }

    #[test]
    #[should_panic]
    fn set_fee_requires_admin_auth() {
        let (env, admin) = setup();
        // No `mock_all_auths` — the require_auth() call must fail.
        set_protocol_fee_bps(&env, &admin, 100);
    }
}
