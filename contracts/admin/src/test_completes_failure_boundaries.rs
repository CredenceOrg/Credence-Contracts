//! Deterministic failure-boundary coverage for the `accept_ownership`
//! entry point — the function that *completes* the two-step ownership
//! transfer declared in `lib.rs` ("This function completes the two-step
//! ownership transfer process", `lib.rs` docs on `accept_ownership`).
//!
//! `accept_ownership` is the only path by which durable control of the
//! contract changes hands, so every input class is asserted to be
//! deterministic:
//!
//! * **valid**      — an eligible pending SuperAdmin accepts after the
//!                    timelock and becomes the owner, exactly once;
//! * **invalid**    — a non-pending caller, a non-admin, a paused contract,
//!                    or a missing proposal is rejected with a stable
//!                    wire code and commits nothing;
//! * **duplicate**  — replaying a consumed proposal is rejected and emits
//!                    nothing, so a retried transaction is safe;
//! * **boundary**   — the exact timelock edge (`now == eligible_at` succeeds,
//!                    `now == eligible_at - 1` does not), a proposal stamped
//!                    so that `proposed_at + timelock` overflows `u64`, and a
//!                    candidate whose status changes *during* the timelock
//!                    (demoted, deactivated, suspended, removed).
//!
//! ## Invariants asserted
//!
//! The retry contract at the top of `lib.rs` requires that a rejected or
//! repeated privileged operation never advances
//! [`AdminContract::get_config_epoch`] and never leaves partial state. Every
//! negative test below therefore asserts, after the rejection:
//!
//! 1. the owner is unchanged,
//! 2. the pending proposal is unchanged (still present, or still absent),
//! 3. the config epoch is unchanged,
//! 4. the rejection produced no contract event.
//!
//! ## A note on event counting
//!
//! `env.events().all()` in soroban-sdk 22 returns the events of the **most
//! recent** top-level invocation only — it is not a cumulative log. Counting
//! events across two separate client calls and subtracting therefore compares
//! unrelated snapshots. To assert "this call emitted nothing" reliably, each
//! negative test inspects the event log of the rejected invocation itself
//! (which contains only diagnostic error events) and asserts that no contract
//! event was published, and compares epochs/state across invocations.

#![cfg(test)]

extern crate std;

use crate::*;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, Env, Symbol, TryFromVal};
use std::string::ToString;

// Wire-stable error discriminants (`credence_errors::ContractError`).
const ERR_NOT_ADMIN: u32 = 100;
const ERR_CONTRACT_PAUSED: u32 = 106;
const ERR_TIMELOCK_NOT_READY: u32 = 112;
const ERR_ADMIN_SUSPENDED: u32 = 113;
const ERR_NO_PENDING_ADMIN: u32 = 115;
const ERR_ALREADY_DEACTIVATED: u32 = 404;
const ERR_OVERFLOW: u32 = 700;

fn setup() -> (Env, Address, AdminContractClient<'static>, Address, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let owner = Address::generate(&e);
    let candidate = Address::generate(&e);

    e.mock_all_auths();
    client.initialize(&owner, &1u32, &100u32);
    client.add_admin(&owner, &candidate, &AdminRole::SuperAdmin);
    client.transfer_ownership(&owner, &candidate);

    (e, contract_id, client, owner, candidate)
}

fn advance(e: &Env, seconds: u64) {
    e.ledger().with_mut(|l| l.timestamp += seconds);
}

/// Names of the non-diagnostic events in an event log.
///
/// Soroban records panics as diagnostic events on topics
/// `(Symbol("error"), Error(..))`; a call that published no contract event
/// yields only those. A successful acceptance publishes exactly
/// `admin_rotated` and `ownership_transfer_accepted`.
fn contract_event_names(e: &Env) -> std::vec::Vec<std::string::String> {
    let mut names: std::vec::Vec<std::string::String> = std::vec::Vec::new();
    for (_, topics, _) in e.events().all().iter() {
        let Some(first) = topics.get(0) else { continue };
        let Ok(sym) = Symbol::try_from_val(e, &first) else {
            continue;
        };
        let name: std::string::String = sym.to_string();
        // Diagnostic events are not contract events.
        if name != "error" {
            names.push(name);
        }
    }
    names
}

/// Assert the full post-rejection invariant: the attempt published no
/// contract event and left owner, proposal, and epoch untouched.
///
/// **Ordering matters**: in soroban-sdk 22 every client call starts a fresh
/// event frame, so this must be invoked immediately after the rejected call,
/// before any other client read. State and epoch are read afterwards because
/// they are cumulative and unaffected by the frame reset.
fn assert_no_op(
    e: &Env,
    client: &AdminContractClient,
    expected_owner: &Address,
    expected_pending: Option<Address>,
    expected_epoch: u64,
) {
    let names = contract_event_names(e);
    assert!(
        names.is_empty(),
        "a rejected acceptance must publish no contract event, saw {names:?}"
    );
    assert_eq!(&client.get_owner(), expected_owner);
    assert_eq!(client.get_pending_owner(), expected_pending);
    assert_eq!(client.get_config_epoch(), expected_epoch);
}

fn err(code: u32) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(code)
}

// ---------------------------------------------------------------------------
// Valid: successful completion
// ---------------------------------------------------------------------------

/// The happy path completes the transfer exactly once: the candidate becomes
/// owner, the proposal is consumed, the epoch advances by exactly one, and the
/// two documented events are published.
#[test]
fn eligible_candidate_completes_transfer_exactly_once() {
    let (e, _id, client, owner, candidate) = setup();
    let epoch_before = client.get_config_epoch();
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);

    client.accept_ownership(&candidate);
    // Read the event log of *this* invocation before any other client call
    // resets the frame.
    let events = contract_event_names(&e);

    assert_eq!(client.get_owner(), candidate);
    assert_eq!(client.get_pending_owner(), None);
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    assert_eq!(
        events,
        std::vec![
            "admin_rotated".to_string(),
            "ownership_transfer_accepted".to_string()
        ]
    );
    assert_ne!(client.get_owner(), owner, "ownership actually moved");
}

// ---------------------------------------------------------------------------
// Invalid: authorization and permission boundaries
// ---------------------------------------------------------------------------

/// Only the pending owner may complete the transfer. A stranger and a
/// *different* eligible SuperAdmin are both rejected with `NotAdmin`, and the
/// live proposal survives untouched so the real candidate can still accept.
#[test]
fn completion_requires_the_pending_owner() {
    let (e, _id, client, owner, candidate) = setup();
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);

    // A SuperAdmin who is not the pending candidate is still unauthorized.
    let other_super = Address::generate(&e);
    client.add_admin(&owner, &other_super, &AdminRole::SuperAdmin);
    let epoch_before = client.get_config_epoch();

    let res = client.try_accept_ownership(&other_super);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_NOT_ADMIN));
    assert_no_op(&e, &client, &owner, Some(candidate.clone()), epoch_before);

    // A non-admin stranger is rejected identically.
    let stranger = Address::generate(&e);
    let res = client.try_accept_ownership(&stranger);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_NOT_ADMIN));
    assert_no_op(&e, &client, &owner, Some(candidate.clone()), epoch_before);

    // The genuine candidate is unaffected by the rejected attempts.
    client.accept_ownership(&candidate);
    assert_eq!(client.get_owner(), candidate);
}

/// Completing with no live proposal is a stable `NoPendingAdmin` rejection, not
/// a state transition. Repeated attempts stay rejected and never move the epoch.
#[test]
fn completion_without_a_proposal_is_rejected() {
    let (e, _id, client, owner, candidate) = setup();
    // Drop the proposal by completing the transfer first.
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&candidate);
    let epoch_after = client.get_config_epoch();

    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_NO_PENDING_ADMIN));
    assert_no_op(&e, &client, &candidate, None, epoch_after);
}

/// A paused contract blocks completion before any state is read, so the live
/// proposal is preserved and can be completed after unpausing.
#[test]
fn paused_contract_blocks_completion_and_recovers_on_unpause() {
    let (e, _id, client, owner, candidate) = setup();
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    client.pause(&owner);

    let epoch_before = client.get_config_epoch();
    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_CONTRACT_PAUSED));
    assert_no_op(&e, &client, &owner, Some(candidate.clone()), epoch_before);

    // Recovery: unpausing lets the same candidate complete the transfer.
    client.unpause(&owner);
    client.accept_ownership(&candidate);
    assert_eq!(client.get_owner(), candidate);
}

// ---------------------------------------------------------------------------
// Duplicate: replaying a consumed proposal
// ---------------------------------------------------------------------------

/// A completed transfer is consumed. Replaying the acceptance — the classic
/// retry after a timeout — is rejected with `NoPendingAdmin` and republishes
/// nothing, so a duplicate submission cannot re-run the rotation.
#[test]
fn replay_after_completion_is_rejected_and_emits_nothing() {
    let (e, _id, client, _owner, candidate) = setup();
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&candidate);

    let epoch_after = client.get_config_epoch();

    for _ in 0..3 {
        let res = client.try_accept_ownership(&candidate);
        assert_eq!(res.unwrap_err().unwrap(), err(ERR_NO_PENDING_ADMIN));
        assert_no_op(&e, &client, &candidate, None, epoch_after);
    }
}

// ---------------------------------------------------------------------------
// Boundary: the timelock edges
// ---------------------------------------------------------------------------

/// The timelock is an inclusive lower bound. At `eligible_at - 1` the candidate
/// is rejected with `TimelockNotReady`; at exactly `eligible_at` it is accepted.
/// The rejection is a no-op, so the same proposal succeeds one second later.
#[test]
fn timelock_boundary_is_inclusive_and_rejection_is_recoverable() {
    let (e, _id, client, owner, candidate) = setup();
    let proposed_at = e.ledger().timestamp();
    let epoch_before = client.get_config_epoch();

    // One second short of the timelock: rejected.
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK - 1);
    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_TIMELOCK_NOT_READY));
    assert_no_op(&e, &client, &owner, Some(candidate.clone()), epoch_before);

    // Exactly at the boundary: accepted.
    advance(&e, 1);
    assert_eq!(
        e.ledger().timestamp(),
        proposed_at + crate::OWNERSHIP_TRANSFER_TIMELOCK
    );
    client.accept_ownership(&candidate);
    assert_eq!(client.get_owner(), candidate);
    assert_eq!(client.get_pending_owner(), None);
}

/// A proposal stamped so close to `u64::MAX` that `proposed_at + timelock`
/// overflows is rejected with `Overflow` instead of wrapping into a value that
/// would make the timelock immediately satisfiable.
#[test]
fn timelock_overflow_is_rejected() {
    let (e, id, client, owner, candidate) = setup();
    e.as_contract(&id, || {
        e.storage()
            .instance()
            .set(&DataKey::TransferProposedAt, &u64::MAX);
    });
    let epoch_before = client.get_config_epoch();

    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_OVERFLOW));
    assert_no_op(&e, &client, &owner, Some(candidate), epoch_before);
}

// ---------------------------------------------------------------------------
// Boundary: candidate eligibility changes during the timelock
// ---------------------------------------------------------------------------

/// A candidate demoted from SuperAdmin during the timelock cannot take
/// ownership, even though the proposal was valid when it was created. This is
/// the core anti-stale-proposal guarantee: eligibility is revalidated at
/// acceptance, not trusted from proposal time.
#[test]
fn candidate_demoted_during_timelock_cannot_complete() {
    let (e, _id, client, owner, candidate) = setup();
    client.update_admin_role(&owner, &candidate, &AdminRole::Operator);
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    let epoch_before = client.get_config_epoch();

    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_NOT_ADMIN));
    assert_no_op(&e, &client, &owner, Some(candidate), epoch_before);
}

/// A candidate still suspended when the timelock elapses is rejected with
/// `AdminSuspended` and the proposal is preserved.
#[test]
fn candidate_suspended_during_timelock_cannot_complete() {
    let (e, _id, client, owner, candidate) = setup();
    let until_ts = e.ledger().timestamp() + crate::OWNERSHIP_TRANSFER_TIMELOCK + 10_000;
    client.suspend_admin(&owner, &candidate, &until_ts);
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    let epoch_before = client.get_config_epoch();

    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_ADMIN_SUSPENDED));
    assert_no_op(&e, &client, &owner, Some(candidate), epoch_before);
}

/// A candidate whose admin record is revoked entirely cannot complete: the
/// revalidation lookup fails with `NotAdmin` rather than granting ownership to
/// an address that no longer holds any role.
#[test]
fn candidate_removed_during_timelock_cannot_complete() {
    let (e, id, client, owner, candidate) = setup();
    e.as_contract(&id, || {
        e.storage()
            .instance()
            .remove(&DataKey::AdminInfo(candidate.clone()));
    });
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    let epoch_before = client.get_config_epoch();

    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_NOT_ADMIN));
    assert_no_op(&e, &client, &owner, Some(candidate), epoch_before);
}

/// A candidate deactivated during the timelock is rejected with
/// `AlreadyDeactivated`. Peer SuperAdmins cannot deactivate one another through
/// the public API, so the terminal state is injected at the storage layer,
/// modelling a future administrative recovery path.
#[test]
fn candidate_deactivated_during_timelock_cannot_complete() {
    let (e, id, client, owner, candidate) = setup();
    e.as_contract(&id, || {
        let mut info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(candidate.clone()))
            .unwrap();
        info.active = false;
        e.storage()
            .instance()
            .set(&DataKey::AdminInfo(candidate.clone()), &info);
    });
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    let epoch_before = client.get_config_epoch();

    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_ALREADY_DEACTIVATED));
    assert_no_op(&e, &client, &owner, Some(candidate), epoch_before);
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

/// A candidate made ineligible during the timelock is not a dead end: the
/// current owner keeps control and can replace the proposal with an eligible
/// candidate, which then completes normally.
#[test]
fn owner_recovers_by_replacing_an_ineligible_candidate() {
    let (e, _id, client, owner, candidate) = setup();
    client.update_admin_role(&owner, &candidate, &AdminRole::Operator);
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);

    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_NOT_ADMIN));
    assert_eq!(&client.get_owner(), &owner, "owner retains control");

    // Replace the proposal with a fresh eligible SuperAdmin.
    let replacement = Address::generate(&e);
    client.add_admin(&owner, &replacement, &AdminRole::SuperAdmin);
    client.transfer_ownership(&owner, &replacement);

    // The replacement's timelock runs from its own proposal time.
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&replacement);
    assert_eq!(client.get_owner(), replacement);
    assert_eq!(client.get_pending_owner(), None);
}

/// A suspension that *expires* during the timelock is a self-expiring clock,
/// not a revocation: once `suspended_until` passes, the same candidate can
/// complete. This is the symmetric boundary to the rejection case above and
/// proves the guard is not a permanent lockout.
#[test]
fn candidate_whose_suspension_expired_during_timelock_can_complete() {
    let (e, _id, client, owner, candidate) = setup();
    // Suspend past the timelock so the first attempt is genuinely blocked...
    let until_ts = e.ledger().timestamp() + crate::OWNERSHIP_TRANSFER_TIMELOCK + 1_000;
    client.suspend_admin(&owner, &candidate, &until_ts);

    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    assert!(
        e.ledger().timestamp() < until_ts,
        "suspension still in force"
    );
    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_ADMIN_SUSPENDED));
    assert_eq!(&client.get_owner(), &owner);

    // ...then let it lapse by one second past `suspended_until`. Suspension is
    // a self-expiring clock, not a revocation, so the guard clears and the very
    // same proposal completes.
    advance(&e, 1_001);
    assert!(e.ledger().timestamp() >= until_ts);
    client.accept_ownership(&candidate);
    assert_eq!(client.get_owner(), candidate);
}

/// Rejecting an ineligible candidate must leave no partial state, so the same
/// proposal can be re-attempted without a fresh `transfer_ownership` once the
/// blocking condition is gone. This is the retry contract from `lib.rs`.
#[test]
fn rejected_attempt_is_retryable_without_reproposing() {
    let (e, _id, client, owner, candidate) = setup();
    client.update_admin_role(&owner, &candidate, &AdminRole::Operator);
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);

    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), err(ERR_NOT_ADMIN));

    // Restore eligibility; the original proposal is still live.
    client.update_admin_role(&owner, &candidate, &AdminRole::SuperAdmin);
    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));

    client.accept_ownership(&candidate);
    assert_eq!(client.get_owner(), candidate);
}

/// The helper used throughout this module must agree with the contract's own
/// event schema, otherwise the negative tests above would be vacuous.
#[test]
fn event_helper_sees_contract_events_and_ignores_diagnostics() {
    let (e, _id, client, _owner, candidate) = setup();
    // After a rejected call the log holds only diagnostics.
    let res = client.try_accept_ownership(&candidate);
    assert!(res.is_err());
    assert!(contract_event_names(&e).is_empty());

    // After a successful call the log holds the two contract events.
    advance(&e, crate::OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&candidate);
    assert_eq!(contract_event_names(&e).len(), 2);
}
