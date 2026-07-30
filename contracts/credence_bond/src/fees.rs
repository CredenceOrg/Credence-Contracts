//! Bond Creation Fee Mechanism
//!
//! Charges a configurable percentage of the bonded amount on creation, transfers
//! the fee to the protocol treasury, and supports fee waiver for certain conditions.
//! Emits fee collection events.
//!
//! # Governance safety rails (issue #1027)
//!
//! `set_config` requires the caller (`admin`) to be the contract's stored
//! admin — **enforced by the entrypoint** (`lib.rs::set_fee_config`). The
//! helper additionally enforces:
//!
//! - **Range check**: `fee_bps` MUST lie within
//!   [`MIN_FEE_BPS`, `MAX_FEE_BPS`] = `[0, 1_000]` (0%..10%). Bounds mirror
//!   the other fee rails in [`crate::parameters`] (`MAX_PROTOCOL_FEE_BPS`)
//!   and [`crate::fee`] (`MAX_FEE_BPS`).
//! - **Event transparency**: every successful update emits
//!   `fee_config_updated` with `(admin, old_treasury, new_treasury,
//!   old_fee_bps, new_fee_bps)` so off-chain indexers can audit fee-config
//!   governance without re-reading storage. Range-check rejections emit no
//!   event (the state is unchanged).
//!
//! Panics with `"fee_bps out of bounds"` if the proposed `fee_bps` is outside
//! the inclusive range; this matches the convention used by
//! [`crate::parameters`] for `protocol_fee_bps` / `attestation_fee_bps`.

use soroban_sdk::{Address, Env, Symbol};

use crate::events;
use crate::math;

// ============================================================================
// Governance bounds (issue #1027)
// ============================================================================

/// Minimum bond-creation fee in basis points (0 bps = 0%, fee disabled).
pub const MIN_FEE_BPS: u32 = 0;

/// Maximum bond-creation fee in basis points (1 000 bps = 10%).
///
/// No admin call can ramp the bond-creation fee above this value. Picked to
/// match `crate::parameters::MAX_PROTOCOL_FEE_BPS` and `crate::fee::MAX_FEE_BPS`
/// so all fee rails across the contract have one consistent ceiling.
pub const MAX_FEE_BPS: u32 = 1_000;

/// Get treasury and fee rate (basis points). Returns (treasury, fee_bps).
/// If not set, fee is zero (no treasury = no fee).
pub fn get_config(e: &Env) -> (Option<Address>, u32) {
    let treasury: Option<Address> = e.storage().instance().get(&crate::DataKey::FeeTreasury);
    let fee_bps: u32 = e
        .storage()
        .instance()
        .get(&crate::DataKey::FeeBps)
        .unwrap_or(0);
    (treasury, fee_bps)
}

/// Set fee config. Caller must be the contract admin (enforced by the
/// `set_fee_config` entrypoint in `lib.rs`, which also makes the call
/// reentrancy-checked and paused-gated).
///
/// `fee_bps` is in basis points (e.g. `100` = 1%). It MUST lie within
/// [`MIN_FEE_BPS`, `MAX_FEE_BPS`] = `[0, 1_000]`; out-of-range values are
/// rejected with `panic!("fee_bps out of bounds")` and the call leaves
/// storage unchanged.
///
/// On success the helper emits `events::emit_fee_config_updated` carrying
/// `(admin, old_treasury, new_treasury, old_fee_bps, new_fee_bps)` so
/// governance transparency is preserved even when both fields are updated
/// in a single call.
///
/// # Panics
/// * `"fee_bps out of bounds"` if `fee_bps` is outside
///   `[MIN_FEE_BPS, MAX_FEE_BPS]`.
pub fn set_config(e: &Env, admin: &Address, treasury: Address, fee_bps: u32) {
    // ── Range check (issue #1027 governance safety rail) ─────────────────
    if !(MIN_FEE_BPS..=MAX_FEE_BPS).contains(&fee_bps) {
        panic!("fee_bps out of bounds");
    }

    // ── CEI: read previous values before overwriting ────────────────────
    let (old_treasury, old_fee_bps) = get_config(e);

    // ── Effects: persist the new config ─────────────────────────────────
    e.storage()
        .instance()
        .set(&crate::DataKey::FeeTreasury, &treasury);
    e.storage()
        .instance()
        .set(&crate::DataKey::FeeBps, &fee_bps);

    // ── Interaction: emit governance event (old/new values) ────────────
    events::emit_fee_config_updated(e, admin, old_treasury, &treasury, old_fee_bps, fee_bps);
}

/// Calculate fee for a bond amount. Returns (fee_amount, net_amount).
/// If fee is waived (e.g. fee_bps is 0 or waiver condition), fee is 0.
#[must_use]
pub fn calculate_fee(e: &Env, amount: i128) -> (i128, i128) {
    let (_treasury, fee_bps) = get_config(e);
    if fee_bps == 0 || amount <= 0 {
        return (0, amount);
    }
    math::split_bps(
        amount,
        fee_bps,
        "fee calculation overflow",
        "fee calculation div-by-zero",
        "fee calculation underflow",
    )
}

/// Check if fee is waived for this bond (e.g. zero amount, or future: whitelisted identity).
#[allow(dead_code)]
#[must_use]
pub fn is_fee_waived(e: &Env, amount: i128, _identity: &Address) -> bool {
    let (_, fee_bps) = get_config(e);
    fee_bps == 0 || amount <= 0
}

/// Record fee to the contract's fee pool (for later transfer to treasury).
/// In full implementation, transfer would happen here; we accumulate and emit event.
pub fn record_fee(e: &Env, identity: &Address, amount: i128, fee: i128, treasury: &Address) {
    if fee <= 0 {
        return;
    }
    let key = Symbol::new(e, "fees");
    let current: i128 = e.storage().instance().get(&key).unwrap_or(0);
    let new_total = math::add_i128(current, fee, "fee pool overflow");
    e.storage().instance().set(&key, &new_total);
    emit_fee_event(e, identity, amount, fee, treasury);
}

/// Emit fee collection event.
pub fn emit_fee_event(
    e: &Env,
    identity: &Address,
    bond_amount: i128,
    fee_amount: i128,
    treasury: &Address,
) {
    e.events().publish(
        (Symbol::new(e, "bond_creation_fee"),),
        (identity.clone(), bond_amount, fee_amount, treasury.clone()),
    );
}
