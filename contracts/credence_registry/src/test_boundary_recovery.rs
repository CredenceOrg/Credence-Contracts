//! # Boundary and recovery coverage — `contracts/credence_registry/src/lib.rs`
//!
//! Issue #1378: the registry's state-mutating and lookup entry points need
//! deterministic coverage for *boundary* inputs (empty / capped / overflow
//! offsets, repeated lifecycle transitions, malformed code hashes) and for
//! *recovery* paths (retry after rejection, unpause after a freeze, re-bound
//! slots after a hard delete) without losing user data.
//!
//! ## Invariants pinned by this module
//!
//! 1. **No partial writes.** Every rejected mutation leaves storage exactly as
//!    it was: no half-written forward/reverse mapping, no phantom entry in
//!    `RegisteredIdentities`, no orphan `AllowNonInterface`/`BondCodeHash`
//!    value. Rejected calls are therefore safe to retry.
//! 2. **Deterministic error precedence.** `register` performs the interface
//!    check before the duplicate checks, so the same inputs always surface the
//!    same `ContractError` code (408 → 400 → 401).
//! 3. **Pagination is total and order-stable.** `get_identities_page` clamps
//!    `limit` to `MAX_IDENTITIES_PAGE_SIZE`, never overflows on
//!    `offset + limit`, and returns insertion order with no gaps or repeats.
//! 4. **Soft delete preserves, hard delete frees.** `deactivate` keeps the
//!    entry (and its original `registered_at`) readable for recovery;
//!    `remove` frees both identity and bond slots so they can be re-bound.
//! 5. **Pause freezes writes, never data.** While paused, mutations fail with
//!    `ContractPaused` before touching storage, and reads keep working.
//! 6. **Trustless self-registration fails closed.** Registration requires an
//!    admin-pinned 32-byte code hash and an exact match; mismatches and
//!    malformed hashes are rejected atomically and are retryable after the
//!    reference is corrected.
//!
//! All timestamps are set explicitly with `Ledger::set_timestamp` so the
//! assertions are deterministic rather than dependent on the test host clock.

// Test-only diagnostics are exempt from the production no-dynamic-strings
// policy: `lib.rs` gates `deny(clippy::disallowed_macros)` behind
// `not(test)` (issue #713), and assertion messages are exactly the kind of
// diagnostic the exemption exists for. Same pattern as the other
// `tests/*.rs` harnesses in this repository.
#![allow(clippy::disallowed_macros)]

use crate::*;
use credence_errors::ContractError;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Bytes, Env, Symbol, Vec};

/// `limit` cap documented on `get_identities_page` in `lib.rs`.
const PAGE_CAP: u32 = crate::MAX_IDENTITIES_PAGE_SIZE;

/// Deterministic ledger timestamps used by the recovery assertions.
const T0: u64 = 1_700_000_000;
const T1: u64 = T0 + 86_400;

/// Fresh, initialized registry.
fn setup(env: &Env) -> (CredenceRegistryClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Register `n` identities with the interface check skipped and return them in
/// registration order — the order `get_identities_page` must preserve.
fn register_identities(env: &Env, client: &CredenceRegistryClient<'_>, n: u32) -> Vec<Address> {
    let mut registered = Vec::new(env);
    for _ in 0..n {
        let identity = Address::generate(env);
        client.register(&identity, &Address::generate(env), &true);
        registered.push_back(identity);
    }
    registered
}

/// Assert that a `try_*` call fails with `error`'s canonical code.
///
/// Using the numeric `ContractError` code (rather than a bare `is_err()`)
/// keeps the assertions deterministic: a swapped or newly-added earlier check
/// would change the reported code and fail the test loudly.
macro_rules! assert_registry_error {
    ($call:expr, $error:expr) => {{
        let err = $call.unwrap_err().unwrap();
        assert_eq!(
            err,
            soroban_sdk::Error::from_contract_error($error as u32),
            "unexpected contract error code"
        );
    }};
}

// ─────────────────────────────────────────────────────────────────────────────
// Pagination / loading boundaries
// ─────────────────────────────────────────────────────────────────────────────

/// An over-cap `limit` is clamped to `MAX_IDENTITIES_PAGE_SIZE`, and the tail
/// page resumes exactly where the clamped page stopped (no gap, no repeat).
#[test]
fn page_limit_is_clamped_to_the_documented_cap() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // Cap + 1 entries: the smallest registry that spans two pages.
    let registered = register_identities(&env, &client, PAGE_CAP + 1);

    let first = client.get_identities_page(&0, &(PAGE_CAP + 1));
    assert_eq!(first.len(), PAGE_CAP, "limit must be clamped to the cap");
    for i in 0..PAGE_CAP {
        assert_eq!(
            first.get(i).unwrap(),
            registered.get(i).unwrap(),
            "page 1 must stay in insertion order"
        );
    }

    let tail = client.get_identities_page(&PAGE_CAP, &PAGE_CAP);
    assert_eq!(tail.len(), 1, "the cap+1'th entry must land on page 2");
    assert_eq!(tail.get(0).unwrap(), registered.get(PAGE_CAP).unwrap());

    // Boundary: `offset + limit` must not overflow when the caller passes
    // `u32::MAX` as a limit near the end of the list.
    let clamped = client.get_identities_page(&PAGE_CAP, &u32::MAX);
    assert_eq!(clamped.len(), 1);
    assert_eq!(clamped.get(0).unwrap(), registered.get(PAGE_CAP).unwrap());
}

/// Offsets at or past the end return an empty page instead of failing, and the
/// surrounding state remains fully loadable afterwards.
#[test]
fn offsets_at_or_past_the_end_return_an_empty_page() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let registered = register_identities(&env, &client, 3);

    assert_eq!(
        client.get_identities_page(&3, &10).len(),
        0,
        "offset == total"
    );
    assert_eq!(
        client.get_identities_page(&4, &10).len(),
        0,
        "offset > total"
    );
    assert_eq!(
        client.get_identities_page(&u32::MAX, &u32::MAX).len(),
        0,
        "extreme offset must saturate, not overflow"
    );

    // Out-of-range reads are pure: the registry is still intact afterwards.
    let full = client.get_identities_page(&0, &10);
    assert_eq!(full.len(), 3);
    for i in 0..3 {
        assert_eq!(full.get(i).unwrap(), registered.get(i).unwrap());
    }
}

/// `limit == 0` is a valid empty read and does not disturb later pages.
#[test]
fn zero_limit_is_a_valid_empty_read() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    register_identities(&env, &client, 2);

    assert_eq!(client.get_identities_page(&0, &0).len(), 0);
    assert_eq!(client.get_identities_page(&1, &0).len(), 0);
    assert_eq!(client.get_identities_page(&2, &0).len(), 0);

    // Loading still works after the empty reads.
    assert_eq!(client.get_identities_page(&0, &2).len(), 2);
}

/// An empty registry answers every read deterministically.
#[test]
fn empty_registry_reads_are_safe_and_stable() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert_eq!(client.get_identities_page(&0, &PAGE_CAP).len(), 0);
    assert_eq!(client.get_identities_page(&u32::MAX, &u32::MAX).len(), 0);

    #[allow(deprecated)]
    let all = client.get_all_identities();
    assert_eq!(all.len(), 0);

    assert_eq!(client.get_admin(), admin);
    assert!(!client.is_paused());
    assert_eq!(client.get_pause_state().signer_count, 0);
    assert_eq!(client.get_pause_state().threshold, 0);
}

/// Unknown keys fail closed and reads never materialize storage entries.
#[test]
fn unknown_keys_fail_closed_without_creating_state() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let unknown = Address::generate(&env);

    assert!(
        !client.is_registered(&unknown),
        "unknown identity is not registered"
    );
    assert_registry_error!(
        client.try_get_bond_contract(&unknown),
        ContractError::IdentityNotRegistered
    );
    assert_registry_error!(
        client.try_get_identity(&unknown),
        ContractError::BondContractNotRegistered
    );

    // Failed reads must not have created entries.
    assert_eq!(client.get_identities_page(&0, &10).len(), 0);
    assert!(!client.is_registered(&unknown));
}

// ─────────────────────────────────────────────────────────────────────────────
// Lifecycle boundaries: deactivate → reactivate → remove
// ─────────────────────────────────────────────────────────────────────────────

/// `deactivate` is a soft delete: visibility flips, but the forward mapping,
/// the reverse mapping and the original `registered_at` all survive so that a
/// later `reactivate` is a lossless recovery.
#[test]
fn deactivate_is_reversible_and_preserves_registration_metadata() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);
    let bond = Address::generate(&env);

    env.ledger().set_timestamp(T0);
    let created = client.register(&identity, &bond, &true);

    client.deactivate(&identity);
    let stale = client.get_bond_contract(&identity);
    assert!(!stale.active, "deactivated entry must report inactive");
    assert_eq!(stale.identity, identity);
    assert_eq!(
        stale.bond_contract, bond,
        "bond binding must survive deactivation"
    );
    assert_eq!(stale.registered_at, created.registered_at);
    assert_eq!(
        client.get_identity(&bond),
        identity,
        "reverse lookup must survive deactivation"
    );
    assert!(!client.is_registered(&identity));
    assert_eq!(
        client.get_identities_page(&0, &10).len(),
        1,
        "the slot is retained so the entry can be reactivated"
    );

    // Recovery at a later ledger time must not rewrite the original metadata.
    env.ledger().set_timestamp(T1);
    client.reactivate(&identity);
    let recovered = client.get_bond_contract(&identity);
    assert!(recovered.active);
    assert_eq!(
        recovered.registered_at, T0,
        "reactivation must not clobber the original timestamp"
    );
    assert!(client.is_registered(&identity));
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
}

/// Repeated lifecycle transitions are rejected with stable codes and leave the
/// entry exactly as the last successful transition left it.
#[test]
fn repeated_lifecycle_transitions_are_rejected_with_stable_codes() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);
    let bond = Address::generate(&env);

    env.ledger().set_timestamp(T0);
    client.register(&identity, &bond, &true);

    client.deactivate(&identity);
    assert_registry_error!(
        client.try_deactivate(&identity),
        ContractError::AlreadyDeactivated
    );

    client.reactivate(&identity);
    assert_registry_error!(
        client.try_reactivate(&identity),
        ContractError::AlreadyActive
    );

    // The failed transitions were no-ops.
    let entry = client.get_bond_contract(&identity);
    assert!(entry.active);
    assert_eq!(entry.registered_at, T0);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
}

/// Removing a middle entry keeps the survivors in insertion order, and the
/// freed identity/bond pair can be re-bound (recovery after a hard delete).
#[test]
fn remove_preserves_order_of_survivors_and_frees_slots() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    env.ledger().set_timestamp(T0);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let bond_a = Address::generate(&env);
    let bond_b = Address::generate(&env);
    let bond_c = Address::generate(&env);
    client.register(&a, &bond_a, &true);
    client.register(&b, &bond_b, &true);
    client.register(&c, &bond_c, &true);

    client.remove(&b);
    assert_registry_error!(
        client.try_get_bond_contract(&b),
        ContractError::IdentityNotRegistered
    );
    assert_registry_error!(
        client.try_get_identity(&bond_b),
        ContractError::BondContractNotRegistered
    );

    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 2, "only the removed slot may disappear");
    assert_eq!(page.get(0).unwrap(), a, "survivors keep their order");
    assert_eq!(page.get(1).unwrap(), c);

    // Re-registration after a hard delete appends the freed slot at the end.
    env.ledger().set_timestamp(T1);
    client.register(&b, &bond_b, &true);
    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(2).unwrap(), b);
    assert_eq!(client.get_bond_contract(&b).registered_at, T1);

    // Draining the registry one slot at a time must never corrupt the list.
    client.remove(&a);
    client.remove(&b);
    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), c);

    client.remove(&c);
    assert_eq!(client.get_identities_page(&0, &u32::MAX).len(), 0);
    assert_registry_error!(client.try_remove(&c), ContractError::IdentityNotRegistered);
}

/// A failed `remove` is a no-op that can be retried successfully once the
/// identity exists.
#[test]
fn failed_remove_leaves_state_intact_and_is_retryable() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);
    let bond = Address::generate(&env);

    env.ledger().set_timestamp(T0);
    client.register(&identity, &bond, &true);

    let unknown = Address::generate(&env);
    assert_registry_error!(
        client.try_remove(&unknown),
        ContractError::IdentityNotRegistered
    );

    // The rejected call wrote nothing anywhere.
    assert!(client.is_registered(&identity));
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
    assert_eq!(client.get_identity(&bond), identity);

    // The retry with the real identity succeeds.
    client.remove(&identity);
    assert_eq!(client.get_identities_page(&0, &10).len(), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Pause boundary and recovery
// ─────────────────────────────────────────────────────────────────────────────

/// While paused, every state-mutating entry point fails with `ContractPaused`
/// *before* touching storage, and all pre-pause data stays readable.
#[test]
fn pause_blocks_all_mutations_without_losing_state() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    env.ledger().set_timestamp(T0);
    let identity = Address::generate(&env);
    let bond = Address::generate(&env);
    let hash = Bytes::from_array(&env, &[7u8; 32]);
    client.set_bond_code_hash(&hash);
    client.register(&identity, &bond, &true);

    client.pause(&admin);
    assert!(
        client.is_paused(),
        "pause with threshold 0 must be immediate"
    );

    let other_identity = Address::generate(&env);
    let other_bond = Address::generate(&env);
    assert_registry_error!(
        client.try_register(&other_identity, &other_bond, &true),
        ContractError::ContractPaused
    );
    assert_registry_error!(
        client.try_deactivate(&identity),
        ContractError::ContractPaused
    );
    assert_registry_error!(
        client.try_reactivate(&identity),
        ContractError::ContractPaused
    );
    assert_registry_error!(client.try_remove(&identity), ContractError::ContractPaused);
    assert_registry_error!(
        client.try_transfer_admin(&other_identity),
        ContractError::ContractPaused
    );
    assert_registry_error!(
        client.try_register_trustless(&other_bond, &other_identity),
        ContractError::ContractPaused
    );

    // Reads stay available and nothing was mutated or lost.
    assert_eq!(client.get_admin(), admin);
    assert!(client.is_registered(&identity));
    let entry = client.get_bond_contract(&identity);
    assert_eq!(entry.bond_contract, bond);
    assert_eq!(entry.registered_at, T0);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
    assert_eq!(
        client.get_bond_code_hash(),
        hash,
        "reference configuration must survive a pause"
    );
}

/// Recovery: the mutation rejected during the pause succeeds unchanged after
/// `unpause`, proving the rejected attempt held no lock or partial state.
#[test]
fn unpause_restores_mutations_and_releases_retries() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let identity = Address::generate(&env);
    let bond = Address::generate(&env);
    client.register(&identity, &bond, &true);

    client.pause(&admin);
    assert_registry_error!(client.try_remove(&identity), ContractError::ContractPaused);
    assert!(
        client.is_registered(&identity),
        "the blocked retry must not have removed the entry"
    );

    client.unpause(&admin);
    assert!(!client.is_paused());
    assert!(!client.get_pause_state().is_paused);

    // Same call, same inputs — now accepted.
    client.remove(&identity);
    assert_registry_error!(
        client.try_get_bond_contract(&identity),
        ContractError::IdentityNotRegistered
    );
    assert_eq!(client.get_identities_page(&0, &10).len(), 0);

    // Lifecycle and admin operations are fully usable again.
    client.register(&identity, &bond, &true);
    assert!(client.is_registered(&identity));
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

/// Boundary: `deactivate` followed by `remove` on the *same* (already
/// soft-deleted) entry still works, and the hard delete cleans up every key
/// written by the soft delete.
#[test]
fn remove_after_deactivate_cleans_up_all_keys() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);
    let bond = Address::generate(&env);

    client.register(&identity, &bond, &true);
    client.deactivate(&identity);
    client.remove(&identity);

    assert!(!client.is_registered(&identity));
    assert_registry_error!(
        client.try_get_identity(&bond),
        ContractError::BondContractNotRegistered
    );
    assert_eq!(client.get_identities_page(&0, &10).len(), 0);

    // Both slots are free again — the pair can be re-bound.
    let rebound = client.register(&identity, &bond, &true);
    assert!(rebound.active);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Registration atomicity, deterministic errors and retry
// ─────────────────────────────────────────────────────────────────────────────

/// A rejected interface check writes nothing, and the documented opt-out retry
/// (`allow_non_interface = true`) completes the registration end-to-end.
#[test]
fn failed_interface_check_is_atomic_and_retryable() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    env.ledger().set_timestamp(T0);
    let identity = Address::generate(&env);
    let non_contract = Address::generate(&env);

    assert_registry_error!(
        client.try_register(&identity, &non_contract, &false),
        ContractError::UnsupportedInterface
    );

    // No half-written state: neither mapping, nor the identity list, nor the
    // code-hash reference was touched.
    assert_registry_error!(
        client.try_get_bond_contract(&identity),
        ContractError::IdentityNotRegistered
    );
    assert_registry_error!(
        client.try_get_identity(&non_contract),
        ContractError::BondContractNotRegistered
    );
    assert_eq!(client.get_identities_page(&0, &10).len(), 0);
    assert!(client.get_bond_code_hash().is_empty());

    // Retry through the opt-out path succeeds and produces a complete entry.
    let entry = client.register(&identity, &non_contract, &true);
    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, non_contract);
    assert!(entry.active);
    assert_eq!(entry.registered_at, T0);
    assert_eq!(client.get_identity(&non_contract), identity);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
}

/// The interface check precedes the duplicate checks, so identical inputs
/// always produce the same error code: 408 → 400 → 401.
#[test]
fn error_precedence_is_deterministic_for_invalid_and_duplicate_inputs() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let identity = Address::generate(&env);
    let bond = Address::generate(&env);
    client.register(&identity, &bond, &true);

    let foreign = Address::generate(&env);
    let other_identity = Address::generate(&env);

    // Duplicate identity + unsupported interface → interface check wins.
    assert_registry_error!(
        client.try_register(&identity, &foreign, &false),
        ContractError::UnsupportedInterface
    );
    // Same duplicate with the check skipped → duplicate identity.
    assert_registry_error!(
        client.try_register(&identity, &foreign, &true),
        ContractError::IdentityAlreadyRegistered
    );

    // Duplicate bond + unsupported interface → interface check wins.
    assert_registry_error!(
        client.try_register(&other_identity, &bond, &false),
        ContractError::UnsupportedInterface
    );
    // …and with the check skipped → duplicate bond.
    assert_registry_error!(
        client.try_register(&other_identity, &bond, &true),
        ContractError::BondContractAlreadyRegistered
    );

    // Repeated rejected attempts never mutate the registry.
    for _ in 0..3 {
        assert!(client.try_register(&identity, &bond, &true).is_err());
    }
    let page = client.get_identities_page(&0, &10);
    assert_eq!(
        page.len(),
        1,
        "retried failures must not duplicate list entries"
    );
    assert_eq!(page.get(0).unwrap(), identity);
    assert_eq!(client.get_bond_contract(&identity).bond_contract, bond);
    assert!(!client.is_registered(&other_identity));
}

/// A deactivated registration still blocks both re-registration attempts
/// (identity and bond), so a rejected retry cannot silently steal a slot.
#[test]
fn deactivated_slots_still_reject_conflicting_registrations() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);
    let other_identity = Address::generate(&env);
    let bond = Address::generate(&env);
    let other_bond = Address::generate(&env);

    client.register(&identity, &bond, &true);
    client.deactivate(&identity);

    assert_registry_error!(
        client.try_register(&identity, &other_bond, &true),
        ContractError::IdentityAlreadyRegistered
    );
    assert_registry_error!(
        client.try_register(&other_identity, &bond, &true),
        ContractError::BondContractAlreadyRegistered
    );

    // The rejected attempts left the soft-deleted entry untouched and no
    // orphan mapping behind.
    assert!(!client.is_registered(&identity));
    assert_eq!(client.get_bond_contract(&identity).bond_contract, bond);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
    assert_registry_error!(
        client.try_get_identity(&other_bond),
        ContractError::BondContractNotRegistered
    );

    // Recovery path: reactivate, then the identity is usable again.
    client.reactivate(&identity);
    assert!(client.is_registered(&identity));
}

// ─────────────────────────────────────────────────────────────────────────────
// Trustless self-registration boundaries (code-hash verified)
// ─────────────────────────────────────────────────────────────────────────────

/// Minimal bond-contract stand-in for the `register_trustless` path.
///
/// It answers the `get_contract_code_hash` introspection call the registry
/// makes with a **configurable** value, so both the matching and the
/// non-matching branches of the constant-time comparison can be exercised
/// deterministically (including malformed, non-32-byte hashes).
#[soroban_sdk::contract]
pub struct MockBond;

#[soroban_sdk::contractimpl]
impl MockBond {
    /// Set the code hash this stand-in reports to the registry.
    pub fn set_reported_code_hash(e: Env, hash: Bytes) {
        e.storage()
            .instance()
            .set(&Symbol::new(&e, "reported_hash"), &hash);
    }

    /// Introspection entry point expected by `register_trustless`.
    pub fn get_contract_code_hash(e: Env) -> Bytes {
        e.storage()
            .instance()
            .get(&Symbol::new(&e, "reported_hash"))
            .unwrap_or_else(|| Bytes::new(&e))
    }
}

/// Deploy a stand-in bond contract reporting `reported` as its code hash.
fn setup_mock_bond(env: &Env, reported: &Bytes) -> Address {
    let address = env.register(MockBond, ());
    MockBondClient::new(env, &address).set_reported_code_hash(reported);
    address
}

/// Without an admin-pinned reference hash there is nothing to verify against,
/// so self-registration is rejected before any state is written.
#[test]
fn trustless_registration_requires_a_pinned_code_hash() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let identity = Address::generate(&env);
    let mock = setup_mock_bond(&env, &Bytes::from_array(&env, &[7u8; 32]));

    assert_registry_error!(
        client.try_register_trustless(&mock, &identity),
        ContractError::NotInitialized
    );

    // No state was written and no reference was pinned.
    assert_eq!(client.get_identities_page(&0, &10).len(), 0);
    assert!(!client.is_registered(&identity));
    assert!(client.get_bond_code_hash().is_empty());
}

/// Happy path plus idempotent retry: a verified bond registers both mapping
/// directions, and replaying the call returns the original entry unchanged.
#[test]
fn trustless_registration_verifies_the_hash_and_is_idempotent() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let hash = Bytes::from_array(&env, &[7u8; 32]);
    client.set_bond_code_hash(&hash);
    let mock = setup_mock_bond(&env, &hash);
    let identity = Address::generate(&env);

    env.ledger().set_timestamp(T0);
    let entry = client.register_trustless(&mock, &identity);
    assert_eq!(entry.identity, identity);
    assert_eq!(entry.bond_contract, mock);
    assert!(entry.active);
    assert_eq!(entry.registered_at, T0);

    // Both directions of the mapping are persisted.
    assert_eq!(client.get_identity(&mock), identity);
    assert_eq!(client.get_bond_contract(&identity).bond_contract, mock);
    assert!(client.is_registered(&identity));
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
    assert_eq!(client.get_bond_code_hash(), hash);

    // A later replay from the same bond is a no-op that returns the stored
    // entry — no second list entry, no timestamp rewrite (recovery-safe).
    env.ledger().set_timestamp(T1);
    let replay = client.register_trustless(&mock, &identity);
    assert_eq!(replay.registered_at, T0);
    assert_eq!(replay.bond_contract, mock);
    assert_eq!(
        client.get_identities_page(&0, &10).len(),
        1,
        "idempotent retry must not grow the identity list"
    );
}

/// Mismatched, truncated and malformed hashes are all rejected without leaving
/// partial state, and the same call succeeds once the reference is corrected.
#[test]
fn trustless_registration_rejects_bad_hashes_atomically() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let expected = Bytes::from_array(&env, &[7u8; 32]);
    client.set_bond_code_hash(&expected);

    let identity = Address::generate(&env);

    // Same length, different content → verification failure.
    let wrong = setup_mock_bond(&env, &Bytes::from_array(&env, &[9u8; 32]));
    assert_registry_error!(
        client.try_register_trustless(&wrong, &identity),
        ContractError::ContractCodeVerificationFailed
    );

    // Reported hash of the wrong length → rejected by the length guard.
    let short = setup_mock_bond(&env, &Bytes::from_array(&env, &[7u8; 16]));
    assert_registry_error!(
        client.try_register_trustless(&short, &identity),
        ContractError::ContractCodeVerificationFailed
    );

    // Admin-pinned hash of the wrong length → also rejected.
    client.set_bond_code_hash(&Bytes::from_array(&env, &[7u8; 16]));
    let good = setup_mock_bond(&env, &expected);
    assert_registry_error!(
        client.try_register_trustless(&good, &identity),
        ContractError::ContractCodeVerificationFailed
    );

    // A non-contract caller cannot answer the introspection call at all.
    let eoa = Address::generate(&env);
    assert!(
        client.try_register_trustless(&eoa, &identity).is_err(),
        "a caller without a code hash must not be able to self-register"
    );

    // None of the rejected attempts wrote anything.
    assert_eq!(client.get_identities_page(&0, &10).len(), 0);
    assert!(!client.is_registered(&identity));
    assert_registry_error!(
        client.try_get_bond_contract(&identity),
        ContractError::IdentityNotRegistered
    );
    assert_registry_error!(
        client.try_get_identity(&good),
        ContractError::BondContractNotRegistered
    );

    // Recovery: re-pin the correct reference and the same call succeeds.
    client.set_bond_code_hash(&expected);
    let entry = client.register_trustless(&good, &identity);
    assert!(entry.active);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
}

/// Identity and bond uniqueness is enforced for self-registration too, across
/// active, deactivated and hard-deleted states.
#[test]
fn trustless_registration_rejects_reused_slots_and_recovers() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let hash = Bytes::from_array(&env, &[7u8; 32]);
    client.set_bond_code_hash(&hash);

    let bond_a = setup_mock_bond(&env, &hash);
    let bond_b = setup_mock_bond(&env, &hash);
    let identity = Address::generate(&env);
    let other_identity = Address::generate(&env);

    client.register_trustless(&bond_a, &identity);

    // Same bond, different identity → bond slot taken.
    assert_registry_error!(
        client.try_register_trustless(&bond_a, &other_identity),
        ContractError::BondContractAlreadyRegistered
    );
    // Different bond, taken identity → identity slot taken.
    assert_registry_error!(
        client.try_register_trustless(&bond_b, &identity),
        ContractError::IdentityAlreadyRegistered
    );

    // Stale (deactivated) self-registration is rejected until reactivation.
    client.deactivate(&identity);
    assert_registry_error!(
        client.try_register_trustless(&bond_a, &identity),
        ContractError::IdentityAlreadyRegistered
    );
    assert!(!client.is_registered(&identity));

    client.reactivate(&identity);
    let replay = client.register_trustless(&bond_a, &identity);
    assert!(replay.active);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);

    // Hard delete frees both slots: the same bond can bind the identity again.
    client.remove(&identity);
    let rebound = client.register_trustless(&bond_a, &identity);
    assert_eq!(rebound.identity, identity);
    assert_eq!(client.get_identity(&bond_a), identity);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);
}

/// Boundary: `set_bond_code_hash` is deliberately *not* pause-gated (it is a
/// reference-configuration write), so an admin can rotate the pinned hash while
/// mutations are frozen. The pause gate is evaluated first, so the rotation is
/// only *enforced* once the registry is unpaused again.
#[test]
fn code_hash_rotation_while_paused_applies_after_unpause() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let identity = Address::generate(&env);

    let old_hash = Bytes::from_array(&env, &[7u8; 32]);
    client.set_bond_code_hash(&old_hash);
    let stale_bond = setup_mock_bond(&env, &old_hash);

    client.pause(&admin);
    assert!(client.is_paused());

    // Frozen: a correct hash is not enough while the registry is paused.
    assert_registry_error!(
        client.try_register_trustless(&stale_bond, &identity),
        ContractError::ContractPaused
    );

    // Rotation while paused is accepted and observable …
    let new_hash = Bytes::from_array(&env, &[8u8; 32]);
    client.set_bond_code_hash(&new_hash);
    assert_eq!(client.get_bond_code_hash(), new_hash);

    // … but writes stay frozen: the pause gate is checked *before* the code
    // hash is verified, so the stale hash still surfaces as `ContractPaused`.
    assert_registry_error!(
        client.try_register_trustless(&stale_bond, &identity),
        ContractError::ContractPaused
    );

    // Recovery: after unpause the rotation is enforced — the bond reporting the
    // new hash registers …
    client.unpause(&admin);
    assert!(!client.is_paused());
    let updated_bond = setup_mock_bond(&env, &new_hash);
    let entry = client.register_trustless(&updated_bond, &identity);
    assert_eq!(entry.bond_contract, updated_bond);
    assert_eq!(client.get_identities_page(&0, &10).len(), 1);

    // … while the stale bond that still reports the old hash is rejected.
    assert_registry_error!(
        client.try_register_trustless(&stale_bond, &Address::generate(&env)),
        ContractError::ContractCodeVerificationFailed
    );
}
