// Test modules are permitted to use format_args / assert macros with messages.
// The workspace clippy.toml bans these only in production contract code.
#![allow(clippy::disallowed_macros)]
//! Boundary and recovery test suite for `DisputeResolutionContract`.
//!
//! Coverage map
//! ─────────────────────────────────────────────────────────────────────────
//! § 1  create_dispute  – happy path, correct initial state
//! § 2  get_dispute     – found / not-found (DisputeNotFound boundary)
//! § 3  close           – success, state persisted, resolver preserved
//! § 4  close           – double-close (AlreadyClosed invariant)
//! § 5  close           – unauthorized caller (Unauthorized invariant)
//! § 6  close           – nonexistent ID (DisputeNotFound)
//! § 7  resolve         – Open → Resolved happy path, state persisted
//! § 8  resolve         – already Resolved re-resolve rejected (AlreadyClosed)
//! § 9  resolve         – already Closed re-resolve rejected (AlreadyClosed)
//! § 10 resolve         – unauthorized caller (Unauthorized)
//! § 11 resolve         – nonexistent ID (DisputeNotFound)
//! § 12 close after resolve – Resolved → Closed is allowed
//! § 13 multiple disputes   – independent records, no cross-contamination
//! § 14 auth boundary       – require_auth is wired for close
//! § 15 auth boundary       – require_auth is wired for resolve
//! § 16 error discriminants – serialised error codes match the spec
//! § 17 id boundary         – u64::MAX and u64::MIN IDs are valid storage keys
//! § 18 concurrent pattern  – resolver A cannot close resolver B's dispute
//! § 19 idempotency guard   – terminal state never reverts
//! § 20 resolver field      – resolver address is preserved through lifecycle
//! § 21 create event        – "created" event is emitted
//! § 22 close event         – "closed"  event is emitted
//! § 23 resolve event       – "resolved" event is emitted
//! § 24 get_dispute is read-only – repeated calls return identical state
//! § 25 full lifecycle      – Open → Resolved → Closed round-trip
//! § 26 failure recovery    – failed calls leave storage unchanged

#![cfg(test)]

use dispute_resolution::{DisputeError, DisputeResolutionContract, DisputeStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal,
};

// ─── helpers ────────────────────────────────────────────────────────────────

/// Creates a fresh env + registered contract client.
/// `mock_all_auths()` satisfies every `require_auth` call automatically so
/// tests can focus on business logic rather than auth scaffolding.
fn setup() -> (
    Env,
    dispute_resolution::DisputeResolutionContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(DisputeResolutionContract, ());
    let client = dispute_resolution::DisputeResolutionContractClient::new(&env, &contract_id);
    (env, client)
}

// ─── § 1  create_dispute happy path ─────────────────────────────────────────

#[test]
fn test_create_dispute_returns_id_with_open_status() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    // The PRNG will produce some u64; confirm the record is immediately
    // retrievable with the correct initial values.
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.id, id);
    assert_eq!(dispute.status, DisputeStatus::Open);
    assert_eq!(dispute.resolver, resolver);
}

// ─── § 2  get_dispute not-found boundary ────────────────────────────────────

#[test]
fn test_get_dispute_not_found_returns_error() {
    let (_, client) = setup();
    let err = client.try_get_dispute(&999_u64).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::DisputeNotFound);
}

#[test]
fn test_get_dispute_found_after_create() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
}

// ─── § 3  close – success ────────────────────────────────────────────────────

#[test]
fn test_close_transitions_open_to_closed() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
    // resolver field must be unchanged after close
    assert_eq!(dispute.resolver, resolver);
}

// ─── § 4  double-close → AlreadyClosed ──────────────────────────────────────

#[test]
fn test_double_close_returns_already_closed() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();
    let err = client.try_close(&resolver, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::AlreadyClosed);

    // State must remain Closed, not revert.
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
}

// ─── § 5  close – unauthorized caller → Unauthorized ────────────────────────

#[test]
fn test_close_by_non_resolver_returns_unauthorized() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let attacker = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    let err = client.try_close(&attacker, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);

    // Dispute must still be Open.
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
}

// ─── § 6  close – nonexistent ID → DisputeNotFound ──────────────────────────

#[test]
fn test_close_nonexistent_dispute_returns_not_found() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let err = client.try_close(&resolver, &42_u64).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::DisputeNotFound);
}

// ─── § 7  resolve – happy path ───────────────────────────────────────────────

#[test]
fn test_resolve_transitions_open_to_resolved() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);
    assert_eq!(dispute.resolver, resolver);
}

// ─── § 8  resolve – already Resolved → AlreadyClosed ────────────────────────

#[test]
fn test_resolve_already_resolved_returns_already_closed() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();
    let err = client.try_resolve(&resolver, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::AlreadyClosed);

    // Status must remain Resolved, not regress to Open.
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);
}

// ─── § 9  resolve – already Closed → AlreadyClosed ───────────────────────────

#[test]
fn test_resolve_closed_dispute_returns_already_closed() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();
    let err = client.try_resolve(&resolver, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::AlreadyClosed);

    // Status must remain Closed.
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
}

// ─── § 10 resolve – unauthorized caller → Unauthorized ───────────────────────

#[test]
fn test_resolve_by_non_resolver_returns_unauthorized() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let attacker = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    let err = client.try_resolve(&attacker, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);

    // State must be unchanged.
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
}

// ─── § 11 resolve – nonexistent ID → DisputeNotFound ────────────────────────

#[test]
fn test_resolve_nonexistent_dispute_returns_not_found() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let err = client
        .try_resolve(&resolver, &999_u64)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, DisputeError::DisputeNotFound);
}

// ─── § 12 Resolved → Closed is a valid transition ────────────────────────────

#[test]
fn test_close_resolved_dispute_succeeds() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();
    client.try_close(&resolver, &id).unwrap().unwrap();

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
}

// ─── § 13 multiple disputes – no cross-contamination ────────────────────────

#[test]
fn test_multiple_independent_disputes() {
    let (_, client) = setup();
    let resolver_a = Address::generate(&client.env);
    let resolver_b = Address::generate(&client.env);

    let id_a = client.create_dispute(&resolver_a);
    let id_b = client.create_dispute(&resolver_b);

    // Close only A.
    client.try_close(&resolver_a, &id_a).unwrap().unwrap();

    // B must still be Open.
    let dispute_b = client.try_get_dispute(&id_b).unwrap().unwrap();
    assert_eq!(dispute_b.status, DisputeStatus::Open);
    assert_eq!(dispute_b.resolver, resolver_b);

    // A must be Closed, resolver unchanged.
    let dispute_a = client.try_get_dispute(&id_a).unwrap().unwrap();
    assert_eq!(dispute_a.status, DisputeStatus::Closed);
    assert_eq!(dispute_a.resolver, resolver_a);
}

// ─── § 14 auth – require_auth is wired for close ─────────────────────────────

/// `mock_all_auths()` records every `require_auth` call.  After a successful
/// `close`, `env.auths()` must contain an entry for the resolver, confirming
/// that `caller.require_auth()` is actually invoked and not bypassed.
#[test]
fn test_close_records_require_auth_for_resolver() {
    let (env, client) = setup();
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| addr == &resolver),
        "require_auth was not recorded for resolver during close"
    );
}

// ─── § 15 auth – require_auth is wired for resolve ───────────────────────────

#[test]
fn test_resolve_records_require_auth_for_resolver() {
    let (env, client) = setup();
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| addr == &resolver),
        "require_auth was not recorded for resolver during resolve"
    );
}

// ─── § 16 error discriminants ────────────────────────────────────────────────

/// The integer discriminants are part of the on-chain XDR interface.
/// They must never be renumbered without a compatibility-breaking release.
#[test]
fn test_error_discriminants_match_spec() {
    assert_eq!(DisputeError::DisputeNotFound as u32, 1);
    assert_eq!(DisputeError::AlreadyClosed as u32, 2);
    assert_eq!(DisputeError::Unauthorized as u32, 3);
}

// ─── § 17 ID boundary – u64 extremes are valid storage keys ─────────────────

#[test]
fn test_boundary_ids_are_valid_storage_keys() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);

    // u64::MIN and u64::MAX must be valid (non-panicking) key arguments even
    // though no dispute has been stored under them.
    let err_min = client.try_get_dispute(&u64::MIN).unwrap_err().unwrap();
    assert_eq!(err_min, DisputeError::DisputeNotFound);

    let err_max = client.try_get_dispute(&u64::MAX).unwrap_err().unwrap();
    assert_eq!(err_max, DisputeError::DisputeNotFound);

    // prng().gen::<u64>() must not panic regardless of the value produced.
    let _id = client.create_dispute(&resolver);
}

// ─── § 18 concurrent pattern – resolver A cannot close resolver B's dispute ──

#[test]
fn test_resolver_a_cannot_close_resolver_b_dispute() {
    let (_, client) = setup();
    let resolver_a = Address::generate(&client.env);
    let resolver_b = Address::generate(&client.env);

    let id_b = client.create_dispute(&resolver_b);

    let err = client.try_close(&resolver_a, &id_b).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);

    // Dispute must remain Open.
    let dispute = client.try_get_dispute(&id_b).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
}

// ─── § 19 idempotency guard – terminal state never reverts ───────────────────

/// Once Closed, repeated failed attempts (double-close and attacker close)
/// must leave the record completely unchanged.
#[test]
fn test_terminal_state_never_reverts() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let attacker = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    // Both fail; AlreadyClosed fires before Unauthorized because the status
    // guard precedes the identity check in the implementation.
    let _ = client.try_close(&resolver, &id);
    let _ = client.try_close(&attacker, &id);

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
    assert_eq!(dispute.resolver, resolver);
}

// ─── § 20 resolver field preserved through lifecycle ─────────────────────────

#[test]
fn test_resolver_field_preserved_through_lifecycle() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    // Open
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().resolver,
        resolver
    );

    // Resolved
    client.try_resolve(&resolver, &id).unwrap().unwrap();
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().resolver,
        resolver
    );

    // Closed
    client.try_close(&resolver, &id).unwrap().unwrap();
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().resolver,
        resolver
    );
}

// ─── § 21 create event ───────────────────────────────────────────────────────

#[test]
fn test_create_dispute_emits_created_event() {
    let (env, client) = setup();
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&resolver);

    let events = env.events().all();
    let found = events.iter().any(|(_, topics, _data)| {
        let expected = vec![
            &env,
            symbol_short!("created").into_val(&env),
            id.into_val(&env),
        ];
        topics == expected
    });
    assert!(found, "expected a 'created' event for dispute id {id}");
}

// ─── § 22 close event ────────────────────────────────────────────────────────

#[test]
fn test_close_emits_closed_event() {
    let (env, client) = setup();
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    let events = env.events().all();
    let found = events.iter().any(|(_, topics, _data)| {
        let expected = vec![
            &env,
            symbol_short!("closed").into_val(&env),
            id.into_val(&env),
        ];
        topics == expected
    });
    assert!(found, "expected a 'closed' event for dispute id {id}");
}

// ─── § 23 resolve event ──────────────────────────────────────────────────────

#[test]
fn test_resolve_emits_resolved_event() {
    let (env, client) = setup();
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();

    let events = env.events().all();
    let found = events.iter().any(|(_, topics, _data)| {
        let expected = vec![
            &env,
            symbol_short!("resolved").into_val(&env),
            id.into_val(&env),
        ];
        topics == expected
    });
    assert!(found, "expected a 'resolved' event for dispute id {id}");
}

// ─── § 24 get_dispute is read-only ───────────────────────────────────────────

#[test]
fn test_get_dispute_does_not_mutate_state() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    let d1 = client.try_get_dispute(&id).unwrap().unwrap();
    let d2 = client.try_get_dispute(&id).unwrap().unwrap();

    assert_eq!(d1, d2);
    assert_eq!(d1.status, DisputeStatus::Open);
}

// ─── § 25 full lifecycle Open → Resolved → Closed ────────────────────────────

#[test]
fn test_full_lifecycle_open_resolved_closed() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    // Open
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Open
    );

    // Resolved
    client.try_resolve(&resolver, &id).unwrap().unwrap();
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Resolved
    );

    // Closed
    client.try_close(&resolver, &id).unwrap().unwrap();
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Closed
    );

    // No further transitions are possible from Closed.
    assert_eq!(
        client.try_close(&resolver, &id).unwrap_err().unwrap(),
        DisputeError::AlreadyClosed
    );
    assert_eq!(
        client.try_resolve(&resolver, &id).unwrap_err().unwrap(),
        DisputeError::AlreadyClosed
    );
}

// ─── § 26 failure recovery – failed calls leave storage clean ────────────────

/// A failed close or resolve (Unauthorized) must not leave any partial write
/// in storage.  The dispute record must be identical before and after.
#[test]
fn test_failed_calls_leave_state_unchanged() {
    let (_, client) = setup();
    let resolver = Address::generate(&client.env);
    let attacker = Address::generate(&client.env);
    let id = client.create_dispute(&resolver);

    let before = client.try_get_dispute(&id).unwrap().unwrap();

    let _ = client.try_close(&attacker, &id); // Unauthorized
    let _ = client.try_resolve(&attacker, &id); // Unauthorized

    let after = client.try_get_dispute(&id).unwrap().unwrap();

    assert_eq!(
        before, after,
        "storage must be unchanged after failed calls"
    );
}
