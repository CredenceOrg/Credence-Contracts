//! Deterministic failure-boundary coverage for [`AdminContract::get_admin_count`].
//!
//! `get_admin_count` is the contract's canonical *total* governance-size read:
//! it returns the length of the persisted `DataKey::AdminList` vector and
//! nothing else. It is the denominator used by the `MaxAdmins` guard in
//! [`AdminContract::add_admin`], so its determinism is load-bearing: a count
//! that could drift would silently mis-size the governance set.
//!
//! ## Invariants proven here
//!
//! * **Raw membership, not liveness.** The value is exactly
//!   `AdminList.len()`. It counts every list entry, including a dangling entry
//!   with no matching `AdminInfo` record, and is therefore *different* from
//!   [`AdminContract::get_active_admin_count`] (which requires an `active`
//!   record) and [`AdminContract::get_effective_active_admin_count`] (which
//!   additionally excludes suspended admins). Callers that need "who can act
//!   right now" must not use this function.
//! * **Pure read.** Apart from the instance TTL bump every entry point shares,
//!   a call mutates nothing: no events, no `ConfigEpoch` advance, no change to
//!   any `AdminInfo`. Repeated reads at the same ledger snapshot, and reads at
//!   different ledger timestamps, return the same value.
//! * **Unconditional access.** The entry point performs no `require_auth`, so
//!   it is readable by unauthenticated callers and while the contract is
//!   paused — the "loading/permission" states of the issue.
//! * **Failures roll back.** A rejected over-limit or duplicate `add_admin`
//!   leaves the count, the list, and the config epoch exactly as they were.
//!
//! ## Input classes covered
//!
//! valid (initialize/add), invalid (duplicate and over-limit adds, final-super
//! removal), boundary (empty/uninitialised list, missing key, exactly
//! `MaxAdmins`, pagination limits), stale/retry (epoch-changed snapshot), and
//! permission (unauthenticated and paused reads).

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Vec};

fn setup_with_limits(
    e: &Env,
    min_admins: u32,
    max_admins: u32,
) -> (AdminContractClient<'static>, Address) {
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(e, &contract_id);
    let super_admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&super_admin, &min_admins, &max_admins);
    (client, super_admin)
}

fn setup(e: &Env) -> (AdminContractClient<'static>, Address) {
    setup_with_limits(e, 1, 100)
}

/// Register `n` fresh admins and return their addresses in insertion order.
fn add_admins(e: &Env, client: &AdminContractClient, caller: &Address, n: u32) -> Vec<Address> {
    let mut added = Vec::new(e);
    for _ in 0..n {
        let admin = Address::generate(e);
        client.add_admin(caller, &admin, &AdminRole::Admin);
        added.push_back(admin);
    }
    added
}

// ---------------------------------------------------------------------------
// Loading / empty boundaries
// ---------------------------------------------------------------------------

/// An uninitialised contract has no `AdminList` key. The read must load as `0`
/// and must not panic or require authentication, so callers can probe a
/// contract before it is configured.
#[test]
fn uninitialised_contract_reads_zero_without_auth() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);

    e.set_auths(&[]);
    assert_eq!(client.get_admin_count(), 0);
    assert_eq!(client.get_all_admins().len(), 0);
}

/// A cleared `AdminList` key is the same boundary as an uninitialised one: it
/// reads as `0` rather than surfacing a storage error.
#[test]
fn missing_admin_list_key_reads_zero() {
    let e = Env::default();
    let (client, _super_admin) = setup(&e);

    e.as_contract(&client.address, || {
        e.storage().instance().remove(&DataKey::AdminList);
    });

    assert_eq!(client.get_admin_count(), 0);
}

// ---------------------------------------------------------------------------
// Valid inputs and determinism
// ---------------------------------------------------------------------------

/// Initialization seeds exactly one super admin, and the count is a pure read:
/// repeating it changes neither the value nor the config epoch.
#[test]
fn repeated_reads_are_stable_and_do_not_advance_the_epoch() {
    let e = Env::default();
    let (client, _super_admin) = setup(&e);

    let epoch = client.get_config_epoch();
    let count = client.get_admin_count();
    assert_eq!(count, 1);

    for _ in 0..32 {
        assert_eq!(client.get_admin_count(), count);
    }
    assert_eq!(client.get_config_epoch(), epoch);
}

/// The count tracks committed additions and always equals the raw list length.
#[test]
fn count_tracks_committed_adds() {
    let e = Env::default();
    let (client, super_admin) = setup(&e);

    let added = add_admins(&e, &client, &super_admin, 4);
    assert_eq!(added.len(), 4);
    assert_eq!(client.get_admin_count(), 5);
    assert_eq!(client.get_all_admins().len(), client.get_admin_count());
}

/// Advancing the ledger does not change a count derived purely from storage.
#[test]
fn count_is_independent_of_ledger_time() {
    let e = Env::default();
    e.ledger().set_timestamp(1_000);
    let (client, super_admin) = setup(&e);
    add_admins(&e, &client, &super_admin, 2);
    let count = client.get_admin_count();

    e.ledger().set_timestamp(1_000 + 86_400 * 365);
    assert_eq!(client.get_admin_count(), count);
}

// ---------------------------------------------------------------------------
// Permission / pause states
// ---------------------------------------------------------------------------

/// The read is available to unauthenticated callers and while paused. Allowing
/// reads while paused is deliberate: pause blocks state changes, not visibility.
#[test]
fn read_is_available_without_auth_and_while_paused() {
    let e = Env::default();
    let (client, super_admin) = setup(&e);
    let before = client.get_admin_count();

    client.pause(&super_admin);
    assert!(client.is_paused());

    e.set_auths(&[]);
    assert_eq!(client.get_admin_count(), before);
}

// ---------------------------------------------------------------------------
// Total vs. effective count (the core semantic boundary)
// ---------------------------------------------------------------------------

/// Suspension removes an admin from the *effective* set but never from the
/// total list, and the total stays constant across the inclusive expiry
/// boundary.
#[test]
fn total_count_ignores_suspension_while_effective_count_drops() {
    let e = Env::default();
    e.ledger().set_timestamp(1_000);
    let (client, super_admin) = setup(&e);

    let admin = Address::generate(&e);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);
    assert_eq!(client.get_effective_active_admin_count(), 2);

    client.suspend_admin(&super_admin, &admin, 2_000);
    assert_eq!(
        client.get_admin_count(),
        2,
        "suspension must not shrink the total list"
    );
    assert_eq!(client.get_active_admin_count(), 2);
    assert_eq!(client.get_effective_active_admin_count(), 1);

    // Expiry is inclusive: at exactly `suspended_until` the admin is effective
    // again, and the total count never moved.
    e.ledger().set_timestamp(2_000);
    assert_eq!(client.get_effective_active_admin_count(), 2);
    assert_eq!(client.get_admin_count(), 2);
}

/// A dangling `AdminList` entry (a list address with no `AdminInfo` record)
/// counts toward the total but is not an active admin. This is the documented
/// semantic gap between total and active counts.
#[test]
fn dangling_list_entry_counts_as_total_but_not_as_active() {
    let e = Env::default();
    let (client, _super_admin) = setup(&e);

    let dangling = Address::generate(&e);
    e.as_contract(&client.address, || {
        let mut list: Vec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::AdminList)
            .unwrap_or(Vec::new(&e));
        list.push_back(dangling.clone());
        e.storage().instance().set(&DataKey::AdminList, &list);
    });

    assert_eq!(client.get_admin_count(), 2, "total is the raw list length");
    assert_eq!(client.get_active_admin_count(), 1);
    assert_eq!(client.get_effective_active_admin_count(), 1);
}

// ---------------------------------------------------------------------------
// Rejection boundaries (failures must roll back)
// ---------------------------------------------------------------------------

/// Re-adding an existing admin is rejected and leaves the count untouched.
#[test]
fn duplicate_add_is_rejected_without_changing_the_count() {
    let e = Env::default();
    let (client, super_admin) = setup(&e);

    let admin = Address::generate(&e);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);

    let duplicate = client.try_add_admin(&super_admin, &admin, &AdminRole::Admin);
    assert!(
        duplicate.is_err(),
        "re-adding an existing admin must be rejected"
    );
    assert_eq!(client.get_admin_count(), 2);
}

/// The `MaxAdmins` boundary: adding the admin that reaches the limit succeeds,
/// the next one is rejected with the stable limit error, and nothing is left
/// half-applied.
#[test]
fn max_admins_boundary_rejects_the_next_add_and_rolls_back() {
    let e = Env::default();
    let (client, super_admin) = setup_with_limits(&e, 1, 3);
    add_admins(&e, &client, &super_admin, 2); // super + 2 == 3 == MaxAdmins
    assert_eq!(client.get_admin_count(), 3);

    let epoch_before = client.get_config_epoch();
    let rejected = client.try_add_admin(&super_admin, &Address::generate(&e), &AdminRole::Admin);
    assert!(
        rejected.is_err(),
        "adding beyond MaxAdmins must be rejected"
    );

    assert_eq!(
        client.get_admin_count(),
        3,
        "a rejected add must not change the count"
    );
    assert_eq!(client.get_all_admins().len(), 3);
    assert_eq!(
        client.get_config_epoch(),
        epoch_before,
        "a rejected add must not advance the config epoch"
    );
}

/// Pin the wire error code of the limit guard (`ContractError::ThresholdExceedsSigners = 601`)
/// so a future refactor cannot silently renumber it.
#[test]
#[should_panic(expected = "Error(Contract, #601)")]
fn over_limit_add_panics_with_threshold_exceeds_signers() {
    let e = Env::default();
    let (client, super_admin) = setup_with_limits(&e, 1, 2);
    add_admins(&e, &client, &super_admin, 1); // now at MaxAdmins == 2

    client.add_admin(&super_admin, &Address::generate(&e), &AdminRole::Admin);
}

/// A count change is always the net of committed writes: removing an admin
/// decrements it, and removing the final super admin is rejected so the count
/// can never reach zero by accident.
#[test]
fn removal_decrements_count_and_last_super_admin_is_protected() {
    let e = Env::default();
    let (client, super_admin) = setup(&e);

    let admin = Address::generate(&e);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);
    assert_eq!(client.get_admin_count(), 2);

    client.remove_admin(&super_admin, &admin);
    assert_eq!(client.get_admin_count(), 1);

    let rejected = client.try_remove_admin(&super_admin, &super_admin);
    assert!(
        rejected.is_err(),
        "the final super admin must not be removable"
    );
    assert_eq!(client.get_admin_count(), 1);
}

// ---------------------------------------------------------------------------
// Stale snapshot / retry contract
// ---------------------------------------------------------------------------

/// A client that cached `(epoch, count)` can detect a concurrent commit via the
/// epoch and re-read a consistent count; undoing the commit returns to the
/// snapshot with nothing half-applied.
#[test]
fn stale_snapshot_retry_observes_only_committed_counts() {
    let e = Env::default();
    let (client, super_admin) = setup(&e);

    let snapshot_epoch = client.get_config_epoch();
    let snapshot_count = client.get_admin_count();

    let admin = Address::generate(&e);
    client.add_admin(&super_admin, &admin, &AdminRole::Admin);

    assert!(client.get_config_epoch() > snapshot_epoch);
    assert_eq!(client.get_admin_count(), snapshot_count + 1);

    client.remove_admin(&super_admin, &admin);
    assert_eq!(client.get_admin_count(), snapshot_count);
}

// ---------------------------------------------------------------------------
// Regression: the count is the pagination total at every limit boundary
// ---------------------------------------------------------------------------

/// Walking the admin set with any page size reproduces the total, including the
/// `limit == 0` (clamped to `MAX_PAGE_LIMIT`) and oversized-limit boundaries.
#[test]
fn count_matches_paginated_total_at_boundaries() {
    let e = Env::default();
    let (client, super_admin) = setup_with_limits(&e, 1, 8);
    add_admins(&e, &client, &super_admin, 6); // super + 6 == 7
    let total = client.get_admin_count();
    assert_eq!(total, 7);

    for limit in [1u32, 2, 7, 0, u32::MAX] {
        let mut walked = 0u32;
        let mut cursor = 0u32;
        loop {
            let (page, next) = client.get_all_admins_page(&cursor, &limit);
            walked += page.len();
            match next {
                Some(next_cursor) => cursor = next_cursor,
                None => break,
            }
        }
        assert_eq!(
            walked, total,
            "paginated total must equal get_admin_count for limit {limit}"
        );
    }
}
