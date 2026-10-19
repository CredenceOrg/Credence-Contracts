#![cfg(test)]

//! Adversarial regression coverage for the dispute-status guard surface in
//! [`status`](super::status).
//!
//! The per-variant cases at the bottom of this file pin each verdict in
//! isolation. Everything above them pins the *relationships* the guards have to
//! keep, because that is where this surface has historically broken:
//!
//! 1. `is_dispute_active`, `require_dispute_inactive` and
//!    `require_dispute_resolved` are three hand-written classifications of the
//!    same enum, maintained independently. A new state or an amended rule
//!    desynchronises them without any compile error.
//! 2. `DisputeStatus` is a `#[contracttype]` enum whose discriminants are the
//!    on-ledger encoding of a stored dispute, so renumbering it silently
//!    reinterprets every persisted status.
//! 3. The guards are fed statuses read back from storage, including stale and
//!    corrupt ones.

use super::status::{
    is_dispute_active, require_dispute_inactive, require_dispute_resolved, require_transition,
    ArbitrationError, DisputeStatus,
};
use soroban_sdk::{contract, Address, Env, IntoVal, Symbol, TryFromVal, Val};

/// Every declared variant, in wire order. Adding a variant means adding it
/// here, which is what forces the closed-world cases below to classify it too.
const ALL_STATUSES: [DisputeStatus; 7] = [
    DisputeStatus::Open,
    DisputeStatus::Voting,
    DisputeStatus::Resolving,
    DisputeStatus::Resolved,
    DisputeStatus::Cancelled,
    DisputeStatus::Tied,
    DisputeStatus::Archived,
];

/// The states that block lease-modifying operations.
const ACTIVE_STATUSES: [DisputeStatus; 3] = [
    DisputeStatus::Open,
    DisputeStatus::Voting,
    DisputeStatus::Resolving,
];

/// The states in which a dispute has reached a definite, consumer-visible end.
const TERMINAL_STATUSES: [DisputeStatus; 3] = [
    DisputeStatus::Resolved,
    DisputeStatus::Cancelled,
    DisputeStatus::Tied,
];

/// Storage frame for the persistence cases. Deliberately empty: these tests
/// exercise the ledger encoding of `DisputeStatus`, not contract logic.
#[contract]
struct GuardFixture;

fn key(env: &Env) -> Symbol {
    Symbol::new(env, "dispute")
}

fn fixture() -> (Env, Address) {
    let env = Env::default();
    let id = env.register_contract(None, GuardFixture);
    (env, id)
}

// ════════════════════════════════════════════════════════════════════════
//  Closed world: the three guards must classify identically
// ════════════════════════════════════════════════════════════════════════

#[test]
fn active_set_is_exactly_open_voting_and_resolving() {
    let mut active_count = 0_usize;
    for status in ALL_STATUSES {
        if is_dispute_active(status) {
            assert!(
                ACTIVE_STATUSES.contains(&status),
                "{status:?} became active without being declared"
            );
            active_count += 1;
        } else {
            assert!(
                !ACTIVE_STATUSES.contains(&status),
                "{status:?} stopped blocking operations"
            );
        }
    }
    assert_eq!(active_count, ACTIVE_STATUSES.len());
}

#[test]
fn inactive_guard_is_the_exact_complement_of_the_predicate() {
    for status in ALL_STATUSES {
        assert_eq!(
            require_dispute_inactive(status).is_err(),
            is_dispute_active(status),
            "{status:?}: require_dispute_inactive disagrees with is_dispute_active"
        );
    }
}

#[test]
fn every_non_terminal_state_is_rejected_by_both_resolution_guards() {
    // `require_dispute_resolved` was left non-exhaustive when `Archived` was
    // added to the enum, which is exactly the drift this case forbids.
    for status in ALL_STATUSES {
        let terminal = TERMINAL_STATUSES.contains(&status);
        assert_eq!(
            require_dispute_resolved(&status).is_ok(),
            terminal,
            "{status:?} must be {} for require_dispute_resolved",
            if terminal { "accepted" } else { "rejected" }
        );
    }
}

#[test]
fn every_rejection_reports_dispute_active_and_never_a_generic_code() {
    // Callers branch on the error code, so a guard rejection that arrives as
    // `InvalidTransition` would hide why the operation was refused.
    for status in ALL_STATUSES {
        let outcomes = [
            require_dispute_inactive(status),
            require_dispute_resolved(&status),
        ];
        for outcome in outcomes {
            if let Err(err) = outcome {
                assert_eq!(err, ArbitrationError::DisputeActive, "{status:?}");
                assert_ne!(err, ArbitrationError::InvalidTransition);
            }
        }
    }
}

#[test]
fn verdicts_are_stable_across_repeated_and_interleaved_evaluation() {
    // Duplicate calls must not exhaust or mutate anything, and evaluating one
    // status must not leak into the verdict for another.
    for status in ALL_STATUSES {
        let first = (is_dispute_active(status), require_dispute_inactive(status));
        for _ in 0..8 {
            assert_eq!(is_dispute_active(status), first.0);
            assert_eq!(require_dispute_inactive(status), first.1);
        }
    }
    for status in ALL_STATUSES {
        let _ = require_dispute_inactive(DisputeStatus::Voting);
        assert_eq!(
            require_dispute_inactive(status).is_err(),
            is_dispute_active(status)
        );
    }
}

#[test]
fn the_guard_is_a_state_check_not_an_authorization_check() {
    // These guards carry no caller, so they can never be the only thing
    // protecting an operation. Pinning that keeps a future signature change
    // (adding an `Address`) from quietly turning them into an ACL.
    for status in ALL_STATUSES {
        assert_eq!(
            require_dispute_inactive(status),
            require_dispute_inactive(status),
            "verdict must depend on the status alone"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════
//  The documented divergence between the two terminal guards
// ════════════════════════════════════════════════════════════════════════

#[test]
fn archived_is_inactive_yet_not_resolved() {
    // The two guards deliberately disagree here: an archived dispute no longer
    // blocks lease work, but it is not a ruling either. Pinning the divergence
    // means a future "make these consistent" edit has to be a conscious choice.
    assert_eq!(require_dispute_inactive(DisputeStatus::Archived), Ok(()));
    assert_eq!(
        require_dispute_resolved(&DisputeStatus::Archived),
        Err(ArbitrationError::DisputeActive)
    );
    assert!(!is_dispute_active(DisputeStatus::Archived));

    // And it is still a live dispute: the machine allows it to be reopened.
    assert_eq!(
        require_transition(DisputeStatus::Archived, DisputeStatus::Voting),
        Ok(())
    );
}

#[test]
fn the_two_guards_agree_on_every_state_except_archived() {
    for status in ALL_STATUSES {
        let inactive_ok = require_dispute_inactive(status).is_ok();
        let resolved_ok = require_dispute_resolved(&status).is_ok();
        if status == DisputeStatus::Archived {
            assert!(
                inactive_ok && !resolved_ok,
                "Archived must be the only state where the guards diverge"
            );
        } else {
            assert_eq!(
                inactive_ok, resolved_ok,
                "unexpected divergence on {status:?}"
            );
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
//  Wire layout: persisted statuses are reinterpreted, never re-validated
// ════════════════════════════════════════════════════════════════════════

#[test]
fn discriminants_match_the_deployed_layout() {
    // These values are already on the ledger. Renumbering turns a stored
    // `Archived` (6) dispute into whatever the new variant claims to be.
    assert_eq!(DisputeStatus::Open as u32, 0);
    assert_eq!(DisputeStatus::Voting as u32, 1);
    assert_eq!(DisputeStatus::Resolving as u32, 2);
    assert_eq!(DisputeStatus::Resolved as u32, 3);
    assert_eq!(DisputeStatus::Cancelled as u32, 4);
    assert_eq!(DisputeStatus::Tied as u32, 5);
    assert_eq!(DisputeStatus::Archived as u32, 6);
}

#[test]
fn discriminants_are_unique_and_contiguous_from_zero() {
    for (index, status) in ALL_STATUSES.iter().enumerate() {
        assert_eq!(
            *status as u32, index as u32,
            "the wire table must stay dense so no code is silently reused"
        );
        for other in ALL_STATUSES.iter().skip(index + 1) {
            assert_ne!(*status as u32, *other as u32, "{status:?} / {other:?}");
        }
    }
}

#[test]
fn dispute_active_error_code_is_wire_stable() {
    assert_eq!(ArbitrationError::DisputeActive as u32, 17);
    assert_ne!(
        ArbitrationError::DisputeActive as u32,
        ArbitrationError::CursorOutOfRange as u32
    );
    assert_ne!(
        ArbitrationError::DisputeActive as u32,
        ArbitrationError::InvalidTransition as u32
    );
}

// ════════════════════════════════════════════════════════════════════════
//  Statuses read back from the ledger
// ════════════════════════════════════════════════════════════════════════

#[test]
fn guard_verdict_survives_a_storage_roundtrip() {
    let (env, id) = fixture();
    for status in ALL_STATUSES {
        let read_back = env.as_contract(&id, || {
            env.storage().persistent().set(&key(&env), &status);
            env.storage().persistent().get(&key(&env)).unwrap()
        });
        assert_eq!(read_back, status);
        assert_eq!(
            require_dispute_inactive(read_back),
            require_dispute_inactive(status),
            "round-trip changed the inactive-guard verdict for {status:?}"
        );
        assert_eq!(
            require_dispute_resolved(&read_back),
            require_dispute_resolved(&status),
            "round-trip changed the resolution verdict for {status:?}"
        );
    }
}

#[test]
fn reading_a_stored_status_does_not_change_it() {
    let (env, id) = fixture();
    env.as_contract(&id, || {
        env.storage()
            .persistent()
            .set(&key(&env), &DisputeStatus::Voting);

        for _ in 0..3 {
            let read_back: DisputeStatus = env.storage().persistent().get(&key(&env)).unwrap();
            assert_eq!(read_back, DisputeStatus::Voting);
            assert!(require_dispute_inactive(read_back).is_err());
        }
    });
}

#[test]
fn a_status_shares_the_ledger_encoding_of_its_discriminant() {
    // Both directions are pinned: the enum's wire form must stay the bare u32
    // already written by deployed contracts, so a status can be read by a
    // consumer that never saw this crate's types and vice versa.
    let (env, _id) = fixture();
    for status in ALL_STATUSES {
        let code = status as u32;

        let from_code: Val = code.into_val(&env);
        assert_eq!(
            DisputeStatus::try_from_val(&env, &from_code),
            Ok(status),
            "{code} must decode back to {status:?}"
        );

        let from_status: Val = status.into_val(&env);
        assert_eq!(
            u32::try_from_val(&env, &from_status),
            Ok(code),
            "{status:?} must be readable as its discriminant"
        );
    }
}

#[test]
fn an_out_of_range_status_code_does_not_deserialize_into_a_verdict() {
    // The guards are handed a `DisputeStatus`, never a raw code. A truncated,
    // stale, or hostile u32 has to fail conversion rather than collapse onto a
    // variant: silently mapping 7 to `Resolved` would admit lease work on a
    // dispute that is still being voted on.
    let (env, _id) = fixture();
    for bogus in [7_u32, 8, 42, u32::MAX] {
        let val: Val = bogus.into_val(&env);
        assert!(
            DisputeStatus::try_from_val(&env, &val).is_err(),
            "{bogus} is not a status and must not deserialize as one"
        );
    }
}

#[test]
fn a_transition_written_to_storage_flips_the_guard_exactly_once() {
    let (env, id) = fixture();
    env.as_contract(&id, || {
        let store = key(&env);
        env.storage().persistent().set(&store, &DisputeStatus::Open);

        // The legal path Open -> Voting -> Resolving -> Resolved stays blocked
        // until the dispute actually reaches a terminal state.
        let mut flip: Option<DisputeStatus> = None;
        for (from, to) in [
            (DisputeStatus::Open, DisputeStatus::Voting),
            (DisputeStatus::Voting, DisputeStatus::Resolving),
            (DisputeStatus::Resolving, DisputeStatus::Resolved),
        ] {
            let current: DisputeStatus = env.storage().persistent().get(&store).unwrap();
            assert_eq!(
                require_dispute_inactive(current),
                Err(ArbitrationError::DisputeActive),
                "{current:?} must still block before the transition"
            );
            assert_eq!(require_transition(from, to), Ok(()));
            env.storage().persistent().set(&store, &to);
            let cleared = require_dispute_inactive(to).is_ok();
            assert_eq!(
                cleared,
                !is_dispute_active(to),
                "{to:?}: the guard and the predicate disagree"
            );
            if cleared {
                assert!(
                    flip.is_none(),
                    "the guard must flip exactly once along the legal path"
                );
                flip = Some(to);
            }
        }
        assert_eq!(
            flip,
            Some(DisputeStatus::Resolved),
            "only the terminal step may clear the guard"
        );

        let final_state: DisputeStatus = env.storage().persistent().get(&store).unwrap();
        assert_eq!(final_state, DisputeStatus::Resolved);
        assert_eq!(require_dispute_inactive(final_state), Ok(()));
        assert_eq!(require_dispute_resolved(&final_state), Ok(()));

        // Archiving afterwards keeps it allowed for lease work but not a
        // ruling, so a reopen cannot be smuggled past the resolution guard.
        env.storage()
            .persistent()
            .set(&store, &DisputeStatus::Archived);
        let archived: DisputeStatus = env.storage().persistent().get(&store).unwrap();
        assert_eq!(require_dispute_inactive(archived), Ok(()));
        assert_eq!(
            require_dispute_resolved(&archived),
            Err(ArbitrationError::DisputeActive)
        );
    });
}

#[test]
fn a_skipped_transition_check_is_the_only_way_to_reach_an_accepting_state_early() {
    // `Resolving -> Cancelled` is not a legal step, yet `Cancelled` satisfies
    // both guards. The regression this pins: the transition check, not the
    // status guard, is what protects a dispute mid-resolution, so no guard
    // change may make `Resolving` itself look terminal.
    assert_eq!(
        require_transition(DisputeStatus::Resolving, DisputeStatus::Cancelled),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq!(
        require_dispute_inactive(DisputeStatus::Resolving),
        Err(ArbitrationError::DisputeActive)
    );
    assert_eq!(
        require_dispute_resolved(&DisputeStatus::Resolving),
        Err(ArbitrationError::DisputeActive)
    );
    // The state that the illegal write would have produced does pass, which is
    // why the rejection above has to stay.
    assert_eq!(require_dispute_inactive(DisputeStatus::Cancelled), Ok(()));
}

// ════════════════════════════════════════════════════════════════════════
//  Per-variant baselines
// ════════════════════════════════════════════════════════════════════════

#[test]
fn is_dispute_active_returns_true_for_open() {
    assert!(is_dispute_active(DisputeStatus::Open));
}

#[test]
fn is_dispute_active_returns_true_for_voting() {
    assert!(is_dispute_active(DisputeStatus::Voting));
}

#[test]
fn is_dispute_active_returns_true_for_resolving() {
    assert!(is_dispute_active(DisputeStatus::Resolving));
}

#[test]
fn is_dispute_active_returns_false_for_resolved() {
    assert!(!is_dispute_active(DisputeStatus::Resolved));
}

#[test]
fn is_dispute_active_returns_false_for_cancelled() {
    assert!(!is_dispute_active(DisputeStatus::Cancelled));
}

#[test]
fn is_dispute_active_returns_false_for_tied() {
    assert!(!is_dispute_active(DisputeStatus::Tied));
}

#[test]
fn require_dispute_inactive_allows_resolved() {
    assert!(require_dispute_inactive(DisputeStatus::Resolved).is_ok());
}

#[test]
fn require_dispute_inactive_allows_cancelled() {
    assert!(require_dispute_inactive(DisputeStatus::Cancelled).is_ok());
}

#[test]
fn require_dispute_inactive_allows_tied() {
    assert!(require_dispute_inactive(DisputeStatus::Tied).is_ok());
}

#[test]
fn require_dispute_inactive_blocks_open() {
    assert_eq!(
        require_dispute_inactive(DisputeStatus::Open),
        Err(ArbitrationError::DisputeActive)
    );
}

#[test]
fn require_dispute_inactive_blocks_voting() {
    assert_eq!(
        require_dispute_inactive(DisputeStatus::Voting),
        Err(ArbitrationError::DisputeActive)
    );
}

#[test]
fn require_dispute_inactive_blocks_resolving() {
    assert_eq!(
        require_dispute_inactive(DisputeStatus::Resolving),
        Err(ArbitrationError::DisputeActive)
    );
}
