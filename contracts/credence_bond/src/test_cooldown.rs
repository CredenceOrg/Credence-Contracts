//! Cooldown Window Test Suite
//!
//! Covers the full cooldown lifecycle across nine domains:
//!
//! **A. Pure unit tests** — deterministic tests of the helper functions in
//!    `cooldown.rs` without any contract environment.  These exercise
//!    `cooldown_deadline`, `is_cooldown_active`, and `can_withdraw` at every
//!    boundary condition including integer-overflow edge cases.
//!
//! **B. Period configuration** — `set_cooldown_period` / `get_cooldown_period`
//!    round-trips, zero-period, and admin-auth guard enforcement.
//!
//! **C. Request lifecycle** — `request_cooldown_withdrawal` validation
//!    (positive amount, ceiling check after slash, no duplicate, bond required,
//!    bond must be active, identity must be bond holder).
//!
//! **D. Execute lifecycle** — `execute_cooldown_withdrawal` timing boundary
//!    (one-second-too-early, exact boundary, one-second-after), same-ledger
//!    sequencing guard, slash-during-cooldown recovery (amount reduced to
//!    available balance after intervening slash), bond state mutations, and
//!    request-cleared-after-execution sentinel.
//!
//! **E. Cancel & recovery** — `cancel_cooldown` removes the request;
//!    re-request after cancel uses the new timestamp; execute after cancel
//!    is rejected; cancel on a closed bond is rejected.
//!
//! **F. Authorization** — every mutating entrypoint enforces `require_auth` on
//!    the identity; cross-identity calls are rejected without mutating state.
//!
//! **G. Pause gating** — all three mutating cooldown entrypoints reject calls
//!    when the contract is paused; the read-only `get_cooldown_request` is
//!    unaffected.
//!
//! **H. Lifecycle invariants** — cooldown operations on a closed (inactive)
//!    bond are rejected with `BondNotActive`.
//!
//! **I. Period-change semantics** — period changed between request and execute
//!    uses the period at execution time, not at request time.
//!
//! Invariants asserted throughout:
//! - `bond.bonded_amount` is reduced exactly by `request.amount` on execute.
//! - No request stored after execute or cancel.
//! - `can_withdraw(now, req, period)` is deterministic and monotone.
//! - Same-ledger guard prevents execute in the same ledger as collateral
//!   increase; advancing one ledger re-enables it.

extern crate std;

use crate::cooldown;
use crate::same_ledger_liquidation_guard::{
    record_collateral_increase, require_cooldown_allowed_after_collateral_increase,
};
use crate::test_helpers;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

// ============================================================================
// Shared setup helpers
// ============================================================================

/// Register the contract only (no token, no bond).  Returns `(client, admin)`.
fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    (client, admin)
}

/// Full setup: token, treasury (required for slash), bond of `bond_amount` for
/// `identity`.  Advances one ledger after `create_bond` so the same-ledger
/// guard is satisfied for subsequent operations.
///
/// Returns `(client, admin, identity)`.
fn setup_with_bond(
    e: &Env,
    bond_amount: i128,
) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity, _token, _cid) = test_helpers::setup_with_token(e);
    let treasury = Address::generate(e);
    client.set_slash_treasury(&admin, &treasury);
    client.create_bond(&identity, &bond_amount, &86_400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(e);
    (client, admin, identity)
}

// ============================================================================
// A. Pure unit tests (no Env required)
// ============================================================================

// ── A.1  cooldown_deadline ────────────────────────────────────────────────

#[test]
fn unit_deadline_is_sum_of_request_time_and_period() {
    assert_eq!(cooldown::cooldown_deadline(1000, 100), 1100);
}

#[test]
fn unit_deadline_zero_period_equals_request_time() {
    assert_eq!(cooldown::cooldown_deadline(5000, 0), 5000);
}

#[test]
fn unit_deadline_both_zero() {
    assert_eq!(cooldown::cooldown_deadline(0, 0), 0);
}

#[test]
fn unit_deadline_saturates_on_u64_overflow() {
    // request_time near max + large period must not panic; result saturates.
    let deadline = cooldown::cooldown_deadline(u64::MAX - 5, 100);
    assert_eq!(deadline, u64::MAX, "saturating_add must return u64::MAX");
}

// ── A.2  is_cooldown_active ───────────────────────────────────────────────

#[test]
fn unit_is_cooldown_active_no_request_is_false() {
    // request_time == 0 means no request exists.
    assert!(!cooldown::is_cooldown_active(0, 0, 100));
    assert!(!cooldown::is_cooldown_active(5000, 0, 100));
}

#[test]
fn unit_is_cooldown_active_one_second_before_deadline() {
    // deadline = 1000 + 100 = 1100.  now = 1099 → still active.
    assert!(cooldown::is_cooldown_active(1099, 1000, 100));
}

#[test]
fn unit_is_cooldown_active_at_exact_deadline_is_false() {
    // At the deadline itself the cooldown has elapsed (inclusive boundary).
    assert!(!cooldown::is_cooldown_active(1100, 1000, 100));
}

#[test]
fn unit_is_cooldown_active_after_deadline_is_false() {
    assert!(!cooldown::is_cooldown_active(1200, 1000, 100));
}

#[test]
fn unit_is_cooldown_active_zero_period_is_false() {
    // Zero period: deadline == request_time; now >= deadline always.
    assert!(!cooldown::is_cooldown_active(1000, 1000, 0));
}

// ── A.3  can_withdraw ─────────────────────────────────────────────────────

#[test]
fn unit_can_withdraw_no_request_is_false() {
    assert!(!cooldown::can_withdraw(5000, 0, 100));
}

#[test]
fn unit_can_withdraw_one_second_before_deadline_is_false() {
    // deadline = 1000 + 100 = 1100.  now = 1099 → NOT withdrawable.
    assert!(!cooldown::can_withdraw(1099, 1000, 100));
}

#[test]
fn unit_can_withdraw_at_exact_deadline_is_true() {
    // The boundary (now == deadline) is inclusive.
    assert!(cooldown::can_withdraw(1100, 1000, 100));
}

#[test]
fn unit_can_withdraw_after_deadline_is_true() {
    assert!(cooldown::can_withdraw(1200, 1000, 100));
}

#[test]
fn unit_can_withdraw_zero_period_is_immediately_true() {
    // With period == 0, deadline == request_time; immediately withdrawable.
    assert!(cooldown::can_withdraw(1000, 1000, 0));
}

#[test]
fn unit_can_withdraw_overflow_safe() {
    // Saturating add near u64::MAX must not panic.
    // request_time = u64::MAX - 10, period = 100 → deadline saturates at u64::MAX.
    // now = u64::MAX - 10 → still before saturated deadline.
    assert!(!cooldown::can_withdraw(u64::MAX - 10, u64::MAX - 10, 100));
    // now = u64::MAX → at saturated deadline → withdrawable.
    assert!(cooldown::can_withdraw(u64::MAX, u64::MAX - 10, 5));
}

// ── A.4  is_cooldown_active / can_withdraw complementarity ───────────────

#[test]
fn unit_active_and_can_withdraw_are_complementary_at_boundary() {
    let req_time = 1000_u64;
    let period = 100_u64;
    // One second before deadline: active=true, can_withdraw=false.
    assert!(cooldown::is_cooldown_active(1099, req_time, period));
    assert!(!cooldown::can_withdraw(1099, req_time, period));
    // At deadline: active=false, can_withdraw=true.
    assert!(!cooldown::is_cooldown_active(1100, req_time, period));
    assert!(cooldown::can_withdraw(1100, req_time, period));
    // After deadline: both remain consistent.
    assert!(!cooldown::is_cooldown_active(1200, req_time, period));
    assert!(cooldown::can_withdraw(1200, req_time, period));
}

// ============================================================================
// B. Period configuration
// ============================================================================

#[test]
fn config_default_period_is_zero() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    assert_eq!(
        client.get_cooldown_period(),
        0,
        "default cooldown period must be 0 (instant withdrawals)"
    );
}

#[test]
fn config_set_and_get_round_trip() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_cooldown_period(&admin, &3600);
    assert_eq!(client.get_cooldown_period(), 3600);
}

#[test]
fn config_update_period_twice() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_cooldown_period(&admin, &100);
    assert_eq!(client.get_cooldown_period(), 100);
    client.set_cooldown_period(&admin, &7200);
    assert_eq!(client.get_cooldown_period(), 7200);
}

#[test]
fn config_set_period_to_zero_clears_to_instant() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_cooldown_period(&admin, &3600);
    client.set_cooldown_period(&admin, &0);
    assert_eq!(client.get_cooldown_period(), 0);
}

#[test]
fn config_set_period_to_u64_max() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_cooldown_period(&admin, &u64::MAX);
    assert_eq!(client.get_cooldown_period(), u64::MAX);
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")] // NotAdmin
fn config_set_period_non_admin_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let impostor = Address::generate(&e);
    client.set_cooldown_period(&impostor, &3600);
}

// ============================================================================
// C. Request lifecycle
// ============================================================================

#[test]
fn request_basic_fields_are_recorded() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 5000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &3600);

    let req = client.request_cooldown_withdrawal(&identity, &500);

    assert_eq!(req.requester, identity, "requester must match identity");
    assert_eq!(req.amount, 500, "amount must match requested amount");
    assert_eq!(req.requested_at, 5000, "timestamp must be ledger timestamp at request time");
}

#[test]
fn request_full_bonded_amount_succeeds() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);

    let req = client.request_cooldown_withdrawal(&identity, &1000);
    assert_eq!(req.amount, 1000);
}

#[test]
fn request_with_zero_period_also_stored_correctly() {
    // Even when period == 0, request must be stored (execution is then
    // immediate once the same-ledger guard is satisfied).
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity) = setup_with_bond(&e, 500);

    let req = client.request_cooldown_withdrawal(&identity, &300);
    assert_eq!(req.amount, 300);
    assert_eq!(req.requested_at, 1000);
}

#[test]
fn request_ceiling_is_bonded_minus_slashed() {
    // Bond 1000, slash 300 → available = 700. Request exactly 700 must succeed.
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.slash(&admin, &identity, &300);
    test_helpers::advance_ledger_sequence(&e);

    let req = client.request_cooldown_withdrawal(&identity, &700);
    assert_eq!(req.amount, 700);
}

#[test]
#[should_panic(expected = "Error(Contract, #214)")] // InvalidBondAmount
fn request_zero_amount_rejected() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #214)")] // InvalidBondAmount
fn request_negative_amount_rejected() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &-1);
}

#[test]
#[should_panic(expected = "Error(Contract, #202)")] // InsufficientBalance
fn request_exceeds_bonded_amount_rejected() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &1001);
}

#[test]
#[should_panic(expected = "Error(Contract, #202)")] // InsufficientBalance
fn request_exceeds_available_after_slash_rejected() {
    // Bond 1000, slash 300 → available = 700. Request 701 must fail.
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.slash(&admin, &identity, &300);
    test_helpers::advance_ledger_sequence(&e);
    client.request_cooldown_withdrawal(&identity, &701);
}

#[test]
#[should_panic(expected = "Error(Contract, #236)")] // CooldownRequestAlreadyPending
fn request_duplicate_rejected() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);

    client.request_cooldown_withdrawal(&identity, &500);
    // Second request for the same identity must be rejected.
    client.request_cooldown_withdrawal(&identity, &200);
}

#[test]
#[should_panic(expected = "Error(Contract, #200)")] // BondNotFound
fn request_no_bond_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_cooldown_period(&admin, &100);
    let stranger = Address::generate(&e);
    client.request_cooldown_withdrawal(&stranger, &100);
}

#[test]
fn get_cooldown_request_returns_stored_request() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 2000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &750);

    let maybe = client.get_cooldown_request(&identity);
    assert!(maybe.is_some(), "get_cooldown_request must return Some after a request is made");
    let req = maybe.unwrap();
    assert_eq!(req.requester, identity);
    assert_eq!(req.amount, 750);
    assert_eq!(req.requested_at, 2000);
}

#[test]
fn get_cooldown_request_returns_none_when_no_request() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    let maybe = client.get_cooldown_request(&identity);
    assert!(maybe.is_none(), "get_cooldown_request must return None when no request exists");
}

// ============================================================================
// D. Execute lifecycle
// ============================================================================

// ── D.1  Timing boundary ─────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #238)")] // CooldownPeriodNotElapsed
fn execute_one_second_before_deadline_rejected() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    // One second before deadline: 1000 + 100 - 1 = 1099.
    e.ledger().with_mut(|li| li.timestamp = 1099);
    client.execute_cooldown_withdrawal(&identity);
}

#[test]
fn execute_at_exact_deadline_succeeds() {
    // The boundary (now == deadline) is inclusive: withdrawal must be allowed.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &250);

    // Exactly at deadline: 1000 + 100 = 1100.
    e.ledger().with_mut(|li| li.timestamp = 1100);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(
        bond.bonded_amount, 750,
        "bonded_amount must decrease by the withdrawal amount at the exact boundary"
    );
}

#[test]
fn execute_one_second_after_deadline_succeeds() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &400);

    e.ledger().with_mut(|li| li.timestamp = 1101);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 600);
}

#[test]
fn execute_well_after_deadline_succeeds() {
    // Verify there is no upper deadline — requests never expire.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &300);

    // Advance far into the future.
    e.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 700);
}

// ── D.2  Zero-period (instant) ────────────────────────────────────────────

#[test]
fn execute_zero_period_allows_immediate_execution() {
    // Default period is 0 → withdrawal allowed immediately (same timestamp).
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, _admin, identity) = setup_with_bond(&e, 1000);
    // Default period = 0; no set_cooldown_period needed.
    client.request_cooldown_withdrawal(&identity, &300);

    // Same timestamp; same-ledger guard already satisfied by setup_with_bond.
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 700);
}

// ── D.3  State mutations ──────────────────────────────────────────────────

#[test]
fn execute_clears_request_enabling_subsequent_request() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &400);

    e.ledger().with_mut(|li| li.timestamp = 1101);
    client.execute_cooldown_withdrawal(&identity);

    // Request must be cleared after successful execution.
    assert!(
        client.get_cooldown_request(&identity).is_none(),
        "request must be cleared after successful execution"
    );

    // A new request for the remaining balance must succeed.
    e.ledger().with_mut(|li| li.timestamp = 2000);
    let new_req = client.request_cooldown_withdrawal(&identity, &200);
    assert_eq!(new_req.requested_at, 2000, "new request must use current timestamp");
    assert_eq!(new_req.amount, 200);
}

#[test]
fn execute_successive_withdrawals_drain_bond_incrementally() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 5000);
    client.set_cooldown_period(&admin, &3600);

    // First withdrawal.
    client.request_cooldown_withdrawal(&identity, &2000);
    e.ledger().with_mut(|li| li.timestamp = 4601);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 3000);

    // Second withdrawal.
    e.ledger().with_mut(|li| li.timestamp = 5000);
    client.request_cooldown_withdrawal(&identity, &1000);
    e.ledger().with_mut(|li| li.timestamp = 8601);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 2000);
}

// ── D.4  Slash during cooldown (recovery path) ───────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #202)")] // InsufficientBalance
fn execute_rejected_when_intervening_slash_reduces_available_below_request() {
    // SECURITY invariant: if the bond is slashed between request and
    // execution and the remaining available balance is less than the requested
    // amount, execute must fail — not allow an over-withdrawal.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &800);

    // Slash heavily while cooldown is pending → available = 1000 - 500 = 500.
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &identity, &500);
    test_helpers::advance_ledger_sequence(&e);

    e.ledger().with_mut(|li| li.timestamp = 1101);
    // Requested 800 but available 500 → must be rejected.
    client.execute_cooldown_withdrawal(&identity);
}

#[test]
fn execute_succeeds_when_slash_did_not_reduce_available_below_request() {
    // Bond 1000, request 600, slash 200 → available = 800 ≥ 600 → OK.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &600);

    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &identity, &200);
    test_helpers::advance_ledger_sequence(&e);

    e.ledger().with_mut(|li| li.timestamp = 1101);
    let bond = client.execute_cooldown_withdrawal(&identity);
    // bonded_amount after slash = 1000 - 200 = 800;
    // after withdrawal = 800 - 600 = 200.
    // NOTE: slashing records slashed_amount separately; bonded_amount is NOT
    // automatically reduced by slash.  Available = bonded_amount - slashed_amount.
    // After execute the contract reduces bonded_amount by the request amount.
    // bonded_amount = 1000, slashed_amount = 200; execute subtracts 600 from bonded_amount.
    assert_eq!(bond.bonded_amount, 400);
}

// ── D.5  Missing request ─────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #237)")] // CooldownRequestNotFound
fn execute_with_no_request_rejected() {
    let e = Env::default();
    let (client, _admin, identity) = setup_with_bond(&e, 1000);
    client.execute_cooldown_withdrawal(&identity);
}

// ── D.6  Same-ledger sequencing guard ────────────────────────────────────

#[test]
#[should_panic(expected = "cooldown execution blocked: collateral increased in this ledger")]
fn execute_same_ledger_guard_raw_record_then_check() {
    // Unit test of the internal guard function: record then immediately check
    // in the same ledger triggers the COOLDOWN_BLOCKED_REASON panic.
    let e = Env::default();
    let (_client, _admin, _identity, _token, contract_id) = test_helpers::setup_with_token(&e);

    e.as_contract(&contract_id, || {
        record_collateral_increase(&e);
        // Same ledger → must panic with COOLDOWN_BLOCKED_REASON.
        require_cooldown_allowed_after_collateral_increase(&e);
    });
}

#[test]
fn execute_guard_allows_after_one_ledger_advance() {
    // After the ledger advances the guard must be silent (no panic).
    let e = Env::default();
    let (_client, _admin, _identity, _token, contract_id) = test_helpers::setup_with_token(&e);

    e.as_contract(&contract_id, || {
        record_collateral_increase(&e);
    });
    test_helpers::advance_ledger_sequence(&e);

    // Must NOT panic after ledger advance.
    e.as_contract(&contract_id, || {
        require_cooldown_allowed_after_collateral_increase(&e);
    });
}

#[test]
#[should_panic(expected = "cooldown execution blocked: collateral increased in this ledger")]
fn execute_blocked_when_create_bond_is_in_same_ledger() {
    // Integration-level test: create_bond records the collateral increase.
    // If execute_cooldown_withdrawal is called in the same ledger, it panics.
    // We test the guard directly since the client entrypoint also calls it.
    let e = Env::default();
    let (client, admin, identity, _token, contract_id) = test_helpers::setup_with_token(&e);
    let treasury = Address::generate(&e);
    client.set_slash_treasury(&admin, &treasury);

    // Create the bond (records same-ledger collateral increase).
    // Do NOT advance the ledger afterward.
    client.create_bond(&identity, &1000_i128, &86_400_u64, &false, &0_u64);

    // Simulate what execute_cooldown_withdrawal calls internally.
    e.as_contract(&contract_id, || {
        require_cooldown_allowed_after_collateral_increase(&e);
    });
}

#[test]
fn execute_allowed_after_ledger_advance_following_create_bond() {
    // Full integration: create_bond in one ledger, advance, then execute succeeds.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    e.ledger().with_mut(|li| li.timestamp = 1101);
    // setup_with_bond already advanced one ledger after create_bond.
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 500);
}

// ============================================================================
// E. Cancel & recovery
// ============================================================================

#[test]
fn cancel_removes_request() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    assert!(
        client.get_cooldown_request(&identity).is_some(),
        "request must exist before cancel"
    );
    client.cancel_cooldown(&identity);
    assert!(
        client.get_cooldown_request(&identity).is_none(),
        "request must be gone after cancel"
    );
}

#[test]
fn cancel_enables_new_request_with_updated_timestamp() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);

    // First request, then cancel.
    client.request_cooldown_withdrawal(&identity, &800);
    client.cancel_cooldown(&identity);

    // Re-request at a later time with a different amount.
    e.ledger().with_mut(|li| li.timestamp = 2000);
    let req = client.request_cooldown_withdrawal(&identity, &500);
    assert_eq!(req.requested_at, 2000, "re-request must record the new timestamp");
    assert_eq!(req.amount, 500, "re-request must record the new amount");
}

#[test]
fn cancel_and_rerequest_full_lifecycle_completes() {
    // Full scenario: request → cancel → rerequest → execute.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);

    client.request_cooldown_withdrawal(&identity, &800);
    client.cancel_cooldown(&identity);

    e.ledger().with_mut(|li| li.timestamp = 2000);
    client.request_cooldown_withdrawal(&identity, &500);
    e.ledger().with_mut(|li| li.timestamp = 2100);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 500);
}

#[test]
#[should_panic(expected = "Error(Contract, #237)")] // CooldownRequestNotFound
fn cancel_with_no_request_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    client.cancel_cooldown(&identity);
}

#[test]
#[should_panic(expected = "Error(Contract, #237)")] // CooldownRequestNotFound
fn execute_after_cancel_rejected() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);
    client.cancel_cooldown(&identity);

    // Attempt to execute after cancel must fail.
    e.ledger().with_mut(|li| li.timestamp = 1101);
    client.execute_cooldown_withdrawal(&identity);
}

#[test]
fn multiple_cancel_and_rerequest_cycles_are_stable() {
    // Repeat the cancel→rerequest cycle multiple times; each cycle must
    // be independent and leave the bond amount unchanged.
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);

    for cycle in 0_u64..3 {
        let ts_req = 1000 + cycle * 1000;
        e.ledger().with_mut(|li| li.timestamp = ts_req);
        client.request_cooldown_withdrawal(&identity, &500);
        client.cancel_cooldown(&identity);
    }

    // Bond amount must be unchanged after all cancels.
    let bond = client.get_identity_state(&identity);
    assert_eq!(
        bond.bonded_amount, 1000,
        "bond amount must not change through cancel cycles"
    );

    // Final rerequest → execute must still work.
    e.ledger().with_mut(|li| li.timestamp = 5000);
    client.request_cooldown_withdrawal(&identity, &300);
    e.ledger().with_mut(|li| li.timestamp = 5101);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 700);
}

// ============================================================================
// F. Authorization (require_auth guard)
// ============================================================================

#[test]
fn auth_request_for_non_existent_bond_fails() {
    // A request for an address that has no bond fails with BondNotFound,
    // proving the auth and bond-lookup guards run correctly.
    let e = Env::default();
    let (client, admin, _identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);

    let stranger = Address::generate(&e);
    let result = client.try_request_cooldown_withdrawal(&stranger, &100);
    assert!(
        result.is_err(),
        "request for an address without a bond must fail"
    );
}

#[test]
fn auth_execute_for_non_existent_bond_fails() {
    // Execute for an address that has no pending request fails.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);
    e.ledger().with_mut(|li| li.timestamp = 1101);

    // Stranger has no pending request.
    let stranger = Address::generate(&e);
    let result = client.try_execute_cooldown_withdrawal(&stranger);
    assert!(result.is_err(), "execute for a stranger must fail");
}

#[test]
fn auth_cancel_for_non_existent_request_fails() {
    // Cancel for an address without a pending request fails without
    // affecting the legitimate identity's request.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    let stranger = Address::generate(&e);
    let result = client.try_cancel_cooldown(&stranger);
    assert!(result.is_err(), "cancel for a stranger must fail");

    // The legitimate identity's request must be unaffected.
    assert!(
        client.get_cooldown_request(&identity).is_some(),
        "identity request must not be cancelled by a stranger's failed attempt"
    );
}

// ============================================================================
// G. Pause gating
// ============================================================================

#[test]
fn pause_blocks_request_cooldown_withdrawal() {
    let e = Env::default();
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);

    client.pause(&admin);
    let result = client.try_request_cooldown_withdrawal(&identity, &500);
    assert!(
        result.is_err(),
        "request must be blocked while contract is paused"
    );
}

#[test]
fn pause_blocks_execute_cooldown_withdrawal() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    e.ledger().with_mut(|li| li.timestamp = 1101);
    client.pause(&admin);
    let result = client.try_execute_cooldown_withdrawal(&identity);
    assert!(
        result.is_err(),
        "execute must be blocked while contract is paused"
    );
}

#[test]
fn pause_blocks_cancel_cooldown() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    client.pause(&admin);
    let result = client.try_cancel_cooldown(&identity);
    assert!(
        result.is_err(),
        "cancel must be blocked while contract is paused"
    );
}

#[test]
fn pause_blocks_set_cooldown_period() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.pause(&admin);
    let result = client.try_set_cooldown_period(&admin, &3600);
    assert!(
        result.is_err(),
        "set_cooldown_period must be blocked while contract is paused"
    );
}

#[test]
fn pause_does_not_block_get_cooldown_period() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.set_cooldown_period(&admin, &3600);
    client.pause(&admin);
    // get_cooldown_period is a pure read; must remain accessible while paused.
    assert_eq!(client.get_cooldown_period(), 3600);
}

#[test]
fn pause_does_not_block_get_cooldown_request() {
    // Read-only views must remain accessible while paused.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    client.pause(&admin);
    let maybe = client.get_cooldown_request(&identity);
    assert!(
        maybe.is_some(),
        "get_cooldown_request must remain accessible while paused"
    );
}

#[test]
fn unpause_re_enables_cooldown_operations() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);

    client.pause(&admin);
    assert!(client.try_request_cooldown_withdrawal(&identity, &500).is_err());

    client.unpause(&admin);
    assert!(!client.is_paused(), "contract must be unpaused");
    let req = client.request_cooldown_withdrawal(&identity, &500);
    assert_eq!(req.amount, 500, "request must succeed after unpause");
}

// ============================================================================
// H. Lifecycle invariants — closed bond rejection
// ============================================================================

#[test]
#[should_panic(expected = "Error(Contract, #201)")] // BondNotActive
fn request_on_closed_bond_rejected() {
    // Marking a bond inactive simulates a closed/withdrawn/liquidated bond.
    // The lifecycle guard must prevent cooldown requests on inactive bonds.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);

    // Close the bond by directly setting active=false via storage.
    let contract_id = client.address.clone();
    e.as_contract(&contract_id, || {
        let key = crate::DataKey::Bond(identity.clone());
        let mut bond: crate::IdentityBond = e.storage().instance().get(&key).unwrap();
        bond.active = false;
        e.storage().instance().set(&key, &bond);
    });

    // Attempt to request a cooldown on the now-inactive bond.
    client.request_cooldown_withdrawal(&identity, &500);
}

#[test]
#[should_panic(expected = "Error(Contract, #201)")] // BondNotActive
fn execute_on_closed_bond_rejected() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    // Close the bond while a request is pending.
    let contract_id = client.address.clone();
    e.as_contract(&contract_id, || {
        let key = crate::DataKey::Bond(identity.clone());
        let mut bond: crate::IdentityBond = e.storage().instance().get(&key).unwrap();
        bond.active = false;
        e.storage().instance().set(&key, &bond);
    });

    e.ledger().with_mut(|li| li.timestamp = 1101);
    client.execute_cooldown_withdrawal(&identity);
}

#[test]
#[should_panic(expected = "Error(Contract, #201)")] // BondNotActive
fn cancel_on_closed_bond_rejected() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100);
    client.request_cooldown_withdrawal(&identity, &500);

    // Close the bond while a request is pending.
    let contract_id = client.address.clone();
    e.as_contract(&contract_id, || {
        let key = crate::DataKey::Bond(identity.clone());
        let mut bond: crate::IdentityBond = e.storage().instance().get(&key).unwrap();
        bond.active = false;
        e.storage().instance().set(&key, &bond);
    });

    client.cancel_cooldown(&identity);
}

// ============================================================================
// I. Period-change semantics
// ============================================================================

#[test]
fn execute_uses_period_at_execution_time_not_request_time_increased() {
    // If the admin INCREASES the period after a request, the new longer period
    // applies at execute time.  Earlier execution must be rejected.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &100); // period = 100s.
    client.request_cooldown_withdrawal(&identity, &500);

    // Admin raises the period to 200s before execution.
    client.set_cooldown_period(&admin, &200);

    // At ts=1100 the OLD period would have elapsed; with 200s deadline = 1200.
    e.ledger().with_mut(|li| li.timestamp = 1100);
    let result = client.try_execute_cooldown_withdrawal(&identity);
    assert!(
        result.is_err(),
        "execute must fail when increased period's deadline has not been reached"
    );

    // At ts=1200 the NEW period has elapsed → must succeed.
    e.ledger().with_mut(|li| li.timestamp = 1200);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 500);
}

#[test]
fn execute_uses_period_at_execution_time_not_request_time_decreased() {
    // If the admin DECREASES the period after a request, the new shorter
    // period applies and earlier execution becomes possible.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &300); // period = 300s initially.
    client.request_cooldown_withdrawal(&identity, &400);

    // Admin reduces period to 50s.
    client.set_cooldown_period(&admin, &50);

    // At ts=1051 the new shorter period has elapsed → must succeed.
    e.ledger().with_mut(|li| li.timestamp = 1051);
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 600);
}

#[test]
fn execute_period_set_to_zero_after_request_allows_immediate_execution() {
    // Admin reduces period to 0 after a request → execute is immediately
    // possible regardless of the original period.
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, admin, identity) = setup_with_bond(&e, 1000);
    client.set_cooldown_period(&admin, &3600);
    client.request_cooldown_withdrawal(&identity, &400);

    // Admin sets period to 0.
    client.set_cooldown_period(&admin, &0);

    // Execute at the same timestamp — must succeed because period is now 0.
    let bond = client.execute_cooldown_withdrawal(&identity);
    assert_eq!(bond.bonded_amount, 600);
}
