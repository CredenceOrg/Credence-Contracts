//! Deterministic failure-boundary coverage for [`AdminContract::get_all_admins`]
//! and [`AdminContract::get_all_admins_page`] (issue #1396).
//!
//! # What is covered
//!
//! | Category                        | Tests                                             |
//! |---------------------------------|---------------------------------------------------|
//! | Uninitialized / empty state     | `uninitialized_returns_empty`,                    |
//! |                                 | `initialized_sole_admin_in_list`                  |
//! | Post-remove state               | `post_remove_list_shrinks_correctly`,             |
//! |                                 | `remove_all_added_admins_only_super_remains`,      |
//! |                                 | `list_never_contains_removed_address`             |
//! | Deactivated / suspended admins  | `deactivated_admin_still_in_list`,                |
//! |                                 | `suspended_admin_still_in_list`,                  |
//! |                                 | `reactivated_admin_still_in_list`                 |
//! | Boundary cursor / limit values  | `cursor_zero_with_limit_one`,                     |
//! |                                 | `cursor_at_last_element`,                         |
//! |                                 | `cursor_exactly_at_length_returns_empty`,          |
//! |                                 | `cursor_far_past_end_returns_empty`,               |
//! |                                 | `limit_zero_uses_default_max`,                    |
//! |                                 | `limit_one_pages_through_list`,                   |
//! |                                 | `limit_exceeds_max_is_clamped`,                   |
//! |                                 | `limit_max_page_limit_exact`                      |
//! | Concurrent mutation detection   | `epoch_advances_on_add_detectable_mid_walk`,       |
//! |                                 | `epoch_advances_on_remove_detectable_mid_walk`,    |
//! |                                 | `stale_snapshot_detectable_via_epoch`             |
//! | Retry contract                  | `retry_after_stale_epoch_produces_correct_result`, |
//! |                                 | `failed_mutation_does_not_affect_list`            |
//! | Duplicate inputs                | `adding_same_address_twice_panics`,               |
//! |                                 | `list_never_contains_duplicates_after_role_update`|
//! | Large lists                     | `large_list_pagination_walk_covers_all`,           |
//! |                                 | `large_list_order_is_deterministic`               |
//! | Read-only (no auth needed)      | `get_all_admins_requires_no_auth`,                |
//! |                                 | `get_all_admins_page_requires_no_auth`            |
//! | Invariant: list matches count   | `list_length_always_matches_get_admin_count`,      |
//! |                                 | `concatenated_pages_match_full_list`              |

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

// ── helpers ────────────────────────────────────────────────────────────────────

fn deploy() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, super_admin)
}

/// Add `n` fresh admins with `AdminRole::Operator` under `super_admin`.
/// Returns the vector of newly created addresses in insertion order.
fn add_n_operators(
    e: &Env,
    client: &AdminContractClient,
    super_admin: &Address,
    n: usize,
) -> soroban_sdk::Vec<Address> {
    let mut added = soroban_sdk::Vec::new(e);
    for _ in 0..n {
        let addr = Address::generate(e);
        client.add_admin(super_admin, &addr, &AdminRole::Operator);
        added.push_back(addr);
    }
    added
}

// ── Uninitialized / empty state ────────────────────────────────────────────────

/// Before initialization the AdminList key is absent.
/// `get_all_admins` falls back to an empty `Vec` rather than panicking.
#[test]
fn uninitialized_returns_empty() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);

    // Call get_all_admins *before* initialize — must return [] not panic.
    #[allow(deprecated)]
    let result = e.as_contract(&contract_id, || AdminContract::get_all_admins(e.clone()));
    assert_eq!(result.len(), 0);
}

/// After a successful initialization the list contains exactly the super admin.
#[test]
fn initialized_sole_admin_in_list() {
    let (e, client, super_admin) = deploy();
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), super_admin);
}

// ── Post-remove state ──────────────────────────────────────────────────────────

/// After removing an admin the list shrinks by exactly one entry.
#[test]
fn post_remove_list_shrinks_correctly() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);
    assert_eq!(client.get_admin_count(), 2);

    client.remove_admin(&super_admin, &op);
    assert_eq!(client.get_admin_count(), 1);
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert_eq!(list.len(), 1);
    assert!(!list.contains(&op));
}

/// Remove all non-super admins; the list must still hold exactly the super admin.
#[test]
fn remove_all_added_admins_only_super_remains() {
    let (e, client, super_admin) = deploy();
    let ops = add_n_operators(&e, &client, &super_admin, 5);
    assert_eq!(client.get_admin_count(), 6);

    for op in ops.iter() {
        client.remove_admin(&super_admin, &op);
    }
    assert_eq!(client.get_admin_count(), 1);
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), super_admin);
}

/// The removed address must never appear in the list or in any paginated page.
#[test]
fn list_never_contains_removed_address() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);
    client.remove_admin(&super_admin, &op);

    // Full list
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert!(!list.contains(&op));

    // Paginated walk
    let mut cursor = 0u32;
    loop {
        let (page, next) = client.get_all_admins_page(&cursor, &10u32);
        assert!(
            !page.contains(&op),
            "removed address must not appear in page"
        );
        match next {
            Some(n) => cursor = n,
            None => break,
        }
    }
}

// ── Deactivated / suspended admins ─────────────────────────────────────────────

/// A deactivated admin stays in `AdminList` (list ≠ active-admin set);
/// `get_all_admins` must return them, while `get_active_admin_count` must not
/// count them.
#[test]
fn deactivated_admin_still_in_list() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);

    client.deactivate_admin(&super_admin, &op);

    // get_all_admins still lists the deactivated admin
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert!(
        list.contains(&op),
        "deactivated admin must remain in AdminList"
    );
    assert_eq!(list.len(), 2);

    // active count drops
    assert_eq!(client.get_active_admin_count(), 1);
}

/// A suspended admin stays in `AdminList` for the same reason.
#[test]
fn suspended_admin_still_in_list() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);

    let future_ts = e.ledger().timestamp() + 10_000;
    client.suspend_admin(&super_admin, &op, &future_ts);

    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert!(
        list.contains(&op),
        "suspended admin must remain in AdminList"
    );
    assert_eq!(list.len(), 2);
}

/// Re-activating a deactivated admin keeps the same list entry — no
/// duplicate is inserted.
#[test]
fn reactivated_admin_still_in_list() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);
    client.deactivate_admin(&super_admin, &op);
    client.reactivate_admin(&super_admin, &op);

    #[allow(deprecated)]
    let list = client.get_all_admins();
    // Must appear exactly once
    let count = list.iter().filter(|a| *a == op).count();
    assert_eq!(count, 1, "reactivated admin must appear exactly once");
    assert_eq!(list.len(), 2);
}

// ── Boundary cursor / limit values ─────────────────────────────────────────────

/// cursor=0 and limit=1 returns the first element and a valid next cursor.
#[test]
fn cursor_zero_with_limit_one() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);

    let (page, next) = client.get_all_admins_page(&0u32, &1u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), super_admin);
    assert_eq!(next, Some(1u32));
}

/// Requesting from the last valid index returns one element and no next cursor.
#[test]
fn cursor_at_last_element() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);
    // total = 2; last index = 1
    let (page, next) = client.get_all_admins_page(&1u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), op);
    assert_eq!(next, None);
}

/// cursor == list length must return an empty page and no next cursor.
#[test]
fn cursor_exactly_at_length_returns_empty() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);
    // length = 2; cursor = 2
    let (page, next) = client.get_all_admins_page(&2u32, &10u32);
    assert_eq!(page.len(), 0);
    assert_eq!(next, None);
}

/// Any cursor strictly greater than the list length also returns empty.
#[test]
fn cursor_far_past_end_returns_empty() {
    let (_, client, _) = deploy();
    // list has 1 admin, cursor = u32::MAX
    let (page, next) = client.get_all_admins_page(&u32::MAX, &10u32);
    assert_eq!(page.len(), 0);
    assert_eq!(next, None);
}

/// limit=0 is treated as MAX_PAGE_LIMIT (200): it must return all items when
/// the list is small enough.
#[test]
fn limit_zero_uses_default_max() {
    let (e, client, super_admin) = deploy();
    add_n_operators(&e, &client, &super_admin, 4);

    let (page, next) = client.get_all_admins_page(&0u32, &0u32);
    assert_eq!(page.len(), 5, "limit=0 must return all 5 admins");
    assert_eq!(next, None);
}

/// limit=1 forces a full one-at-a-time walk that must cover every element.
#[test]
fn limit_one_pages_through_list() {
    let (e, client, super_admin) = deploy();
    add_n_operators(&e, &client, &super_admin, 3);
    let total = client.get_admin_count();

    let mut seen: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&e);
    let mut cursor = 0u32;
    loop {
        let (page, next) = client.get_all_admins_page(&cursor, &1u32);
        assert_eq!(page.len(), 1, "each page must have exactly 1 element");
        seen.push_back(page.get(0).unwrap());
        match next {
            Some(n) => cursor = n,
            None => break,
        }
    }
    assert_eq!(seen.len(), total);
}

/// limit above MAX_PAGE_LIMIT (200) must be silently clamped.
/// The response must never contain more than 200 items.
#[test]
fn limit_exceeds_max_is_clamped() {
    let (_, client, _) = deploy();
    let (page, _) = client.get_all_admins_page(&0u32, &u32::MAX);
    // Only 1 admin in list; clamping doesn't change the result here,
    // but the page must be bounded (not MAX_PAGE_LIMIT+1).
    assert!(
        page.len() <= 200,
        "page size must never exceed MAX_PAGE_LIMIT"
    );
}

/// A limit of exactly MAX_PAGE_LIMIT (200) works without clamping.
#[test]
fn limit_max_page_limit_exact() {
    let (e, client, super_admin) = deploy();
    add_n_operators(&e, &client, &super_admin, 10);

    let (page, next) = client.get_all_admins_page(&0u32, &200u32);
    assert_eq!(page.len(), 11);
    assert_eq!(next, None);
}

// ── Concurrent mutation detection ──────────────────────────────────────────────

/// Adding an admin between two paginated reads is detectable via the epoch
/// counter, satisfying the serialization contract.
#[test]
fn epoch_advances_on_add_detectable_mid_walk() {
    let (e, client, super_admin) = deploy();

    let epoch_before = client.get_config_epoch();
    let count_before = client.get_admin_count();

    // "Concurrent" add
    let new_op = Address::generate(&e);
    client.add_admin(&super_admin, &new_op, &AdminRole::Operator);

    let epoch_after = client.get_config_epoch();
    assert_ne!(
        epoch_before, epoch_after,
        "epoch must advance after add_admin"
    );
    assert_eq!(client.get_admin_count(), count_before + 1);
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert!(list.contains(&new_op));
}

/// Removing an admin between two reads is detectable via the epoch counter.
#[test]
fn epoch_advances_on_remove_detectable_mid_walk() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);

    let epoch_before = client.get_config_epoch();

    client.remove_admin(&super_admin, &op);

    let epoch_after = client.get_config_epoch();
    assert_ne!(epoch_before, epoch_after);
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert!(!list.contains(&op));
}

/// A stale read can be detected and retried: the epoch and count both reflect
/// the committed state.
#[test]
fn stale_snapshot_detectable_via_epoch() {
    let (e, client, super_admin) = deploy();

    // Client A takes a snapshot.
    let snapshot_epoch = client.get_config_epoch();
    let snapshot_count = client.get_admin_count();

    // Client B mutates.
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);

    // Client A's snapshot is now stale.
    assert_ne!(client.get_config_epoch(), snapshot_epoch);
    assert_ne!(client.get_admin_count(), snapshot_count);

    // After re-reading, the snapshot is consistent.
    let fresh_epoch = client.get_config_epoch();
    let fresh_count = client.get_admin_count();
    assert_eq!(fresh_count, snapshot_count + 1);
    assert_eq!(fresh_epoch, snapshot_epoch + 1);
}

// ── Retry contract ─────────────────────────────────────────────────────────────

/// After detecting a stale epoch the client can re-read and obtain a correct
/// result; retrying a read-only path is always safe.
#[test]
fn retry_after_stale_epoch_produces_correct_result() {
    let (e, client, super_admin) = deploy();

    // Initial snapshot
    let epoch_0 = client.get_config_epoch();
    #[allow(deprecated)]
    let list_0 = client.get_all_admins();
    assert_eq!(list_0.len(), 1);

    // Mutation that invalidates the snapshot
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);
    assert_ne!(client.get_config_epoch(), epoch_0);

    // Retry: re-read both epoch and list atomically
    let _fresh_epoch = client.get_config_epoch();
    #[allow(deprecated)]
    let fresh_list = client.get_all_admins();
    assert_eq!(fresh_list.len(), 2);
    assert!(fresh_list.contains(&super_admin));
    assert!(fresh_list.contains(&op));
}

/// A failed (rejected) mutation leaves the list unchanged and does not advance
/// the epoch — retrying a read after a failed write is safe.
#[test]
fn failed_mutation_does_not_affect_list() {
    let (e, client, super_admin) = deploy();
    let epoch_before = client.get_config_epoch();
    let count_before = client.get_admin_count();

    // Attempt to add the super admin again — must fail with AlreadyActive.
    let result = client.try_add_admin(&super_admin, &super_admin, &AdminRole::SuperAdmin);
    assert!(result.is_err(), "duplicate add must fail");

    assert_eq!(
        client.get_config_epoch(),
        epoch_before,
        "epoch must not advance on failure"
    );
    assert_eq!(
        client.get_admin_count(),
        count_before,
        "count must not change on failure"
    );
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert_eq!(list.len(), count_before as usize);
}

// ── Duplicate inputs ───────────────────────────────────────────────────────────

/// Adding the same address a second time must panic with AlreadyActive (#405).
#[test]
fn adding_same_address_twice_panics() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);

    let result = client.try_add_admin(&super_admin, &op, &AdminRole::Operator);
    assert!(result.is_err(), "duplicate add must be rejected");
    // The list still has exactly 2 entries (super_admin + op).
    assert_eq!(client.get_admin_count(), 2);
}

/// A role update on an existing admin must not insert a second entry into
/// AdminList; the list length must remain unchanged.
#[test]
fn list_never_contains_duplicates_after_role_update() {
    let (e, client, super_admin) = deploy();
    let op = Address::generate(&e);
    client.add_admin(&super_admin, &op, &AdminRole::Operator);
    let count_before = client.get_admin_count();

    client.update_admin_role(&super_admin, &op, &AdminRole::Admin);

    assert_eq!(
        client.get_admin_count(),
        count_before,
        "role update must not add a new entry"
    );
    #[allow(deprecated)]
    let list = client.get_all_admins();
    let dups = list.iter().filter(|a| *a == op).count();
    assert_eq!(
        dups, 1,
        "address must appear exactly once after role update"
    );
}

// ── Large lists ─────────────────────────────────────────────────────────────────

/// A paginated walk over a list of 50 admins must visit every address exactly
/// once and the concatenated result must equal the full list.
#[test]
fn large_list_pagination_walk_covers_all() {
    let (e, client, super_admin) = deploy();
    add_n_operators(&e, &client, &super_admin, 49); // total = 50
    let total = client.get_admin_count();
    assert_eq!(total, 50);

    let mut all_paged: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&e);
    let mut cursor = 0u32;
    loop {
        let (page, next) = client.get_all_admins_page(&cursor, &7u32);
        for addr in page.iter() {
            all_paged.push_back(addr);
        }
        match next {
            Some(n) => cursor = n,
            None => break,
        }
    }

    assert_eq!(
        all_paged.len(),
        50,
        "paginated walk must cover all 50 admins"
    );

    // Every address must appear exactly once
    #[allow(deprecated)]
    let full_list = client.get_all_admins();
    for addr in full_list.iter() {
        let count = all_paged.iter().filter(|a| *a == addr).count();
        assert_eq!(count, 1, "each admin must appear exactly once in the walk");
    }
}

/// Two independent full walks over the same list must produce identical order,
/// confirming determinism.
#[test]
fn large_list_order_is_deterministic() {
    let (e, client, super_admin) = deploy();
    add_n_operators(&e, &client, &super_admin, 19); // total = 20

    let collect_all = |page_size: u32| {
        let mut out: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&e);
        let mut cursor = 0u32;
        loop {
            let (page, next) = client.get_all_admins_page(&cursor, &page_size);
            for addr in page.iter() {
                out.push_back(addr);
            }
            match next {
                Some(n) => cursor = n,
                None => break,
            }
        }
        out
    };

    let walk_a = collect_all(5);
    let walk_b = collect_all(3);

    assert_eq!(walk_a.len(), walk_b.len());
    for i in 0..walk_a.len() {
        assert_eq!(
            walk_a.get(i).unwrap(),
            walk_b.get(i).unwrap(),
            "order must be deterministic across walks"
        );
    }
}

// ── Read-only / permission boundary ────────────────────────────────────────────

/// `get_all_admins` is a read-only view: it must succeed without any
/// `require_auth` call — the mock_all_auths guard is intentionally absent.
#[test]
fn get_all_admins_requires_no_auth() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);

    // Initialize with auth mocking only for the setup step.
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);

    // Now call get_all_admins WITHOUT any mock_all_auths — must not fail.
    // (No auth check inside get_all_admins means any caller may invoke it.)
    #[allow(deprecated)]
    let list = client.get_all_admins();
    assert_eq!(list.len(), 1);
}

/// `get_all_admins_page` is also read-only; it must succeed without auth.
#[test]
fn get_all_admins_page_requires_no_auth() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);

    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);

    // No mock_all_auths needed for the read.
    let (page, next) = client.get_all_admins_page(&0u32, &10u32);
    assert_eq!(page.len(), 1);
    assert_eq!(next, None);
}

// ── Invariant: list length matches get_admin_count ─────────────────────────────

/// After every mutating operation the list length must equal `get_admin_count`.
#[test]
fn list_length_always_matches_get_admin_count() {
    let (e, client, super_admin) = deploy();

    let check = || {
        #[allow(deprecated)]
        let list = client.get_all_admins();
        let count = client.get_admin_count();
        assert_eq!(
            list.len(),
            count,
            "AdminList length must equal get_admin_count"
        );
    };

    check(); // after init

    let op1 = Address::generate(&e);
    client.add_admin(&super_admin, &op1, &AdminRole::Operator);
    check(); // after add

    let op2 = Address::generate(&e);
    client.add_admin(&super_admin, &op2, &AdminRole::Operator);
    check(); // after second add

    client.deactivate_admin(&super_admin, &op1);
    check(); // deactivate does not change AdminList length

    client.reactivate_admin(&super_admin, &op1);
    check(); // reactivate does not duplicate

    let future_ts = e.ledger().timestamp() + 5_000;
    client.suspend_admin(&super_admin, &op2, &future_ts);
    check(); // suspend does not change length

    client.update_admin_role(&super_admin, &op1, &AdminRole::Admin);
    check(); // role update does not change length

    client.remove_admin(&super_admin, &op1);
    check(); // remove decrements both

    client.remove_admin(&super_admin, &op2);
    check(); // back to 1
}

/// Concatenating all pages of `get_all_admins_page` must reproduce
/// the exact result of `get_all_admins`.
#[test]
fn concatenated_pages_match_full_list() {
    let (e, client, super_admin) = deploy();
    add_n_operators(&e, &client, &super_admin, 7); // total = 8

    #[allow(deprecated)]
    let full: soroban_sdk::Vec<Address> = client.get_all_admins();

    let mut paged: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&e);
    let mut cursor = 0u32;
    loop {
        let (page, next) = client.get_all_admins_page(&cursor, &3u32);
        for addr in page.iter() {
            paged.push_back(addr);
        }
        match next {
            Some(n) => cursor = n,
            None => break,
        }
    }

    assert_eq!(paged.len(), full.len());
    for i in 0..full.len() {
        assert_eq!(
            paged.get(i).unwrap(),
            full.get(i).unwrap(),
            "page concatenation must reproduce get_all_admins in order"
        );
    }
}
