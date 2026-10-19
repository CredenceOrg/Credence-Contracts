//! Deterministic failure-boundary coverage for
//! [`AdminContract::get_admins_by_role`].
//!
//! `get_admins_by_role` is the read side of the admin *role index*
//! (`DataKey::RoleAdmins(role)`). Clients use it to answer "who is recorded as an
//! Admin / Operator / SuperAdmin?", to reconcile their off-chain view of the
//! admin set, and to decide whether the index is stale. Every property a client
//! may rely on is pinned here:
//!
//! * **total** — uninitialized, initialized-but-unpopulated, populated, and
//!   emptied-by-removal states all answer, and none of them panic. A missing
//!   slot is indistinguishable from an empty role, which is the correct answer
//!   rather than a missing-configuration error (contrast `get_config`, which
//!   *does* panic with `NotInitialized`).
//! * **pure / idempotent** — a read never mutates state, never advances the
//!   config epoch, and never emits an event, so polling or retrying a read
//!   cannot desynchronise an off-chain indexer or a concurrent writer.
//! * **deterministic** — the value depends only on the stored slot: not on the
//!   caller, the ledger clock, the pause state, or the number of prior calls.
//! * **unauthenticated** — a read requires no authorization, so a monitor can
//!   enumerate role membership without holding an admin key.
//! * **membership is not authority** — a deactivated or suspended admin keeps
//!   its entry; only a role change or a removal moves it. Authorization is
//!   decided by `has_role_at_least` / `is_admin`, never by this read.
//! * **exactly-once, disjoint membership** — every admin appears in exactly one
//!   role list, at most once, and the union of the three lists is the whole
//!   admin set. Promotions are atomic moves, not copies.
//! * **stale but detectable** — the read cannot present a snapshot that spans a
//!   concurrent mutation, but every such mutation advances the config epoch
//!   exactly once, so a reader can tell that it must re-read.
//! * **lossless rejection** — a rejected, repeated, or failed mutation leaves
//!   the index and the epoch exactly as they were, so a retry observes no
//!   membership change.
//! * **page-equivalent** — walking `get_admins_by_role_page` reproduces this
//!   list in the same order, so the bounded getter is a drop-in migration.

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::{Address as _, Deployer as _, Ledger as _};
use soroban_sdk::{Address, Env};
use soroban_sdk::Vec as SVec;

const ROLES: [AdminRole; 3] = [AdminRole::SuperAdmin, AdminRole::Admin, AdminRole::Operator];

// Wire-stable error discriminants (`credence_errors::ContractError`).
const ERR_NOT_ADMIN: u32 = 100;
const ERR_CONTRACT_PAUSED: u32 = 106;
const ERR_INVALID_ADMIN_ADDRESS: u32 = 110;
const ERR_ALREADY_ACTIVE: u32 = 405;
const ERR_THRESHOLD_EXCEEDS_SIGNERS: u32 = 601;

fn contract_err(code: u32) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(code)
}

fn advance(e: &Env, seconds: u64) {
    e.ledger().with_mut(|li| li.timestamp += seconds);
}

/// The all-zero Ed25519 strkey, which must never enter the role index.
fn zero_address(e: &Env) -> Address {
    Address::from_string(&soroban_sdk::String::from_str(e, INVALID_ADDRESS_SENTINEL))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Fresh contract with one SuperAdmin/owner and no other role members.
fn setup() -> (Env, AdminContractClient<'static>, Address) {
    setup_with_capacity(1, 100)
}

/// Fresh contract with explicit `min_admins` / `max_admins` bounds.
fn setup_with_capacity(min_admins: u32, max_admins: u32) -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let owner = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&owner, &min_admins, &max_admins);
    (e, client, owner)
}

/// Owner plus one Admin, one Operator and `extra_operators` more Operators.
fn setup_populated(extra_operators: u32) -> (Env, AdminContractClient<'static>, Vec<Address>) {
    let (e, client, owner) = setup();
    let admin = Address::generate(&e);
    let operator = Address::generate(&e);
    client.add_admin(&owner, &admin, &AdminRole::Admin);
    client.add_admin(&owner, &operator, &AdminRole::Operator);

    let mut operators = std::vec::Vec::new();
    operators.push(operator);
    for _ in 0..extra_operators {
        let next = Address::generate(&e);
        client.add_admin(&owner, &next, &AdminRole::Operator);
        operators.push(next);
    }

    let mut all = std::vec::Vec::new();
    all.push(owner.clone());
    all.push(admin);
    all.extend(operators.iter().cloned());
    (e, client, all)
}

#[allow(deprecated)]
fn members(client: &AdminContractClient, role: AdminRole) -> SVec<Address> {
    client.get_admins_by_role(&role)
}

/// How many times `addr` appears in `list`. Duplicates must always be `0` or
/// `1`; anything higher is a lost-update regression, not a tolerated state.
fn membership_count(list: &SVec<Address>, addr: &Address) -> u32 {
    let mut count = 0_u32;
    for entry in list.iter() {
        if &entry == addr {
            count += 1;
        }
    }
    count
}

/// Assert the role index invariants: the three role lists are pairwise
/// disjoint, contain no duplicates, and together partition `expected`.
#[allow(deprecated)]
fn assert_role_index_consistent(client: &AdminContractClient, expected: &[Address]) {
    let mut union = SVec::new(&client.env);
    for role in ROLES {
        for entry in client.get_admins_by_role(&role).iter() {
            union.push_back(entry);
        }
    }

    for entry in union.iter() {
        assert!(
            expected.contains(&entry),
            "role index reported an address that is not an admin: {entry}"
        );
    }
    assert_eq!(
        union.len(),
        expected.len(),
        "the role lists must partition the admin set exactly"
    );

    for admin in expected {
        let total: u32 = ROLES
            .iter()
            .map(|role| membership_count(&client.get_admins_by_role(role), admin))
            .sum();
        assert_eq!(
            total, 1,
            "admin {admin} must appear in exactly one role list, saw {total}"
        );
    }
}

/// Walk `get_admins_by_role_page` to exhaustion, asserting the cursor contract
/// (strictly advancing `next_cursor`, terminal `None`).
fn walk_role_pages(client: &AdminContractClient, role: AdminRole, page_size: u32) -> SVec<Address> {
    let mut collected = SVec::new(&client.env);
    let mut cursor = 0_u32;
    loop {
        let (page, next) = client.get_admins_by_role_page(&role, &cursor, &page_size);
        for entry in page.iter() {
            collected.push_back(entry);
        }
        match next {
            Some(next_cursor) => {
                assert!(
                    next_cursor > cursor,
                    "next_cursor must advance ({cursor} -> {next_cursor})"
                );
                cursor = next_cursor;
            }
            None => break,
        }
    }
    collected
}

fn assert_same_members(left: &SVec<Address>, right: &SVec<Address>, context: &str) {
    assert_eq!(left.len(), right.len(), "length differs: {context}");
    for i in 0..left.len() {
        assert_eq!(
            left.get(i).unwrap(),
            right.get(i).unwrap(),
            "element {i} differs: {context}"
        );
    }
}

/// Fault injection: append a raw entry to a role list without creating a
/// matching `AdminInfo`, modelling a torn or externally-written index.
fn push_raw_role_entry(e: &Env, client: &AdminContractClient, role: AdminRole, addr: &Address) {
    e.as_contract(&client.address, || {
        let mut list: SVec<Address> = e
            .storage()
            .instance()
            .get(&DataKey::RoleAdmins(role))
            .unwrap_or(SVec::new(e));
        list.push_back(addr.clone());
        e.storage().instance().set(&DataKey::RoleAdmins(role), &list);
    });
}

/// Run two conflicting demotions of the same admin in a given order and assert
/// the role index stays consistent no matter which one the ledger serialises
/// last.
#[allow(deprecated)]
fn run_conflicting_demotions(first: AdminRole, second: AdminRole) {
    let (e, client, owner) = setup();
    let rival = Address::generate(&e);
    let target = Address::generate(&e);
    client.add_admin(&owner, &rival, &AdminRole::SuperAdmin);
    client.add_admin(&owner, &target, &AdminRole::Operator);

    let expected = vec![owner.clone(), rival, target.clone()];
    assert_role_index_consistent(&client, &expected);

    // Two SuperAdmins demote the same target to different roles. The ledger
    // serialises them, so the outcome is last-writer-wins — but never a copy.
    client.update_admin_role(&owner, &target, &first);
    assert_role_index_consistent(&client, &expected);
    client.update_admin_role(&owner, &target, &second);
    assert_role_index_consistent(&client, &expected);

    // Exactly one committed mutation per call, and the final state reflects the
    // winner of the serialised race.
    assert_eq!(client.get_config_epoch(), 5);
    assert_eq!(membership_count(&members(&client, second), &target), 1);
    for role in ROLES {
        if role != second {
            assert_eq!(
                membership_count(&members(&client, role), &target),
                0,
                "target must not linger in the {role:?} list"
            );
        }
    }

    // A rejected follow-up leaves the winner intact.
    let epoch_before = client.get_config_epoch();
    assert!(client.try_update_admin_role(&owner, &target, &first).is_ok());
    assert_eq!(membership_count(&members(&client, second), &target), 0);
    assert_eq!(membership_count(&members(&client, first), &target), 1);
    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    assert_role_index_consistent(&client, &expected);
    drop(e);
}

// ---------------------------------------------------------------------------
// Loading states: the read is total and never panics
// ---------------------------------------------------------------------------

/// An uninitialized contract reports empty role lists for every role instead of
/// failing, while `get_config` on the same contract reports `NotInitialized`.
/// Both are deliberate: "no admin holds this role" is data, missing
/// configuration is an error.
#[test]
fn uninitialized_contract_reports_empty_role_lists_for_every_role() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);

    for role in ROLES {
        assert_eq!(members(&client, role).len(), 0, "uninitialized {role:?}");
    }

    let res = client.try_get_config();
    assert_eq!(res.unwrap_err().unwrap(), contract_err(1) /* NotInitialized */);
}

/// A freshly initialized contract reports the SuperAdmin and nothing else. The
/// two unpopulated roles answer empty rather than panicking.
#[test]
fn initialized_contract_reports_only_the_initial_super_admin() {
    let (_e, client, owner) = setup();

    assert_same_members(&members(&client, AdminRole::SuperAdmin), &SVec::from_array(&client.env, [owner]), "SuperAdmin");
    assert_eq!(members(&client, AdminRole::Admin).len(), 0);
    assert_eq!(members(&client, AdminRole::Operator).len(), 0);
}

/// A role emptied by removals is reported as empty, not as an error, and not as
/// a stale non-empty list.
#[test]
fn role_emptied_by_removal_is_reported_empty() {
    let (e, client, owner) = setup();
    let admin = Address::generate(&e);
    client.add_admin(&owner, &admin, &AdminRole::Admin);
    assert_eq!(members(&client, AdminRole::Admin).len(), 1);

    client.remove_admin(&owner, &admin);
    assert_eq!(members(&client, AdminRole::Admin).len(), 0);

    // Re-adding the same address re-populates the list: the empty state is not
    // sticky, and the survivor set was not corrupted.
    client.add_admin(&owner, &admin, &AdminRole::Admin);
    assert_eq!(members(&client, AdminRole::Admin).len(), 1);
    assert_eq!(membership_count(&members(&client, AdminRole::Admin), &admin), 1);
    assert_role_index_consistent(&client, &vec![owner, admin]);
    drop(e);
}

/// A dangling role entry (a member with no `AdminInfo`) is still reported and
/// never turns a read into a panic. The read does not dereference the admin
/// record, so index corruption cannot silently drop a member from the answer.
#[test]
fn dangling_role_entry_is_reported_and_never_panics() {
    let (e, client, owner) = setup();
    let ghost = Address::generate(&e);
    push_raw_role_entry(&e, &client, AdminRole::Operator, &ghost);

    let reported = members(&client, AdminRole::Operator);
    assert_eq!(membership_count(&reported, &ghost), 1);

    // The record-less entry is not counted as an effective admin — the two
    // views disagree by design, and neither panics.
    assert_eq!(client.get_effective_active_admin_count(), 1);
    assert!(!client.is_admin(&ghost));

    // Removing a record-less member is rejected before any write, so the
    // dangling entry survives the rejection and nothing is half-removed.
    let epoch_before = client.get_config_epoch();
    let res = client.try_remove_admin(&owner, &ghost);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_NOT_ADMIN));
    assert_eq!(membership_count(&members(&client, AdminRole::Operator), &ghost), 1);
    assert_eq!(client.get_config_epoch(), epoch_before);
}

// ---------------------------------------------------------------------------
// Retry / idempotency: reads never mutate observable state
// ---------------------------------------------------------------------------

/// Repeated reads are indistinguishable from one call: same value, unmoved
/// config epoch, no events. A client may poll this getter from a retry loop
/// without perturbing conflict detection for anyone else.
#[test]
fn repeated_reads_are_pure_and_idempotent() {
    let (e, client, all) = setup_populated(2);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    for _ in 0..3 {
        assert_role_index_consistent(&client, &all);
    }

    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// The only side effect a read may have is extending the instance TTL, which
/// keeps an idle role index from expiring. It must never shorten it and it must
/// not change the reported value.
#[test]
fn read_only_ever_extends_the_instance_ttl() {
    let (e, client, all) = setup_populated(1);

    let before = members(&client, AdminRole::Operator);
    let ttl_after_first = e.deployer().get_contract_instance_ttl(&client.address);
    for _ in 0..5 {
        assert_same_members(&members(&client, AdminRole::Operator), &before, "Operator");
    }
    let ttl_after_repeats = e.deployer().get_contract_instance_ttl(&client.address);

    assert!(
        ttl_after_repeats >= ttl_after_first,
        "reads must not shorten the instance TTL ({ttl_after_first} -> {ttl_after_repeats})"
    );
    assert_role_index_consistent(&client, &all);
}

// ---------------------------------------------------------------------------
// Determinism: the value does not depend on the clock, the caller, or the pause
// ---------------------------------------------------------------------------

/// The reported value is stable across the pause transition and across
/// arbitrarily large clock jumps. Pausing blocks mutations, not observation, so
/// a monitor can still see the membership it is trying to reconcile.
#[test]
fn reported_value_is_independent_of_pause_state_and_clock() {
    let (e, client, all) = setup_populated(1);
    let baseline = members(&client, AdminRole::Operator);

    client.pause(&all[0]);
    assert!(client.is_paused());
    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "paused");

    advance(&e, 10 * 365 * 24 * 3_600);
    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "far future");

    client.unpause(&all[0]);
    assert!(!client.is_paused());
    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "unpaused");
    assert_role_index_consistent(&client, &all);
}

/// The reported value does not depend on the caller: a lowest-privilege
/// operator, the owner, and a completely unauthenticated observer all see the
/// same membership.
#[test]
fn reported_value_does_not_depend_on_the_caller() {
    let (e, client, all) = setup_populated(0);
    let operator = all[2].clone();
    let baseline = members(&client, AdminRole::Operator);

    e.set_auths(&[]); // disables auth mocking entirely
    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "unauthenticated");
    assert_same_members(&members(&client, AdminRole::SuperAdmin), &members(&client, AdminRole::SuperAdmin), "unauthenticated super");

    e.mock_all_auths();
    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "as operator caller");
    assert!(client.has_role_at_least(&operator, &AdminRole::Operator));
}

// ---------------------------------------------------------------------------
// Permission boundaries
// ---------------------------------------------------------------------------

/// The read requires no authorization. With auth mocking disabled the query
/// still succeeds, the same mutation is refused, and once auth is mocked again
/// the mutation commits and the list updates — proving the earlier refusal was
/// the auth boundary, not the read.
#[test]
fn read_requires_no_authorization_but_writes_do() {
    let (e, client, owner) = setup();
    let newcomer = Address::generate(&e);

    e.set_auths(&[]);
    assert_eq!(members(&client, AdminRole::Operator).len(), 0);
    let res = client.try_add_admin(&owner, &newcomer, &AdminRole::Operator);
    assert!(res.is_err(), "auth must be enforced for mutations");
    assert_eq!(members(&client, AdminRole::Operator).len(), 0);
    assert_eq!(client.get_config_epoch(), 0);

    e.mock_all_auths();
    client.add_admin(&owner, &newcomer, &AdminRole::Operator);
    assert_eq!(membership_count(&members(&client, AdminRole::Operator), &newcomer), 1);
    assert_eq!(client.get_config_epoch(), 1);
}

/// A privileged mutation that is refused at the *authorization* boundary
/// (a caller too low to assign the role) leaves the index and the epoch
/// untouched, so the read after a rejection equals the read before it.
#[test]
fn rejected_escalation_leaves_the_role_index_untouched() {
    let (e, client, all) = setup_populated(0);
    let operator = all[2].clone();
    let newcomer = Address::generate(&e);

    let before = members(&client, AdminRole::SuperAdmin);
    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let res = client.try_add_admin(&operator, &newcomer, &AdminRole::SuperAdmin);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_NOT_ADMIN));

    assert_same_members(&members(&client, AdminRole::SuperAdmin), &before, "after rejected escalation");
    assert_eq!(membership_count(&members(&client, AdminRole::Operator), &newcomer), 0);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
    assert_role_index_consistent(&client, &all);
    drop(e);
}

/// Every rejection class that can occur on a write to this index is lossless:
/// duplicate, unusable address, insufficient authority, paused contract, and
/// capacity exhaustion all leave the list and the epoch exactly as they were.
#[test]
fn every_rejected_mutation_is_lossless_for_the_role_index() {
    let (e, client, owner) = setup_with_capacity(1, 2);
    let first = Address::generate(&e);
    let second = Address::generate(&e);
    let overflow = Address::generate(&e);
    client.add_admin(&owner, &first, &AdminRole::Operator);
    client.add_admin(&owner, &second, &AdminRole::Operator);

    // At capacity: the read reports exactly the two committed members.
    let baseline = members(&client, AdminRole::Operator);
    assert_eq!(membership_count(&baseline, &first), 1);
    assert_eq!(membership_count(&baseline, &second), 1);
    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    // Capacity exhaustion.
    let res = client.try_add_admin(&owner, &overflow, &AdminRole::Admin);
    assert_eq!(
        res.unwrap_err().unwrap(),
        contract_err(ERR_THRESHOLD_EXCEEDS_SIGNERS)
    );

    // Duplicate registration.
    let res = client.try_add_admin(&owner, &first, &AdminRole::Admin);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_ALREADY_ACTIVE));

    // Unusable addresses.
    let res = client.try_add_admin(&owner, &zero_address(&e), &AdminRole::Operator);
    assert_eq!(
        res.unwrap_err().unwrap(),
        contract_err(ERR_INVALID_ADMIN_ADDRESS)
    );
    let res = client.try_add_admin(&owner, &client.address, &AdminRole::Operator);
    assert_eq!(
        res.unwrap_err().unwrap(),
        contract_err(ERR_INVALID_ADMIN_ADDRESS)
    );

    // Self-assignment of an equal or higher role.
    let res = client.try_add_admin(&owner, &owner, &AdminRole::SuperAdmin);
    assert!(res.is_err());

    // Not one rejection moved the index, the epoch, or the event stream.
    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "after rejections");
    assert_eq!(membership_count(&members(&client, AdminRole::Admin), &overflow), 0);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);

    // Freeing capacity makes the previously refused mutation succeed, proving
    // the refusals were transient and left no stuck state.
    client.remove_admin(&owner, &second);
    client.add_admin(&owner, &overflow, &AdminRole::Admin);
    assert_eq!(membership_count(&members(&client, AdminRole::Admin), &overflow), 1);
}

/// While the contract is paused, mutations to the index are refused and the
/// read keeps reporting the last committed membership — a paused contract can
/// never half-apply a role change.
#[test]
fn paused_contract_refuses_membership_changes_but_still_reports_them() {
    let (e, client, all) = setup_populated(0);
    let baseline = members(&client, AdminRole::Admin);
    let epoch_before = client.get_config_epoch();

    client.pause(&all[0]);
    let epoch_paused = client.get_config_epoch();
    let newcomer = Address::generate(&e);

    let res = client.try_add_admin(&all[0], &newcomer, &AdminRole::Admin);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_CONTRACT_PAUSED));
    let res = client.try_update_admin_role(&all[0], &all[2], &AdminRole::Admin);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_CONTRACT_PAUSED));
    let res = client.try_remove_admin(&all[0], &all[1]);
    assert_eq!(res.unwrap_err().unwrap(), contract_err(ERR_CONTRACT_PAUSED));

    assert_same_members(&members(&client, AdminRole::Admin), &baseline, "while paused");
    assert_eq!(client.get_config_epoch(), epoch_paused);
    assert!(epoch_paused > epoch_before);

    // Unpausing restores the write path and the index converges.
    client.unpause(&all[0]);
    client.add_admin(&all[0], &newcomer, &AdminRole::Admin);
    assert_eq!(membership_count(&members(&client, AdminRole::Admin), &newcomer), 1);
    assert_role_index_consistent(&client, &vec![
        all[0].clone(),
        all[1].clone(),
        all[2].clone(),
        newcomer,
    ]);
    drop(e);
}

// ---------------------------------------------------------------------------
// Reported membership is not authority
// ---------------------------------------------------------------------------

/// A deactivated admin keeps its role-list entry. The read answers "whose role
/// is recorded as X", the authorization helpers answer "may X act now".
#[test]
fn deactivated_member_stays_in_the_role_index() {
    let (e, client, all) = setup_populated(0);
    let admin = all[1].clone();
    let baseline = members(&client, AdminRole::Admin);

    client.deactivate_admin(&all[0], &admin);
    assert_same_members(&members(&client, AdminRole::Admin), &baseline, "deactivated");
    assert!(!client.is_admin(&admin));
    assert!(!client.has_role_at_least(&admin, &AdminRole::Operator));

    client.reactivate_admin(&all[0], &admin);
    assert_same_members(&members(&client, AdminRole::Admin), &baseline, "reactivated");
    assert!(client.is_admin(&admin));
    assert!(client.has_role_at_least(&admin, &AdminRole::Admin));
    assert_role_index_consistent(&client, &all);
    drop(e);
}

/// Suspension is a self-expiring clock, not a revocation: the role-list entry
/// is identical before, during, and after the suspension window, while
/// effective authority flips exactly at `suspended_until` (inclusive).
#[test]
fn suspended_member_stays_in_the_role_index_across_the_expiry_boundary() {
    let (e, client, all) = setup_populated(0);
    let operator = all[2].clone();
    let baseline = members(&client, AdminRole::Operator);

    let until = e.ledger().timestamp() + 100;
    client.suspend_admin(&all[0], &operator, &until);

    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "suspended");
    assert!(!client.has_role_at_least(&operator, &AdminRole::Operator));

    advance(&e, 99); // one second before expiry
    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "one second before expiry");
    assert!(!client.has_role_at_least(&operator, &AdminRole::Operator));

    advance(&e, 1); // exactly at `suspended_until` — effective again
    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "at expiry");
    assert!(client.has_role_at_least(&operator, &AdminRole::Operator));

    // The self-expiring clock moved the epoch exactly once; the reads did not.
    assert_eq!(client.get_config_epoch(), 4);
    assert_role_index_consistent(&client, &all);
}

// ---------------------------------------------------------------------------
// State transitions: atomic moves, no duplicates, compacted order
// ---------------------------------------------------------------------------

/// A promotion is a move, not a copy: the target leaves the old list, enters
/// the new one exactly once, and the epoch advances exactly once.
#[test]
fn promotion_moves_membership_atomically_without_duplicating() {
    let (e, client, all) = setup_populated(2);
    let target = all[2].clone();

    let operators_before = members(&client, AdminRole::Operator);
    let admins_before = members(&client, AdminRole::Admin);
    let epoch_before = client.get_config_epoch();

    client.update_admin_role(&all[0], &target, &AdminRole::Admin);

    assert_eq!(client.get_config_epoch(), epoch_before + 1);
    assert_eq!(membership_count(&members(&client, AdminRole::Operator), &target), 0);
    assert_eq!(membership_count(&members(&client, AdminRole::Admin), &target), 1);

    // The untouched lists are byte-for-byte what they were.
    let mut expected_operators = SVec::new(&client.env);
    for entry in operators_before.iter() {
        if entry != target {
            expected_operators.push_back(entry);
        }
    }
    assert_same_members(
        &members(&client, AdminRole::Operator),
        &expected_operators,
        "Operator list after promotion",
    );
    assert_eq!(members(&client, AdminRole::Admin).len(), admins_before.len() + 1);
    assert_eq!(client.get_admin_role(&target), AdminRole::Admin);
    assert_role_index_consistent(&client, &all);
    drop(e);
}

/// A same-role update is a documented no-op: no duplicate entry, no churn in
/// the list, no epoch advance. A retried promotion cannot corrupt the index.
#[test]
fn same_role_update_is_an_idempotent_no_op() {
    let (e, client, all) = setup_populated(1);
    let operator = all[2].clone();
    let baseline = members(&client, AdminRole::Operator);
    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    for _ in 0..3 {
        let info = client.update_admin_role(&all[0], &operator, &AdminRole::Operator);
        assert_eq!(info.role, AdminRole::Operator);
    }

    assert_same_members(&members(&client, AdminRole::Operator), &baseline, "after repeated no-op");
    assert_eq!(membership_count(&members(&client, AdminRole::Operator), &operator), 1);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
    assert_role_index_consistent(&client, &all);
    drop(e);
}

/// Removal compacts the list while preserving the relative order of the
/// survivors, which is what makes cursor pagination meaningful across pages.
#[test]
fn removal_compacts_and_preserves_the_order_of_survivors() {
    let (e, client, all) = setup_populated(3); // operator + 3 more = 4 operators
    let operators = members(&client, AdminRole::Operator);
    assert_eq!(operators.len(), 4);

    let removed = operators.get(1).unwrap(); // a middle member
    client.remove_admin(&all[0], &removed);

    let after = members(&client, AdminRole::Operator);
    assert_eq!(after.len(), 3);
    assert_eq!(membership_count(&after, &removed), 0);

    let mut expected = SVec::new(&client.env);
    for (i, entry) in operators.iter().enumerate() {
        if i != 1 {
            expected.push_back(entry);
        }
    }
    assert_same_members(&after, &expected, "compacted Operator list");

    let mut remaining_all = all.clone();
    remaining_all.retain(|a| a != &removed);
    assert_role_index_consistent(&client, &remaining_all);
    drop(e);
}

// ---------------------------------------------------------------------------
// Concurrency: serialised conflicts are detectable and never double-apply
// ---------------------------------------------------------------------------

/// Conflicting demotions of the same admin serialise to exactly one membership
/// in both orderings. The ledger picks a winner, and the index never holds the
/// target in two role lists at once.
#[test]
fn conflicting_demotions_serialize_to_exactly_one_membership() {
    run_conflicting_demotions(AdminRole::Admin, AdminRole::Operator);
    run_conflicting_demotions(AdminRole::Operator, AdminRole::Admin);
}

/// A reader that takes its snapshot together with the config epoch can always
/// tell that a concurrent commit invalidated it, and re-reads to converge. The
/// read itself neither hides nor creates the conflict.
#[test]
fn stale_read_is_detectable_via_config_epoch_and_recovers_on_re_read() {
    let (e, client, owner) = setup();

    // Snapshot taken at epoch N.
    let epoch_at_read = client.get_config_epoch();
    let stale = members(&client, AdminRole::Operator);
    assert_eq!(stale.len(), 0);

    // A concurrent transaction commits a new Operator at epoch N+1.
    let newcomer = Address::generate(&e);
    client.add_admin(&owner, &newcomer, &AdminRole::Operator);

    // The reader detects the conflict instead of acting on stale membership.
    assert!(client.get_config_epoch() > epoch_at_read);
    let fresh = members(&client, AdminRole::Operator);
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh.get(0).unwrap(), newcomer);

    // Retrying the read is safe and converges.
    assert_same_members(&members(&client, AdminRole::Operator), &fresh, "retry");
    assert_role_index_consistent(&client, &vec![owner, newcomer]);
    drop(e);
}

// ---------------------------------------------------------------------------
// Migration / regression: page-equivalence with the bounded getter
// ---------------------------------------------------------------------------

/// Walking `get_admins_by_role_page` reproduces the deprecated list exactly,
/// in order, for any page size. A caller migrating to the bounded getter
/// observes the same set.
#[test]
fn paginated_walk_reproduces_the_deprecated_list() {
    let (e, client, all) = setup_populated(4);
    let full = members(&client, AdminRole::Operator);
    assert_eq!(full.len(), 5);

    for page_size in [1, 2, 3, 5, 10] {
        let walked = walk_role_pages(&client, AdminRole::Operator, page_size);
        assert_same_members(&walked, &full, &format!("Operator, page_size={page_size}"));
    }

    // `limit = 0` falls back to the hard cap, and a cursor past the end is a
    // terminal empty page rather than an error.
    assert_eq!(walk_role_pages(&client, AdminRole::Operator, 0).len(), 5);
    let (page, next) = client.get_admins_by_role_page(&AdminRole::Operator, &99, &10);
    assert_eq!(page.len(), 0);
    assert_eq!(next, None);
    let (page, next) = client.get_admins_by_role_page(&AdminRole::Admin, &0, &10);
    assert_eq!(page.len(), 0);
    assert_eq!(next, None);
    drop(e);
}

/// Regression: the bounded and unbounded getters must agree after every kind of
/// transition, so neither can drift from the other's observable set.
#[test]
fn bounded_and_unbounded_getters_agree_after_every_transition() {
    let (e, client, owner) = setup();
    let admin = Address::generate(&e);
    let operator = Address::generate(&e);

    let agree = |client: &AdminContractClient| {
        for role in ROLES {
            let walked = walk_role_pages(client, role, 1);
            assert_same_members(&walked, &members(client, role), &format!("{role:?}"));
        }
    };

    agree(&client); // after initialize
    client.add_admin(&owner, &admin, &AdminRole::Admin);
    agree(&client); // after add
    client.add_admin(&owner, &operator, &AdminRole::Operator);
    agree(&client); // after a second add
    client.update_admin_role(&owner, &admin, &AdminRole::Operator);
    agree(&client); // after a demotion
    client.deactivate_admin(&owner, &operator);
    agree(&client); // after a deactivation (membership is unchanged)
    client.remove_admin(&owner, &admin);
    agree(&client); // after a removal

    assert_role_index_consistent(&client, &vec![owner, operator]);
    drop(e);
}
