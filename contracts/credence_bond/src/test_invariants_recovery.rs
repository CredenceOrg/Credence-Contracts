//! Recovery, atomicity, and determinism coverage for the bond drift detector
//! (`crate::invariants`), issue #1334.
//!
//! The boundary suite proves the detector classifies state correctly. This
//! suite proves the properties that actually matter on chain:
//!
//! 1. the detector is **wired into** the real write paths, not merely correct
//!    in isolation — a detector nothing calls is the same defect as a
//!    detector that silently returns;
//! 2. a breach **aborts the whole transaction** with no partial state and no
//!    committed events;
//! 3. the system **recovers** once the underlying cause is repaired, so a
//!    detection cannot permanently wedge a bond;
//! 4. the **classification is deterministic**, so an indexer can deduplicate
//!    alerts and an operator can tell a one-off from a systematic fault.
//!
//! Every test drives the public contract surface, so it exercises the same
//! `assert_self_consistent` call that `create_bond`, `top_up`, `slash_bond`,
//! `withdraw_bond`, `liquidate`, `add_attestation`, `add_attestation_batch`,
//! and `revoke_attestation` make.

#![cfg(test)]

extern crate std;

use crate::invariants::BondDriftKind;
use crate::test_helpers;
use crate::{CredenceBondClient, DataKey, IdentityBond};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, Bytes, Env, String, Symbol, TryFromVal, Vec};

/// Bond amount used throughout. In-crate tests build with `cfg(test)`, where
/// `validation::MIN_BOND_AMOUNT` is 1_000 rather than the 1e18 the released
/// WASM enforces.
const BOND: i128 = 1_000_000;
const DURATION: u64 = 3_600;
const DEADLINE_OFFSET: u64 = 3_600;

// ===== Fixtures =====

/// An initialized bond contract holding a live bond for `identity`.
struct Fixture {
    env: &'static Env,
    client: CredenceBondClient<'static>,
    contract: Address,
    admin: Address,
    identity: Address,
}

fn setup() -> Fixture {
    // Leaked so the client can carry a `'static` borrow while the fixture also
    // hands out `&Env` for direct storage surgery. Tests are short-lived, so
    // this is a fixed, bounded cost rather than a leak that grows per case.
    let env: &'static Env = std::boxed::Box::leak(std::boxed::Box::new(Env::default()));
    env.mock_all_auths();
    let (client, admin, identity, _token, contract) = test_helpers::setup_with_token(env);
    client.create_bond(&identity, &BOND, &DURATION, &false, &0_u64);
    Fixture {
        env,
        client,
        contract,
        admin,
        identity,
    }
}

/// Write a raw bond record, bypassing the contract's own writes.
fn put_bond_raw(f: &Fixture, subject: &Address, b: &IdentityBond) {
    let key = DataKey::Bond(subject.clone());
    let value = b.clone();
    f.env.as_contract(&f.contract, move || {
        f.env.storage().instance().set(&key, &value);
    });
}

/// Overwrite the subject's bond to simulate drift or a repaired state.
fn put_bond(f: &Fixture, b: &IdentityBond) {
    put_bond_raw(f, &f.identity, b);
}

/// Force `SubjectAttestationCount`, the counter half of I7.
fn set_attestation_count(f: &Fixture, subject: &Address, count: u32) {
    let key = DataKey::SubjectAttestationCount(subject.clone());
    f.env.as_contract(&f.contract, move || {
        f.env.storage().instance().set(&key, &count);
    });
}

/// Run the detector in-process, trapping the abort so it can be asserted on.
///
/// Warning: a `panic_with_error!` raised inside a top-level `as_contract` frame
/// leaves the Soroban test `Env` poisoned — every *subsequent* host call on
/// that `Env` fails with `Error(Context, InvalidAction)`. So a test may either
/// call this and then make no further contract calls on the same `Env`, or
/// build a throwaway fixture. Prefer a reverting `try_*` call on the contract
/// surface where the behaviour under test allows it; that is also a stronger
/// assertion, because it proves the guard is reached from real write paths.
fn run_check(f: &Fixture, subject: &Address) -> Result<(), ()> {
    let env = f.env;
    let contract = f.contract.clone();
    let subject = subject.clone();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        env.as_contract(&contract, || {
            crate::invariants::assert_self_consistent(env, &subject);
        });
    }))
    .map_err(|_| ())
}

fn read_bond(f: &Fixture) -> IdentityBond {
    f.client.get_identity_state(&f.identity)
}

fn slash(f: &Fixture, amount: i128) {
    f.client
        .slash_bond(&f.admin, &f.identity, &amount, &Bytes::new(f.env));
}

/// A bond record with every field filled in except the two amounts.
fn bond_record(identity: &Address, bonded: i128, slashed: i128) -> IdentityBond {
    IdentityBond {
        identity: identity.clone(),
        bonded_amount: bonded,
        bond_start: 0,
        bond_duration: DURATION,
        slashed_amount: slashed,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    }
}

/// The drift fixture used by the "abort, then recover" cases.
///
/// `slashed_amount = -1` is the one breach no mutator can absorb: every
/// bond-writing entry point either raises `bonded_amount` (which cannot fix a
/// negative `slashed_amount`) or raises `slashed_amount` by a validated
/// positive amount. So a contract already holding this state is genuinely
/// blocked, which is what makes it the right fixture for proving that a
/// detection does not permanently wedge a bond.
fn unrepairable_drift(f: &Fixture) -> IdentityBond {
    bond_record(&f.identity, BOND, -1)
}

/// Every `bond_drift_detected` payload in the env's log, oldest first.
///
/// Note on scope, which matters when reading the assertions below: a *reverted
/// contract call* (`try_top_up` and friends) rolls its events back with the
/// rest of the frame, so an aborted call contributes nothing here. That is why
/// the event is an operator/diagnostic-log signal rather than an on-ledger one.
/// A direct `as_contract` abort inside a test does leave its event behind,
/// which is what the classification-stability assertions rely on.
fn drift_reports(env: &Env) -> std::vec::Vec<(BondDriftKind, i128, i128, u32, u32)> {
    let topic = Symbol::new(env, "bond_drift_detected");
    let mut out = std::vec::Vec::new();
    for (_c, topics, data) in env.events().all().iter() {
        if let Some(first) = topics.get(0) {
            if Symbol::try_from_val(env, &first).map_or(false, |s| s == topic) {
                if let Ok(r) = <(BondDriftKind, i128, i128, u32, u32)>::try_from_val(env, &data) {
                    out.push(r);
                }
            }
        }
    }
    out
}

/// Count of `bond_drift_detected` events in the log.
fn committed_drift_events(env: &Env) -> u32 {
    drift_reports(env).len() as u32
}

fn register_attester(f: &Fixture) -> Address {
    let attester = Address::generate(f.env);
    f.client.register_attester(&attester);
    attester
}

fn add_attestation(f: &Fixture, attester: &Address, tag: &str, nonce: u64) -> u64 {
    f.client
        .add_attestation(
            attester,
            &f.identity,
            &String::from_str(f.env, tag),
            &f.contract,
            &f.env.ledger().timestamp().saturating_add(DEADLINE_OFFSET),
            &nonce,
        )
        .id
}

// ===== 1. Success: the detector stays silent on every happy path =====

/// A freshly created bond is self-consistent, and creation emits no drift
/// event. If the detector were wired in wrongly, this is the first place it
/// would show.
#[test]
fn create_bond_leaves_state_self_consistent() {
    let f = setup();
    let bond = read_bond(&f);

    assert_eq!(bond.bonded_amount, BOND);
    assert_eq!(bond.slashed_amount, 0);
    assert!(bond.active);
    assert_eq!(run_check(&f, &f.identity), Ok(()));
    assert_eq!(
        committed_drift_events(f.env),
        0,
        "no drift event on a clean create"
    );
}

/// Adding collateral keeps `slashed <= bonded` and stays clean.
#[test]
fn top_up_keeps_bond_self_consistent() {
    let f = setup();
    f.client.top_up(&f.identity, &BOND);

    let bond = read_bond(&f);
    assert_eq!(bond.bonded_amount, BOND * 2);
    assert!(bond.slashed_amount <= bond.bonded_amount);
    assert_eq!(committed_drift_events(f.env), 0);
}

// ===== 2. I2 boundary through the public API =====

/// Slashing the entire bond is legal and is exactly the I2 equality boundary.
/// This is the case that regresses first if the bound is tightened.
#[test]
fn slash_to_exactly_full_bond_is_accepted() {
    let f = setup();
    slash(&f, BOND);

    let bond = read_bond(&f);
    assert_eq!(bond.bonded_amount, BOND);
    assert_eq!(bond.slashed_amount, BOND, "a full slash is the I2 boundary");
    assert_eq!(committed_drift_events(f.env), 0);
}

/// One wei past the available balance is rejected, the bond is untouched, and
/// the same call still works for an amount the bond can cover.
#[test]
fn slash_beyond_available_is_rejected_without_state_change() {
    let f = setup();
    let before = read_bond(&f);
    let events_before = f.env.events().all().len();

    assert!(f
        .client
        .try_slash_bond(&f.admin, &f.identity, &(BOND + 1), &Bytes::new(f.env))
        .is_err());

    assert_eq!(
        read_bond(&f),
        before,
        "rejected slash must not move the bond"
    );
    assert_eq!(
        f.env.events().all().len(),
        events_before,
        "rejected slash must not publish events"
    );

    // Recovery: the same entry point works once the amount is in range.
    slash(&f, 1);
    assert_eq!(read_bond(&f).slashed_amount, 1);
}

// ===== 3. Closing a slashed bond (regression for the I2 scope) =====

/// `withdraw_bond` writes `bonded_amount = 0` while retaining
/// `slashed_amount` as history. With the detector live, that shape must not be
/// mistaken for drift — otherwise a slashed bond could never be closed and the
/// collateral would be stranded.
#[test]
fn withdraw_bond_on_a_slashed_bond_succeeds() {
    let f = setup();
    slash(&f, BOND / 2);
    f.client.withdraw_bond(&f.identity);

    let bond = read_bond(&f);
    assert!(!bond.active, "withdraw_bond must close the bond");
    assert_eq!(bond.bonded_amount, 0);
    assert_eq!(bond.slashed_amount, BOND / 2, "slash history is retained");
    assert_eq!(run_check(&f, &f.identity), Ok(()));
    assert_eq!(committed_drift_events(f.env), 0);
}

/// Liquidation keeps both amounts and only flips `active`, so the I2 bound
/// still holds afterwards and the detector stays quiet.
#[test]
fn liquidate_keeps_slashed_within_bonded() {
    let f = setup();
    // `expired_unrenewed` eligibility: push past `bond_start + bond_duration`.
    f.env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp.saturating_add(DURATION + 1);
    });
    f.client.liquidate(&f.admin, &f.identity);

    let bond = read_bond(&f);
    assert!(!bond.active);
    assert!(bond.slashed_amount <= bond.bonded_amount);
    assert_eq!(run_check(&f, &f.identity), Ok(()));
    assert_eq!(committed_drift_events(f.env), 0);
}

// ===== 4. I7 through the attestation entry points =====

/// Each add appends to `SubjectAttestations` and increments
/// `SubjectAttestationCount` together; the detector is the thing that proves
/// the two stayed in step.
#[test]
fn add_attestation_keeps_counter_in_sync() {
    let f = setup();
    let attester = register_attester(&f);

    for nonce in 0..3u64 {
        add_attestation(&f, &attester, &std::format!("att-{nonce}"), nonce);
    }

    assert_eq!(f.client.get_subject_attestation_count(&f.identity), 3);
    assert_eq!(f.client.get_subject_attestations(&f.identity).len(), 3);
    assert_eq!(committed_drift_events(f.env), 0);
}

/// A revoke removes the ID and decrements the counter in the same write.
#[test]
fn revoke_attestation_keeps_counter_in_sync() {
    let f = setup();
    let attester = register_attester(&f);
    let id = add_attestation(&f, &attester, "revocable", 0);

    f.client.revoke_attestation(
        &attester,
        &id,
        &f.contract,
        &f.env.ledger().timestamp().saturating_add(DEADLINE_OFFSET),
        &1u64,
    );

    assert_eq!(f.client.get_subject_attestation_count(&f.identity), 0);
    assert_eq!(f.client.get_subject_attestations(&f.identity).len(), 0);
    assert_eq!(committed_drift_events(f.env), 0);
}

/// Revoking down to an empty list and then re-adding is the round trip that
/// would break if the counter saturated or failed to decrement.
#[test]
fn revoke_then_re_add_keeps_counter_in_sync() {
    let f = setup();
    let attester = register_attester(&f);
    let id = add_attestation(&f, &attester, "cycle", 0);
    f.client.revoke_attestation(
        &attester,
        &id,
        &f.contract,
        &f.env.ledger().timestamp().saturating_add(DEADLINE_OFFSET),
        &1u64,
    );
    assert_eq!(f.client.get_subject_attestation_count(&f.identity), 0);

    add_attestation(&f, &attester, "cycle-2", 2);
    assert_eq!(f.client.get_subject_attestation_count(&f.identity), 1);
    assert_eq!(f.client.get_subject_attestations(&f.identity).len(), 1);
}

/// Batch attestation adds many IDs in one write and bumps the counter by the
/// same amount; the detector is what proves the aggregate was consistent.
///
/// Each item needs a distinct attester (the entry point rejects duplicates) and
/// its own domain-bound nonce.
#[test]
fn add_attestation_batch_keeps_counter_in_sync() {
    let f = setup();
    let deadline = f.env.ledger().timestamp().saturating_add(DEADLINE_OFFSET);

    let mut items = Vec::new(f.env);
    for i in 0..3u32 {
        let attester = register_attester(&f);
        items.push_back(crate::AttestationBatchItem {
            attester,
            attestation_data: String::from_str(f.env, &std::format!("batch-{i}")),
            contract_id: f.contract.clone(),
            deadline,
            nonce: 0,
        });
    }
    f.client.add_attestation_batch(&f.identity, &items);

    assert_eq!(f.client.get_subject_attestation_count(&f.identity), 3);
    assert_eq!(f.client.get_subject_attestations(&f.identity).len(), 3);
    assert_eq!(committed_drift_events(f.env), 0);
}

/// A rejected attestation write must leave both halves of I7 exactly as they
/// were. This is the atomicity half of the claim: a mutator that aborted after
/// touching one half of the pair would desynchronise the counter, and every
/// later check would then be reporting damage instead of preventing it.
#[test]
fn rejected_attestation_leaves_counter_untouched() {
    let f = setup();
    let stranger = Address::generate(f.env);

    assert!(f
        .client
        .try_add_attestation(
            &stranger,
            &f.identity,
            &String::from_str(f.env, "unregistered"),
            &f.contract,
            &f.env.ledger().timestamp().saturating_add(DEADLINE_OFFSET),
            &0u64
        )
        .is_err());

    assert_eq!(f.client.get_subject_attestation_count(&f.identity), 0);
    assert_eq!(f.client.get_subject_attestations(&f.identity).len(), 0);
    assert_eq!(
        run_check(&f, &f.identity),
        Ok(()),
        "a rejected write must not desynchronise I7"
    );
}

// ===== 5. Breach: abort, no partial state, then recovery =====

/// Overwrite the stored bond with drifted state, then attempt a normal
/// mutation. The write path runs the detector, so the transaction must abort.
///
/// This is the end-to-end proof that the detector is actually wired into
/// `top_up` — the boundary suite only proves the detector works, not that the
/// contract calls it.
#[test]
fn injected_drift_makes_mutation_revert_without_partial_state() {
    let f = setup();
    let drifted = unrepairable_drift(&f);
    put_bond(&f, &drifted);
    let events_before = f.env.events().all().len();

    assert!(
        f.client.try_top_up(&f.identity, &BOND).is_err(),
        "a drifted bond must block further mutation"
    );

    // Atomicity: the aborted top-up left the drifted record exactly as it was
    // rather than half-applying the increase.
    assert_eq!(read_bond(&f), drifted, "no partial write may survive");
    assert_eq!(
        f.env.events().all().len(),
        events_before,
        "an aborted contract call commits no events"
    );
}

/// The public `slash_bond` entry point writes `slashed_amount` directly rather
/// than through `slashing::slash_bond`, so it needs its own guard. Without one
/// it was the single bond mutator with no drift detection, on the very field
/// I2 is about.
#[test]
fn slash_bond_is_blocked_by_injected_drift() {
    let f = setup();
    // A misfiled bond: the record stored under `f.identity` names someone else.
    // `slash_bond` writes `slashed_amount` straight through, so this is the
    // only breach it cannot incidentally repair on its way past I5/I2.
    let drifted = IdentityBond {
        identity: Address::generate(f.env),
        ..bond_record(&f.identity, BOND, 0)
    };
    put_bond(&f, &drifted);

    assert!(
        f.client
            .try_slash_bond(&f.admin, &f.identity, &1, &Bytes::new(f.env))
            .is_err(),
        "slash_bond must run the detector too"
    );
    assert_eq!(read_bond(&f), drifted, "the aborted slash left no trace");

    // Recovery: once the record is filed under its own key again, the very same
    // call succeeds.
    let repaired = IdentityBond {
        identity: f.identity.clone(),
        ..drifted
    };
    put_bond(&f, &repaired);
    slash(&f, 1);
    assert_eq!(read_bond(&f).slashed_amount, 1);
}

/// The other half of the recovery story: once the corrupted state is corrected,
/// the very same call that was blocked succeeds. A detector that permanently
/// wedged the contract would fail here.
#[test]
fn operation_succeeds_after_drift_is_repaired() {
    let f = setup();
    let drifted = unrepairable_drift(&f);
    put_bond(&f, &drifted);
    assert!(f.client.try_top_up(&f.identity, &BOND).is_err());

    // Repair: bring `slashed_amount` back to a non-negative value.
    put_bond(
        &f,
        &IdentityBond {
            slashed_amount: 0,
            ..drifted
        },
    );

    f.client.top_up(&f.identity, &BOND);
    let bond = read_bond(&f);
    assert_eq!(bond.bonded_amount, BOND * 2);
    assert_eq!(bond.slashed_amount, 0);
}

/// A `slashed > bonded` bond is *healable* by a mutator that raises
/// `bonded_amount`, so `top_up` legitimately absorbs the drift instead of
/// aborting. Pinned because it is the one case where a mutation is allowed to
/// proceed on a drifted bond, and a future change must not silently turn it
/// into a hard block (or quietly rely on it).
#[test]
fn top_up_absorbs_an_amount_only_breach() {
    let f = setup();
    put_bond(&f, &bond_record(&f.identity, BOND, BOND + 1));

    f.client.top_up(&f.identity, &BOND);
    let bond = read_bond(&f);
    assert_eq!(bond.bonded_amount, BOND * 2);
    assert_eq!(bond.slashed_amount, BOND + 1);
    assert!(
        bond.slashed_amount <= bond.bonded_amount,
        "the top-up raised bonded_amount past slashed_amount"
    );
}

/// Same shape for the attestation counter: an injected half-applied write is
/// repaired and the contract keeps working.
#[test]
fn attestation_flow_recovers_after_counter_drift_is_repaired() {
    let f = setup();
    let attester = register_attester(&f);
    add_attestation(&f, &attester, "drifty", 0);

    // Break I7 behind the contract's back.
    set_attestation_count(&f, &f.identity, 99);

    // Any attestation write for this subject now trips I7 and aborts.
    assert!(f
        .client
        .try_add_attestation(
            &attester,
            &f.identity,
            &String::from_str(f.env, "blocked"),
            &f.contract,
            &f.env.ledger().timestamp().saturating_add(DEADLINE_OFFSET),
            &1u64
        )
        .is_err());
    assert_eq!(f.client.get_subject_attestation_count(&f.identity), 99);

    // Repair the counter to match the one live attestation, then retry. The
    // rejected attempt rolled its nonce write back with the rest of the
    // transaction, so the retry reuses nonce 1.
    set_attestation_count(&f, &f.identity, 1);
    add_attestation(&f, &attester, "unblocked", 1);
    assert_eq!(f.client.get_subject_attestation_count(&f.identity), 2);
    assert_eq!(f.client.get_subject_attestations(&f.identity).len(), 2);
}

// ===== 6. Authorization and determinism =====

/// A caller without admin rights cannot move `slashed_amount` at all, so the
/// detector never sees attacker-chosen values on that field.
#[test]
fn unauthorized_slash_cannot_reach_the_drift_check() {
    let f = setup();
    let stranger = Address::generate(f.env);
    let before = read_bond(&f);

    assert!(f
        .client
        .try_slash_bond(&stranger, &f.identity, &1, &Bytes::new(f.env))
        .is_err());

    assert_eq!(read_bond(&f), before);
    assert_eq!(committed_drift_events(f.env), 0);
}

/// The same corrupted state must always produce the same classification, or an
/// indexer cannot deduplicate alerts and an operator cannot tell a one-off from
/// a systematic fault.
#[test]
fn diagnosis_is_deterministic_across_repeated_checks() {
    // A fresh fixture per iteration, because the aborting in-process check
    // poisons its `Env` (see `run_check`). Independent runs from independent
    // state is the stronger form of the claim anyway: the classification must
    // not depend on any accumulated history.
    let mut reports = std::vec::Vec::new();
    for _ in 0..5 {
        let f = setup();
        let drifted = bond_record(&f.identity, BOND, BOND * 2);
        put_bond(&f, &drifted);
        assert_eq!(read_bond(&f), drifted, "the injected state is in place");

        assert!(
            run_check(&f, &f.identity).is_err(),
            "every run must abort on the same state"
        );

        let found = drift_reports(f.env);
        assert_eq!(found.len(), 1, "exactly one report per aborted check");
        assert_eq!(found[0].0, BondDriftKind::SlashedExceedsBonded);
        assert_eq!((found[0].1, found[0].2), (BOND, BOND * 2));

        reports.push(found[0].clone());
    }

    assert!(
        reports.windows(2).all(|w| w[0] == w[1]),
        "the same corrupted state must classify identically every time"
    );
}

/// Interleaving a drifted subject with a clean one must not contaminate the
/// clean subject: the detector is per-subject and holds no shared state.
#[test]
fn drift_in_one_subject_does_not_affect_another() {
    let f = setup();
    let clean = f.identity.clone();
    let other = Address::generate(f.env);
    f.client
        .create_bond(&other, &BOND, &DURATION, &false, &0_u64);

    // Break only `other`, with a breach no mutator can absorb.
    put_bond_raw(&f, &other, &bond_record(&other, BOND, -1));

    // The clean subject is unaffected and still mutable.
    f.client.top_up(&clean, &1);
    assert_eq!(read_bond(&f).bonded_amount, BOND + 1);
    assert_eq!(run_check(&f, &clean), Ok(()));

    // The drifted subject is blocked.
    assert!(f.client.try_top_up(&other, &1).is_err());
}

/// A subject with attestations but no bond is still checked for I7; the write
/// path must not skip it just because `DataKey::Bond` is absent.
#[test]
fn unbonded_subject_attestation_drift_is_caught() {
    let f = setup();
    let attester = register_attester(&f);
    let subject = Address::generate(f.env);
    let deadline = f.env.ledger().timestamp().saturating_add(DEADLINE_OFFSET);
    f.client.add_attestation(
        &attester,
        &subject,
        &String::from_str(f.env, "no-bond"),
        &f.contract,
        &deadline,
        &0u64,
    );
    assert_eq!(f.client.get_subject_attestation_count(&subject), 1);

    // Desynchronise the counter; the next attestation for this subject aborts.
    set_attestation_count(&f, &subject, 7);

    assert!(f
        .client
        .try_add_attestation(
            &attester,
            &subject,
            &String::from_str(f.env, "blocked"),
            &f.contract,
            &deadline,
            &1u64
        )
        .is_err());
    assert_eq!(f.client.get_subject_attestation_count(&subject), 7);
}

/// The event payload is the module's contract with indexers. Pinned here so a
/// change to the payload type is caught by this suite rather than by a
/// downstream decoder.
#[test]
fn drift_kind_enum_remains_decodable() {
    let f = setup();
    put_bond(&f, &bond_record(&f.identity, BOND, BOND + 1));

    f.env.as_contract(&f.contract, || {
        crate::events::emit_bond_drift_detected(
            f.env,
            &crate::invariants::BondDriftDetails {
                kind: BondDriftKind::SlashedExceedsBonded,
                subject: f.identity.clone(),
                bonded_amount: BOND,
                slashed_amount: BOND + 1,
                attestation_count: 0,
                attestation_list_len: 0,
            },
        );
    });

    let (kind, bonded, slashed, _, _) =
        drift_reports(f.env).pop().expect("drift event must decode");
    assert_eq!(kind, BondDriftKind::SlashedExceedsBonded);
    assert_eq!((bonded, slashed), (BOND, BOND + 1));
}
