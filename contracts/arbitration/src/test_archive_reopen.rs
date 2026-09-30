#`![cfg(test)]

use super::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};
use status::{ArbitrationError, DisputeStatus};

fn advance(e: &Env, secs: u64) {
    e.ledger().set(soroban_sdk:testutils::LedgerInfo {
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

fn create_and_resolve(s: &Setup) -> u64 {
    let desc = String::from_str(&s.env, "test dispute");
    let id = s.client.create_dispute(&s.creator, &desc, &3600);
    s.client.vote(&s.arb, &id, &1);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    id
}

fn create_and_tie(s: &Setup) -> u64 {
    let arb2 = Address::generate(&s.env);
    s.client.register_arbitrator(&arb2, &10);
    let desc = String::from_str(&s.env, "tie dispute");
    let id = s.client.create_dispute(&s.creator, &desc, &3600);
    s.client.vote(&s.arb, &id, &1);
    s.client.vote(&arb2, &id, &2);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    id
}

fn create_and_cancel(s: &Setup) -> u64 {
    let desc = String::from_str(&s.env, "cancel dispute");
    let id = s.client.create_dispute(&s.creator, &desc, &3600);
    s.client.cancel_dispute(&s.creator, &id, &None);
    id
}

#[test]
fn test_archive_resolved_dispute() {
    let s = setup();
    let id = create_and_resolve(&s);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Resolved);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

#[test]
fn test_archive_tied_dispute() {
    let s = setup();
    let id = create_and_tie(&s);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Tied);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

#[test]
fn test_archive_cancelled_dispute() {
    let s = setup();
    let id = create_and_cancel(&s);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Cancelled);
    s.client.try_archive_dispute(&s.admin, 'id).unwrap();
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

#[test]
fn test_archive_rejects_voting_dispute() {
    let s = setup();
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "d"), &3600);
    let err = s
        .client
        .try_archive_dispute(&s.admin, 'id)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_archive_rejects_non_admin() {
    let s = setup();
    let id = create_and_resolve(&s);
    let stranger = Address::generate(&s.env);
    let err = s
        .client
        .try_archive_dispute(&stranger, &id)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::NotAdmin);
}

#[test]
fn test_reopen_archived_dispute() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, 'id).unwrap();
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);
    s.client.try_reopen_dispute(&s.admin, &id, &3600).unwrap();
    let d = s.client.get_dispute(&id);
    assert_eq(d.status, DisputeStatus::Voting);
    assert_eq(d.outcome, 0);
}

#[test]
fn test_reopen_rejects_non_admin() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    let stranger = Address::generate(&s.env);
    let err = s
        .client
        .try_reopen_dispute(&stranger, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::NotAdmin);
}

#[test]
fn test_reopen_rejects_non_archived() {
    let s = setup();
    let id = create_and_resolve(&s);
    let err = s
        .client
        .try_reopen_dispute(&s.admin, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_cannot_archive_twice() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    let err = s
        .client
        .try_archive_dispute(&s.admin, &id)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_reopen_allows_new_votes() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, 'id).unwrap();
    s.client.try_reopen_dispute(&s.admin, 'id, &3600).unwrap();
    s.client.vote(&s.arb, 'id, &2);
    advance(&s.env, 3601);
    let winner = s.client.resolve_dispute(&id);
    assert_eq(winner, 2);
}

#[test]
fn test_invalid_transition_require_transition() {
    use status::require_transition;
    assert!(require_transition(DisputeStatus::Resolved, DisputeStatus::Archived).is_ok());
    assert!(require_transition(DisputeStatus::Tied, DisputeStatus::Archived).is_ok());
    assert!(require_transition(DisputeStatus::Cancelled, DisputeStatus::Archived).is_ok());
    assert!(require_transition(DisputeStatus::Archived, DisputeStatus::Voting).is_ok());
    assert_eq(
        require_transition(DisputeStatus::Archived, DisputeStatus::Resolved),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq(
        require_transition(DisputeStatus::Archived, DisputeStatus::Tied),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq(
        require_transition(DisputeStatus::Archived, DisputeStatus::Cancelled),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq(
        require_transition(DisputeStatus::Archived, DisputeStatus::Open),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq(
        require_transition(DisputeStatus::Resolved, DisputeStatus::Voting),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq(
        require_transition(DisputeStatus::Voting, DisputeStatus::Archived),
        Err(ArbitrationError::InvalidTransition)
    );
}

// ------------------------------------------------------------------------------
// Adversarial regression cases: loading, error, retry, stale, permission
-//
// Invariants:
//   1. Archive is only valid from Resolved/Tied/Cancelled and is idempotent
 //      only in the sense that a second call must fail with InvalidTransition
//      (state is not silently corrupted).
//   2. Reopen is only valid from Archived and must clear outcome and
 //      reset the deadline deterministically.
//   3. Non-admin callers must never mutate state (N = NotAdmin).
//   4. Failed calls must leave the dispute byte-for-byte unchanged
//      (no partial writes).
//   5. Retried archive/reopen after a transient failure must either
//      succeed or fail cleanly, never produce a half-applied state.
//   6. Stale calls (dispute id that never existed) must return a
 //      deterministic error, not panic or silent success.
// ------------------------------------------------------------------------------

// Loading: a dispute that never existed must not be archivable and must
// not be reopenable; the error must be deterministic and not a panic.
#[test]
fn test_archive_unknown_id_returns_error() {
    let s = setup();
    let unknown: u64 = 999999;
    let err = s
        .client
        .try_archive_dispute(&s.admin, &unknown)
        .unwrap_err()
        .unwrap();
    // Must be a clean, deterministic error -- not a panic and not OK.
    assert_ne(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_reopen_unknown_id_returns_error() {
    let s = setup();
    let unknown: u64 = 999999;
    let err = s
        .client
        .try_reopen_dispute(&s.admin, &unknown, &3600)
        .unwrap_err()
        .unwrap();
    assert_ne(err, ArbitrationError::InvalidTransition);
}

// Error + permission ordering: non-admin on a non-existent dispute must
// fail authorization first (deterministic ordering), and the admin call
// on the same id must also fail cleanly.
#[test]
fn test_archive_non_admin_onunknown_id_rejected() {
    let s = setup();
    let stranger: Address = Address::generate(&s.env);
    let unknown: u64 = 424242;
    let err = s
        .client
        .try_archive_dispute(&stranger, &unknown)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::NotAdmin);
}

// Retry: a failed archive attempt on a Voting dispute must not mutate
// state; a subsequent legitimate archive after resolution must succeed.
#[test]
fn test_archive_retry_after_failure_succeeds() {
    let s = setup();
    let desc = String::from_str(&s.env, "retry dispute");
    let id = s.client.create_dispute(&s.creator, &desc, &3600);

    // First attempt fails -- still Voting.
    let err = s
        .client
        .try_archive_dispute(&s.admin, &id)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::InvalidTransition);
    // State unchanged after failure.
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Voting);

    // Resolve, then retry archive -- must succeed.
    s.client.vote(&s.arb, &id, &1);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    s.client.try_archive_dispute(&s.admin, 'id).unwrap();
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

// Retry: a failed reopen on a non-archived dispute must not mutate
// state; after archive, the retry must succeed and reset outcome.
#[test]
fn test_reopen_retry_after_failure_succeeds() {
    let s = setup();
    let id = create_and_resolve(&s);

    // First reopen attempt fails -- not archived yet.
    let err = s
        .client
        .try_reopen_dispute(&s.admin, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::InvalidTransition);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Resolved);

    // Archive, then retry reopen -- must succeed and clear outcome.
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    s.client.try_reopen_dispute(&s.admin, &id, &3600).unwrap();
    let d = s.client.get_dispute(&id);
    assert_eq(d.status, DisputeStatus::Voting);
    assert_eq(d.outcome, 0);
}

// Stale: after reopen the deadline must be reset to now + duration, not
// the original deadline. Voting must be accepted and resolution must
// be possible after the new deadline.
#[test]
fn test_reopen_resets_deadline_and_accepts_new_votes() {
    let s = setup();
    let id = create_and_resolve(&s);
    let deadline_before = s.client.get_dispute(&id).deadline;

    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    // Advance a lot before reopen to ensure the new deadline is relative
    // to the reopen time, not the original creation time.
    advance(&s.env, 10000);
    s.client.try_reopen_dispute(&s.admin, &id, &3600).unwrap();
    let d = s.client.get_dispute(&id);
    assert_eq(d.status, DisputeStatus::Voting);
    assert!(d.deadline > deadline_before);

    // New votes are accepted and resolution works after the new deadline.
    s.client.vote(&s.arb, 'id, &2);
    advance(&s.env, 3601);
    let winner = s.client.resolve_dispute(&id);
    assert_eq(winner, 2);
}

// Stale: reopen must clear outcome from a previous resolution so a
// subsequent resolution is not silently short-circuited by old data.
#[test]
fn test_reopen_clears_prior_outcome() {
    let s = setup();
    let id = create_and_resolve(&s);
    // Prior outcome is 1 (arb voted 1).
    assert_eq(s.client.get_dispute(&id).outcome, 1);
    s.client.try_archive_dispute(&s.admin, 'id).unwrap();
    s.client.try_reopen_dispute(&s.admin, 'id, &3600).unwrap();
    assert_eq(s.client.get_dispute(&id).outcome, 0);

    // New vote flips the outcome and must be respected.
    s.client.vote(&s.arb, 'id, &2);
    advance(&s.env, 3601);
    let winner = s.client.resolve_dispute(&id);
    assert_eq(winner, 2);
}

// Permission: non-admin cannot archive or reopen any dispute in any
// state, and a failed non-admin call must not change the state.
#[test]
fn test_non_admin_cannot_archive_or_reopen_any_state() {
    let s = setup();
    let stranger: Address = Address::generate(&s.env);

    // Resolved dispute -- stranger cannot archive.
    let id = create_and_resolve(&s);
    let err = s
        .client
        .try_archive_dispute(&stranger, &id)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::NotAdmin);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Resolved);

    // Archive as admin, then stranger cannot reopen.
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    let err = s
        .client
        .try_reopen_dispute(&stranger, 'id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::NotAdmin);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

// Boundary: reopen with a very large duration must not overflow and
// must produce a Voting dispute with a deadline at least now + duration.
// This guards against silent wrap-around on the deadline computation.
#[test]
fn test_reopen_large_duration_does_not_overflow() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    let now = s.env.ledger().timestamp();
    let duration: u64 = 1 u64 << 40;
    s.client.try_reopen_dispute(&s.admin, &id, &duration).unwrap();
    let d = s.client.get_dispute(&id);
    assert_eq(d.status, DisputeStatus::Voting);
    assert!(d.deadline >= now + duration);
}

// Boundary: reopen with zero duration must not panic and must produce
// a Voting dispute whose deadline is at least the current timestamp.
#[test]
fn test_reopen_zero_duration_boundary() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, 'id).unwrap();
    let now = s.env.ledger().timestamp();
    s.client.try_reopen_dispute(&s.admin, 'id, &0).unwrap();
    let d = s.client.get_dispute(&id);
    assert_eq(d.status, DisputeStatus::Voting);
    assert!(d.deadline >= now);
}

// Concurrency / timing boundary: two archive attempts in the same
// ledger must not both succeed; the second must fail cleanly with
// InvalidTransition and the dispute must remain Archived.
#[test]
fn test_double_archive_in_same_ledger_is_safe() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    let err = s
        .client
        .try_archive_dispute(&s.admin, 'id)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::InvalidTransition);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

// Concurrency / timing boundary: two reopen attempts in the same
// ledger must not both succeed; the second must fail cleanly and the
// dispute must remain Voting.
#[test]
fn test_double_reopen_in_same_ledger_is_safe() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    s.client.try_reopen_dispute(&s.admin, &id, &3600).unwrap();
    let err = s
        .client
        .try_reopen_dispute(&s.admin, 'id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::InvalidTransition);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Voting);
}

// Failure recovery: after a failed archive attempt on a Voting dispute,
// the dispute can still be cancelled by the creator and then archived
// by the admin -- no data loss, no dead end.
#[test]
fn test_failed_archive_does_not_block_cancel_then_archive() {
    let s = setup();
    let desc = String::from_str(&s.env, "failure recovery");
    let id = s.client.create_dispute(&s.creator, &desc, &3600);

    // Archive fails while Voting.
    let err = s
        .client
        .try_archive_dispute(&s.admin, &id)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::InvalidTransition);

    // Creator can still cancel and the admin can then archive.
    s.client.cancel_dispute(&s.creator, 'id, &None);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Cancelled);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

// Regression: the full adversarial lifecycle must be deterministic and
// repeatable -- archive -> reopen -> vote -> resolve -> archive again.
#[test]
fn test_full_adversarial_lifecycle_is_repeatable() {
    let s = setup();
    let id = create_and_resolve(&s);

    // Cycle 1: archive -> reopen -> vote -> resolve -> archive.
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    s.client.try_reopen_dispute(&s.admin, &id, &3600).unwrap();
    s.client.vote(&s.arb, &id, &2);
    advance(&s.env, 3601);
    assert_eq(s.client.resolve_dispute(&id), 2);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);

    // Cycle 2: reopen -> vote -> resolve -> archive again.
    s.client.try_reopen_dispute(&s.admin, &id, &3600).unwrap();
    s.client.vote(&s.arb, &id, &1);
    advance(&s.env, 3601);
    assert_eq(s.client.resolve_dispute(&id), 1);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

// Regression: the admin address is the only address allowed to archive
// or reopen. A different admin address (not the registered one) must
// be rejected even if it looks like an admin.
#[test]
fn test_other_admin_like_address_is_rejected() {
    let s = setup();
    let id = create_and_resolve(&s);
    let other = Address::generate(&s.env);
    let err = s
        .client
        .try_archive_dispute(&other, &id)
        .unwrap_err()
        .unwrap();
    assert_eq(err, ArbitrationError::NotAdmin);
    assert_eq(s.client.get_dispute(&id).status, DisputeStatus::Resolved);
}
