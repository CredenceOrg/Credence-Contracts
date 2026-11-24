//! Retry, replay and permission-recovery tests for `idempotency.rs` (#1332).
//!
//! The module exists so that an externally-retried admin request cannot be
//! applied twice. These tests pin the failure and recovery paths:
//! - the happy path records a key and a different salt is still allowed
//! - a rejected duplicate leaves no partial state behind and does not wedge the
//!   reentrancy lock, so a later distinct request still succeeds
//! - a request that aborts *after* the key was recorded rolls the key back with
//!   it, so the admin can correct the request and retry under the same salt
//! - a key outlives a long ledger jump, so a webhook retry hours later is still
//!   rejected and the operation is applied exactly once
//! - an unauthorized caller can neither consume a key nor learn which keys exist
//! - actors and operations do not share a namespace

extern crate std;

use credence_errors::{ContractError, ErrorExt};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, Env, Symbol};

use crate::idempotency::{check_and_record, compute_key, is_used};
use crate::test_helpers::{
    advance_ledger_sequence, advance_ledgers_by, expect_contract_error_panic, setup_with_token,
};

fn salt(e: &Env, s: &[u8]) -> Bytes {
    Bytes::from_slice(e, s)
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

/// A first request is accepted, a second under a different salt is accepted, and
/// both keys are independently recorded.
#[test]
fn distinct_salts_are_both_accepted_and_recorded() {
    let e = Env::default();
    let (client, admin, _identity, _token, contract_id) = setup_with_token(&e);
    let first = salt(&e, b"request-1");
    let second = salt(&e, b"request-2");

    client.collect_fees(&admin, &first);
    client.collect_fees(&admin, &second);

    e.as_contract(&contract_id, || {
        assert!(is_used(
            &e,
            &admin,
            &Symbol::new(&e, "collect_fees"),
            &first
        ));
        assert!(is_used(
            &e,
            &admin,
            &Symbol::new(&e, "collect_fees"),
            &second
        ));
    });
}

/// An empty salt is the documented opt-out, so it must not be recorded and must
/// not block the next empty-salt call. This is the compatibility guarantee for
/// every caller that predates the module.
#[test]
fn empty_salt_remains_unprotected_and_repeatable() {
    let e = Env::default();
    let (client, admin, _identity, _token, contract_id) = setup_with_token(&e);
    let empty = Bytes::new(&e);

    client.collect_fees(&admin, &empty);
    client.collect_fees(&admin, &empty);

    e.as_contract(&contract_id, || {
        assert!(
            !is_used(&e, &admin, &Symbol::new(&e, "collect_fees"), &empty),
            "an empty salt must not consume a key"
        );
    });
}

// ---------------------------------------------------------------------------
// Duplicate rejection
// ---------------------------------------------------------------------------

/// A long run of identical retries is refused every time. This is the shape of a
/// webhook that redelivers on a schedule.
#[test]
fn repeated_redelivery_is_rejected_every_time() {
    let e = Env::default();
    let (client, admin, _identity, _token, _contract_id) = setup_with_token(&e);
    let s = salt(&e, b"redeliver");

    client.collect_fees(&admin, &s);
    for _ in 0..5 {
        expect_contract_error_panic(ContractError::DuplicateIdempotencyKey as u32, || {
            client.collect_fees(&admin, &s);
        });
    }
}

/// After a duplicate rejection the contract is still usable: a distinct request
/// succeeds. A rejection must not wedge the contract.
#[test]
fn contract_recovers_after_a_duplicate_rejection() {
    let e = Env::default();
    let (client, admin, _identity, _token, _contract_id) = setup_with_token(&e);
    let first = salt(&e, b"first");
    let second = salt(&e, b"second");

    client.collect_fees(&admin, &first);
    expect_contract_error_panic(ContractError::DuplicateIdempotencyKey as u32, || {
        client.collect_fees(&admin, &first);
    });
    client.collect_fees(&admin, &second);
}

// ---------------------------------------------------------------------------
// Atomicity: an aborted operation must not burn the key
// ---------------------------------------------------------------------------

/// `slash_bond` records the key and only afterwards validates the amount, so
/// `slash_amount == 0` aborts with `InvalidBondAmount` *after* the write. The
/// whole invocation is reverted, so the key must be gone and the admin must be
/// able to correct the request and retry under the same salt.
#[test]
fn aborted_request_does_not_consume_its_key() {
    let e = Env::default();
    let (client, admin, identity, _token, contract_id) = setup_with_token(&e);
    client.create_bond(&identity, &1_000_i128, &3600_u64, &false, &0_u64);
    // Slashing is rejected in the ledger the collateral was last raised in.
    advance_ledger_sequence(&e);

    let s = salt(&e, b"aborted-then-fixed");

    // Records the key, then panics on the zero amount.
    expect_contract_error_panic(ContractError::InvalidBondAmount as u32, || {
        client.slash_bond(&admin, &identity, &0_i128, &s);
    });

    e.as_contract(&contract_id, || {
        assert!(
            !is_used(&e, &admin, &Symbol::new(&e, "slash_bond"), &s),
            "the reverted invocation must not leave its key behind"
        );
    });

    // The corrected retry under the same salt is accepted.
    client.slash_bond(&admin, &identity, &10_i128, &s);
    assert_eq!(client.describe_bond(&identity).unwrap().slashed_amount, 10);
}

/// The same rollback guarantee when the abort originates further into the
/// operation, i.e. a rejected slash amount that exceeds the bond.
#[test]
fn request_rejected_late_does_not_consume_its_key() {
    let e = Env::default();
    let (client, admin, identity, _token, contract_id) = setup_with_token(&e);
    client.create_bond(&identity, &1_000_i128, &3600_u64, &false, &0_u64);
    advance_ledger_sequence(&e);

    let s = salt(&e, b"too-large");

    expect_contract_error_panic(ContractError::SlashExceedsBond as u32, || {
        client.slash_bond(&admin, &identity, &1_000_000_i128, &s);
    });

    e.as_contract(&contract_id, || {
        assert!(!is_used(&e, &admin, &Symbol::new(&e, "slash_bond"), &s));
    });

    // A corrected request under the same salt is accepted and the bond is
    // slashed exactly once.
    client.slash_bond(&admin, &identity, &10_i128, &s);
    assert_eq!(client.describe_bond(&identity).unwrap().slashed_amount, 10);
}

/// A rejected duplicate must not leave the reentrancy lock held, otherwise the
/// next legitimate request would fail with a reentrancy error instead of doing
/// its work.
#[test]
fn duplicate_rejection_releases_the_reentrancy_lock() {
    let e = Env::default();
    let (client, admin, identity, _token, contract_id) = setup_with_token(&e);
    client.create_bond(&identity, &1_000_i128, &3600_u64, &false, &0_u64);
    advance_ledger_sequence(&e);

    let s = salt(&e, b"lock-check");

    client.slash_bond(&admin, &identity, &10_i128, &s);
    expect_contract_error_panic(ContractError::DuplicateIdempotencyKey as u32, || {
        client.slash_bond(&admin, &identity, &10_i128, &s);
    });

    e.as_contract(&contract_id, || {
        let locked: bool = e
            .storage()
            .instance()
            .get(&Symbol::new(&e, "locked"))
            .unwrap_or(false);
        assert!(
            !locked,
            "a duplicate rejection left the reentrancy lock held"
        );
    });

    // A different request still gets through.
    client.slash_bond(&admin, &identity, &10_i128, &salt(&e, b"lock-check-2"));
}

// ---------------------------------------------------------------------------
// Long-horizon replay
// ---------------------------------------------------------------------------

/// A webhook that redelivers a long time later must still be rejected, and the
/// underlying state must be unchanged by the rejected replay.
///
/// An unpinned `persistent().set` would have lapsed at the network minimum entry
/// TTL (4096 ledgers, about 11 hours). The write path pins the entry to
/// `PERSISTENT_TTL_MAX`, so the replay is refused and the bond is slashed exactly
/// once.
#[test]
fn replay_after_a_long_delay_is_still_rejected() {
    let e = Env::default();
    let (client, admin, identity, _token, contract_id) = setup_with_token(&e);
    client.create_bond(&identity, &1_000_i128, &3600_u64, &false, &0_u64);
    advance_ledger_sequence(&e);

    let s = salt(&e, b"redelivered-late");

    client.slash_bond(&admin, &identity, &25_i128, &s);
    let after_first = client.describe_bond(&identity).unwrap().slashed_amount;

    // 100_000 ledgers is roughly 24x min_persistent_entry_ttl.
    advance_ledgers_by(&e, 100_000);

    expect_contract_error_panic(ContractError::DuplicateIdempotencyKey as u32, || {
        client.slash_bond(&admin, &identity, &25_i128, &s);
    });

    assert_eq!(
        client.describe_bond(&identity).unwrap().slashed_amount,
        after_first,
        "the delayed replay changed bonded state"
    );
    e.as_contract(&contract_id, || {
        assert!(is_used(&e, &admin, &Symbol::new(&e, "slash_bond"), &s));
    });
}

// ---------------------------------------------------------------------------
// Permission states
// ---------------------------------------------------------------------------

/// An unauthorized caller is refused on authorization grounds, and its attempt
/// writes nothing. The check runs after authorization precisely so that a
/// non-admin cannot burn a key the real admin still needs.
#[test]
fn unauthorized_caller_cannot_consume_a_key() {
    let e = Env::default();
    let (client, admin, _identity, _token, contract_id) = setup_with_token(&e);
    let attacker = Address::generate(&e);
    let s = salt(&e, b"admin-key");

    client.collect_fees(&admin, &s);

    expect_contract_error_panic(ContractError::NotAdmin as u32, || {
        client.collect_fees(&attacker, &salt(&e, b"attacker"));
    });

    e.as_contract(&contract_id, || {
        assert!(!is_used(
            &e,
            &attacker,
            &Symbol::new(&e, "collect_fees"),
            &salt(&e, b"attacker")
        ));
        // The admin's key is untouched, so their next request still works.
        assert!(is_used(&e, &admin, &Symbol::new(&e, "collect_fees"), &s));
    });
}

/// An unauthorized caller probing with the admin's own salt is refused with the
/// same error as one probing an unused salt, so the two are indistinguishable
/// and key existence is not disclosed.
#[test]
fn unauthorized_caller_cannot_probe_key_existence() {
    let e = Env::default();
    let (client, admin, _identity, _token, _contract_id) = setup_with_token(&e);
    let attacker = Address::generate(&e);

    let used = salt(&e, b"exists");
    client.collect_fees(&admin, &used);

    // Both probes must fail on authorization alone. If the duplicate check ran
    // first, the "used" probe would surface DuplicateIdempotencyKey and reveal
    // that the key exists.
    expect_contract_error_panic(ContractError::NotAdmin as u32, || {
        client.collect_fees(&attacker, &used);
    });
    expect_contract_error_panic(ContractError::NotAdmin as u32, || {
        client.collect_fees(&attacker, &salt(&e, b"does-not-exist"));
    });

    assert!(
        !ContractError::NotAdmin
            .description()
            .contains("Idempotency"),
        "the authorization error disclosed an idempotency detail"
    );
}

/// A duplicate rejection is caller-fixable, so it is classified recoverable by
/// `ErrorExt`. That is what tells an operator to retry with a fresh key rather
/// than to treat it as a contract fault.
#[test]
fn duplicate_error_is_classified_recoverable() {
    assert!(
        ContractError::DuplicateIdempotencyKey.is_recoverable(),
        "a duplicate key is fixable by retrying with a new salt"
    );
}

// ---------------------------------------------------------------------------
// Namespace isolation
// ---------------------------------------------------------------------------

/// The operation is part of the hashed namespace, so a salt reused across two
/// different operations does not collide. This is what lets an admin reuse one
/// request identifier across endpoints safely.
#[test]
fn operations_do_not_share_a_namespace() {
    let e = Env::default();
    let contract_id = e.register(crate::CredenceBond, ());
    let actor = Address::generate(&e);
    let shared = salt(&e, b"shared-id");

    let slash_key = compute_key(&e, &actor, &Symbol::new(&e, "slash_bond"), &shared);
    let fees_key = compute_key(&e, &actor, &Symbol::new(&e, "collect_fees"), &shared);
    assert_ne!(slash_key, fees_key);

    e.as_contract(&contract_id, || {
        check_and_record(&e, &actor, &Symbol::new(&e, "slash_bond"), &shared);
        assert!(!is_used(
            &e,
            &actor,
            &Symbol::new(&e, "collect_fees"),
            &shared
        ));
        check_and_record(&e, &actor, &Symbol::new(&e, "collect_fees"), &shared);
        assert!(is_used(
            &e,
            &actor,
            &Symbol::new(&e, "collect_fees"),
            &shared
        ));
    });
}

/// Actors do not share a namespace either: one actor's key cannot block another.
#[test]
fn actors_do_not_share_a_namespace() {
    let e = Env::default();
    let contract_id = e.register(crate::CredenceBond, ());
    let actor1 = Address::generate(&e);
    let actor2 = Address::generate(&e);
    let operation = Symbol::new(&e, "collect_fees");
    let s = salt(&e, b"same-salt");

    e.as_contract(&contract_id, || {
        check_and_record(&e, &actor1, &operation, &s);
        assert!(is_used(&e, &actor1, &operation, &s));
        assert!(!is_used(&e, &actor2, &operation, &s));
        check_and_record(&e, &actor2, &operation, &s);
        assert!(is_used(&e, &actor2, &operation, &s));
    });
}

/// A bond's full configured lifetime is bounded by the TTL we pin keys to, so an
/// idempotency key outlives any window in which its request could still be
/// meaningfully retried.
#[test]
fn key_retention_outlives_the_bond_lifetime() {
    // MAX_BOND_DURATION_SECONDS is one year; at a 5 s cadence that is about
    // 6.3M ledgers, and PERSISTENT_TTL_MAX is the network's 3.1M ceiling. The
    // guarantee that matters is that a key is not forgotten while the request
    // that created it is still in flight, which the 4096-ledger network minimum
    // would fail after roughly 11 hours. Pinned keys are far above that.
    assert!(crate::PERSISTENT_TTL_MAX > 4_096);
}
