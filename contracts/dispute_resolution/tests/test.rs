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

use credence_arbitration::{CredenceArbitration, CredenceArbitrationClient};
use dispute_resolution::{DisputeError, DisputeResolutionContract, DisputeStatus};
use interfaces::arbitration::{ArbitrationClient, ArbitrationStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{storage::Instance, storage::Persistent, Address as _, Events, Ledger},
    vec, Address, Env, IntoVal, String,
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
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

    // IDs start at 0 and increment sequentially (counter-based, not PRNG).
    assert_eq!(id, 0);
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.id, id);
    assert_eq!(dispute.creator, creator);
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
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
    // Verify creator is set correctly
    assert_eq!(dispute.creator, creator);
}

// ─── § 3  close – success ────────────────────────────────────────────────────

#[test]
fn test_close_transitions_open_to_closed() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
    // resolver and creator fields must be unchanged after close
    assert_eq!(dispute.resolver, resolver);
    assert_eq!(dispute.creator, creator);
}

// ─── § 4  double-close → AlreadyClosed ──────────────────────────────────────

#[test]
fn test_double_close_returns_already_closed() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

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
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let attacker = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

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
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);
    assert_eq!(dispute.resolver, resolver);
    assert_eq!(dispute.creator, creator);
}

// ─── § 8  resolve – already Resolved → AlreadyClosed ────────────────────────

#[test]
fn test_resolve_already_resolved_returns_already_closed() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

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
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

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
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let attacker = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

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
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();
    client.try_close(&resolver, &id).unwrap().unwrap();

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
}

// ─── § 13 multiple disputes – no cross-contamination ────────────────────────

#[test]
fn test_multiple_independent_disputes() {
    let (_, client) = setup();
    let creator_a = Address::generate(&client.env);
    let resolver_a = Address::generate(&client.env);
    let creator_b = Address::generate(&client.env);
    let resolver_b = Address::generate(&client.env);

    let id_a = client.create_dispute(&creator_a, &resolver_a);
    let id_b = client.create_dispute(&creator_b, &resolver_b);

    // Close only A.
    client.try_close(&resolver_a, &id_a).unwrap().unwrap();

    // B must still be Open.
    let dispute_b = client.try_get_dispute(&id_b).unwrap().unwrap();
    assert_eq!(dispute_b.status, DisputeStatus::Open);
    assert_eq!(dispute_b.resolver, resolver_b);
    assert_eq!(dispute_b.creator, creator_b);

    // A must be Closed, resolver unchanged.
    let dispute_a = client.try_get_dispute(&id_a).unwrap().unwrap();
    assert_eq!(dispute_a.status, DisputeStatus::Closed);
    assert_eq!(dispute_a.resolver, resolver_a);
    assert_eq!(dispute_a.creator, creator_a);
}

// ─── § 14 auth – require_auth is wired for close ─────────────────────────────

/// `mock_all_auths()` records every `require_auth` call.  After a successful
/// `close`, `env.auths()` must contain an entry for the resolver, confirming
/// that `caller.require_auth()` is actually invoked and not bypassed.
#[test]
fn test_close_records_require_auth_for_resolver() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

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
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

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
    assert_eq!(DisputeError::Paused as u32, 4);
}

// ─── § 17 ID boundary – sequential IDs and creator tracking ─────────────────

#[test]
fn test_ids_are_sequential() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);

    let id_0 = client.create_dispute(&creator, &resolver);
    let id_1 = client.create_dispute(&creator, &resolver);
    let id_2 = client.create_dispute(&creator, &resolver);

    assert_eq!(id_0, 0);
    assert_eq!(id_1, 1);
    assert_eq!(id_2, 2);

    let dispute_1 = client.try_get_dispute(&id_1).unwrap().unwrap();
    assert_eq!(dispute_1.creator, creator);
}

// ─── § 18 unauthorized on closed dispute ─────────────────────────────────────

/// After the resolver closes a dispute, a non-resolver attempting to close
/// must get Unauthorized (not AlreadyClosed). The resolver check now comes
/// before the status check.
#[test]
fn test_non_resolver_gets_unauthorized_on_closed() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let stranger = Address::generate(&client.env);

    let id = client.create_dispute(&creator, &resolver);

    // Resolver closes successfully.
    client.try_close(&resolver, &id).unwrap().unwrap();

    // Non-resolver attempting close on a closed dispute gets Unauthorized.
    let err = client.try_close(&stranger, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);
}

// ─── § 19 concurrent pattern – resolver A cannot close resolver B's dispute ──

#[test]
fn test_resolver_a_cannot_close_resolver_b_dispute() {
    let (_, client) = setup();
    let _creator_a = Address::generate(&client.env);
    let resolver_a = Address::generate(&client.env);
    let creator_b = Address::generate(&client.env);
    let resolver_b = Address::generate(&client.env);

    let id_b = client.create_dispute(&creator_b, &resolver_b);

    let err = client.try_close(&resolver_a, &id_b).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);

    // Dispute must remain Open.
    let dispute = client.try_get_dispute(&id_b).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
}

// ─── § 20 idempotency guard – terminal state never reverts ───────────────────

/// Once Closed, repeated failed attempts (double-close and attacker close)
/// must leave the record completely unchanged.
#[test]
fn test_terminal_state_never_reverts() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let attacker = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    // Unauthorized fires before AlreadyClosed because the resolver check
    // now precedes the status check.
    let _ = client.try_close(&attacker, &id);
    let _ = client.try_close(&resolver, &id);

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
    assert_eq!(dispute.resolver, resolver);
    assert_eq!(dispute.creator, creator);
}

// ─── § 21 resolver field preserved through lifecycle ─────────────────────────

#[test]
fn test_resolver_field_preserved_through_lifecycle() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

    // Open
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.resolver, resolver);
    assert_eq!(dispute.creator, creator);

    // Resolved
    client.try_resolve(&resolver, &id).unwrap().unwrap();
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.resolver, resolver);
    assert_eq!(dispute.creator, creator);

    // Closed
    client.try_close(&resolver, &id).unwrap().unwrap();
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.resolver, resolver);
    assert_eq!(dispute.creator, creator);
}

// ─── § 22 create event ───────────────────────────────────────────────────────

#[test]
fn test_create_dispute_emits_created_event() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

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

// ─── § 23 close event ────────────────────────────────────────────────────────

#[test]
fn test_close_emits_closed_event() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

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

// ─── § 24 resolve event ──────────────────────────────────────────────────────

#[test]
fn test_resolve_emits_resolved_event() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

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

// ─── § 25 get_dispute is read-only ───────────────────────────────────────────

#[test]
fn test_get_dispute_does_not_mutate_state() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

    let d1 = client.try_get_dispute(&id).unwrap().unwrap();
    let d2 = client.try_get_dispute(&id).unwrap().unwrap();

    assert_eq!(d1, d2);
    assert_eq!(d1.status, DisputeStatus::Open);
}

// ─── § 26 full lifecycle Open → Resolved → Closed ────────────────────────────

#[test]
fn test_full_lifecycle_open_resolved_closed() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

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

// ─── § 27 failure recovery – failed calls leave storage clean ────────────────

/// A failed close or resolve (Unauthorized) must not leave any partial write
/// in storage.  The dispute record must be identical before and after.
#[test]
fn test_failed_calls_leave_state_unchanged() {
    let (_, client) = setup();
    let creator = Address::generate(&client.env);
    let resolver = Address::generate(&client.env);
    let attacker = Address::generate(&client.env);
    let id = client.create_dispute(&creator, &resolver);

    let before = client.try_get_dispute(&id).unwrap().unwrap();

    let _ = client.try_close(&attacker, &id); // Unauthorized
    let _ = client.try_resolve(&attacker, &id); // Unauthorized

    let after = client.try_get_dispute(&id).unwrap().unwrap();

    assert_eq!(
        before, after,
        "storage must be unchanged after failed calls"
    );
}

// ─── § 28 acceptance criteria: create_requires_creator_auth ──────────────────

/// AC for #1624: without mocked auth, try_create_dispute fails,
/// and no record exists.
#[test]
fn test_create_requires_creator_auth() {
    let env = Env::default();
    // Do NOT mock all auths - require_auth should fail without it.
    let contract_id = env.register(DisputeResolutionContract, ());
    let client = dispute_resolution::DisputeResolutionContractClient::new(&env, &contract_id);

    let resolver = Address::generate(&env);
    let result = client.try_create_dispute(&resolver, &resolver);
    assert!(result.is_err(), "create_dispute should require auth");
}

// ─── § 29 acceptance criteria: records_in_persistent_storage ─────────────────

/// AC for #1624: the persistent `has(&DataKey::Dispute(0))` check is true.
#[test]
fn test_records_in_persistent_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(DisputeResolutionContract, ());
    let client = dispute_resolution::DisputeResolutionContractClient::new(&env, &contract_id);
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    // Verify the record exists in persistent storage (must be called as the contract).
    env.as_contract(&contract_id, || {
        let key = dispute_resolution::DataKey::Dispute(id);
        let has = env.storage().persistent().has(&key);
        assert!(has, "dispute should be in persistent storage");
    });
}

// ─── § 30 acceptance criteria: ttl_extended_on_access ────────────────────────

/// AC for #1624: after resolve, the persistent TTL of the record is at least
/// the configured threshold, and the instance TTL is too.
#[test]
fn test_ttl_extended_on_access() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(DisputeResolutionContract, ());
    let client = dispute_resolution::DisputeResolutionContractClient::new(&env, &contract_id);
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    // Resolve the dispute (which should extend TTL)
    client.try_resolve(&resolver, &id).unwrap().unwrap();

    // Check that the persistent storage TTL was extended (must be called as the contract).
    env.as_contract(&contract_id, || {
        let key = dispute_resolution::DataKey::Dispute(id);
        let persistent_ttl = env.storage().persistent().get_ttl(&key);
        // TTL should be at least half of STORAGE_TTL_EXTEND_TO (1800)
        assert!(
            persistent_ttl >= 1800,
            "persistent TTL should be extended, got {persistent_ttl}"
        );

        // Check instance TTL is also extended
        let instance_ttl = env.storage().instance().get_ttl();
        assert!(
            instance_ttl >= 1800,
            "instance TTL should be extended, got {instance_ttl}"
        );
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// § 31+ arbitration-backed resolution
//
// These tests register BOTH contracts (dispute_resolution + credence_arbitration)
// on a single ledger and drive a real arbitration ruling into `resolve`, proving
// the binding works end to end rather than only in isolation.
//
// Coverage map
// ─────────────────────────────────────────────────────────────────────────
// § 31 happy path   – final Resolved ruling maps to DisputeStatus::Resolved
// § 32 tie          – a tied arbitration ruling is rejected (ArbitrationTied)
// § 33 no quorum    – a no-quorum ruling is rejected (ArbitrationNoQuorum)
// § 34 not final    – a non-final ruling is rejected, then retried once final
// § 35 auth         – only the bound arbitration contract may resolve
// § 36 plain flow   – the address-resolver flow is unchanged (no arbitration)
// § 37 wire compat  – the mirrored interfaces client decodes the real ruling
// ═══════════════════════════════════════════════════════════════════════════

/// Registers both contracts on one ledger and initializes the arbitration admin.
///
/// Returns the arbitration contract address (which doubles as the
/// arbitration-backed resolver) and both clients so a test can create an
/// arbitration dispute and then bind a `dispute_resolution` dispute to it.
fn setup_pair() -> (
    Env,
    dispute_resolution::DisputeResolutionContractClient<'static>,
    Address,
    CredenceArbitrationClient<'static>,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let dispute_contract_id = env.register(DisputeResolutionContract, ());
    let client =
        dispute_resolution::DisputeResolutionContractClient::new(&env, &dispute_contract_id);
    let arbitration_id = env.register(CredenceArbitration, ());
    let arbitration_client = CredenceArbitrationClient::new(&env, &arbitration_id);
    let admin = Address::generate(&env);
    arbitration_client.initialize(&admin);
    (env, client, arbitration_id, arbitration_client, admin)
}

/// Advance the ledger clock so an arbitration voting window can elapse.
fn advance(env: &Env, secs: u64) {
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: env.ledger().timestamp() + secs,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });
}

/// True when an event with the given 2-topic prefix was emitted.
fn has_event(env: &Env, name: soroban_sdk::Symbol, id: u64) -> bool {
    let events = env.events().all();
    events
        .iter()
        .any(|(_, topics, _data)| topics == vec![env, name.into_val(env), id.into_val(env)])
}

// ─── § 31 happy path: arbitration ruling → Resolved ──────────────────────────

#[test]
fn test_arbitration_backed_resolve_succeeds() {
    let (env, client, arbitration_id, arbitration, _admin) = setup_pair();

    let arb1 = Address::generate(&env);
    let arb2 = Address::generate(&env);
    let arbitration_creator = Address::generate(&env);
    arbitration.register_arbitrator(&arb1, &10);
    arbitration.register_arbitrator(&arb2, &5);

    let arbitration_dispute_id = arbitration.create_dispute(
        &arbitration_creator,
        &String::from_str(&env, "arb dispute"),
        &3600,
    );
    arbitration.vote(&arb1, &arbitration_dispute_id, &1);
    arbitration.vote(&arb2, &arbitration_dispute_id, &2);

    advance(&env, 3601);
    let winner = arbitration.resolve_dispute(&arbitration_dispute_id);
    assert_eq!(winner, 1);

    let creator = Address::generate(&env);
    let id = client.create_arbitration_dispute(&creator, &arbitration_id, &arbitration_dispute_id);

    // The binding is recorded and announced.
    assert!(has_event(&env, symbol_short!("arb_bound"), id));

    // Only the bound arbitration contract resolves the dispute.
    client.try_resolve(&arbitration_id, &id).unwrap().unwrap();

    // Event assertions must run before the next contract call: `events().all()`
    // reports the events of the most recent invocation only.
    assert!(has_event(&env, symbol_short!("resolved"), id));
    assert!(has_event(&env, symbol_short!("arb_res"), id));

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);
    assert_eq!(dispute.resolver, arbitration_id);
    assert_eq!(dispute.creator, creator);
}

// ─── § 32 a tie is rejected ──────────────────────────────────────────────────

#[test]
fn test_arbitration_tied_is_rejected() {
    let (env, client, arbitration_id, arbitration, _admin) = setup_pair();

    let arb1 = Address::generate(&env);
    let arb2 = Address::generate(&env);
    let arbitration_creator = Address::generate(&env);
    arbitration.register_arbitrator(&arb1, &10);
    arbitration.register_arbitrator(&arb2, &10);

    let arbitration_dispute_id =
        arbitration.create_dispute(&arbitration_creator, &String::from_str(&env, "tie"), &3600);
    arbitration.vote(&arb1, &arbitration_dispute_id, &1);
    arbitration.vote(&arb2, &arbitration_dispute_id, &2);

    advance(&env, 3601);
    assert_eq!(arbitration.resolve_dispute(&arbitration_dispute_id), 0);
    assert_eq!(
        ArbitrationClient::new(&env, &arbitration_id)
            .get_dispute(&arbitration_dispute_id)
            .status,
        ArbitrationStatus::Tied
    );

    let creator = Address::generate(&env);
    let id = client.create_arbitration_dispute(&creator, &arbitration_id, &arbitration_dispute_id);

    let err = client
        .try_resolve(&arbitration_id, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, DisputeError::ArbitrationTied);
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Open
    );
}

// ─── § 33 no quorum is rejected ──────────────────────────────────────────────

#[test]
fn test_arbitration_no_quorum_is_rejected() {
    let (env, client, arbitration_id, arbitration, admin) = setup_pair();

    let arb1 = Address::generate(&env);
    let arbitration_creator = Address::generate(&env);
    arbitration.register_arbitrator(&arb1, &10);
    // Require more weight than any single arbitrator can contribute.
    arbitration.set_quorum(&admin, &100, &0);

    let arbitration_dispute_id = arbitration.create_dispute(
        &arbitration_creator,
        &String::from_str(&env, "no quorum"),
        &3600,
    );
    arbitration.vote(&arb1, &arbitration_dispute_id, &1);

    advance(&env, 3601);
    assert_eq!(arbitration.resolve_dispute(&arbitration_dispute_id), 0);
    assert_eq!(
        ArbitrationClient::new(&env, &arbitration_id)
            .get_dispute(&arbitration_dispute_id)
            .status,
        ArbitrationStatus::NoQuorum
    );

    let creator = Address::generate(&env);
    let id = client.create_arbitration_dispute(&creator, &arbitration_id, &arbitration_dispute_id);

    let err = client
        .try_resolve(&arbitration_id, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, DisputeError::ArbitrationNoQuorum);
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Open
    );
}

// ─── § 34 a non-final ruling is rejected, then retried once final ────────────

#[test]
fn test_arbitration_not_final_is_rejected_then_retryable() {
    let (env, client, arbitration_id, arbitration, _admin) = setup_pair();

    let arb1 = Address::generate(&env);
    let arbitration_creator = Address::generate(&env);
    arbitration.register_arbitrator(&arb1, &10);

    let arbitration_dispute_id = arbitration.create_dispute(
        &arbitration_creator,
        &String::from_str(&env, "pending"),
        &3600,
    );
    arbitration.vote(&arb1, &arbitration_dispute_id, &1);

    let creator = Address::generate(&env);
    let id = client.create_arbitration_dispute(&creator, &arbitration_id, &arbitration_dispute_id);

    // Still Voting → rejected, and the dispute stays Open so it can be retried.
    let err = client
        .try_resolve(&arbitration_id, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, DisputeError::ArbitrationNotResolved);
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Open
    );

    // Once arbitration finalizes, the same dispute resolves.
    advance(&env, 3601);
    assert_eq!(arbitration.resolve_dispute(&arbitration_dispute_id), 1);
    client.try_resolve(&arbitration_id, &id).unwrap().unwrap();
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Resolved
    );
}

// ─── § 35 only the bound arbitration contract may resolve ────────────────────

#[test]
fn test_arbitration_backed_resolve_rejects_non_binding_caller() {
    let (env, client, arbitration_id, arbitration, _admin) = setup_pair();

    let arb1 = Address::generate(&env);
    let arbitration_creator = Address::generate(&env);
    let attacker = Address::generate(&env);
    arbitration.register_arbitrator(&arb1, &10);

    let arbitration_dispute_id =
        arbitration.create_dispute(&arbitration_creator, &String::from_str(&env, "auth"), &3600);
    arbitration.vote(&arb1, &arbitration_dispute_id, &1);
    advance(&env, 3601);
    assert_eq!(arbitration.resolve_dispute(&arbitration_dispute_id), 1);

    let creator = Address::generate(&env);
    let id = client.create_arbitration_dispute(&creator, &arbitration_id, &arbitration_dispute_id);

    let err = client.try_resolve(&attacker, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Open
    );
}

// ─── § 36 plain address-resolver flow is unchanged ──────────────────────────

#[test]
fn test_plain_resolver_flow_is_unchanged() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();

    // A plain resolver flow must not emit arbitration-specific events. This is
    // checked before the next call, since `events().all()` reports the most
    // recent invocation only.
    assert!(
        !has_event(&env, symbol_short!("arb_res"), id),
        "plain resolver flow must not emit 'arb_res'"
    );

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);
    assert_eq!(dispute.resolver, resolver);
}

// ─── § 37 the mirrored interfaces client decodes the real ruling ────────────

/// Pins wire-compatibility between `interfaces::arbitration` mirrored types and
/// the arbitration contract's own `Dispute`/`DisputeStatus` encoding.
#[test]
fn test_interfaces_client_decodes_arbitration_dispute() {
    let (env, _client, arbitration_id, arbitration, _admin) = setup_pair();

    let arb1 = Address::generate(&env);
    let arbitration_creator = Address::generate(&env);
    arbitration.register_arbitrator(&arb1, &7);

    let arbitration_dispute_id =
        arbitration.create_dispute(&arbitration_creator, &String::from_str(&env, "wire"), &1234);
    arbitration.vote(&arb1, &arbitration_dispute_id, &9);

    let decoded =
        ArbitrationClient::new(&env, &arbitration_id).get_dispute(&arbitration_dispute_id);
    assert_eq!(decoded.id, arbitration_dispute_id);
    assert_eq!(decoded.creator, arbitration_creator);
    assert_eq!(decoded.status, ArbitrationStatus::Voting);
    assert_eq!(decoded.outcome, 0);
    assert_eq!(decoded.voting_end, decoded.voting_start + 1234);
    assert_eq!(decoded.cancellation_reason, None);

    advance(&env, 1235);
    assert_eq!(arbitration.resolve_dispute(&arbitration_dispute_id), 9);
    let decoded =
        ArbitrationClient::new(&env, &arbitration_id).get_dispute(&arbitration_dispute_id);
    assert_eq!(decoded.status, ArbitrationStatus::Resolved);
    assert_eq!(decoded.outcome, 9);
}
