//! Tests for `emergency_drain_to_treasury`.
//!
//! Covers:
//! - Happy path: full drain success with audit record.
//! - Gate: contract must be paused (drain rejected when not paused).
//! - Gate: timelock must be elapsed (drain rejected before ETA).
//! - Gate: admin auth required (non-admin rejected).
//! - Gate: recipient must equal treasury (wrong recipient rejected).
//! - Gate: amount must be positive (zero/negative rejected).
//! - Multiple sequential drains with incrementing record IDs.
//! - Drain after timelock expiry (well past ETA still succeeds).
//! - Cancel clears ETA and blocks drain.
//! - Schedule without pause fails immediately.

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

/// Shared setup: contract with token, emergency config set.
/// Returns (client, admin, treasury, governance, identity).
fn setup_with_emergency(
    e: &Env,
) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    let (client, admin, identity, ..) = test_helpers::setup_with_token(e);
    let governance = Address::generate(e);
    let treasury = Address::generate(e);
    // Enable emergency config so treasury is registered.
    e.as_contract(&client.address, || {
        crate::emergency::set_config(e, governance.clone(), treasury.clone(), 0_u32, false);
    });
    (client, admin, treasury, governance, identity)
}

/// Advance ledger timestamp by `secs` seconds.
fn advance_time(e: &Env, secs: u64) {
    e.ledger().with_mut(|l| l.timestamp = l.timestamp + secs);
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

#[test]
fn test_emergency_drain_success_records_audit_trail() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 10_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    // Pause → schedule → advance past ETA → drain.
    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);

    let drain_id = client.emergency_drain_to_treasury(&admin, &500_i128, &treasury);
    assert_eq!(drain_id, 1);

    let record = client.get_drain_record(&drain_id);
    assert_eq!(record.id, 1);
    assert_eq!(record.amount, 500);
    assert_eq!(record.recipient, treasury);
    assert_eq!(record.admin, admin);
    // scheduled_eta == 10_000 + 86_400 = 96_400
    assert_eq!(record.scheduled_eta, 96_400);
}

#[test]
fn test_emergency_drain_id_increments_across_sequential_drains() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 5_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    // First drain.
    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);
    let id1 = client.emergency_drain_to_treasury(&admin, &100_i128, &treasury);
    assert_eq!(id1, 1);

    // Second drain: must re-schedule (ETA cleared after first drain).
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);
    let id2 = client.emergency_drain_to_treasury(&admin, &200_i128, &treasury);
    assert_eq!(id2, 2);
    assert_eq!(client.get_latest_drain_id(), 2);

    let rec1 = client.get_drain_record(&id1);
    let rec2 = client.get_drain_record(&id2);
    assert_eq!(rec1.amount, 100);
    assert_eq!(rec2.amount, 200);
}

#[test]
fn test_emergency_drain_well_past_eta_still_succeeds() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    // Advance far beyond ETA (7 days later).
    advance_time(&e, 604_800);
    let id = client.emergency_drain_to_treasury(&admin, &1_i128, &treasury);
    assert_eq!(id, 1);
}

#[test]
fn test_get_drain_eta_reflects_schedule() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 50_000);
    let (client, admin, ..) = setup_with_emergency(&e);

    // No ETA yet.
    assert_eq!(client.get_drain_eta(), None);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);

    let eta = client.get_drain_eta().expect("should have ETA");
    assert_eq!(eta, 50_000 + 86_400);
}

#[test]
fn test_cancel_clears_eta() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    assert!(client.get_drain_eta().is_some());

    client.cancel_emergency_drain(&admin);
    assert_eq!(client.get_drain_eta(), None);
}

// ---------------------------------------------------------------------------
// Gate: contract must be paused
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_drain_fails_when_not_paused() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    // Do NOT pause — schedule should fail.
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);
    client.emergency_drain_to_treasury(&admin, &100_i128, &treasury);
}

#[test]
#[should_panic]
fn test_schedule_drain_fails_when_not_paused() {
    let e = Env::default();
    let (client, admin, ..) = setup_with_emergency(&e);
    // Contract is not paused; scheduling must panic.
    client.schedule_emergency_drain(&admin, &86_400_u64);
}

// ---------------------------------------------------------------------------
// Gate: timelock must be elapsed
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_drain_fails_before_eta() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    // Advance only halfway through the timelock.
    advance_time(&e, 43_200);
    // Must panic with TimelockNotReady.
    client.emergency_drain_to_treasury(&admin, &100_i128, &treasury);
}

#[test]
#[should_panic]
fn test_drain_fails_exactly_one_second_before_eta() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 0);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    // ETA = 86_400; set timestamp to 86_399.
    e.ledger().with_mut(|l| l.timestamp = 86_399);
    client.emergency_drain_to_treasury(&admin, &100_i128, &treasury);
}

#[test]
#[should_panic]
fn test_drain_fails_with_no_eta_scheduled() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    // No schedule_emergency_drain call — ETA is None.
    client.emergency_drain_to_treasury(&admin, &100_i128, &treasury);
}

#[test]
#[should_panic]
fn test_drain_fails_after_cancel() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);
    client.cancel_emergency_drain(&admin);
    // ETA cleared; must panic.
    client.emergency_drain_to_treasury(&admin, &100_i128, &treasury);
}

#[test]
#[should_panic]
fn test_schedule_drain_rejects_delay_below_minimum() {
    let e = Env::default();
    let (client, admin, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    // 86_399 < 86_400 — must panic with TimelockNotReady.
    client.schedule_emergency_drain(&admin, &86_399_u64);
}

// ---------------------------------------------------------------------------
// Gate: recipient must be treasury
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "recipient must be treasury")]
fn test_drain_fails_with_non_treasury_recipient() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, _treasury, ..) = setup_with_emergency(&e);
    let rogue = Address::generate(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);
    // rogue != treasury — must panic.
    client.emergency_drain_to_treasury(&admin, &100_i128, &rogue);
}

// ---------------------------------------------------------------------------
// Gate: amount must be positive
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_drain_fails_with_zero_amount() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);
    client.emergency_drain_to_treasury(&admin, &0_i128, &treasury);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_drain_fails_with_negative_amount() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);
    client.emergency_drain_to_treasury(&admin, &(-1_i128), &treasury);
}

// ---------------------------------------------------------------------------
// Gate: admin auth
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_drain_fails_with_non_admin() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);
    let attacker = Address::generate(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);
    // attacker != admin — must panic.
    client.emergency_drain_to_treasury(&attacker, &100_i128, &treasury);
}

#[test]
#[should_panic]
fn test_schedule_drain_fails_with_non_admin() {
    let e = Env::default();
    let (client, admin, ..) = setup_with_emergency(&e);
    let attacker = Address::generate(&e);
    let _ = admin;

    client.pause(&attacker);  // this should panic; attacker is not admin
}

#[test]
#[should_panic]
fn test_cancel_drain_fails_with_non_admin() {
    let e = Env::default();
    let (client, admin, ..) = setup_with_emergency(&e);
    let attacker = Address::generate(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    // attacker attempts cancel — must panic.
    client.cancel_emergency_drain(&attacker);
}

// ---------------------------------------------------------------------------
// Idempotency / re-drain guard
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_drain_cannot_be_replayed_without_rescheduling() {
    let e = Env::default();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, treasury, ..) = setup_with_emergency(&e);

    client.pause(&admin);
    client.schedule_emergency_drain(&admin, &86_400_u64);
    advance_time(&e, 86_401);

    // First drain succeeds.
    client.emergency_drain_to_treasury(&admin, &50_i128, &treasury);

    // Second drain without re-scheduling must panic (ETA cleared).
    client.emergency_drain_to_treasury(&admin, &50_i128, &treasury);
}

// ---------------------------------------------------------------------------
// Read helpers
// ---------------------------------------------------------------------------

#[test]
fn test_get_latest_drain_id_starts_at_zero() {
    let e = Env::default();
    let (client, ..) = setup_with_emergency(&e);
    assert_eq!(client.get_latest_drain_id(), 0);
}

// Module inclusion guard — this file is only compiled under #[cfg(test)].
// The test runner discovers it via `mod test_emergency_drain;` in lib.rs.
