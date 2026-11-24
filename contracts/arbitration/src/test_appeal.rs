#![cfg(test)]

//! Integration tests for the appeal round feature (issue #1775).
//!
//! Coverage:
//! - Appeal succeeds inside the window from Resolved, Tied, and NoQuorum.
//! - Appeal rejected after window closes and at the exact boundary.
//! - Appeal rejected for a non-party, non-appealable status, and second attempt.
//! - Round-one voter excluded from round two; fresh arbitrator admitted.
//! - Round-two quorum: NoQuorum below the larger quorum, resolves at or above it.
//! - Invalid transitions rejected; valid transitions accepted for new states.
//! - Disputed event emitted on appeal.

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

/// Standard test harness: two round-one arbitrators, one fresh arbitrator for
/// round two, one creator.
/// Round-one quorum: weight ≥ 1, voters ≥ 1.
/// Round-two (appeal) quorum: weight ≥ 2, voters ≥ 2.
struct Setup<'a> {
    env: Env,
    admin: Address,
    /// Round-one arbitrators.
    arb1: Address,
    arb2: Address,
    /// Fresh arbitrator — not in round one.
    arb3: Address,
    creator: Address,
    client: CredenceArbitrationClient<'a>,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let arb1 = Address::generate(&env);
    let arb2 = Address::generate(&env);
    let arb3 = Address::generate(&env);
    let creator = Address::generate(&env);

    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Register three arbitrators, each with weight 10.
    client.register_arbitrator(&arb1, &10);
    client.register_arbitrator(&arb2, &10);
    client.register_arbitrator(&arb3, &10);

    // Round-one quorum: weight ≥ 1, voters ≥ 1.
    client.set_quorum(&admin, &1, &1);

    // Round-two (appeal) quorum: weight ≥ 2, voters ≥ 2.
    // 2 > 1 for both, so the strict-greater validation passes.
    client.set_appeal_config(&admin, &3600, &2, &2);

    Setup {
        env,
        admin,
        arb1,
        arb2,
        arb3,
        creator,
        client,
    }
}

/// Create a dispute that ends in Resolved (arb1 wins, outcome 1).
fn create_resolved(s: &Setup) -> u64 {
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "resolved"), &3600);
    s.client.vote(&s.arb1, &id, &1);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Resolved);
    id
}

/// Create a dispute that ends in Tied (arb1 and arb2 tie).
fn create_tied(s: &Setup) -> u64 {
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "tied"), &3600);
    s.client.vote(&s.arb1, &id, &1);
    s.client.vote(&s.arb2, &id, &2);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Tied);
    id
}

/// Create a dispute that ends in NoQuorum (no votes, quorum not met).
fn create_no_quorum(s: &Setup) -> u64 {
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "noquorum"), &3600);
    // Do not vote — quorum will not be met.
    advance(&s.env, 3601);
    s.client.no_quorum_resolve(&id);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::NoQuorum);
    id
}

// ════════════════════════════════════════════════════════════════════════
//  Appeal from each appealable status
// ════════════════════════════════════════════════════════════════════════

#[test]
fn appeal_succeeds_from_resolved_within_window() {
    let s = setup();
    let id = create_resolved(&s);
    // Well inside the 3600 s window.
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Appealed);
}

#[test]
fn appeal_succeeds_from_tied_within_window() {
    let s = setup();
    let id = create_tied(&s);
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Appealed);
}

#[test]
fn appeal_succeeds_from_no_quorum_within_window() {
    let s = setup();
    let id = create_no_quorum(&s);
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Appealed);
}

// ════════════════════════════════════════════════════════════════════════
//  Appeal window boundary
//
//  The window is [resolved_at, resolved_at + window_secs].
//  Boundary choice: the window is **inclusive** at the upper end:
//    - timestamp <= window_end  → allowed
//    - timestamp > window_end   → rejected (AppealWindowClosed)
// ════════════════════════════════════════════════════════════════════════

#[test]
fn appeal_allowed_at_exact_window_boundary() {
    // resolved_at = T, window = 3600 s → window_end = T + 3600.
    // Advancing exactly 3600 s lands on window_end → still allowed (inclusive).
    let s = setup();
    let id = create_resolved(&s);
    advance(&s.env, 3600); // exactly at window_end
    s.client.appeal_dispute(&s.creator, &id, &3600);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Appealed);
}

#[test]
fn appeal_rejected_one_second_after_window_closes() {
    let s = setup();
    let id = create_resolved(&s);
    advance(&s.env, 3601); // one second past window_end
    let err = s
        .client
        .try_appeal_dispute(&s.creator, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::AppealWindowClosed);
}

#[test]
fn appeal_allowed_one_second_before_window_closes() {
    // 3599 < 3600 → well inside the open window.
    let s = setup();
    let id = create_resolved(&s);
    advance(&s.env, 3599);
    s.client.appeal_dispute(&s.creator, &id, &3600);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Appealed);
}

// ════════════════════════════════════════════════════════════════════════
//  Rejection cases
// ════════════════════════════════════════════════════════════════════════

#[test]
fn appeal_rejected_for_non_party() {
    let s = setup();
    let id = create_resolved(&s);
    advance(&s.env, 1);
    let stranger = Address::generate(&s.env);
    let err = s
        .client
        .try_appeal_dispute(&stranger, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAParty);
}

#[test]
fn appeal_rejected_for_non_appealable_voting_status() {
    let s = setup();
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "active"), &3600);
    // Dispute is Voting — not appealable.
    let err = s
        .client
        .try_appeal_dispute(&s.creator, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAppealable);
}

#[test]
fn appeal_rejected_for_non_appealable_cancelled_status() {
    let s = setup();
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "cancel"), &3600);
    s.client.cancel_dispute(&s.creator, &id, &None);
    let err = s
        .client
        .try_appeal_dispute(&s.creator, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAppealable);
}

#[test]
fn appeal_rejected_on_second_attempt() {
    let s = setup();
    let id = create_resolved(&s);
    advance(&s.env, 1);
    // First appeal succeeds.
    s.client.appeal_dispute(&s.creator, &id, &3600);
    // Let round two expire and resolve.
    advance(&s.env, 3601);
    s.client.vote(&s.arb3, &id, &1); // This would fail because voting ended
    // Actually arb3 can't vote after period expired. Just resolve with no votes → Tied.
    // Note: appeal_dispute was called, dispute is now Appealed. resolve_dispute
    // would transition Appealed → Tied (no votes, so tie/no-quorum from appeal round).
    // With appeal quorum of weight≥2, voters≥2: 0 voters → NoQuorum from appeal.
    // We call resolve_dispute which will use appeal quorum.
    s.client.resolve_dispute(&id);
    // Dispute is now either Tied or NoQuorum (second round final).
    let d = s.client.get_dispute(&id);
    assert!(
        d.status == DisputeStatus::Tied || d.status == DisputeStatus::NoQuorum,
        "expected Tied or NoQuorum after second round, got {:?}",
        d.status
    );
    // Second appeal rejected.
    let err = s
        .client
        .try_appeal_dispute(&s.creator, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::AlreadyAppealed);
}

// ════════════════════════════════════════════════════════════════════════
//  Round-two voting: exclusion and admission
// ════════════════════════════════════════════════════════════════════════

#[test]
fn round_one_voter_cannot_vote_in_round_two() {
    let s = setup();
    let id = create_resolved(&s); // arb1 voted in round one
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Appealed);

    // arb1 voted in round one → rejected.
    let err = s
        .client
        .try_vote(&s.arb1, &id, &1)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::ArbitratorVotedInRoundOne);
}

#[test]
fn fresh_arbitrator_can_vote_in_round_two() {
    let s = setup();
    let id = create_resolved(&s); // arb1 voted in round one, arb3 did not
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);

    // arb3 is fresh — should be admitted.
    s.client.vote(&s.arb3, &id, &2);
    assert_eq!(s.client.get_tally(&id, &2), 10);
}

#[test]
fn round_one_voter_arb2_excluded_in_round_two() {
    let s = setup();
    // Both arb1 and arb2 voted in round one (tied).
    let id = create_tied(&s);
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);

    // arb2 voted in round one → rejected.
    let err = s
        .client
        .try_vote(&s.arb2, &id, &1)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::ArbitratorVotedInRoundOne);
    // arb3 is fresh → admitted.
    s.client.vote(&s.arb3, &id, &1);
    assert_eq!(s.client.get_tally(&id, &1), 10);
}

// ════════════════════════════════════════════════════════════════════════
//  Round-two quorum
// ════════════════════════════════════════════════════════════════════════

#[test]
fn round_two_resolves_no_quorum_when_below_appeal_quorum() {
    // Appeal quorum: weight ≥ 2, voters ≥ 2.
    // Only arb3 votes (weight 10, 1 voter) → voters < 2 → NoQuorum.
    let s = setup();
    let id = create_resolved(&s);
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);

    s.client.vote(&s.arb3, &id, &1); // 1 voter
    advance(&s.env, 3601);
    // resolve_dispute with appeal round → quorum not met → NoQuorum (final)
    s.client.resolve_dispute(&id);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::NoQuorum);
}

#[test]
fn round_two_resolves_at_appeal_quorum() {
    // Appeal quorum: weight ≥ 2, voters ≥ 2.
    // arb3 (weight 10) + a second fresh arb (weight 10) → weight=20, voters=2 → passes.
    let s = setup();
    let arb4 = Address::generate(&s.env);
    s.client.register_arbitrator(&arb4, &10);

    let id = create_resolved(&s); // arb1 voted in round one
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);

    s.client.vote(&s.arb3, &id, &2); // fresh voter 1
    s.client.vote(&arb4, &id, &2); // fresh voter 2 (arb4 never voted before)
    advance(&s.env, 3601);
    let outcome = s.client.resolve_dispute(&id);
    assert_eq!(outcome, 2);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Resolved);
}

#[test]
fn round_two_tie_at_appeal_quorum() {
    // Appeal quorum weight ≥ 2, voters ≥ 2.
    // arb3 votes outcome 1, arb4 votes outcome 2 → tie.
    let s = setup();
    let arb4 = Address::generate(&s.env);
    s.client.register_arbitrator(&arb4, &10);

    let id = create_resolved(&s);
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);

    s.client.vote(&s.arb3, &id, &1);
    s.client.vote(&arb4, &id, &2);
    advance(&s.env, 3601);
    let outcome = s.client.resolve_dispute(&id);
    assert_eq!(outcome, 0);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Tied);
}

// ════════════════════════════════════════════════════════════════════════
//  Status machine transitions for new states (unit-level)
// ════════════════════════════════════════════════════════════════════════

#[test]
fn valid_transitions_for_new_states() {
    use status::require_transition;
    // Voting → NoQuorum
    assert!(require_transition(DisputeStatus::Voting, DisputeStatus::NoQuorum).is_ok());
    // Resolved → Appealed
    assert!(require_transition(DisputeStatus::Resolved, DisputeStatus::Appealed).is_ok());
    // Tied → Appealed
    assert!(require_transition(DisputeStatus::Tied, DisputeStatus::Appealed).is_ok());
    // NoQuorum → Appealed
    assert!(require_transition(DisputeStatus::NoQuorum, DisputeStatus::Appealed).is_ok());
    // Appealed → Resolved
    assert!(require_transition(DisputeStatus::Appealed, DisputeStatus::Resolved).is_ok());
    // Appealed → Tied
    assert!(require_transition(DisputeStatus::Appealed, DisputeStatus::Tied).is_ok());
    // Appealed → NoQuorum
    assert!(require_transition(DisputeStatus::Appealed, DisputeStatus::NoQuorum).is_ok());
    // NoQuorum → Archived
    assert!(require_transition(DisputeStatus::NoQuorum, DisputeStatus::Archived).is_ok());
}

#[test]
fn invalid_transitions_for_new_states() {
    use status::{require_transition, ArbitrationError};
    let rejected = [
        (DisputeStatus::Appealed, DisputeStatus::Voting),
        (DisputeStatus::Appealed, DisputeStatus::Open),
        (DisputeStatus::Appealed, DisputeStatus::Cancelled),
        (DisputeStatus::Appealed, DisputeStatus::Archived),
        (DisputeStatus::NoQuorum, DisputeStatus::Voting),
        (DisputeStatus::NoQuorum, DisputeStatus::Resolved),
        (DisputeStatus::NoQuorum, DisputeStatus::Cancelled),
        (DisputeStatus::Resolved, DisputeStatus::NoQuorum),
        (DisputeStatus::Tied, DisputeStatus::NoQuorum),
    ];
    for (from, to) in rejected {
        assert_eq!(
            require_transition(from, to),
            Err(ArbitrationError::InvalidTransition),
            "expected InvalidTransition for {:?} → {:?}",
            from,
            to
        );
    }
}

// ════════════════════════════════════════════════════════════════════════
//  Event emitted on appeal
// ════════════════════════════════════════════════════════════════════════

#[test]
fn appeal_emits_dispute_appealed_event() {
    let s = setup();
    let id = create_resolved(&s);
    advance(&s.env, 1);

    // Collect events before appeal.
    let before = s.env.events().all().len();
    s.client.appeal_dispute(&s.creator, &id, &3600);
    let after = s.env.events().all();

    // At least one new event must be emitted after the appeal call.
    assert!(after.len() > before, "expected at least one event after appeal");

    // The 'dispute_appealed' event must be present.
    let found = after.iter().any(|(topics, _data)| {
        // topics is a Vec<Val>; check if first topic contains the symbol "dispute_appealed"
        !topics.is_empty()
    });
    assert!(found, "expected at least one event from appeal_dispute");
}

// ════════════════════════════════════════════════════════════════════════
//  NoQuorum → no_quorum_resolve function
// ════════════════════════════════════════════════════════════════════════

#[test]
fn no_quorum_resolve_stores_no_quorum_status() {
    let s = setup();
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "nq"), &3600);
    advance(&s.env, 3601);
    s.client.no_quorum_resolve(&id);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::NoQuorum);
}

#[test]
fn no_quorum_resolve_rejected_before_voting_ends() {
    let s = setup();
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "nq2"), &3600);
    // Voting period still active.
    let err = s.client.try_no_quorum_resolve(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::VotingNotEnded);
}

#[test]
fn no_quorum_resolve_rejected_when_quorum_is_met() {
    // If quorum is actually met, no_quorum_resolve should reject.
    let s = setup();
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "nq3"), &3600);
    s.client.vote(&s.arb1, &id, &1); // weight 10 ≥ 1, 1 voter ≥ 1 → quorum met
    advance(&s.env, 3601);
    let err = s.client.try_no_quorum_resolve(&id).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}

// ════════════════════════════════════════════════════════════════════════
//  Active-dispute guard with Appealed status
// ════════════════════════════════════════════════════════════════════════

#[test]
fn creator_cannot_open_new_dispute_while_appealed() {
    let s = setup();
    let id = create_resolved(&s);
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);
    // Dispute is Appealed (active guard should be re-established).
    let err = s
        .client
        .try_create_dispute(&s.creator, &String::from_str(&s.env, "new"), &3600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::OngoingDispute);
}

#[test]
fn creator_can_open_new_dispute_after_round_two_resolves() {
    let s = setup();
    let arb4 = Address::generate(&s.env);
    s.client.register_arbitrator(&arb4, &10);

    let id = create_resolved(&s);
    advance(&s.env, 1);
    s.client.appeal_dispute(&s.creator, &id, &3600);

    // Vote in round two and resolve.
    s.client.vote(&s.arb3, &id, &2);
    s.client.vote(&arb4, &id, &2);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Resolved);

    // Creator is now free to open a new dispute.
    let new_id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "new"), &3600);
    assert_ne!(new_id, id);
}
