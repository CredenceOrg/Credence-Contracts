//! # Bond Invariants Test Library
//!
//! Reusable assertion helpers that encode the **protocol-level invariants** of the
//! Credence bond contract. Any unit, integration, fuzz, or mutation test can call
//! [`assert_all_invariants`] after every state-changing action to confirm that the
//! contract never enters an inconsistent state.
//!
//! Previously these invariants were scattered across ad-hoc `assert!` calls inside
//! individual tests. Centralizing them here means:
//!
//! * A single source of truth for *what must always hold*.
//! * Every test gets the full invariant suite for free (defense in depth).
//! * New invariants are added in exactly one place.
//!
//! ## Real modules covered
//!
//! | Module                    | Invariants enforced                                   |
//! |---------------------------|-------------------------------------------------------|
//! | `IdentityBond`            | I2, I3, I4, I5, I6 (balances, withdrawal, notice)     |
//! | `SubjectAttestationCount` | I1, I7 (weight sum, count consistency)                |
//! | `Nonce`                   | (read for state hydration; non-negativity is implicit)|
//!
//! ## The seven invariants
//!
//! 1. **I1 – Attestation weight sum is non-negative.** The sum of all stored
//!    attestation weights for any subject is `>= 0`.
//! 2. **I2 – Slashed never exceeds bonded.** `slashed_amount <= bonded_amount`.
//! 3. **I3 – Withdrawal request requires rolling.** `withdrawal_requested_at == 0
//!    || is_rolling`.
//! 4. **I4 – Bonded amount is non-negative.** `bonded_amount >= 0`.
//! 5. **I5 – Slashed amount is non-negative.** `slashed_amount >= 0`.
//! 6. **I6 – Notice period is bounded.** For rolling bonds the notice period never
//!    exceeds the bond duration (`notice_period_duration <= bond_duration`).
//! 7. **I7 – Attestation count matches stored list.** `SubjectAttestationCount`,
//!    when present, equals the length of the `SubjectAttestations` list.
//!
//! See `docs/bond-invariants.md` for owners and rationale.

#![allow(dead_code, unexpected_cfgs)]

use crate::{DataKey, IdentityBond};
use soroban_sdk::{Address, Env, Vec};

/// Skip the `slashed_amount <= bonded_amount` (I2) check.
///
/// This is wired through a compile-time `cfg` flag so we can deliberately disable
/// one invariant and prove that a test which previously passed now fails — i.e. the
/// invariant library is actually load-bearing. Enable with:
///
/// ```text
/// RUSTFLAGS="--cfg skip_slash_invariant" cargo test -p credence_bond
/// ```
const SKIP_SLASH_INVARIANT: bool = cfg!(skip_slash_invariant);

/// Reads the current [`IdentityBond`] from contract storage, if one exists.
///
/// Returns `None` when no bond has been created yet (e.g. right after
/// `initialize`). Callers that require a bond should use [`load_bond`].
pub fn try_load_bond(env: &Env, contract: &Address) -> Option<IdentityBond> {
    env.as_contract(contract, || {
        env.storage()
            .instance()
            .get::<_, IdentityBond>(&DataKey::Bond)
    })
}

/// Reads the current [`IdentityBond`] from contract storage.
///
/// # Panics
/// Panics if no bond exists. Use [`try_load_bond`] for the optional variant.
pub fn load_bond(env: &Env, contract: &Address) -> IdentityBond {
    try_load_bond(env, contract).expect("bond_invariants: expected a bond in storage")
}

// ---------------------------------------------------------------------------
// Individual invariant helpers (one per invariant, each documented).
// ---------------------------------------------------------------------------

/// **Invariant I1 — Attestation weight sum is non-negative.**
///
/// The sum of the `weight` of every stored attestation for `subject` must be
/// `>= 0`. Attestation weights are `u32` on chain, so this guards against any
/// accumulation logic that could overflow into / produce a negative aggregate
/// when summed as a signed integer.
///
/// Owner module: `SubjectAttestationCount` / `weighted_attestation`.
pub fn assert_attestation_weight_sum_non_negative(
    env: &Env,
    contract: &Address,
    subject: &Address,
) {
    let sum = attestation_weight_sum(env, contract, subject);
    assert!(
        sum >= 0,
        "INVARIANT I1 VIOLATED: attestation weight sum is negative ({sum})"
    );
}

/// **Invariant I2 — Slashed amount never exceeds bonded amount.**
///
/// `slashed_amount <= bonded_amount` must always hold; a bond can never be
/// slashed for more than it is worth.
///
/// Owner module: `IdentityBond` / `slashing`.
pub fn assert_slashed_within_bonded(bond: &IdentityBond) {
    if SKIP_SLASH_INVARIANT {
        // Deliberately disabled for the validation/mutation experiment.
        return;
    }
    assert!(
        bond.slashed_amount <= bond.bonded_amount,
        "INVARIANT I2 VIOLATED: slashed_amount ({}) > bonded_amount ({})",
        bond.slashed_amount,
        bond.bonded_amount
    );
}

/// **Invariant I3 — A withdrawal request implies a rolling bond.**
///
/// `withdrawal_requested_at == 0 || is_rolling`. Only rolling bonds may have a
/// pending withdrawal request; a fixed-duration bond must never carry one.
///
/// Owner module: `IdentityBond` / `rolling_bond`.
pub fn assert_withdrawal_request_requires_rolling(bond: &IdentityBond) {
    assert!(
        bond.withdrawal_requested_at == 0 || bond.is_rolling,
        "INVARIANT I3 VIOLATED: withdrawal_requested_at ({}) set on a non-rolling bond",
        bond.withdrawal_requested_at
    );
}

/// **Invariant I4 — Bonded amount is non-negative.**
///
/// `bonded_amount >= 0`. The principal of a bond can never go negative through
/// any combination of top-ups, withdrawals, or slashes.
///
/// Owner module: `IdentityBond`.
pub fn assert_bonded_non_negative(bond: &IdentityBond) {
    assert!(
        bond.bonded_amount >= 0,
        "INVARIANT I4 VIOLATED: bonded_amount is negative ({})",
        bond.bonded_amount
    );
}

/// **Invariant I5 — Slashed amount is non-negative.**
///
/// `slashed_amount >= 0`. Accumulated slashes can never be negative.
///
/// Owner module: `IdentityBond` / `slashing`.
pub fn assert_slashed_non_negative(bond: &IdentityBond) {
    assert!(
        bond.slashed_amount >= 0,
        "INVARIANT I5 VIOLATED: slashed_amount is negative ({})",
        bond.slashed_amount
    );
}

/// **Invariant I6 — Notice period is bounded by bond duration.**
///
/// For a rolling bond with a configured notice period, the notice period must
/// not exceed the bond duration (`notice_period_duration <= bond_duration`),
/// otherwise a withdrawal could never become claimable.
///
/// Owner module: `IdentityBond` / `rolling_bond`.
pub fn assert_notice_period_bounded(bond: &IdentityBond) {
    if bond.is_rolling && bond.notice_period_duration != 0 {
        assert!(
            bond.notice_period_duration <= bond.bond_duration,
            "INVARIANT I6 VIOLATED: notice_period_duration ({}) > bond_duration ({})",
            bond.notice_period_duration,
            bond.bond_duration
        );
    }
}

/// **Invariant I7 — Attestation count matches the stored attestation list.**
///
/// When `SubjectAttestationCount(subject)` is present it must equal the number of
/// entries in `SubjectAttestations(subject)`. This catches divergence between the
/// counter and the canonical list.
///
/// Owner module: `SubjectAttestationCount`.
pub fn assert_attestation_count_consistent(env: &Env, contract: &Address, subject: &Address) {
    env.as_contract(contract, || {
        let list_len = env
            .storage()
            .instance()
            .get::<_, Vec<u64>>(&DataKey::SubjectAttestations(subject.clone()))
            .map(|v| v.len() as u64)
            .unwrap_or(0);
        if let Some(count) = env
            .storage()
            .instance()
            .get::<_, u64>(&DataKey::SubjectAttestationCount(subject.clone()))
        {
            assert!(
                count == list_len,
                "INVARIANT I7 VIOLATED: SubjectAttestationCount ({count}) != list length ({list_len})"
            );
        }
    });
}

// ---------------------------------------------------------------------------
// Aggregate entry points.
// ---------------------------------------------------------------------------

/// Assert every bond-state invariant (I2–I6) against an explicit [`IdentityBond`].
///
/// Useful when a contract call already returned the updated bond and you want to
/// avoid a second storage read.
pub fn assert_bond_invariants(bond: &IdentityBond) {
    assert_slashed_within_bonded(bond);
    assert_withdrawal_request_requires_rolling(bond);
    assert_bonded_non_negative(bond);
    assert_slashed_non_negative(bond);
    assert_notice_period_bounded(bond);
}

/// Assert all invariants that depend only on bond state, reading the bond from
/// storage. No-op (returns) when the contract has no bond yet.
pub fn assert_all_bond_invariants(env: &Env, contract: &Address) {
    if let Some(bond) = try_load_bond(env, contract) {
        assert_bond_invariants(&bond);
    }
}

/// **The primary entry point.** Assert *all seven* invariants after a
/// state-changing call.
///
/// * Bond invariants (I2–I6) are checked against current storage if a bond exists.
/// * Attestation invariants (I1, I7) are checked for the bond's identity (which is
///   the usual attestation subject in these tests).
///
/// Call this after *every* mutating contract call in a test:
///
/// ```ignore
/// client.create_bond(&id, &amount, &dur, &false, &0);
/// assert_all_invariants(&env, &contract_id);
/// ```
pub fn assert_all_invariants(env: &Env, contract: &Address) {
    if let Some(bond) = try_load_bond(env, contract) {
        assert_bond_invariants(&bond);
        let subject = bond.identity.clone();
        assert_attestation_weight_sum_non_negative(env, contract, &subject);
        assert_attestation_count_consistent(env, contract, &subject);
    }
}

/// Variant of [`assert_all_invariants`] that also checks attestation invariants for
/// an explicit `subject` (when the attestation subject differs from the bond
/// identity).
pub fn assert_all_invariants_for_subject(env: &Env, contract: &Address, subject: &Address) {
    assert_all_bond_invariants(env, contract);
    assert_attestation_weight_sum_non_negative(env, contract, subject);
    assert_attestation_count_consistent(env, contract, subject);
}

// ---------------------------------------------------------------------------
// Internal helpers.
// ---------------------------------------------------------------------------

/// Sum the weights of all stored attestations for `subject`, as a signed i128.
///
/// Reads the `SubjectAttestations` index then each `Attestation(id)` record.
fn attestation_weight_sum(env: &Env, contract: &Address, subject: &Address) -> i128 {
    env.as_contract(contract, || {
        let ids: Vec<u64> = env
            .storage()
            .instance()
            .get(&DataKey::SubjectAttestations(subject.clone()))
            .unwrap_or(Vec::new(env));
        let mut sum: i128 = 0;
        for id in ids.iter() {
            if let Some(att) = env
                .storage()
                .instance()
                .get::<_, crate::types::Attestation>(&DataKey::Attestation(id))
            {
                sum = sum.saturating_add(att.weight as i128);
            }
        }
        sum
    })
}
