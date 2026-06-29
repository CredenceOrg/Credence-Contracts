//! On-chain bond drift detection (issue #436).
//!
//! [`assert_self_consistent`] runs after every bond-module storage write to catch
//! accounting drift before it propagates to downstream operations.

use crate::{DataKey, IdentityBond};
use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Vec};

/// Kind of invariant breach detected during a self-check.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BondDriftKind {
    /// `slashed_amount > bonded_amount` (or bonded/slashed negative).
    SlashedExceedsBonded,
    /// `SubjectAttestationCount(subject)` does not match `SubjectAttestations(subject)` length.
    AttestationCountMismatch,
}

/// Structured payload for [`crate::events::emit_bond_drift_detected`].
#[contracttype]
#[derive(Clone, Debug)]
pub struct BondDriftDetails {
    pub kind: BondDriftKind,
    pub subject: Address,
    pub bonded_amount: i128,
    pub slashed_amount: i128,
    pub attestation_count: u32,
    pub attestation_list_len: u32,
}

/// Post-write self-check for bond accounting and attestation counters.
///
/// Verifies:
/// - `bonded_amount >= slashed_amount` when a bond exists (I2 drift guard).
/// - `SubjectAttestationCount(subject) == len(SubjectAttestations(subject))` when the
///   counter key is present (I7 drift guard), for the bond identity when a bond exists.
///
/// ## Performance / cost
///
/// Each call performs at least one instance-storage read of [`DataKey::Bond`], and when a
/// bond is present may read `SubjectAttestationCount` plus walk `SubjectAttestations`
/// (O(n) in the number of attestation IDs for that subject). This is intentional defense
/// in depth on mutation hot paths: expect roughly **2–4 additional storage reads** on
/// bond-only writes and **3–5** on attestation writes, plus O(n) list length work for
/// I7. The check is skipped when no bond exists (e.g. before `create_bond`).
///
/// On failure the contract emits `bond_drift_detected` then panics with
/// [`ContractError::InvariantViolation`] so indexers can alert before the transaction aborts.
pub fn assert_self_consistent(e: &Env) {
    let bond_key = DataKey::Bond;
    let Some(bond) = e.storage().instance().get::<_, IdentityBond>(&bond_key) else {
        return;
    };

    check_bond_slashed_within_bonded(e, &bond);

    let subject = bond.identity.clone();
    check_attestation_count_consistent(e, &subject, &bond);
}

/// Same as [`assert_self_consistent`] but also validates attestation counters for `subject`.
///
/// Used after `add_attestation` / `revoke_attestation` when the attestation subject may
/// differ from the bond identity.
pub fn assert_self_consistent_for_subject(e: &Env, subject: &Address) {
    assert_self_consistent(e);
    if let Some(bond) = e
        .storage()
        .instance()
        .get::<_, IdentityBond>(&DataKey::Bond)
    {
        if bond.identity != *subject {
            check_attestation_count_consistent(e, subject, &bond);
        }
    } else {
        check_attestation_count_consistent(
            e,
            subject,
            &IdentityBond {
                identity: subject.clone(),
                bonded_amount: 0,
                bond_start: 0,
                bond_duration: 0,
                slashed_amount: 0,
                active: false,
                is_rolling: false,
                withdrawal_requested_at: 0,
                notice_period_duration: 0,
            },
        );
    }
}

fn check_bond_slashed_within_bonded(e: &Env, bond: &IdentityBond) {
    if bond.slashed_amount > bond.bonded_amount || bond.bonded_amount < 0 || bond.slashed_amount < 0
    {
        fail_drift(
            e,
            BondDriftDetails {
                kind: BondDriftKind::SlashedExceedsBonded,
                subject: bond.identity.clone(),
                bonded_amount: bond.bonded_amount,
                slashed_amount: bond.slashed_amount,
                attestation_count: 0,
                attestation_list_len: 0,
            },
        );
    }
}

fn check_attestation_count_consistent(e: &Env, subject: &Address, bond: &IdentityBond) {
    let list_len = e
        .storage()
        .instance()
        .get::<_, Vec<u64>>(&DataKey::SubjectAttestations(subject.clone()))
        .map(|v| v.len())
        .unwrap_or(0);

    let count_key = DataKey::SubjectAttestationCount(subject.clone());
    if let Some(count) = e.storage().instance().get::<_, u32>(&count_key) {
        if list_len != count {
            fail_drift(
                e,
                BondDriftDetails {
                    kind: BondDriftKind::AttestationCountMismatch,
                    subject: subject.clone(),
                    bonded_amount: bond.bonded_amount,
                    slashed_amount: bond.slashed_amount,
                    attestation_count: count,
                    attestation_list_len: list_len,
                },
            );
        }
    }
}

fn fail_drift(e: &Env, details: BondDriftDetails) {
    crate::events::emit_bond_drift_detected(e, &details);
    panic_with_error!(e, ContractError::InvariantViolation);
}
