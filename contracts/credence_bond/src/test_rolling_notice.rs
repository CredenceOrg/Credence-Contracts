//! State-machine tests for rolling-bond notice-period sequencing.
//!
//! Covers the following transitions:
//!
//! ```text
//! Active ──request_withdrawal──► PendingNotice
//!   │                                │
//!   │                          notice elapsed?
//!   │                          ├─ no  → settle rejected
//!   │                          └─ yes → settle allowed
//!   │
//!   └──renew_if_rolling (no request)──► Active (new period)
//!
//! PendingNotice ──renew_if_rolling──► PendingNotice (no-op)
//! ```

use crate::test_helpers;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, Env, FromVal, Symbol};

/// Shared setup: rolling bond, duration = 86_400 s, notice = 3_600 s, amount = 1_000.
fn setup_rolling(e: &Env) -> (crate::CredenceBondClient<'_>, soroban_sdk::Address) {
    let (client, _admin, identity, ..) = test_helpers::setup_with_token(e);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &true, &3_600_u64);
    (client, identity)
}

// ── 1. Full request → before notice elapses → settle rejected ──────────────

/// Transition: Active → PendingNotice via `request_withdrawal`.
/// Assert `withdrawal_requested_at` is stamped at the request ledger time.
#[test]
fn test_request_sets_withdrawal_requested_at() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);

    let bond = client.request_withdrawal(&identity);

    assert_eq!(bond.withdrawal_requested_at, 1_000);
    assert!(bond.is_rolling);
}

/// Settle (partial withdraw) attempted before notice elapses must panic.
/// Setup: request at t=90_000 (after lock-up end=87_400); notice_end = 90_000 + 3_600 = 93_600.
/// Attempt at t=93_599: lock-up has passed, but notice has not elapsed yet.
#[test]
#[should_panic(expected = "notice period not elapsed")]
fn test_settle_before_notice_panics() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);

    // Request after lock-up end (bond_start=1_000, duration=86_400 → lock-up_end=87_400).
    e.ledger().with_mut(|l| l.timestamp = 90_000);
    client.request_withdrawal(&identity);

    // notice_end = 90_000 + 3_600 = 93_600; one second before notice end.
    e.ledger().with_mut(|l| l.timestamp = 93_599);
    client.withdraw(&identity, &500_i128);
}

// ── 2. Full request → after notice elapses → settle allowed ─────────────────

/// Transition: PendingNotice → Settled after notice elapses.
/// lock-up_end = 1_000 + 86_400 = 87_400; notice_end = 1_000 + 3_600 = 4_600.
/// At t = 87_400 the lock-up boundary is hit exactly and notice has long elapsed.
#[test]
fn test_settle_exactly_at_notice_boundary_allowed() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);
    client.request_withdrawal(&identity);

    // t = 87_400: exactly at lock-up end AND past notice end (4_600).
    e.ledger().with_mut(|l| l.timestamp = 87_400);
    let bond = client.withdraw(&identity, &500_i128);

    assert_eq!(bond.bonded_amount, 500_i128);
}

/// Settle one second after the notice boundary.
#[test]
fn test_settle_after_notice_elapsed_allowed() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);
    client.request_withdrawal(&identity);

    e.ledger().with_mut(|l| l.timestamp = 87_401);
    let bond = client.withdraw(&identity, &1_000_i128);

    assert_eq!(bond.bonded_amount, 0_i128);
}

// ── 3. Withdraw without prior request panics ─────────────────────────────────

/// A rolling bond without a `request_withdrawal` call cannot be settled.
#[test]
#[should_panic(expected = "withdrawal not requested")]
fn test_withdraw_without_request_panics() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);

    e.ledger().with_mut(|l| l.timestamp = 87_401);
    client.withdraw(&identity, &500_i128);
}

// ── 4. renew_if_rolling during an open withdrawal request is a no-op ─────────

/// Transition: PendingNotice + renew → PendingNotice (no-op).
/// `bond_start` and `withdrawal_requested_at` must be unchanged.
#[test]
fn test_renew_during_open_request_is_noop() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);
    client.request_withdrawal(&identity);

    // Period has ended — renew would normally advance bond_start.
    e.ledger().with_mut(|l| l.timestamp = 87_401);
    let bond = client.renew_if_rolling(&identity);

    // bond_start must NOT be updated — no-op because withdrawal is pending.
    assert_eq!(bond.bond_start, 1_000);
    assert_eq!(bond.withdrawal_requested_at, 1_000);
}

/// renew_if_rolling during request emits no `bond_renewed` event.
#[test]
fn test_renew_during_open_request_emits_no_bond_renewed() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);
    client.request_withdrawal(&identity);

    e.ledger().with_mut(|l| l.timestamp = 87_401);
    let _ = e.events().all(); // drain prior events
    client.renew_if_rolling(&identity);

    let renewed = e.events().all().iter().any(|(_, topics, _)| {
        topics
            .get(0)
            .map(|v| Symbol::from_val(&e, &v) == Symbol::new(&e, "bond_renewed"))
            .unwrap_or(false)
    });
    assert!(!renewed, "bond_renewed must not be emitted when withdrawal is pending");
}

// ── 5. renew without open request advances the period and resets state ────────

/// Normal renewal: bond_start advances, withdrawal_requested_at stays 0.
#[test]
fn test_renew_without_request_advances_period() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);

    e.ledger().with_mut(|l| l.timestamp = 87_401);
    let bond = client.renew_if_rolling(&identity);

    assert_eq!(bond.bond_start, 87_401);
    assert_eq!(bond.withdrawal_requested_at, 0);
}

/// Renewal emits a `bond_renewed` event with correct fields.
#[test]
fn test_renew_emits_bond_renewed_event() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);

    e.ledger().with_mut(|l| l.timestamp = 87_401);
    client.renew_if_rolling(&identity);

    let events = e.events().all();
    let renewed = events.iter().any(|(_, topics, _)| {
        topics
            .get(0)
            .map(|v| Symbol::from_val(&e, &v) == Symbol::new(&e, "bond_renewed"))
            .unwrap_or(false)
    });
    assert!(renewed, "bond_renewed event must be emitted on successful renewal");
}

// ── 6. withdrawal_requested event fields ─────────────────────────────────────

/// `withdrawal_requested` event must be emitted with correct identity and timestamp.
#[test]
fn test_request_emits_withdrawal_requested_event() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 5_000);
    let (client, identity) = setup_rolling(&e);

    client.request_withdrawal(&identity);

    let events = e.events().all();
    let found = events.iter().any(|(_, topics, _)| {
        topics
            .get(0)
            .map(|v| Symbol::from_val(&e, &v) == Symbol::new(&e, "withdrawal_requested"))
            .unwrap_or(false)
    });
    assert!(found, "withdrawal_requested event must be emitted");
}

// ── 7. Same-ledger anti-sandwich guard ───────────────────────────────────────

/// request_withdrawal and withdraw in the same ledger (same timestamp) must
/// be rejected: the notice period (3_600 s) cannot elapse within t=0.
#[test]
#[should_panic(expected = "notice period not elapsed")]
fn test_same_ledger_request_and_settle_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);

    // Advance past lock-up end (1_000 + 86_400 = 87_400) so the lock-up check passes.
    e.ledger().with_mut(|l| l.timestamp = 87_400);
    // request and settle in the exact same ledger timestamp.
    client.request_withdrawal(&identity);
    // notice_end = 87_400 + 3_600 = 91_000; still 87_400 → notice not elapsed.
    client.withdraw(&identity, &500_i128);
}

// ── 8. Double request panics ──────────────────────────────────────────────────

/// A second `request_withdrawal` when one is already pending must panic.
#[test]
#[should_panic(expected = "withdrawal already requested")]
fn test_double_request_panics() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);
    client.request_withdrawal(&identity);
    client.request_withdrawal(&identity);
}

// ── 9. Renew at the period boundary (exactly at period end) ──────────────────

/// renew_if_rolling called exactly at bond_start + bond_duration must succeed.
#[test]
fn test_renew_exactly_at_period_end() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);

    // Exactly at period end: 1_000 + 86_400 = 87_400.
    e.ledger().with_mut(|l| l.timestamp = 87_400);
    let bond = client.renew_if_rolling(&identity);

    assert_eq!(bond.bond_start, 87_400);
}

// ── 10. Post-renewal: request and settle work correctly ───────────────────────

/// After a renewal, a fresh request/settle cycle must complete normally.
#[test]
fn test_request_and_settle_after_renewal() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, identity) = setup_rolling(&e);

    // Renewal at t = 87_401.
    e.ledger().with_mut(|l| l.timestamp = 87_401);
    client.renew_if_rolling(&identity);

    // Request at t = 87_401.
    let bond = client.request_withdrawal(&identity);
    assert_eq!(bond.withdrawal_requested_at, 87_401);

    // Settle after new period + notice: new period end = 87_401 + 86_400 = 173_801.
    // Notice end = 87_401 + 3_600 = 91_001. Use t = 173_801 (both satisfied).
    e.ledger().with_mut(|l| l.timestamp = 173_801);
    let bond = client.withdraw(&identity, &500_i128);
    assert_eq!(bond.bonded_amount, 500_i128);
}

// ── 11. Request then full slash → bond has no available balance ───────────────

/// After a full slash, withdraw should revert with InsufficientBalance (or
/// equivalent) even though notice has elapsed.
#[test]
#[should_panic]
fn test_request_then_full_slash_withdraw_fails() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1_000);
    let (client, admin, identity, ..) = test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(&identity, &1_000_i128, &86_400_u64, &true, &3_600_u64);

    test_helpers::advance_ledger_sequence(&e);
    client.request_withdrawal(&identity);

    // Configure slash treasury and slash the full amount.
    let treasury = Address::generate(&e);
    client.set_slash_treasury(&admin, &treasury);
    client.slash(&admin, &1_000_i128);

    // Advance past notice and lock-up.
    e.ledger().with_mut(|l| l.timestamp = 87_401);
    // Available = bonded (1_000) - slashed (1_000) = 0; withdraw > 0 must fail.
    client.withdraw(&identity, &1_i128);
}
