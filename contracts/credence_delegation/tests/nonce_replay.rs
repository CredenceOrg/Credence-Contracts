//! Property tests for `invalidate_nonce_range` — replay-window proof.
//!
//! These tests verify the central security invariant:
//!
//! > After `invalidate_nonce_range(identity, new_nonce)` succeeds, **every**
//! > payload whose `nonce` field falls in the half-open range
//! > `[prev_nonce, new_nonce)` is permanently rejected with `InvalidNonce`,
//! > regardless of when the payload was produced.
//!
//! We exercise this property with **10 000 random (current, span, offset)**
//! triples, plus deterministic boundary tests for the edge cases explicitly
//! required by the issue:
//!
//! * Invalidation by exactly 1 nonce.
//! * Invalidation by `MAX_NONCE_INVALIDATION_SPAN` (10 000).
//! * Attempted invalidation by `MAX_NONCE_INVALIDATION_SPAN + 1` (must fail).
//! * The upper boundary nonce `new_nonce - 1` is rejected.
//! * The first post-invalidation nonce `new_nonce` is accepted.

use credence_delegation::{
    CredenceDelegation, CredenceDelegationClient, DelegatedActionPayload, DelegationType, DomainTag,
};
use soroban_sdk::{testutils::Address as _, Address, Env};

use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Test environment helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let e = Env::default();
    e.mock_all_auths();
    e
}

fn setup_client(e: &Env) -> CredenceDelegationClient<'static> {
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    client
}

/// Build a `DelegatedActionPayload` with the given nonce for `execute_delegated_delegate`.
fn make_delegate_payload(
    owner: &Address,
    delegate: &Address,
    contract_id: &Address,
    nonce: u64,
) -> DelegatedActionPayload {
    DelegatedActionPayload {
        domain: DomainTag::Delegate,
        owner: owner.clone(),
        target: delegate.clone(),
        contract_id: contract_id.clone(),
        nonce,
        scheme: 0,
    }
}

/// Advance the identity's stored nonce to `target` using `invalidate_nonce_range`.
///
/// Panics if `target == 0` (no advance needed) or if the advance exceeds
/// `MAX_NONCE_INVALIDATION_SPAN` in one step (splits automatically).
fn advance_nonce_to(client: &CredenceDelegationClient, identity: &Address, target: u64) {
    const MAX_SPAN: u64 = 10_000;
    let mut current = client.get_nonce(identity);
    while current < target {
        let step = (target - current).min(MAX_SPAN);
        client.invalidate_nonce_range(identity, &(current + step));
        current += step;
    }
}

// ---------------------------------------------------------------------------
// 1. Deterministic boundary tests
// ---------------------------------------------------------------------------

/// Invalidation by exactly 1: nonce 0 becomes unreachable after jump to 1.
#[test]
fn boundary_invalidate_by_one() {
    let e = make_env();
    let client = setup_client(&e);
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.invalidate_nonce_range(&identity, &1);
    assert_eq!(client.get_nonce(&identity), 1);

    let result = client.try_execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_delegate_payload(&identity, &delegate, &client.address, 0),
    );
    assert!(result.is_err(), "nonce 0 must be rejected after jump to 1");
}

/// Invalidation by MAX_NONCE_INVALIDATION_SPAN (10 000) succeeds.
#[test]
fn boundary_invalidate_max_span_succeeds() {
    let e = make_env();
    let client = setup_client(&e);
    let identity = Address::generate(&e);

    client.invalidate_nonce_range(&identity, &10_000);
    assert_eq!(client.get_nonce(&identity), 10_000);
}

/// Attempting to invalidate by MAX_NONCE_INVALIDATION_SPAN + 1 must fail.
#[test]
fn boundary_invalidate_over_max_span_fails() {
    let e = make_env();
    let client = setup_client(&e);
    let identity = Address::generate(&e);

    let result = client.try_invalidate_nonce_range(&identity, &10_001);
    assert!(
        result.is_err(),
        "span of 10 001 must be rejected (exceeds MAX_NONCE_INVALIDATION_SPAN)"
    );
    // Stored nonce must remain unchanged.
    assert_eq!(client.get_nonce(&identity), 0);
}

/// The last nonce in the invalidated range (`new_nonce - 1`) must be rejected.
#[test]
fn boundary_last_invalidated_nonce_rejected() {
    let e = make_env();
    let client = setup_client(&e);
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Invalidate [0, 50) — last invalidated nonce is 49.
    client.invalidate_nonce_range(&identity, &50);

    let result = client.try_execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_delegate_payload(&identity, &delegate, &client.address, 49),
    );
    assert!(
        result.is_err(),
        "nonce 49 (last in [0,50)) must be rejected"
    );
}

/// The first nonce after the invalidated range (`new_nonce`) must be accepted.
#[test]
fn boundary_first_valid_nonce_accepted() {
    let e = make_env();
    let client = setup_client(&e);
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.invalidate_nonce_range(&identity, &50);

    // nonce 50 is the first valid nonce.
    client.execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_delegate_payload(&identity, &delegate, &client.address, 50),
    );
    assert_eq!(client.get_nonce(&identity), 51);
}

/// Once a range is invalidated a second invalidation covering the same nonces
/// from a higher base still rejects the old nonces.
#[test]
fn boundary_double_invalidation_still_rejects_old_nonces() {
    let e = make_env();
    let client = setup_client(&e);
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.invalidate_nonce_range(&identity, &5);
    client.invalidate_nonce_range(&identity, &10);
    assert_eq!(client.get_nonce(&identity), 10);

    for n in 0u64..10 {
        let result = client.try_execute_delegated_delegate(
            &identity,
            &delegate,
            &DelegationType::Attestation,
            &(e.ledger().timestamp() + 86_400),
            &make_delegate_payload(&identity, &delegate, &client.address, n),
        );
        assert!(
            result.is_err(),
            "nonce {n} must be rejected after two invalidations"
        );
    }
}

/// Monotonicity: a new_nonce that is not strictly greater than the stored
/// nonce must be rejected.
#[test]
fn boundary_non_monotone_invalidation_fails() {
    let e = make_env();
    let client = setup_client(&e);
    let identity = Address::generate(&e);

    client.invalidate_nonce_range(&identity, &5);

    // Trying to set new_nonce = 5 (equal, not greater) must fail.
    let result = client.try_invalidate_nonce_range(&identity, &5);
    assert!(
        result.is_err(),
        "new_nonce <= stored_nonce must be rejected"
    );

    // Trying to set new_nonce = 3 (less than) must also fail.
    let result = client.try_invalidate_nonce_range(&identity, &3);
    assert!(result.is_err(), "new_nonce < stored_nonce must be rejected");
}

/// After invalidation, a valid subsequent nonce is consumed exactly once.
#[test]
fn boundary_post_invalidation_nonce_consumed_once() {
    let e = make_env();
    let client = setup_client(&e);
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.invalidate_nonce_range(&identity, &100);

    // Use nonce 100 successfully.
    client.execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_delegate_payload(&identity, &delegate, &client.address, 100),
    );

    // Attempting to replay nonce 100 must now fail.
    let result = client.try_execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_delegate_payload(&identity, &delegate, &client.address, 100),
    );
    assert!(
        result.is_err(),
        "replaying nonce 100 after it was consumed must fail"
    );
}

// ---------------------------------------------------------------------------
// 2. Property tests — 10 000 random (current, span, offset) triples
// ---------------------------------------------------------------------------

/// Generate a valid (current_nonce, span, offset_within_span) triple where:
/// - `current` is the stored nonce before invalidation (0..1_000).
/// - `span` is the size of the invalidated range (1..=MAX_NONCE_INVALIDATION_SPAN).
/// - `offset` is the nonce to test — must be in `[current, current + span)`.
fn nonce_triple() -> impl Strategy<Value = (u64, u64, u64)> {
    (0u64..1_000u64, 1u64..=10_000u64).prop_flat_map(|(current, span)| {
        let offset_strat = 0u64..span;
        (Just(current), Just(span), offset_strat)
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// Core property: any nonce in `[current, new_nonce)` is rejected after
    /// `invalidate_nonce_range(identity, new_nonce)`.
    #[test]
    fn prop_all_invalidated_nonces_rejected(
        (current, span, offset) in nonce_triple()
    ) {
        let e = make_env();
        let client = setup_client(&e);
        let identity = Address::generate(&e);
        let delegate = Address::generate(&e);

        // Advance stored nonce to `current` (if non-zero).
        if current > 0 {
            advance_nonce_to(&client, &identity, current);
        }

        let new_nonce = current + span;
        let test_nonce = current + offset; // offset in [0, span) → test_nonce in [current, new_nonce)

        // Invalidate [current, new_nonce).
        client.invalidate_nonce_range(&identity, &new_nonce);
        prop_assert_eq!(client.get_nonce(&identity), new_nonce);

        // Any nonce in the invalidated range must be rejected.
        let result = client.try_execute_delegated_delegate(
            &identity,
            &delegate,
            &DelegationType::Attestation,
            &(e.ledger().timestamp() + 86_400),
            &make_delegate_payload(&identity, &delegate, &client.address, test_nonce),
        );
        prop_assert!(
            result.is_err(),
            "nonce {} must be rejected: stored nonce is {} (invalidated range [{}, {}))",
            test_nonce, new_nonce, current, new_nonce
        );
    }

    /// Corollary: the first nonce after the invalidated range is always accepted.
    #[test]
    fn prop_post_invalidation_nonce_accepted(
        (current, span, _) in nonce_triple()
    ) {
        let e = make_env();
        let client = setup_client(&e);
        let identity = Address::generate(&e);
        let delegate = Address::generate(&e);

        if current > 0 {
            advance_nonce_to(&client, &identity, current);
        }

        let new_nonce = current + span;
        client.invalidate_nonce_range(&identity, &new_nonce);

        // new_nonce itself must be accepted (it is the first valid nonce).
        let result = client.try_execute_delegated_delegate(
            &identity,
            &delegate,
            &DelegationType::Attestation,
            &(e.ledger().timestamp() + 86_400),
            &make_delegate_payload(&identity, &delegate, &client.address, new_nonce),
        );
        prop_assert!(
            result.is_ok(),
            "nonce {} (first post-invalidation) must be accepted",
            new_nonce
        );
    }
}
