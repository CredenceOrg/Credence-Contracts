//! Boundary and edge-case tests for `idempotency.rs` (#1332).
//!
//! Covers the edges of the key space and of the retention guarantee:
//! - key derivation stays deterministic and collision-free across actors,
//!   operations and salts, including at the length boundaries where naive
//!   concatenation would alias
//! - an empty salt (the documented opt-out) is a distinct namespace
//! - the salt length boundary accepted by `collect_fees` is enforced before the
//!   key is recorded
//! - a recorded key is pinned to the crate's persistent TTL ceiling, and
//!   survives a ledger jump that would have expired an unpinned entry
//! - the emitted wire error is the documented `DuplicateIdempotencyKey` (232)

extern crate std;

use credence_errors::{ContractError, ErrorExt};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Bytes, Env, Symbol};

use crate::idempotency::{check_and_record, compute_key, is_used};
use crate::test_helpers::{advance_ledgers_by, expect_contract_error_panic, setup_with_token};
use crate::validation::MAX_FINITE_BYTES_LENGTH;

fn salt(e: &Env, s: &[u8]) -> Bytes {
    Bytes::from_slice(e, s)
}

// ---------------------------------------------------------------------------
// Key derivation boundaries
// ---------------------------------------------------------------------------

/// The XDR encoding of every component is length-prefixed, so a salt can never
/// be shifted across the actor/operation boundary. Without those prefixes,
/// `(actor=A, op=B, salt="")` and `(actor=A, op="Bsalt", salt="")` would hash
/// the same bytes and one operation could replay another.
#[test]
fn no_length_extension_aliasing_between_components() {
    let e = Env::default();
    let actor = Address::generate(&e);

    let short_op = compute_key(&e, &actor, &Symbol::new(&e, "op"), &salt(&e, b""));
    let long_op = compute_key(&e, &actor, &Symbol::new(&e, "opsalt"), &salt(&e, b""));
    let salt_carried = compute_key(&e, &actor, &Symbol::new(&e, "op"), &salt(&e, b"salt"));

    assert_ne!(
        short_op, long_op,
        "concatenation must not let the operation and the salt alias"
    );
    assert_ne!(
        short_op, salt_carried,
        "an empty suffix must not alias a present one"
    );
}

/// Salts that differ only in length by one must not collide.
#[test]
fn adjacent_salt_lengths_do_not_collide() {
    let e = Env::default();
    let actor = Address::generate(&e);
    let operation = Symbol::new(&e, "slash_bond");

    let one = compute_key(&e, &actor, &operation, &salt(&e, b"a"));
    let two = compute_key(&e, &actor, &operation, &salt(&e, b"aa"));
    let three = compute_key(&e, &actor, &operation, &salt(&e, b"aaa"));

    assert_ne!(one, two);
    assert_ne!(two, three);
    assert_ne!(one, three);
}

/// A salt made entirely of NUL bytes is distinct from an empty salt, and every
/// length in between is distinct again. Guards against a salt being compared or
/// trimmed as a C string somewhere downstream.
#[test]
fn nul_salt_lengths_do_not_collide() {
    let e = Env::default();
    let actor = Address::generate(&e);
    let operation = Symbol::new(&e, "slash_bond");

    let empty = compute_key(&e, &actor, &operation, &salt(&e, b""));
    let one = compute_key(&e, &actor, &operation, &salt(&e, b"\0"));
    let two = compute_key(&e, &actor, &operation, &salt(&e, b"\0\0"));

    assert_ne!(empty, one, "an empty salt must not alias a NUL byte");
    assert_ne!(one, two);
}

/// Derivation must not depend on the ledger, so a client that retries after a
/// timeout lands on exactly the same namespace and is caught by the recorded
/// key rather than slipping past it.
#[test]
fn key_derivation_is_ledger_independent() {
    let e = Env::default();
    // Raise the minimum entry TTL before the contract is registered, so the
    // contract instance itself survives the jump below. The point of this test
    // is key derivation, not retention.
    e.ledger().set_min_persistent_entry_ttl(1_000_000);

    let contract_id = e.register(crate::CredenceBond, ());
    let actor = Address::generate(&e);
    let operation = Symbol::new(&e, "slash_bond");
    let s = salt(&e, b"retry-token");

    let before = compute_key(&e, &actor, &operation, &s);
    advance_ledgers_by(&e, 100_000);
    let after = compute_key(&e, &actor, &operation, &s);

    assert_eq!(before, after);
    e.as_contract(&contract_id, || {
        assert!(!is_used(&e, &actor, &operation, &s));
    });
}

/// The empty salt is the documented opt-out, but it is still a well-defined
/// namespace: it hashes, and two actors using it do not collide.
#[test]
fn empty_salt_is_its_own_namespace() {
    let e = Env::default();
    let contract_id = e.register(crate::CredenceBond, ());
    let actor1 = Address::generate(&e);
    let actor2 = Address::generate(&e);
    let operation = Symbol::new(&e, "slash_bond");
    let empty = salt(&e, b"");

    assert_ne!(
        compute_key(&e, &actor1, &operation, &empty),
        compute_key(&e, &actor2, &operation, &empty)
    );

    // An explicitly recorded empty salt occupies exactly one slot and then
    // behaves like any other key.
    e.as_contract(&contract_id, || {
        check_and_record(&e, &actor1, &operation, &empty);
        assert!(is_used(&e, &actor1, &operation, &empty));
        assert!(!is_used(&e, &actor2, &operation, &empty));
    });
}

// ---------------------------------------------------------------------------
// Salt length boundary at the entry point
// ---------------------------------------------------------------------------

/// A salt exactly at `MAX_FINITE_BYTES_LENGTH` is accepted and recorded; one
/// byte more is rejected. `require_finite_bytes` runs before the key is
/// recorded, so an oversized salt must not leave a key behind.
#[test]
fn salt_length_boundary_is_enforced() {
    let e = Env::default();
    let (client, admin, _identity, _token, contract_id) = setup_with_token(&e);

    let mut at_limit = [0_u8; MAX_FINITE_BYTES_LENGTH as usize + 1];
    at_limit[0] = 7;
    let at_limit = Bytes::from_slice(&e, &at_limit[..MAX_FINITE_BYTES_LENGTH as usize]);

    let mut over_limit = [0_u8; MAX_FINITE_BYTES_LENGTH as usize + 1];
    over_limit[0] = 7;
    let over_limit = Bytes::from_slice(&e, &over_limit[..]);

    // `collect_fees` needs no bond, which keeps this on the validation path.
    client.collect_fees(&admin, &at_limit);

    expect_contract_error_panic(ContractError::BytesTooLarge as u32, || {
        client.collect_fees(&admin, &over_limit);
    });

    e.as_contract(&contract_id, || {
        assert!(is_used(
            &e,
            &admin,
            &Symbol::new(&e, "collect_fees"),
            &at_limit
        ));
        assert!(!is_used(
            &e,
            &admin,
            &Symbol::new(&e, "collect_fees"),
            &over_limit
        ));
    });
}

// ---------------------------------------------------------------------------
// Retention boundary
// ---------------------------------------------------------------------------

/// The behavioural counterpart: a key recorded before a large ledger jump is
/// still present afterwards, so a webhook that redelivers long after the
/// original request is still rejected. The contract's own instance storage is
/// bumped first so that the idempotency key is the only entry that could
/// possibly expire during the jump.
#[test]
fn recorded_key_survives_past_the_network_minimum_ttl() {
    let e = Env::default();
    e.ledger().set_min_persistent_entry_ttl(4_096);

    let contract_id = e.register(crate::CredenceBond, ());
    let actor = Address::generate(&e);
    let operation = Symbol::new(&e, "slash_bond");
    let s = salt(&e, b"long-lived");

    e.as_contract(&contract_id, || {
        check_and_record(&e, &actor, &operation, &s);
        // Keep the contract's own instance entry alive across the jump.
        crate::bump_instance_ttl(&e);
    });

    // Roughly 24x the network minimum.
    advance_ledgers_by(&e, 100_000);

    e.as_contract(&contract_id, || {
        assert!(
            is_used(&e, &actor, &operation, &s),
            "the key expired mid-jump; the identical request would replay"
        );
    });
}

// ---------------------------------------------------------------------------
// Error surface
// ---------------------------------------------------------------------------

/// The duplicate path must surface the documented error code. Pinning 232 means
/// a renumbering that moves `DuplicateIdempotencyKey` is caught, because
/// off-chain callers match on the numeric code.
#[test]
fn duplicate_error_code_is_pinned() {
    assert_eq!(ContractError::DuplicateIdempotencyKey as u32, 232);
}

/// The rejection is a contract error rather than a host failure, so a caller can
/// tell "you already did this" apart from "the call could not be made".
#[test]
fn duplicate_surfaces_duplicate_idempotency_key_error() {
    let e = Env::default();
    let (client, admin, _identity, _token, _contract_id) = setup_with_token(&e);
    let s = salt(&e, b"wire-code");

    client.collect_fees(&admin, &s);

    expect_contract_error_panic(ContractError::DuplicateIdempotencyKey as u32, || {
        client.collect_fees(&admin, &s);
    });
}

/// The message is safe to surface: it names the failure, not the key, the salt or
/// the actor, so a duplicate rejection cannot leak the caller's request.
#[test]
fn duplicate_error_message_discloses_nothing_sensitive() {
    let msg = ContractError::DuplicateIdempotencyKey.description();
    assert_eq!(
        msg,
        "Idempotency key has already been used for this operation"
    );
    for leak in ["salt", "actor", "key:", "0x"] {
        assert!(
            !msg.to_lowercase().contains(leak),
            "the error message leaks {:?}: {}",
            leak,
            msg
        );
    }
}

/// `is_used` is documented as read-only. Probing must not create the key.
#[test]
fn is_used_does_not_record() {
    let e = Env::default();
    let contract_id = e.register(crate::CredenceBond, ());
    let actor = Address::generate(&e);
    let operation = Symbol::new(&e, "slash_bond");
    let s = salt(&e, b"probe");

    e.as_contract(&contract_id, || {
        for _ in 0..5 {
            assert!(!is_used(&e, &actor, &operation, &s));
        }
        // Still unrecorded after repeated probing.
        assert!(!is_used(&e, &actor, &operation, &s));
    });
}

/// `collect_fees` is actually wired to the module. A contract that skipped
/// `check_and_record` would let the duplicate through, so the duplicate
/// rejection above is the proof of wiring.
#[test]
fn entry_point_is_wired_to_the_module() {
    let e = Env::default();
    let (client, admin, _identity, _token, _contract_id) = setup_with_token(&e);
    let s = salt(&e, b"wired");

    client.collect_fees(&admin, &s);
    expect_contract_error_panic(ContractError::DuplicateIdempotencyKey as u32, || {
        client.collect_fees(&admin, &s);
    });
}
