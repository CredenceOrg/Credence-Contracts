//! On-chain bond drift detection (issue #436).
//!
//! [`assert_self_consistent`] runs after every bond-module storage write to catch
//! accounting drift before it propagates to downstream operations.
//!
//! # Invariants enforced
//!
//! | Id | Invariant | Breach kind |
//! |----|-----------|-------------|
//! | I0 | `DataKey::Bond(s).identity == s` (the payload owner matches the key it is filed under) | [`BondDriftKind::BondIdentityMismatch`] |
//! | I2 | `bonded_amount >= slashed_amount` on an **active** bond | [`BondDriftKind::SlashedExceedsBonded`] |
//! | I4 | `bonded_amount >= 0` | [`BondDriftKind::NegativeBondedAmount`] |
//! | I5 | `slashed_amount >= 0` | [`BondDriftKind::NegativeSlashedAmount`] |
//! | I7 | `SubjectAttestationCount(s) == len(SubjectAttestations(s))` | [`BondDriftKind::AttestationCountMismatch`] / [`BondDriftKind::MissingAttestationCounter`] |
//!
//! # Why the order of checks is fixed
//!
//! Checks run I0, I4, I5, I2, I7 in that order and the first breach aborts the
//! transaction. The order is part of the contract: given a bond that breaches
//! more than one invariant, the reported [`BondDriftKind`] must be the same on
//! every run and on every node, otherwise indexers cannot deduplicate alerts.
//! Negativity is therefore classified before magnitude, so a negative
//! `slashed_amount` is reported as [`BondDriftKind::NegativeSlashedAmount`]
//! rather than being misreported as [`BondDriftKind::SlashedExceedsBonded`].
//!
//! # Failure semantics
//!
//! On breach the contract emits `bond_drift_detected` and then panics with
//! [`ContractError::InvariantViolation`].
//!
//! A Soroban transaction is atomic, so the panic reverts the write that
//! introduced the drift: there is no partial state to reconcile, and a drifted
//! record is never committed. The event is emitted first, before the panic, so
//! the abort is *classifiable* rather than an opaque `Error(Contract, #233)`.
//!
//! Be precise about where that event is observable, because the distinction
//! matters to anyone building a monitor: the revert rolls the event back out of
//! the **committed** event stream, so an indexer tailing committed events will
//! never see it. It survives in the **failed-transaction diagnostic log**,
//! which is what an operator inspects after `Error(Contract, #233)` and what
//! RPC-level monitors read. The payload is therefore restricted to the subject
//! address and the two amounts and two counters needed to locate the offending
//! key pair — enough to act on, and nothing that would leak caller-supplied
//! data into a log that survives the transaction.

use crate::{DataKey, IdentityBond};
use credence_errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Vec};

/// Kind of invariant breach detected during a self-check.
///
/// Discriminants are wire-visible: they are part of the `bond_drift_detected`
/// event payload that indexers decode. New kinds are **appended**; existing
/// discriminants are never renumbered.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BondDriftKind {
    /// `slashed_amount > bonded_amount`. Wire discriminant 0.
    SlashedExceedsBonded,
    /// `SubjectAttestationCount(subject)` does not match `SubjectAttestations(subject)` length.
    /// Wire discriminant 1.
    AttestationCountMismatch,
    /// `bonded_amount` is negative, so the bond's net value is not well defined.
    /// Wire discriminant 2.
    NegativeBondedAmount,
    /// `slashed_amount` is negative, meaning a slash has been undone past zero.
    /// Wire discriminant 3.
    NegativeSlashedAmount,
    /// `SubjectAttestations(subject)` is non-empty but `SubjectAttestationCount(subject)`
    /// was never written, so the counter cannot be compared against the list.
    /// Wire discriminant 4.
    MissingAttestationCounter,
    /// The `IdentityBond` filed under `DataKey::Bond(subject)` records a different
    /// `identity`, so reads for `subject` would return another tenant's bond.
    /// Wire discriminant 5.
    BondIdentityMismatch,
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

/// Post-write self-check for bond accounting and attestation counters of `subject`.
///
/// Verifies, in the fixed order documented at the module level:
///
/// - **I0** the `IdentityBond` stored under `DataKey::Bond(subject)` records
///   `subject` as its own owner;
/// - **I2** `bonded_amount >= slashed_amount` (active bonds only — see
///   [`check_bond_amounts_consistent`]);
/// - **I4** `bonded_amount >= 0`;
/// - **I5** `slashed_amount >= 0`;
/// - **I7** `SubjectAttestationCount(subject) == len(SubjectAttestations(subject))`.
///
/// A subject with no bond is not an error: I2/I4/I5 are vacuous and I0 has no
/// payload to check, but I7 still applies because attestations can be recorded
/// for a subject that has not bonded yet. In that case the check runs against a
/// zeroed reference bond so the emitted details still carry both amounts as `0`.
///
/// ## Signature
///
/// This takes the `subject` to check. The pre-#1334 form took only `&Env`,
/// which under the multi-identity `DataKey::Bond(Address)` layout gave it no
/// way to name a bond; it silently returned, so every one of its call sites
/// believed it had a guard and had none. All in-crate callers now pass the
/// identity they just wrote. See [`assert_self_consistent_for_subject`] for the
/// retained alias.
///
/// ## Performance / cost
///
/// The call performs one instance read of `DataKey::Bond(subject)` and, when a
/// bond is present, one read each of `SubjectAttestations(subject)` and
/// `SubjectAttestationCount(subject)`: **at most 3 additional instance reads**.
/// Reading the list length is a single host read of the vector header, not a
/// walk of the attestation IDs, so the check does not grow with the number of
/// attestations. Expect roughly **2 additional reads** on a bond-only write and
/// **3** on an attestation write. Subjects with no bond cost 1–2 reads.
///
/// ## Failure behaviour
///
/// On breach the contract emits `bond_drift_detected` and then panics with
/// [`ContractError::InvariantViolation`] (wire code 233). The panic reverts the
/// whole transaction, so a drifted write is never committed. The event lands in
/// the failed transaction's diagnostic log rather than the committed event
/// stream — see the module-level "Failure semantics" for why that distinction
/// is load-bearing.
pub fn assert_self_consistent(e: &Env, subject: &Address) {
    if let Some(bond) = e
        .storage()
        .instance()
        .get::<_, IdentityBond>(&DataKey::Bond(subject.clone()))
    {
        check_bond_identity_matches_key(e, subject, &bond);
        check_bond_amounts_consistent(e, subject, &bond);
        check_attestation_count_consistent(e, subject, &bond);
    } else {
        check_attestation_count_consistent(e, subject, &absent_bond(subject));
    }
}

/// Compatibility alias for [`assert_self_consistent`].
///
/// Retained so the attestation call sites (`add_attestation`,
/// `add_attestation_batch`, `revoke_attestation`) and any out-of-tree caller
/// keep the entry point they were written against. It is a thin forward; there
/// is no second implementation to drift out of sync.
pub fn assert_self_consistent_for_subject(e: &Env, subject: &Address) {
    assert_self_consistent(e, subject);
}

/// Self-check for every bond touched by a multi-identity batch operation.
///
/// Iterates in the order given so the first breached identity is the reported
/// one, matching the single-identity path.
pub fn assert_self_consistent_for_bonds(e: &Env, subjects: &Vec<Address>) {
    for i in 0..subjects.len() {
        if let Some(subject) = subjects.get(i) {
            assert_self_consistent(e, &subject);
        }
    }
}

/// Zeroed reference bond for a subject that has no `DataKey::Bond` entry.
///
/// Used so I7 breaches on an unbonded subject still report `bonded_amount` and
/// `slashed_amount` as `0` instead of leaving the fields unspecified.
fn absent_bond(subject: &Address) -> IdentityBond {
    IdentityBond {
        identity: subject.clone(),
        bonded_amount: 0,
        bond_start: 0,
        bond_duration: 0,
        slashed_amount: 0,
        active: false,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    }
}

/// I0: the bond payload must be filed under its own identity's key.
///
/// Catches a bond written to `DataKey::Bond(a)` whose `identity` field is `b`.
/// Without this, `describe_bond(a)` and every `Bond(a)` read would hand one
/// tenant the accounting of another.
fn check_bond_identity_matches_key(e: &Env, subject: &Address, bond: &IdentityBond) {
    if bond.identity != *subject {
        fail_drift(
            e,
            BondDriftDetails {
                kind: BondDriftKind::BondIdentityMismatch,
                subject: subject.clone(),
                bonded_amount: bond.bonded_amount,
                slashed_amount: bond.slashed_amount,
                attestation_count: 0,
                attestation_list_len: 0,
            },
        );
    }
}

/// I4, I5 then I2, in that order.
///
/// Negativity is classified first so the diagnosis names the actual defect
/// rather than reporting it as a magnitude violation.
///
/// I2 is scoped to **active** bonds. A bond closed by `withdraw_bond` is
/// written with `bonded_amount = 0` while `slashed_amount` is retained as
/// history, so `slashed > bonded` is the expected shape of a closed position
/// and not drift. Scoping the check this way keeps the two established
/// terminal states (`Withdrawn`, `Liquidated`, see
/// [`crate::lifecycle`]) representable without weakening detection where it
/// matters: every mutator calls [`crate::lifecycle::require_bond_active`]
/// first, so a closed bond can never be driven further off-invariant. I4 and
/// I5 are unscoped, because a negative amount is meaningless in any state.
fn check_bond_amounts_consistent(e: &Env, subject: &Address, bond: &IdentityBond) {
    if bond.bonded_amount < 0 {
        fail_drift(
            e,
            BondDriftDetails {
                kind: BondDriftKind::NegativeBondedAmount,
                subject: subject.clone(),
                bonded_amount: bond.bonded_amount,
                slashed_amount: bond.slashed_amount,
                attestation_count: 0,
                attestation_list_len: 0,
            },
        );
    }
    if bond.slashed_amount < 0 {
        fail_drift(
            e,
            BondDriftDetails {
                kind: BondDriftKind::NegativeSlashedAmount,
                subject: subject.clone(),
                bonded_amount: bond.bonded_amount,
                slashed_amount: bond.slashed_amount,
                attestation_count: 0,
                attestation_list_len: 0,
            },
        );
    }
    if bond.active && bond.slashed_amount > bond.bonded_amount {
        fail_drift(
            e,
            BondDriftDetails {
                kind: BondDriftKind::SlashedExceedsBonded,
                subject: subject.clone(),
                bonded_amount: bond.bonded_amount,
                slashed_amount: bond.slashed_amount,
                attestation_count: 0,
                attestation_list_len: 0,
            },
        );
    }
}

/// I7: `SubjectAttestationCount(subject) == len(SubjectAttestations(subject))`.
///
/// The counter is compared unconditionally, treating an absent counter as `0`
/// — the same default the attestation writers use when they read the key. A
/// counter that is missing while the list is populated is reported as
/// [`BondDriftKind::MissingAttestationCounter`] rather than as a numeric
/// mismatch, so the alert says which half of the pair is missing.
fn check_attestation_count_consistent(e: &Env, subject: &Address, bond: &IdentityBond) {
    let list_len = e
        .storage()
        .instance()
        .get::<_, Vec<u64>>(&DataKey::SubjectAttestations(subject.clone()))
        .map(|v| v.len())
        .unwrap_or(0);

    let count_key = DataKey::SubjectAttestationCount(subject.clone());
    match e.storage().instance().get::<_, u32>(&count_key) {
        Some(count) => {
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
        None => {
            if list_len != 0 {
                fail_drift(
                    e,
                    BondDriftDetails {
                        kind: BondDriftKind::MissingAttestationCounter,
                        subject: subject.clone(),
                        bonded_amount: bond.bonded_amount,
                        slashed_amount: bond.slashed_amount,
                        attestation_count: 0,
                        attestation_list_len: list_len,
                    },
                );
            }
        }
    }
}

/// Emit the drift event, then abort.
///
/// Ordering is the point: the event is published first so the abort carries a
/// classification instead of a bare error code, and the payload is limited to
/// the subject plus the four numbers that locate the offending key pair, so
/// nothing caller-supplied is written into a diagnostic log that outlives the
/// transaction. See the module-level "Failure semantics" for how the revert
/// interacts with the event.
fn fail_drift(e: &Env, details: BondDriftDetails) -> ! {
    crate::events::emit_bond_drift_detected(e, &details);
    panic_with_error!(e, ContractError::InvariantViolation);
}
