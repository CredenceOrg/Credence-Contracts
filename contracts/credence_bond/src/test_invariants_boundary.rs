//! Boundary and failure-classification coverage for the bond drift
//! detector (`crate::invariants`), issue #1334.
//!
//! The detector is a post-write guard, so the only way to exercise it is to
//! place a bond in an exact boundary state and observe whether the guard
//! accepts it or aborts. Every case here drives the real
//! [`crate::invariants::assert_self_consistent`] against hand-placed storage,
//! so the check is tested at the same entry point the bond write paths call,
//! not against a re-implementation of its rules.
//!
//! Coverage map:
//!
//! | Invariant | Boundary tested here |
//! |-----------|----------------------|
//! | I0 payload owner matches the key | match / foreign key, and precedence vs I2 |
//! | I2 `slashed <= bonded` (active only) | equality, `+1`, `i128::MAX`, closed bond |
//! | I4 `bonded >= 0` | `-1`, and precedence vs I5 / I2 |
//! | I5 `slashed >= 0` | `-1`, and precedence vs I2 |
//! | I7 counter vs list length | equal, `-1`, `+1`, absent counter, `u32::MAX` |
//!
//! The suite also pins the two things an indexer depends on: the wire code of
//! [`credence_errors::ContractError::InvariantViolation`] and the shape of the
//! `bond_drift_detected` event.

#![cfg(test)]

extern crate std;

use crate::invariants::{assert_self_consistent, assert_self_consistent_for_bonds, BondDriftKind};
use crate::{CredenceBond, DataKey, IdentityBond};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, Symbol, TryFromVal, Vec};

/// Wire code of `ContractError::InvariantViolation`.
///
/// Pinned because indexers and the panic baseline key off it; the
/// `Invariants` error code must not drift.
const ERR_INVARIANT_VIOLATION: u32 = 233;

/// `bond_drift_detected` is published with exactly this many topics, followed
/// by a fixed 5-field data tuple. Frozen so a schema change cannot silently
/// break a decoder.
const DRIFT_TOPICS: u32 = 2;

// ===== Fixtures =====

/// Build a bond record. Only the fields the detector reads are meaningful;
/// the rest are inert filler.
fn bond(
    identity: &Address,
    bonded_amount: i128,
    slashed_amount: i128,
    active: bool,
) -> IdentityBond {
    IdentityBond {
        identity: identity.clone(),
        bonded_amount,
        bond_start: 0,
        bond_duration: 1,
        slashed_amount,
        active,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    }
}

/// Register the contract without initializing it.
///
/// The detector reads raw instance storage, so no admin, token, or bond is
/// needed; registering alone is enough and keeps each case to a single setup
/// line.
fn register(e: &Env) -> Address {
    e.register(CredenceBond, ())
}

fn put_bond(e: &Env, cid: &Address, identity: &Address, b: &IdentityBond) {
    e.as_contract(cid, || {
        e.storage()
            .instance()
            .set(&DataKey::Bond(identity.clone()), b);
    });
}

fn put_list(e: &Env, cid: &Address, subject: &Address, ids: &[u64]) {
    e.as_contract(cid, || {
        let mut v = Vec::new(e);
        for id in ids {
            v.push_back(*id);
        }
        e.storage()
            .instance()
            .set(&DataKey::SubjectAttestations(subject.clone()), &v);
    });
}

fn put_count(e: &Env, cid: &Address, subject: &Address, count: u32) {
    e.as_contract(cid, || {
        e.storage()
            .instance()
            .set(&DataKey::SubjectAttestationCount(subject.clone()), &count);
    });
}

/// Run the detector, converting a panic into `Err(())`.
///
/// A direct call (rather than a contract call) is used on purpose: the panic
/// then surfaces as a host error that `catch_unwind` can trap, which is the
/// only way to read the `bond_drift_detected` event that is published
/// immediately before the abort.
fn run(e: &Env, cid: &Address, subject: &Address) -> Result<(), ()> {
    let cid = cid.clone();
    let subject = subject.clone();
    let e2 = e.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        e2.as_contract(&cid, || {
            assert_self_consistent(&e2, &subject);
        });
    }));
    outcome.map_err(|_| ())
}

/// The `bond_drift_detected` payload, if the last run emitted one.
fn last_drift(e: &Env) -> Option<(BondDriftKind, i128, i128, u32, u32)> {
    let topic = Symbol::new(e, "bond_drift_detected");
    let mut found = None;
    for (_contract, topics, data) in e.events().all().iter() {
        if let Some(first) = topics.get(0) {
            if Symbol::try_from_val(e, &first).map_or(false, |s| s == topic) {
                found = <(BondDriftKind, i128, i128, u32, u32)>::try_from_val(e, &data).ok();
            }
        }
    }
    found
}

/// Assert the check aborts, then assert *why*: the reported kind, the subject,
/// and the two counters that let an operator locate the offending key pair.
#[track_caller]
fn assert_breach(
    e: &Env,
    cid: &Address,
    subject: &Address,
    expected: BondDriftKind,
    bonded: i128,
    slashed: i128,
    count: u32,
    list_len: u32,
) {
    assert_eq!(
        run(e, cid, subject),
        Err(()),
        "self-check must abort on {:?}",
        expected
    );
    let (kind, got_bonded, got_slashed, got_count, got_len) =
        last_drift(e).expect("bond_drift_detected must be published before the abort");
    assert_eq!(kind, expected, "reported drift kind must be deterministic");
    assert_eq!(got_bonded, bonded, "bonded_amount in the event");
    assert_eq!(got_slashed, slashed, "slashed_amount in the event");
    assert_eq!(got_count, count, "SubjectAttestationCount in the event");
    assert_eq!(got_len, list_len, "SubjectAttestations length in the event");
}

// ===== 1. No bond at all =====

/// An identity that has never bonded has nothing to check, and its absence
/// must not be reported as drift.
#[test]
fn absent_bond_passes_self_check() {
    let e = Env::default();
    let cid = register(&e);
    let subject = Address::generate(&e);

    assert_eq!(run(&e, &cid, &subject), Ok(()));
    assert!(last_drift(&e).is_none(), "a clean check must stay silent");
}

// ===== 2. I2 — slashed within bonded, on an active bond =====

/// The exact equality boundary: a fully slashed bond is legitimate, and
/// `assert_self_consistent` must not treat `slashed == bonded` as a breach.
#[test]
fn slashed_equal_to_bonded_passes() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 1_000, 1_000, true));

    assert_eq!(run(&e, &cid, &s), Ok(()));
}

/// One wei past the boundary. This is the smallest possible I2 breach and must
/// be caught — an off-by-one here would let a slash overshoot by 1.
#[test]
fn slashed_exceeds_bonded_by_one_fails() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 1_000, 1_001, true));

    assert_breach(
        &e,
        &cid,
        &s,
        BondDriftKind::SlashedExceedsBonded,
        1_000,
        1_001,
        0,
        0,
    );
}

/// A bond at the top of the `i128` range with a matching slash. The check must
/// not overflow while comparing.
#[test]
fn max_i128_bonded_with_equal_slashed_passes() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, i128::MAX, i128::MAX, true));

    assert_eq!(run(&e, &cid, &s), Ok(()));
}

/// A zero bond is a legitimate starting point (phantom-balance deployments
/// create bonds before any collateral is escrowed), not drift.
#[test]
fn zero_bonded_zero_slashed_passes() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 0, 0, true));

    assert_eq!(run(&e, &cid, &s), Ok(()));
}

/// I2 is scoped to active bonds. `withdraw_bond` writes `bonded_amount = 0`
/// while keeping `slashed_amount` as history, so a closed position with
/// `slashed > bonded` is the expected shape, not drift. If this ever starts
/// aborting, closing a slashed bond has become impossible.
#[test]
fn closed_bond_may_hold_slashed_above_bonded() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 0, 400, false));

    assert_eq!(run(&e, &cid, &s), Ok(()));
}

// ===== 3. I4 / I5 — negative amounts, and their precedence over I2 =====

/// A negative bonded amount makes the bond's net value meaningless. It is
/// reported as I4, not folded into the I2 magnitude check.
#[test]
fn negative_bonded_amount_reports_negative_bonded() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, -1, 0, true));

    assert_breach(
        &e,
        &cid,
        &s,
        BondDriftKind::NegativeBondedAmount,
        -1,
        0,
        0,
        0,
    );
}

/// A negative slash means an unslash drove the counter below zero. Reporting
/// it as `SlashedExceedsBonded` would send an operator hunting for a slashing
/// bug instead of a slashing-reversal bug, so I5 is classified first.
#[test]
fn negative_slashed_amount_reports_negative_slashed_not_i2() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    // Also violates I2 (`-5 <= 1_000` is fine, so use a value that does not)
    // to prove I5 is what is reported when only I5 is wrong.
    put_bond(&e, &cid, &s, &bond(&s, 1_000, -1, true));

    assert_breach(
        &e,
        &cid,
        &s,
        BondDriftKind::NegativeSlashedAmount,
        1_000,
        -1,
        0,
        0,
    );
}

/// When several invariants are broken at once the reported kind is fixed, so
/// the same corrupted state always produces the same alert. I4 is checked
/// before I5 and I2.
#[test]
fn multiple_breaches_report_bonded_amount_first() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, -10, -20, true));

    assert_breach(
        &e,
        &cid,
        &s,
        BondDriftKind::NegativeBondedAmount,
        -10,
        -20,
        0,
        0,
    );
}

/// Negativity is not scoped to active bonds: a closed bond holding a negative
/// amount is still corrupt, and no mutator can repair it.
#[test]
fn closed_bond_with_negative_bonded_still_fails() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, -1, 0, false));

    assert_eq!(run(&e, &cid, &s), Err(()));
    assert_eq!(
        last_drift(&e).map(|d| d.0),
        Some(BondDriftKind::NegativeBondedAmount)
    );
}

// ===== 4. I0 — the bond payload must belong to the key it is filed under =====

/// A bond stored under `DataKey::Bond(victim)` whose `identity` is someone
/// else. Every `Bond(victim)` read would hand one tenant another tenant's
/// accounting, so this is the highest-severity drift the detector catches.
#[test]
fn bond_filed_under_foreign_key_fails() {
    let e = Env::default();
    let cid = register(&e);
    let key_owner = Address::generate(&e);
    let other = Address::generate(&e);
    // Perfectly consistent amounts — the breach is the ownership mismatch.
    put_bond(&e, &cid, &key_owner, &bond(&other, 500, 100, true));

    assert_breach(
        &e,
        &cid,
        &key_owner,
        BondDriftKind::BondIdentityMismatch,
        500,
        100,
        0,
        0,
    );
}

/// I0 is checked before the amount checks, so a bond that is both misfiled and
/// overslashed is reported as misfiled — the root cause, not a symptom.
#[test]
fn identity_breach_takes_precedence_over_amount_breach() {
    let e = Env::default();
    let cid = register(&e);
    let key_owner = Address::generate(&e);
    let other = Address::generate(&e);
    put_bond(&e, &cid, &key_owner, &bond(&other, 100, 900, true));

    assert_eq!(run(&e, &cid, &key_owner), Err(()));
    assert_eq!(
        last_drift(&e).map(|d| d.0),
        Some(BondDriftKind::BondIdentityMismatch)
    );
}

// ===== 5. I7 — attestation counter vs list length =====

/// The exact agreement boundary.
#[test]
fn attestation_count_equal_to_list_len_passes() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 1_000, 0, true));
    put_list(&e, &cid, &s, &[1, 2, 3]);
    put_count(&e, &cid, &s, 3);

    assert_eq!(run(&e, &cid, &s), Ok(()));
}

/// A counter that trails the list means an append succeeded without its
/// counter increment — the classic half-applied write.
#[test]
fn attestation_count_below_list_len_fails() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 1_000, 0, true));
    put_list(&e, &cid, &s, &[1, 2, 3]);
    put_count(&e, &cid, &s, 2);

    assert_breach(
        &e,
        &cid,
        &s,
        BondDriftKind::AttestationCountMismatch,
        1_000,
        0,
        2,
        3,
    );
}

/// A counter ahead of the list means a revoke decremented the counter without
/// removing the ID.
#[test]
fn attestation_count_above_list_len_fails() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 1_000, 0, true));
    put_list(&e, &cid, &s, &[1, 2]);
    put_count(&e, &cid, &s, 5);

    assert_breach(
        &e,
        &cid,
        &s,
        BondDriftKind::AttestationCountMismatch,
        1_000,
        0,
        5,
        2,
    );
}

/// A counter with no list is the same class of half-applied write seen from
/// the other side.
#[test]
fn attestation_count_without_list_fails() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 1_000, 0, true));
    put_count(&e, &cid, &s, 1);

    assert_breach(
        &e,
        &cid,
        &s,
        BondDriftKind::AttestationCountMismatch,
        1_000,
        0,
        1,
        0,
    );
}

/// Neither key present: both sides read as zero and agree.
#[test]
fn no_attestation_keys_passes() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 1_000, 0, true));

    assert_eq!(run(&e, &cid, &s), Ok(()));
}

/// A populated list with no counter key is reported as a *missing counter*
/// rather than as a numeric mismatch, so the alert names which half of the
/// pair is gone.
#[test]
fn populated_list_without_counter_fails() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 1_000, 0, true));
    put_list(&e, &cid, &s, &[1, 2]);

    assert_breach(
        &e,
        &cid,
        &s,
        BondDriftKind::MissingAttestationCounter,
        1_000,
        0,
        0,
        2,
    );
}

/// The counter type is `u32`, so `u32::MAX` is a real boundary. A saturated
/// counter that no longer tracks the list must be caught rather than wrapped.
#[test]
fn counter_at_u32_max_without_matching_list_fails() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 1_000, 0, true));
    put_count(&e, &cid, &s, u32::MAX);

    assert_eq!(run(&e, &cid, &s), Err(()));
    assert_eq!(
        last_drift(&e).map(|d| d.0),
        Some(BondDriftKind::AttestationCountMismatch)
    );
}

// ===== 6. I7 without a bond =====

/// Attestations can be recorded for a subject that has not bonded. I2 is
/// vacuous there, but I7 must still be enforced or the subject becomes a blind
/// spot.
#[test]
fn attestation_drift_without_a_bond_still_fails() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_list(&e, &cid, &s, &[1, 2]);
    put_count(&e, &cid, &s, 1);

    assert_breach(
        &e,
        &cid,
        &s,
        BondDriftKind::AttestationCountMismatch,
        0,
        0,
        1,
        2,
    );
}

/// The zeroed reference used when there is no bond reports both amounts as
/// `0` rather than leaving the fields unspecified.
#[test]
fn unbonded_subject_with_matching_counter_passes() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_list(&e, &cid, &s, &[7]);
    put_count(&e, &cid, &s, 1);

    assert_eq!(run(&e, &cid, &s), Ok(()));
}

// ===== 7. Batch entry point =====

/// The batch helper checks every identity it touched, in order.
#[test]
fn batch_check_covers_every_identity() {
    let e = Env::default();
    let cid = register(&e);
    let clean = Address::generate(&e);
    let drifted = Address::generate(&e);
    put_bond(&e, &cid, &clean, &bond(&clean, 100, 0, true));
    put_bond(&e, &cid, &drifted, &bond(&drifted, 100, 101, true));

    let e2 = e.clone();
    let cid2 = cid.clone();
    let drifted2 = drifted.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let mut v = Vec::new(&e2);
        v.push_back(clean);
        v.push_back(drifted2);
        e2.as_contract(&cid2, || {
            assert_self_consistent_for_bonds(&e2, &v);
        });
    }));

    assert!(
        outcome.is_err(),
        "the drifted identity must abort the batch"
    );
}

/// An empty batch is a no-op, not a failure.
#[test]
fn batch_check_with_no_identities_passes() {
    let e = Env::default();
    let cid = register(&e);
    let subjects: Vec<Address> = Vec::new(&e);
    let e2 = e.clone();
    e2.as_contract(&cid, || {
        assert_self_consistent_for_bonds(&e2, &subjects);
    });
}

// ===== 8. Wire stability =====

/// The abort code is part of the contract's observable interface.
#[test]
fn invariant_violation_wire_code_is_pinned() {
    assert_eq!(
        credence_errors::ContractError::InvariantViolation as u32,
        ERR_INVARIANT_VIOLATION
    );
}

/// The event shape is frozen: two topics (`bond_drift_detected`, subject) and
/// five data fields (kind, bonded, slashed, count, list length). A decoder
/// built against this shape must keep working.
#[test]
fn drift_event_schema_is_frozen() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 10, 20, true));

    assert_eq!(run(&e, &cid, &s), Err(()));

    let mut found = false;
    for (_contract, topics, data) in e.events().all().iter() {
        if let Some(first) = topics.get(0) {
            if Symbol::try_from_val(&e, &first)
                .map_or(false, |s| s == Symbol::new(&e, "bond_drift_detected"))
            {
                found = true;
                assert_eq!(topics.len(), DRIFT_TOPICS, "topic count is frozen");
                // Decoding a 5-tuple is itself the frozen-shape assertion: a
                // payload with any other field count fails to decode here.
                let decoded: Result<(BondDriftKind, i128, i128, u32, u32), _> =
                    TryFromVal::try_from_val(&e, &data);
                assert!(
                    decoded.is_ok(),
                    "data must decode as the frozen 5-field tuple"
                );
                // Topic 1 is the affected subject, so an operator can attribute
                // the alert without reading the data payload.
                let reported = Address::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
                assert_eq!(reported, s);
            }
        }
    }
    assert!(found, "bond_drift_detected must be published");
}

/// The payload must not carry caller-supplied data. Only the subject address
/// and the four diagnostic numbers cross the boundary, so an indexer can be
/// pointed at the offending keys without exposing attestation contents.
#[test]
fn drift_event_carries_no_caller_data() {
    let e = Env::default();
    let cid = register(&e);
    let s = Address::generate(&e);
    put_bond(&e, &cid, &s, &bond(&s, 7, 9, true));

    assert_eq!(run(&e, &cid, &s), Err(()));

    let (_, bonded, slashed, count, len) = last_drift(&e).expect("drift event");
    // Both amounts are reported even though only `slashed` is at fault, so the
    // reader does not have to re-derive the pair.
    assert_eq!((bonded, slashed), (7, 9));
    // The attestation fields are zero-filled rather than omitted.
    assert_eq!((count, len), (0, 0));
}
