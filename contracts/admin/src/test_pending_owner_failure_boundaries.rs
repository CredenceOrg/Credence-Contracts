//! Deterministic failure-boundary coverage for [`AdminContract::get_pending_owner`].
//!
//! `get_pending_owner` is the read side of the two-step ownership transfer. It is
//! the value a client polls to decide whether a proposal exists, who it targets,
//! and which timelock window it must respect. Every property a client may rely
//! on is pinned here:
//!
//! * **total** — uninitialized, initialized, proposed, timelock-elapsed, paused
//!   and post-acceptance states all answer, and none of them panic. Note the
//!   deliberate asymmetry with `get_owner`, which *does* panic with
//!   `NotInitialized`: "no proposal" is a value, not a missing-configuration
//!   error.
//! * **pure / idempotent** — a read never mutates state, never advances the
//!   config epoch, and never emits an event, so polling or retrying a read
//!   cannot desynchronise an off-chain indexer or a concurrent writer.
//! * **deterministic** — the value depends only on the stored slot: not on the
//!   caller, the ledger timestamp, the timelock clock, or the pause state.
//! * **unauthenticated** — a read requires no authorization, so a monitor can
//!   observe ownership state without holding any admin key.
//! * **reported state ≠ eligibility** — a candidate that lost authority during
//!   the timelock (demoted, deactivated, suspended, removed) is still reported,
//!   because `accept_ownership` — not this read — is what revalidates it. The
//!   read is lossless with respect to stale proposals: the current owner can
//!   always observe, and then replace, what was proposed.
//! * **bounded reachability of unusable values** — the slot can never hold the
//!   zero/invalid sentinel, the contract's own address, or the current owner, and
//!   it only ever holds one candidate at a time.

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::{Address as _, Deployer as _, Ledger as _};
use soroban_sdk::{Address, Env};

// Wire-stable error discriminants (`credence_errors::ContractError`).
const ERR_NOT_INITIALIZED: u32 = 1;
const ERR_NOT_ADMIN: u32 = 100;
const ERR_CONTRACT_PAUSED: u32 = 106;
const ERR_INVALID_ADMIN_ADDRESS: u32 = 110;
const ERR_TIMELOCK_NOT_READY: u32 = 112;
const ERR_ADMIN_SUSPENDED: u32 = 113;
const ERR_NO_PENDING_ADMIN: u32 = 115;
const ERR_ALREADY_DEACTIVATED: u32 = 404;

fn contract_err(code: u32) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(code)
}

fn advance(e: &Env, seconds: u64) {
    e.ledger().with_mut(|li| li.timestamp += seconds);
}

/// One SuperAdmin who is also the owner; no proposal exists yet.
fn setup() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let owner = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&owner, &1u32, &100u32);
    (e, client, owner)
}

/// Owner plus a second SuperAdmin, with a live proposal targeting `candidate`.
fn setup_with_proposal() -> (Env, AdminContractClient<'static>, Address, Address) {
    let (e, client, owner) = setup();
    let candidate = Address::generate(&e);
    client.add_admin(&owner, &candidate, &AdminRole::SuperAdmin);
    client.transfer_ownership(&owner, &candidate);
    (e, client, owner, candidate)
}

/// Owner plus two SuperAdmins, no proposal.
fn setup_with_two_candidates() -> (Env, AdminContractClient<'static>, Address, Address, Address) {
    let (e, client, owner) = setup();
    let c1 = Address::generate(&e);
    let c2 = Address::generate(&e);
    client.add_admin(&owner, &c1, &AdminRole::SuperAdmin);
    client.add_admin(&owner, &c2, &AdminRole::SuperAdmin);
    (e, client, owner, c1, c2)
}

/// The all-zero Ed25519 strkey, which must never be accepted as a proposal
/// target.
fn zero_address(e: &Env) -> Address {
    Address::from_string(&soroban_sdk::String::from_str(e, INVALID_ADDRESS_SENTINEL))
}

/// Fault-inject `active`. Peer SuperAdmins cannot deactivate one another through
/// the public API, so the terminal-inactive state is written directly (mirrors
/// `test_atomic_rollback`).
fn set_admin_active(e: &Env, client: &AdminContractClient, admin: &Address, active: bool) {
    e.as_contract(&client.address, || {
        let mut info: AdminInfo = e
            .storage()
            .instance()
            .get(&DataKey::AdminInfo(admin.clone()))
            .unwrap();
        info.active = active;
        e.storage()
            .instance()
            .set(&DataKey::AdminInfo(admin.clone()), &info);
    });
}

/// Remove the admin record entirely, modelling authority revoked during the
/// timelock by a future governance path.
fn remove_admin_record(e: &Env, client: &AdminContractClient, admin: &Address) {
    e.as_contract(&client.address, || {
        e.storage()
            .instance()
            .remove(&DataKey::AdminInfo(admin.clone()));
    });
}

// ---------------------------------------------------------------------------
// Loading states: the read is total and never panics
// ---------------------------------------------------------------------------

/// An uninitialized contract reports "no proposal" rather than failing, while
/// `get_owner` on the same contract reports `NotInitialized`. Both are
/// deliberate: absence of a proposal is data, absence of an owner is a broken
/// contract.
#[test]
fn uninitialized_contract_reports_no_pending_owner_without_panicking() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);

    assert_eq!(client.get_pending_owner(), None);

    let res = client.try_get_owner();
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_NOT_INITIALIZED));
}

/// Freshly initialized: an owner exists and the proposal slot is empty.
#[test]
fn initialized_contract_without_proposal_reports_none() {
    let (_e, client, owner) = setup();
    assert_eq!(client.get_pending_owner(), None);
    assert_eq!(client.get_owner(), owner);
}

/// A live proposal is reported verbatim, and the owner is unchanged until the
/// candidate accepts.
#[test]
fn proposal_is_reported_and_owner_is_unchanged_until_acceptance() {
    let (_e, client, owner, candidate) = setup_with_proposal();

    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    assert_eq!(client.get_owner(), owner);
}

// ---------------------------------------------------------------------------
// Boundary: the timelock clock must not change what the read reports
// ---------------------------------------------------------------------------

/// The reported value is independent of the timelock clock. At
/// `proposed_at + TIMELOCK - 1` the acceptance is still rejected, one second
/// later it succeeds, and the read is `Some(candidate)` at every point in
/// between. A client therefore never has to reason about *when* a read happened
/// to know *what* it means.
#[test]
fn reported_value_is_stable_across_the_whole_timelock_window() {
    let (e, client, owner, candidate) = setup_with_proposal();

    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK / 2);
    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK / 2 - 1);

    // One second short of eligibility: readable, but not acceptable.
    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    let res = client.try_accept_ownership(&candidate);
    assert_eq!(
        res.unwrap_err().unwrap(),
        contract_err(ERR_TIMELOCK_NOT_READY)
    );
    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    assert_eq!(client.get_owner(), owner);

    // Exactly at the boundary the acceptance commits and the slot is cleared.
    advance(&e, 1);
    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    client.accept_ownership(&candidate);
    assert_eq!(client.get_pending_owner(), None);
    assert_eq!(client.get_owner(), candidate);
}

// ---------------------------------------------------------------------------
// Retry / idempotency: reads never mutate observable state
// ---------------------------------------------------------------------------

/// Repeated reads are indistinguishable from one call: same value, unmoved
/// config epoch, no events. A client may poll this getter from a retry loop
/// without perturbing conflict detection for anyone else.
#[test]
fn repeated_reads_are_pure_and_idempotent() {
    let (e, client, _owner, candidate) = setup_with_proposal();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    for _ in 0..3 {
        assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    }

    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// A read on the terminal (post-acceptance) state is equally pure: it must not
/// resurrect a consumed proposal.
#[test]
fn reads_after_acceptance_stay_none_and_pure() {
    let (e, client, _owner, candidate) = setup_with_proposal();
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&candidate);

    let epoch_after = client.get_config_epoch();
    let events_after = e.events().all().len();

    for _ in 0..3 {
        assert_eq!(client.get_pending_owner(), None);
    }

    assert_eq!(client.get_config_epoch(), epoch_after);
    assert_eq!(e.events().all().len(), events_after);
    assert_eq!(client.get_owner(), candidate);
}

/// The only side effect a read may have is extending the instance TTL, which
/// keeps an un-acted-on proposal from expiring. It must never shorten it and it
/// must not change the reported value.
#[test]
fn read_only_ever_extends_the_instance_ttl() {
    let (e, client, _owner, candidate) = setup_with_proposal();

    let ttl_after_first = e.deployer().get_contract_instance_ttl(&client.address);
    for _ in 0..5 {
        assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    }
    let ttl_after_repeats = e.deployer().get_contract_instance_ttl(&client.address);

    assert!(
        ttl_after_repeats >= ttl_after_first,
        "reads must not shorten the instance TTL ({ttl_after_first} -> {ttl_after_repeats})"
    );
}

// ---------------------------------------------------------------------------
// Permission / pause boundaries
// ---------------------------------------------------------------------------

/// The read requires no authorization. With all auth mocking disabled the query
/// still succeeds, so a monitor can watch ownership state without holding an
/// admin key — while a privileged mutation on the same contract fails, proving
/// auth really is off.
#[test]
fn read_requires_no_authorization() {
    let (e, client, owner, candidate) = setup_with_proposal();

    e.set_auths(&[]); // disables auth mocking

    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));

    let res = client.try_transfer_ownership(&owner, &candidate);
    assert!(res.is_err(), "auth must be enforced for mutations");
    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
}

/// The reported value does not depend on the caller: an operator-level admin and
/// a completely unauthenticated observer both see the same proposal.
#[test]
fn reported_value_does_not_depend_on_the_caller() {
    let (e, client, owner, candidate) = setup_with_proposal();
    let operator = Address::generate(&e);
    client.add_admin(&owner, &operator, &AdminRole::Operator);

    let first = client.get_pending_owner();
    e.set_auths(&[]);
    assert_eq!(client.get_pending_owner(), first);
    assert_eq!(client.get_pending_owner(), Some(candidate));
}

/// Reads are not gated by the pause flag: a paused contract still reports its
/// proposal, so a monitor can see *why* an acceptance is being refused. The
/// pause blocks mutations only, and leaves the reported proposal intact.
#[test]
fn read_is_available_while_paused() {
    let (e, client, owner, candidate) = setup_with_proposal();

    client.pause(&owner);
    assert!(client.is_paused());

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();
    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // The mutation is refused while paused, and the refusal changes nothing.
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    let res = client.try_accept_ownership(&candidate);
    assert_eq!(
        res.unwrap_err().unwrap(),
        contract_err(ERR_CONTRACT_PAUSED)
    );
    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(client.get_owner(), owner);
}

// ---------------------------------------------------------------------------
// Stale proposals: reported state is not eligibility
// ---------------------------------------------------------------------------

/// A candidate demoted during the timelock is still reported. The read answers
/// "what was proposed", not "may it proceed": `accept_ownership` rejects and the
/// proposal survives that rejection so the owner can replace it.
#[test]
fn demoted_candidate_remains_reported_and_acceptance_is_rejected() {
    let (e, client, owner, candidate) = setup_with_proposal();
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    client.update_admin_role(&owner, &candidate, &AdminRole::Admin);

    let epoch_before = client.get_config_epoch();
    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_NOT_ADMIN));

    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.get_config_epoch(), epoch_before);
}

/// A candidate deactivated during the timelock is still reported, and the
/// rejected acceptance leaves no partial state behind.
#[test]
fn deactivated_candidate_remains_reported_and_acceptance_is_rejected() {
    let (e, client, owner, candidate) = setup_with_proposal();
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    set_admin_active(&e, &client, &candidate, false);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();
    let res = client.try_accept_ownership(&candidate);
    assert_eq!(
        res.unwrap_err().unwrap(),
        contract_err(ERR_ALREADY_DEACTIVATED)
    );

    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// A candidate suspended past the timelock is still reported; suspension is a
/// self-expiring clock, not a revocation.
#[test]
fn suspended_candidate_remains_reported_and_acceptance_is_rejected() {
    let (e, client, owner, candidate) = setup_with_proposal();
    let until = e.ledger().timestamp() + OWNERSHIP_TRANSFER_TIMELOCK + 3_600;
    client.suspend_admin(&owner, &candidate, &until);
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);

    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_ADMIN_SUSPENDED));
    assert_eq!(client.get_owner(), owner);
}

/// A candidate whose admin record is removed during the timelock is still
/// reported: the read mirrors the proposal slot, which the owner's recovery path
/// depends on.
#[test]
fn removed_candidate_remains_reported_and_acceptance_is_rejected() {
    let (e, client, owner, candidate) = setup_with_proposal();
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    remove_admin_record(&e, &client, &candidate);

    assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
    let res = client.try_accept_ownership(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_NOT_ADMIN));
    assert_eq!(client.get_owner(), owner);
}

/// A proposal that is never consumed stays visible however long the timelock
/// lapses: there is no silent expiry that could hide an outstanding transfer
/// from the current owner.
#[test]
fn unconsumed_proposal_is_never_silently_dropped() {
    let (e, client, owner, candidate) = setup_with_proposal();
    for _ in 0..3 {
        advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
        assert_eq!(client.get_pending_owner(), Some(candidate.clone()));
        assert_eq!(client.get_owner(), owner);
    }
    // Contrast with a consumed proposal, which reports `None`.
    client.accept_ownership(&candidate);
    assert_eq!(client.get_pending_owner(), None);
}

// ---------------------------------------------------------------------------
// Validation boundaries: the slot can never hold an unusable value
// ---------------------------------------------------------------------------

/// A proposal target is never the zero/invalid sentinel. Attempting to propose
/// one is rejected atomically: no proposal is reported and the epoch does not
/// move.
#[test]
fn zero_sentinel_is_never_reported_as_pending_owner() {
    let (e, client, owner) = setup();
    let zero = zero_address(&e);

    let epoch_before = client.get_config_epoch();
    let res = client.try_transfer_ownership(&owner, &zero);
    assert_eq!(
        res.unwrap_err().unwrap(),
        contract_err(ERR_INVALID_ADMIN_ADDRESS)
    );

    assert_eq!(client.get_pending_owner(), None);
    assert_eq!(client.get_config_epoch(), epoch_before);
}

/// The contract's own address is likewise unusable as a proposal target, and
/// rejecting it leaves the slot empty rather than half-written.
#[test]
fn contract_address_is_never_reported_as_pending_owner() {
    let (_e, client, owner) = setup();
    let self_address = client.address.clone();

    let epoch_before = client.get_config_epoch();
    let res = client.try_transfer_ownership(&owner, &self_address);
    assert_eq!(
        res.unwrap_err().unwrap(),
        contract_err(ERR_INVALID_ADMIN_ADDRESS)
    );

    assert_eq!(client.get_pending_owner(), None);
    assert_eq!(client.get_config_epoch(), epoch_before);
}

/// Proposing the current owner is rejected, so the read never reports a
/// self-transfer that would be indistinguishable from "no transfer needed".
/// A previous live proposal is left untouched by the rejection.
#[test]
fn current_owner_is_never_reported_as_pending_owner() {
    let (e, client, owner, candidate) = setup_with_proposal();

    let res = client.try_transfer_ownership(&owner, &owner);
    assert!(res.is_err());
    assert_eq!(
        client.get_pending_owner(),
        Some(candidate),
        "a rejected self-transfer must not disturb the live proposal"
    );
}

// ---------------------------------------------------------------------------
// Overwrite: at most one proposal exists at a time
// ---------------------------------------------------------------------------

/// Re-proposing atomically replaces the visible candidate; the superseded
/// candidate is never observable and can never accept.
#[test]
fn overwritten_proposal_replaces_the_reported_candidate_atomically() {
    let (e, client, owner, c1, c2) = setup_with_two_candidates();

    client.transfer_ownership(&owner, &c1);
    assert_eq!(client.get_pending_owner(), Some(c1.clone()));

    client.transfer_ownership(&owner, &c2);
    assert_eq!(client.get_pending_owner(), Some(c2.clone()));

    // The superseded candidate cannot accept, even once the timelock elapses.
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    let res = client.try_accept_ownership(&c1);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_NOT_ADMIN));
    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.get_pending_owner(), Some(c2));
}

/// A failed acceptance is lossless: the read after a rejection equals the read
/// before it, so a client that retries on rejection observes no data loss.
#[test]
fn failed_acceptance_is_lossless_for_the_reported_proposal() {
    let (e, client, owner, candidate) = setup_with_proposal();
    let reported_before = client.get_pending_owner();
    let epoch_before = client.get_config_epoch();

    // Too early.
    assert!(client.try_accept_ownership(&candidate).is_err());
    assert_eq!(client.get_pending_owner(), reported_before);
    assert_eq!(client.get_config_epoch(), epoch_before);

    // Eligible by time, but the candidate lost authority meanwhile.
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    set_admin_active(&e, &client, &candidate, false);
    assert!(client.try_accept_ownership(&candidate).is_err());
    assert_eq!(client.get_pending_owner(), reported_before);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(client.get_owner(), owner);
}

/// Recovery: the owner observes the stale proposal and replaces it with an
/// eligible candidate, which becomes the reported value. A failed transfer flow
/// is not a dead end.
#[test]
fn owner_can_recover_by_replacing_an_ineligible_proposal() {
    let (e, client, owner, candidate) = setup_with_proposal();
    let replacement = Address::generate(&e);
    client.add_admin(&owner, &replacement, &AdminRole::SuperAdmin);

    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    set_admin_active(&e, &client, &candidate, false);
    assert!(client.try_accept_ownership(&candidate).is_err());
    assert_eq!(client.get_pending_owner(), Some(candidate));

    client.transfer_ownership(&owner, &replacement);
    assert_eq!(client.get_pending_owner(), Some(replacement.clone()));

    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&replacement);
    assert_eq!(client.get_owner(), replacement);
    assert_eq!(client.get_pending_owner(), None);
}

// ---------------------------------------------------------------------------
// Replay / terminal-state regression
// ---------------------------------------------------------------------------

/// A consumed proposal cannot be replayed and cannot reappear in a read. The
/// terminal state is a stable `None`, and a fresh cycle under the new owner is
/// tracked correctly.
#[test]
fn consumed_proposal_is_not_resurrected_by_replay_or_read() {
    let (e, client, owner, candidate) = setup_with_proposal();
    advance(&e, OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&candidate);
    assert_ne!(owner, candidate);

    let epoch_after = client.get_config_epoch();
    let events_after = e.events().all().len();

    let res = client.try_accept_ownership(&candidate);
    assert_eq!(
        res.unwrap_err().unwrap(),
        contract_err(ERR_NO_PENDING_ADMIN)
    );

    for _ in 0..3 {
        assert_eq!(client.get_pending_owner(), None);
    }
    assert_eq!(client.get_owner(), candidate);
    assert_eq!(client.get_config_epoch(), epoch_after);
    assert_eq!(e.events().all().len(), events_after);

    // The new owner can start a fresh cycle, and the read tracks it.
    let next = Address::generate(&e);
    client.add_admin(&candidate, &next, &AdminRole::SuperAdmin);
    client.transfer_ownership(&candidate, &next);
    assert_eq!(client.get_pending_owner(), Some(next));
}
