#![cfg(test)]

//! Lifecycle event and invalid-transition regression tests.
//!
//! Covers every valid transition and every invalid one to ensure the
//! status machine is exhaustive and cannot be bypassed.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String};
use status::{ArbitrationError, DisputeStatus};

// ── helpers ──────────────────────────────────────────────────────────────────

fn advance(e: &Env, secs: u64) {
    e.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: e.ledger().timestamp() + secs,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });
}

struct Setup<'a> {
    env: Env,
    admin: Address,
    arb: Address,
    creator: Address,
    client: CredenceArbitrationClient<'a>,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let arb = Address::generate(&env);
    let creator = Address::generate(&env);
    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);
    Setup {
        env,
        admin,
        arb,
        creator,
        client,
    }
}

fn open_dispute(s: &Setup) -> u64 {
    let desc = String::from_str(&s.env, "test dispute");
    s.client.create_dispute(&s.creator, &desc, &3600)
}

// ── valid transition tests ────────────────────────────────────────────────────

#[test]
fn test_status_is_voting_after_creation() {
    let s = setup();
    let id = open_dispute(&s);
    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, DisputeStatus::Voting);
}

#[test]
fn test_valid_transition_voting_to_resolved() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.vote(&s.arb, &id, &1);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, DisputeStatus::Resolved);
}

#[test]
fn test_valid_transition_voting_to_cancelled_by_creator() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.cancel_dispute(&s.creator, &id, &None);
    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, DisputeStatus::Cancelled);
}

#[test]
fn test_valid_transition_voting_to_cancelled_by_admin() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.cancel_dispute(&s.admin, &id, &None);
    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, DisputeStatus::Cancelled);
}

#[test]
fn test_resolve_with_no_votes_gives_outcome_zero() {
    let s = setup();
    let id = open_dispute(&s);
    advance(&s.env, 3601);
    let outcome = s.client.resolve_dispute(&id);
    assert_eq!(outcome, 0);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Resolved);
}

// ── invalid transition regression tests ──────────────────────────────────────

#[test]
fn test_invalid_resolve_while_voting_active() {
    let s = setup();
    let id = open_dispute(&s);
    // Voting period still active — cannot resolve yet
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::VotingNotEnded);
}

#[test]
fn test_invalid_resolve_already_resolved() {
    let s = setup();
    let id = open_dispute(&s);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    // Resolved → Resolving is not a valid transition
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_invalid_resolve_cancelled_dispute() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.cancel_dispute(&s.creator, &id, &None);
    // Cancelled → Resolving is not valid
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_invalid_cancel_already_resolved() {
    let s = setup();
    let id = open_dispute(&s);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    // Resolved → Cancelled is not valid
    let err = s
        .client
        .try_cancel_dispute(&s.creator, &id, &None)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_invalid_cancel_already_cancelled() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.cancel_dispute(&s.creator, &id, &None);
    // Cancelled → Cancelled is not valid
    let err = s
        .client
        .try_cancel_dispute(&s.creator, &id, &None)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_invalid_vote_on_cancelled_dispute() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.cancel_dispute(&s.creator, &id, &None);
    let err = s.client.try_vote(&s.arb, &id, &1).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::VotingInactive);
}

#[test]
fn test_invalid_vote_on_resolved_dispute() {
    let s = setup();
    let id = open_dispute(&s);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    let err = s.client.try_vote(&s.arb, &id, &1).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::VotingInactive);
}

#[test]
fn test_invalid_vote_after_voting_period_expired() {
    let s = setup();
    let id = open_dispute(&s);
    advance(&s.env, 3601); // past voting_end but not yet resolved
    let err = s.client.try_vote(&s.arb, &id, &1).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::VotingInactive);
}

#[test]
fn test_invalid_cancel_by_non_creator_non_admin() {
    let s = setup();
    let id = open_dispute(&s);
    let stranger = Address::generate(&s.env);
    let err = s
        .client
        .try_cancel_dispute(&stranger, &id, &None)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAuthorized);
}

#[test]
fn test_invalid_vote_outcome_zero() {
    let s = setup();
    let id = open_dispute(&s);
    let err = s.client.try_vote(&s.arb, &id, &0).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::InvalidOutcome);
}

#[test]
fn test_invalid_double_initialize() {
    let s = setup();
    let err = s.client.try_initialize(&s.admin).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::AlreadyInitialized);
}

#[test]
fn test_invalid_register_zero_weight() {
    let s = setup();
    let arb2 = Address::generate(&s.env);
    let err = s
        .client
        .try_register_arbitrator(&arb2, &0)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::WeightNotPositive);
}

#[test]
fn test_invalid_register_negative_weight() {
    let s = setup();
    let arb2 = Address::generate(&s.env);
    let err = s
        .client
        .try_register_arbitrator(&arb2, &-1)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::WeightNotPositive);
}

// ── quorum tests ──────────────────────────────────────────────────────────────

#[test]
fn test_resolve_fails_when_weight_quorum_not_met() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.vote(&s.arb, &id, &1); // weight = 10
    s.client.set_quorum(&s.admin, &100, &0); // need weight ≥ 100
    advance(&s.env, 3601);
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::QuorumNotMet);
    // Dispute stays Voting — caller can try again later
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Voting);
}

#[test]
fn test_resolve_fails_when_voter_quorum_not_met() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.vote(&s.arb, &id, &1); // 1 voter
    s.client.set_quorum(&s.admin, &0, &2); // need ≥ 2 voters
    advance(&s.env, 3601);
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::QuorumNotMet);
}

#[test]
fn test_resolve_succeeds_when_both_quorum_conditions_met() {
    let s = setup();
    let arb2 = Address::generate(&s.env);
    s.client.register_arbitrator(&arb2, &5);
    let id = open_dispute(&s);
    s.client.vote(&s.arb, &id, &1); // weight 10, 1 voter
    s.client.vote(&arb2, &id, &2); // weight 5,  2 voters
    s.client.set_quorum(&s.admin, &10, &2); // need weight ≥ 10 AND voters ≥ 2
    advance(&s.env, 3601);
    let outcome = s.client.resolve_dispute(&id);
    assert_eq!(outcome, 1); // outcome 1 has weight 10 > weight 5
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Resolved);
}

#[test]
fn test_resolve_with_weight_quorum_met_but_voter_quorum_not() {
    let s = setup();
    let id = open_dispute(&s);
    s.client.vote(&s.arb, &id, &1); // weight 10, 1 voter
    s.client.set_quorum(&s.admin, &10, &2); // weight OK (10≥10), voters NOT (1<2)
    advance(&s.env, 3601);
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::QuorumNotMet);
}

#[test]
fn test_resolve_with_voter_quorum_met_but_weight_quorum_not() {
    let s = setup();
    let arb2 = Address::generate(&s.env);
    s.client.register_arbitrator(&arb2, &5);
    let id = open_dispute(&s);
    s.client.vote(&s.arb, &id, &1); // weight 10, 1 voter
    s.client.vote(&arb2, &id, &2); // weight 5,  2 voters
    s.client.set_quorum(&s.admin, &20, &2); // weight NOT (15<20), voters OK (2≥2)
    advance(&s.env, 3601);
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::QuorumNotMet);
}

#[test]
fn test_resolve_legacy_quorum_unset_behaviour() {
    // Default (0, 0) — no quorum gate, preserves legacy behaviour
    let s = setup();
    let id = open_dispute(&s);
    // No quorum configured; resolve with no votes should still work
    advance(&s.env, 3601);
    let outcome = s.client.resolve_dispute(&id);
    assert_eq!(outcome, 0);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Resolved);
}

// ── require_transition unit tests (status module) ────────────────────────────

#[test]
fn test_status_machine_all_valid_transitions() {
    use status::require_transition;
    assert!(require_transition(DisputeStatus::Open, DisputeStatus::Voting).is_ok());
    assert!(require_transition(DisputeStatus::Open, DisputeStatus::Cancelled).is_ok());
    assert!(require_transition(DisputeStatus::Voting, DisputeStatus::Resolving).is_ok());
    assert!(require_transition(DisputeStatus::Voting, DisputeStatus::Cancelled).is_ok());
    assert!(require_transition(DisputeStatus::Resolving, DisputeStatus::Resolved).is_ok());
}

#[test]
fn test_status_machine_all_invalid_transitions() {
    use status::require_transition;
    let invalid = [
        (DisputeStatus::Open, DisputeStatus::Resolving),
        (DisputeStatus::Open, DisputeStatus::Resolved),
        (DisputeStatus::Voting, DisputeStatus::Open),
        (DisputeStatus::Voting, DisputeStatus::Resolved),
        (DisputeStatus::Resolving, DisputeStatus::Open),
        (DisputeStatus::Resolving, DisputeStatus::Voting),
        (DisputeStatus::Resolving, DisputeStatus::Cancelled),
        (DisputeStatus::Resolved, DisputeStatus::Open),
        (DisputeStatus::Resolved, DisputeStatus::Voting),
        (DisputeStatus::Resolved, DisputeStatus::Resolving),
        (DisputeStatus::Resolved, DisputeStatus::Cancelled),
        (DisputeStatus::Cancelled, DisputeStatus::Open),
        (DisputeStatus::Cancelled, DisputeStatus::Voting),
        (DisputeStatus::Cancelled, DisputeStatus::Resolving),
        (DisputeStatus::Cancelled, DisputeStatus::Resolved),
    ];
    for (from, to) in invalid {
        assert_eq!(
            require_transition(from, to),
            Err(ArbitrationError::InvalidTransition),
            "expected InvalidTransition for {:?} → {:?}",
            from,
            to
        );
    }
}

#[test]
fn test_cancel_with_reason_and_role() {
    let s = setup();
    let id = open_dispute(&s);
    let reason = Some(String::from_str(&s.env, "Dispute invalid"));
    s.client.cancel_dispute(&s.creator, &id, &reason);

    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, DisputeStatus::Cancelled);
    assert_eq!(d.cancellation_reason, reason);
    assert_eq!(
        d.cancelled_by_role,
        Some(soroban_sdk::Symbol::short("creator"))
    );
}

#[test]
fn test_cancel_admin_role() {
    let s = setup();
    let id = open_dispute(&s);
    let reason = Some(String::from_str(&s.env, "Admin override"));
    s.client.cancel_dispute(&s.admin, &id, &reason);

    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, DisputeStatus::Cancelled);
    assert_eq!(
        d.cancelled_by_role,
        Some(soroban_sdk::Symbol::short("admin"))
    );
}

#[test]
fn test_cancel_reason_too_long() {
    let s = setup();
    let id = open_dispute(&s);
    // Create a string of length 257
    let arr = [b'A'; 257];
    let long_str = core::str::from_utf8(&arr).unwrap();
    let reason = Some(soroban_sdk::String::from_str(&s.env, long_str));
    let err = s
        .client
        .try_cancel_dispute(&s.creator, &id, &reason)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::ReasonTooLong);
}
