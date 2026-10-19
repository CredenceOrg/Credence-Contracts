//! Inline replay-window tests for `invalidate_nonce_range`.
//!
//! These tests live in `src/` and run under the `no_std` environment, so they
//! use a deterministic xorshift64 PRNG instead of the `proptest` crate.
//! The proptest integration tests in `tests/nonce_replay.rs` cover the same
//! property with 10 000 randomised cases; these tests focus on the boundary
//! cases and deterministic full-range sweeps.

use super::*;
use soroban_sdk::{String, testutils::Address as _};
use soroban_sdk::Env;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, CredenceDelegationClient<'static>) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    (e, client)
}

fn make_payload(
    e: &Env,
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
        ledger_number: 0,
        signature_domain: String::from_str(e, "CredenceDelegation"),
    }
}

/// xorshift64 — fast, deterministic PRNG for no_std property sweeps.
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

// ---------------------------------------------------------------------------
// Future-nonce hardening (issue #1062)
// ---------------------------------------------------------------------------

/// The future-nonce window allows relays within MAX_NONCE_FUTURE_WINDOW,
/// but any nonce beyond that must be rejected even before the equality check.
#[test]
fn future_nonce_beyond_window_rejected() {
    let (e, client) = setup();
    let identity = Address::generate(&e);

    // Current nonce is 0. Nonce 1 is valid. Nonce > MAX_NONCE_FUTURE_WINDOW
    // must be rejected by the future-nonce guard.
    let far_future = nonce::MAX_NONCE_FUTURE_WINDOW + 1;

    let delegate = Address::generate(&e);
    let result = client.try_execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_payload(&e, &identity, &delegate, &client.address, far_future),
    );
    assert!(
        result.is_err(),
        "nonce {} (beyond MAX_NONCE_FUTURE_WINDOW) must be rejected",
        far_future
    );
}

/// A nonce exactly at the future-nonce window boundary is still checked by
/// the equality guard (and will fail with InvalidNonce because it does not
/// equal the current nonce).
#[test]
fn future_nonce_at_window_boundary_rejected_by_equality() {
    let (e, client) = setup();
    let identity = Address::generate(&e);

    // Current nonce is 0. Nonce at MAX_NONCE_FUTURE_WINDOW passes the
    // future-nonce guard but fails the equality check.
    let at_window = nonce::MAX_NONCE_FUTURE_WINDOW;

    let delegate = Address::generate(&e);
    let result = client.try_execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_payload(&e, &identity, &delegate, &client.address, at_window),
    );
    assert!(
        result.is_err(),
        "nonce {} (at window boundary) must be rejected by equality check",
        at_window
    );
}

/// When the stored nonce is non-zero, the future-nonce window slides forward.
#[test]
fn future_nonce_window_slides_with_stored_nonce() {
    let (e, client) = setup();
    let identity = Address::generate(&e);

    // Advance to nonce 100.
    client.invalidate_nonce_range(&identity, &100);
    assert_eq!(client.get_nonce(&identity), 100);

    // Nonce 100 + MAX_NONCE_FUTURE_WINDOW + 1 must be rejected.
    let far_future = 100 + nonce::MAX_NONCE_FUTURE_WINDOW + 1;

    let delegate = Address::generate(&e);
    let result = client.try_execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_payload(&e, &identity, &delegate, &client.address, far_future),
    );
    assert!(
        result.is_err(),
        "nonce {} (beyond sliding window from 100) must be rejected",
        far_future
    );

    // Nonce 100 (exact match) must be accepted.
    client.execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_payload(&e, &identity, &delegate, &client.address, 100),
    );
    assert_eq!(client.get_nonce(&identity), 101);
}

/// The same nonce consumed twice in rapid succession must be rejected.
/// This validates near-simultaneous duplicate rejection (issue #1062).
#[test]
fn same_nonce_duplicate_rejected_after_consumption() {
    let (e, client) = setup();
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);

    // First submission: nonce 0 succeeds.
    client.execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_payload(&e, &identity, &delegate, &client.address, 0),
    );

    // Second submission with same nonce 0 must be rejected.
    let result = client.try_execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_payload(&e, &identity, &delegate, &client.address, 0),
    );
    assert!(
        result.is_err(),
        "duplicate nonce 0 must be rejected after first consumption"
    );

    // Nonce 1 must be the next valid nonce.
    client.execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_payload(&e, &identity, &delegate, &client.address, 1),
    );
    assert_eq!(client.get_nonce(&identity), 2);
}

/// A nonce in the range (current, current + MAX_NONCE_FUTURE_WINDOW]
/// passes the future-nonce guard but fails the equality check because it
/// doesn't match the stored nonce. This is the correct behavior — only
/// exact matches succeed.
#[test]
fn future_nonce_within_window_but_not_equal_fails_equality() {
    let (e, client) = setup();
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Advance to nonce 5.
    client.invalidate_nonce_range(&identity, &5);

    // Nonce 6 is within the window from 5, but != 5, so equality fails.
    let result = client.try_execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_payload(&e, &identity, &delegate, &client.address, 6),
    );
    assert!(
        result.is_err(),
        "nonce 6 (within future window but != stored 5) must be rejected"
    );
}

/// The future-nonce guard applies to the direct `delegate()` path as well
/// (not just the delegated relay path), since both call `consume_nonce`.
#[test]
fn future_nonce_guard_on_direct_delegate_path() {
    let (e, client) = setup();
    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Nonce 0 must succeed on the direct path.
    client.delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &0_u64,
    );
    assert_eq!(client.get_nonce(&owner), 1);

    // Nonce far beyond MAX_NONCE_FUTURE_WINDOW (from current=1) must be rejected.
    let far_future = 1 + nonce::MAX_NONCE_FUTURE_WINDOW + 1;
    let result = client.try_delegate(
        &owner,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &far_future,
    );
    assert!(
        result.is_err(),
        "direct delegate with nonce {} (beyond future window) must be rejected",
        far_future
    );
}

// ---------------------------------------------------------------------------
// Deterministic boundary cases
// ---------------------------------------------------------------------------

#[test]
fn nonce_replay_span_one_rejects_only_nonce() {
    let (e, client) = setup();
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);

    client.invalidate_nonce_range(&identity, &1);
    assert_eq!(client.get_nonce(&identity), 1);

    // Nonce 0 is invalidated.
    assert!(
        client
            .try_execute_delegated_delegate(
                &identity,
                &delegate,
                &DelegationType::Attestation,
                &(e.ledger().timestamp() + 86_400),
                &make_payload(&e, &identity, &delegate, &client.address, 0),
            )
            .is_err(),
        "nonce 0 must be rejected after invalidating [0, 1)"
    );

    // Nonce 1 is the first valid nonce.
    client
        .execute_delegated_delegate(
            &identity,
            &delegate,
            &DelegationType::Attestation,
            &(e.ledger().timestamp() + 86_400),
            &make_payload(&e, &identity, &delegate, &client.address, 1),
        );
    assert_eq!(client.get_nonce(&identity), 2);
}

#[test]
fn nonce_replay_max_span_succeeds_and_rejects_all_prior_nonces() {
    let (e, client) = setup();
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);

    // Jump by the maximum allowed span.
    client.invalidate_nonce_range(&identity, &MAX_NONCE_INVALIDATION_SPAN);
    assert_eq!(client.get_nonce(&identity), MAX_NONCE_INVALIDATION_SPAN);

    // Every nonce in [0, MAX_NONCE_INVALIDATION_SPAN) must be rejected.
    for n in [0, 1, 999, 5_000, 9_999] {
        assert!(
            client
                .try_execute_delegated_delegate(
                    &identity,
                    &delegate,
                    &DelegationType::Attestation,
                    &(e.ledger().timestamp() + 86_400),
                    &make_payload(&e, &identity, &delegate, &client.address, n),
                )
                .is_err(),
            "nonce {n} must be rejected after full max-span invalidation"
        );
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #208)")]
fn nonce_replay_over_max_span_panics() {
    let (e, client) = setup();
    let identity = Address::generate(&e);

    // MAX_NONCE_INVALIDATION_SPAN + 1 must be rejected.
    client.invalidate_nonce_range(&identity, &(MAX_NONCE_INVALIDATION_SPAN + 1));
}

#[test]
fn nonce_replay_boundary_last_in_range_rejected() {
    let (e, client) = setup();
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);
    const NEW_NONCE: u64 = 42;

    client.invalidate_nonce_range(&identity, &NEW_NONCE);

    // new_nonce - 1 = 41 is the last invalidated nonce.
    assert!(
        client
            .try_execute_delegated_delegate(
                &identity,
                &delegate,
                &DelegationType::Attestation,
                &(e.ledger().timestamp() + 86_400),
                &make_payload(&e, &identity, &delegate, &client.address, NEW_NONCE - 1),
            )
            .is_err(),
        "nonce {} (last in range) must be rejected",
        NEW_NONCE - 1
    );
}

#[test]
fn nonce_replay_boundary_first_after_range_accepted() {
    let (e, client) = setup();
    let identity = Address::generate(&e);
    let delegate = Address::generate(&e);
    const NEW_NONCE: u64 = 42;

    client.invalidate_nonce_range(&identity, &NEW_NONCE);

    // new_nonce = 42 is the first valid nonce.
    client.execute_delegated_delegate(
        &identity,
        &delegate,
        &DelegationType::Attestation,
        &(e.ledger().timestamp() + 86_400),
        &make_payload(&e, &identity, &delegate, &client.address, NEW_NONCE),
    );
    assert_eq!(client.get_nonce(&identity), NEW_NONCE + 1);
}

// ---------------------------------------------------------------------------
// Deterministic sweep: 10 000 iterations with xorshift64 PRNG
//
// Each iteration independently generates (current, span, offset) and verifies
// that the nonce at offset is rejected after invalidating [current, current+span).
// ---------------------------------------------------------------------------

#[test]
fn nonce_replay_10k_deterministic_sweep() {
    let (e, client) = setup();
    let mut rng: u64 = 0xdeadbeef_12345678;

    for _iter in 0..10_000_u32 {
        let identity = Address::generate(&e);
        let delegate = Address::generate(&e);

        // current: 0..100  (small enough for a single MAX_SPAN jump from 0)
        let current = xorshift64(&mut rng) % 100;
        // span: 1..=MAX_NONCE_INVALIDATION_SPAN
        let span = (xorshift64(&mut rng) % MAX_NONCE_INVALIDATION_SPAN) + 1;
        // offset: 0..span  →  test_nonce in [current, current+span)
        let offset = xorshift64(&mut rng) % span;
        let new_nonce = current + span;
        let test_nonce = current + offset;

        // Advance stored nonce to `current`.
        if current > 0 {
            client.invalidate_nonce_range(&identity, &current);
        }

        // Invalidate [current, new_nonce).
        client.invalidate_nonce_range(&identity, &new_nonce);
        assert_eq!(
            client.get_nonce(&identity),
            new_nonce,
            "stored nonce must equal new_nonce after invalidation (iter {_iter})"
        );

        // Test nonce must be rejected.
        let result = client.try_execute_delegated_delegate(
            &identity,
            &delegate,
            &DelegationType::Attestation,
            &(e.ledger().timestamp() + 86_400),
            &make_payload(&e, &identity, &delegate, &client.address, test_nonce),
        );
        assert!(
            result.is_err(),
            "iter {_iter}: nonce {test_nonce} must be rejected (range [{current}, {new_nonce}))"
        );
    }
}
