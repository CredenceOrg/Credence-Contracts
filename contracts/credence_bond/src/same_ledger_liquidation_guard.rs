//! Same-ledger collateral-increase vs sensitive-operation sequencing guard.
//!
//! ## Rationale (issue #996, anti-sandwich)
//!
//! Within one ledger entry (Soroban block) transaction ordering is decided by
//! the host. A caller that observes pending transactions can craft sequences
//! where a slash ("liquidation") runs in the same block as a collateral
//! increase ("borrow" / top-up). When that happens the holder appears to lose
//! stake against a deposit that did not yet exist at the moment the slash
//! decision was made, enabling sandwich-like unfair outcomes and turning the
//! bond invariants into a moving target.
//!
//! The guard:
//!
//! 1. Persists the ledger sequence whenever collateral is added
//!    ([`record_collateral_increase`]).
//! 2. Rejects slash entry points whose current ledger sequence still matches
//!    the recorded one ([`require_slash_allowed_after_collateral_increase`]).
//! 3. Rejects cooldown-withdrawal execution whose current ledger sequence
//!    still matches the recorded one ([`require_cooldown_allowed_after_collateral_increase`]).
//!    This prevents an attacker from collateralizing and then immediately
//!    draining the cooldown window in the same ledger.
//!
//! The check is intentionally one-ledger-only — there is no global throttle
//! and unrelated flows (attestations, withdrawals, parameter changes) are
//! unaffected. Slashes that span two ledger entries are processed normally.
//!
//! ## Backwards compatibility
//!
//! If the storage key has never been written (pre-upgrade contract, freshly
//! deployed contract whose first transaction is a slash without any prior
//! collateral increase) the guard is a no-op so that existing bonds are not
//! bricked.
//!
//! ## Scope
//!
//! The guard sits in front of the canonical slash entry point and is **not**
//! a cross-cutting rate limiter:
//!
//! - Slash (admin) ✅ blocked when same-ledger as a collateral increase
//! - Unslash, slash history, treasury withdrawals ✅ unaffected
//! - Attestations, parameter changes ✅ unaffected
//! - Withdraw (bonded → liquid) ✅ unaffected, even in the same ledger
//! - Cooldown withdrawal execution ✅ blocked when same-ledger as a collateral increase
//!
//! See `../../docs/same-ledger-sequencing.md` for the policy note and the
//! threat model justification.

use crate::DataKey;
use soroban_sdk::Env;

/// Reason symbol emitted / matched by
/// [`require_slash_allowed_after_collateral_increase`].
///
/// Kept as a module constant so tests can assert on the exact string without
/// hard-coding it twice.
pub const SLASH_BLOCKED_REASON: &str = "slash blocked: collateral increased in this ledger";

/// Reason symbol emitted / matched by
/// [`require_cooldown_allowed_after_collateral_increase`].
pub const COOLDOWN_BLOCKED_REASON: &str = "cooldown execution blocked: collateral increased in this ledger";

/// Panics if the last collateral increase happened in the current ledger.
///
/// Reads [`DataKey::LastCollateralIncreaseLedger`]. If the key was never set
/// (e.g. a freshly deployed contract whose first mutating action is a slash,
/// or a contract that was recently upgraded from a build that did not write
/// the key) the function is a silent no-op so legacy slashing paths keep
/// working.
///
/// # Panics
/// Panics with [`SLASH_BLOCKED_REASON`] when the recorded ledger sequence
/// equals the current ledger sequence.
pub fn require_slash_allowed_after_collateral_increase(e: &Env) {
    let current = e.ledger().sequence();
    if let Some(last) = e
        .storage()
        .instance()
        .get::<_, u32>(&DataKey::LastCollateralIncreaseLedger)
    {
        if last == current {
            panic!("{}", SLASH_BLOCKED_REASON);
        }
    }
}

/// Panics if the last collateral increase happened in the current ledger
/// and a cooldown-withdrawal execution is being attempted.
///
/// This prevents an attacker from front-running a collateral increase with a
/// cooldown withdrawal in the same ledger entry.
///
/// Reads [`DataKey::LastCollateralIncreaseLedger`]. If the key was never set
/// the function is a silent no-op, preserving backward compatibility.
///
/// # Panics
/// Panics with [`COOLDOWN_BLOCKED_REASON`] when the recorded ledger sequence
/// equals the current ledger sequence.
pub fn require_cooldown_allowed_after_collateral_increase(e: &Env) {
    let current = e.ledger().sequence();
    if let Some(last) = e
        .storage()
        .instance()
        .get::<_, u32>(&DataKey::LastCollateralIncreaseLedger)
    {
        if last == current {
            panic!("{}", COOLDOWN_BLOCKED_REASON);
        }
    }
}

/// Persist the current ledger sequence after a successful collateral increase.
///
/// Called by [`crate::CredenceBond::create_bond`] and the canonical top-up
/// entry points so that any subsequent same-ledger slash is rejected by
/// [`require_slash_allowed_after_collateral_increase`] and any subsequent
/// same-ledger cooldown execution is rejected by
/// [`require_cooldown_allowed_after_collateral_increase`].
/// The function is infallible: it is a single `set` of a `u32` value and
/// produces no observable side effect beyond the storage write.
pub fn record_collateral_increase(e: &Env) {
    let seq = e.ledger().sequence();
    e.storage()
        .instance()
        .set(&DataKey::LastCollateralIncreaseLedger, &seq);
}

/// Read-only diagnostic helper returning the most recent ledger sequence that
/// recorded a collateral increase, or `None` if no such event has been recorded
/// yet on this contract instance.
///
/// Useful for tests and for off-chain indexers that want to know when the
/// guard was last tripped without having to call the slashing path.
pub fn last_collateral_increase_ledger(e: &Env) -> Option<u32> {
    e.storage()
        .instance()
        .get::<_, u32>(&DataKey::LastCollateralIncreaseLedger)
}
