//! Boundary and recovery coverage for `contracts/credence_errors/src/lease.rs` (#1365).
//!
//! The lease guards are pure, fail-closed predicates: they either return or
//! panic with a typed [`ContractError`](crate::ContractError). These tests pin
//! the bit-mask and expiry boundaries — including the `scope = 0`, `op = 0`,
//! `expires_at = 0` and `u64::MAX` edges — and prove recovery: a rejected guard
//! leaves no state behind, so the next valid call still succeeds.

extern crate std;

use crate::lease::{lease_op, require_matching_lease_scope, require_no_expired_lease, Lease};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn lease(e: &Env, scope: u32, expires_at: u64) -> Lease {
    Lease {
        signer: Address::generate(e),
        scope,
        expires_at,
    }
}

fn at(e: &Env, ts: u64) {
    e.ledger().with_mut(|li| li.timestamp = ts);
}

// ---------------------------------------------------------------------------
// Scope guard — bitmask boundaries
// ---------------------------------------------------------------------------

#[test]
fn scope_op_zero_is_always_authorised() {
    // Requesting no operation requires no bit, even from an empty lease.
    let e = Env::default();
    require_matching_lease_scope(&e, &lease(&e, 0, 0), 0);
    require_matching_lease_scope(&e, &lease(&e, lease_op::READ, 0), 0);
}

#[test]
fn scope_all_covers_every_defined_op_and_combination() {
    let e = Env::default();
    let all = lease(&e, lease_op::ALL, u64::MAX);
    for op in [
        lease_op::READ,
        lease_op::WRITE,
        lease_op::RENEW,
        lease_op::TRANSFER,
        lease_op::ALL,
    ] {
        require_matching_lease_scope(&e, &all, op);
    }
    require_matching_lease_scope(
        &e,
        &all,
        lease_op::READ | lease_op::WRITE | lease_op::RENEW | lease_op::TRANSFER,
    );
}

#[test]
fn scope_all_mask_is_the_union_of_defined_bits() {
    assert_eq!(
        lease_op::ALL,
        lease_op::READ | lease_op::WRITE | lease_op::RENEW | lease_op::TRANSFER
    );
}

#[test]
fn scope_superset_passes_and_a_single_covered_bit_is_enough() {
    let e = Env::default();
    // WRITE|RENEW covers a WRITE-only request.
    require_matching_lease_scope(
        &e,
        &lease(&e, lease_op::WRITE | lease_op::RENEW, u64::MAX),
        lease_op::WRITE,
    );
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #121)")]
fn scope_empty_lease_rejects_read() {
    let e = Env::default();
    require_matching_lease_scope(&e, &lease(&e, 0, u64::MAX), lease_op::READ);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #121)")]
fn scope_missing_a_single_required_bit_panics() {
    let e = Env::default();
    require_matching_lease_scope(
        &e,
        &lease(&e, lease_op::READ | lease_op::WRITE, u64::MAX),
        lease_op::WRITE | lease_op::RENEW,
    );
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #121)")]
fn scope_unknown_requested_bit_is_rejected() {
    let e = Env::default();
    // Lease grants every defined bit, but the request also asks for an undefined bit.
    require_matching_lease_scope(
        &e,
        &lease(&e, lease_op::ALL, u64::MAX),
        lease_op::ALL | (1 << 31),
    );
}

#[test]
fn scope_extra_undefined_lease_bits_are_harmless() {
    // Coverage is per requested bit; unrelated bits in the lease do not hurt.
    let e = Env::default();
    require_matching_lease_scope(
        &e,
        &lease(&e, lease_op::READ | (1 << 30), u64::MAX),
        lease_op::READ,
    );
}

// ---------------------------------------------------------------------------
// Expiry guard — timestamp boundaries
// ---------------------------------------------------------------------------

#[test]
fn expiry_allows_every_second_before_the_cliff() {
    let e = Env::default();
    at(&e, 1_000_000);
    require_no_expired_lease(&e, &lease(&e, lease_op::ALL, 1_000_001));
    require_no_expired_lease(&e, &lease(&e, lease_op::ALL, u64::MAX));
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #122)")]
fn expiry_rejects_the_exact_boundary() {
    // now == expires_at is the hard cliff.
    let e = Env::default();
    at(&e, 1_000_000);
    require_no_expired_lease(&e, &lease(&e, lease_op::ALL, 1_000_000));
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #122)")]
fn expiry_rejects_zero_expiry() {
    // expires_at = 0 is already past for any timestamp >= 0.
    let e = Env::default();
    require_no_expired_lease(&e, &lease(&e, lease_op::ALL, 0));
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #122)")]
fn expiry_rejects_one_second_past_the_cliff() {
    let e = Env::default();
    at(&e, 1_000_001);
    require_no_expired_lease(&e, &lease(&e, lease_op::ALL, 1_000_000));
}

// ---------------------------------------------------------------------------
// Recovery — pure guards leave no residue; combined gate fails closed
// ---------------------------------------------------------------------------

#[test]
fn recovery_after_scope_rejection_a_valid_call_still_succeeds() {
    let e = Env::default();
    let bad = lease(&e, lease_op::READ, u64::MAX);
    let good = lease(&e, lease_op::WRITE, u64::MAX);

    let rejected = catch_unwind(AssertUnwindSafe(|| {
        require_matching_lease_scope(&e, &bad, lease_op::WRITE)
    }));
    assert!(rejected.is_err(), "a mismatched scope must panic");

    // The rejection left no state behind: a fresh, valid guard still passes.
    require_matching_lease_scope(&e, &good, lease_op::WRITE);
}

#[test]
fn recovery_after_expiry_rejection_a_fresh_lease_still_succeeds() {
    let e = Env::default();
    at(&e, 500);
    let expired = lease(&e, lease_op::ALL, 400);
    let fresh = lease(&e, lease_op::ALL, 900);

    let rejected = catch_unwind(AssertUnwindSafe(|| require_no_expired_lease(&e, &expired)));
    assert!(rejected.is_err(), "an expired lease must panic");

    require_no_expired_lease(&e, &fresh);
}

#[test]
fn combined_guards_are_deterministic_and_fail_closed() {
    let e = Env::default();
    at(&e, 10_000);
    let ok = lease(&e, lease_op::READ | lease_op::WRITE, 20_000);

    // Deterministic: repeated evaluation gives the same passing outcome.
    for _ in 0..3 {
        require_matching_lease_scope(&e, &ok, lease_op::WRITE);
        require_no_expired_lease(&e, &ok);
    }

    // The combined gate still fails closed once the lease expires.
    let expired = lease(&e, lease_op::ALL, 9_999);
    let rejected = catch_unwind(AssertUnwindSafe(|| {
        require_matching_lease_scope(&e, &expired, lease_op::READ);
        require_no_expired_lease(&e, &expired);
    }));
    assert!(
        rejected.is_err(),
        "an expired lease must fail the combined gate"
    );
}
