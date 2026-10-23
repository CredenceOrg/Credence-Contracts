//! # Indexed registry storage — issue #1629
//!
//! The registry used to keep the forward mapping, the reverse mapping, the
//! `AllowNonInterface` flags and the complete `RegisteredIdentities` vector in
//! **instance** storage. Every call therefore loaded the whole registry, and
//! `register` / `remove` scanned and rewrote an unbounded vector.
//!
//! This module pins the replacement design:
//!
//! * mappings live in **persistent** storage and are TTL-bumped on read/write;
//! * the identity list is an insertion index — `IdentityCount` in instance,
//!   `IdentityAt(u32)` and `IdentityIndex(Address)` in persistent;
//! * `remove` is swap-and-pop, `get_identities_page` reads only its slice, and
//!   `register` no longer scans or rewrites the list.
//!
//! ## About the cost measurements
//!
//! The native test host meters its own in-memory ledger map, so the *absolute*
//! CPU cost of any storage access grows slowly with the total number of ledger
//! entries in the environment — independently of what the contract does. To
//! isolate the property that matters, the cost tests below put a small registry
//! and a large registry in the **same** environment and compare the same
//! operation on each. Both calls then see exactly the same host overhead, so
//! the only remaining difference is registry size.

#![allow(clippy::disallowed_macros)]

extern crate std;

use crate::storage::DataKey;
use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

const T0: u64 = 1_700_000_000;

fn setup(env: &Env) -> (CredenceRegistryClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

/// Register one identity with a fresh bond contract and the interface check
/// skipped. Returns the identity.
fn register_one(env: &Env, client: &CredenceRegistryClient<'_>) -> Address {
    let identity = Address::generate(env);
    client.register(&identity, &Address::generate(env), &true);
    identity
}

/// Register `n` identities and return them in registration order.
fn register_many(
    env: &Env,
    client: &CredenceRegistryClient<'_>,
    n: u32,
) -> soroban_sdk::Vec<Address> {
    let mut identities = soroban_sdk::Vec::new(env);
    for _ in 0..n {
        identities.push_back(register_one(env, client));
    }
    identities
}

/// Measure the CPU instructions consumed by `f`.
///
/// The budget counters are cleared immediately before the call so the result
/// covers only the work performed by `f`.
fn measure_cpu<F: FnOnce()>(env: &Env, f: F) -> u64 {
    let mut budget = env.cost_estimate().budget();
    budget.reset_unlimited();
    budget.reset_tracker();
    f();
    env.cost_estimate().budget().cpu_instruction_cost()
}

// ─────────────────────────────────────────────────────────────────────────────
// Index correctness
// ─────────────────────────────────────────────────────────────────────────────

/// Remove uses swap-and-pop: the tail entry takes the freed slot, so pages stay
/// gap-free and every surviving identity appears exactly once.
#[test]
fn page_order_stable_after_removal() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let identity_a = Address::generate(&env);
    let identity_b = Address::generate(&env);
    let identity_c = Address::generate(&env);
    let identity_d = Address::generate(&env);

    client.register(&identity_a, &Address::generate(&env), &true);
    client.register(&identity_b, &Address::generate(&env), &true);
    client.register(&identity_c, &Address::generate(&env), &true);
    client.register(&identity_d, &Address::generate(&env), &true);

    let before = client.get_identities_page(&0, &10);
    assert_eq!(before.len(), 4, "all four identities are indexed");
    assert_eq!(before.get(0).unwrap(), identity_a);
    assert_eq!(before.get(1).unwrap(), identity_b);
    assert_eq!(before.get(2).unwrap(), identity_c);
    assert_eq!(before.get(3).unwrap(), identity_d);

    client.remove(&identity_b);

    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 3, "the removed identity is gone");
    assert_eq!(client.get_identity_count(), 3);

    // Swap-and-pop: the last entry (D) fills B's former position (index 1).
    assert_eq!(page.get(0).unwrap(), identity_a);
    assert_eq!(page.get(1).unwrap(), identity_d);
    assert_eq!(page.get(2).unwrap(), identity_c);

    // Each survivor appears exactly once.
    for identity in [&identity_a, &identity_c, &identity_d] {
        let occurrences = page.iter().filter(|a| a == identity).count();
        assert_eq!(occurrences, 1, "each identity must appear exactly once");
    }
    assert!(
        !client.is_registered(&identity_b),
        "removed identity must not be registered"
    );
}

/// Re-registering an identity after removal reuses the single freed slot: the
/// count returns to one and the pages contain no duplicate.
#[test]
fn register_remove_register_same_identity() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let identity = Address::generate(&env);
    let first_bond = Address::generate(&env);
    let second_bond = Address::generate(&env);

    env.ledger().set_timestamp(T0);
    client.register(&identity, &first_bond, &true);
    assert_eq!(client.get_identity_count(), 1);

    client.remove(&identity);
    assert_eq!(client.get_identity_count(), 0);

    env.ledger().set_timestamp(T0 + 100);
    let entry = client.register(&identity, &second_bond, &true);
    assert_eq!(entry.bond_contract, second_bond);
    assert_eq!(entry.registered_at, T0 + 100);

    assert_eq!(
        client.get_identity_count(),
        1,
        "re-registration must not leave a stale index entry"
    );

    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), identity);

    // The next registration appends at position 1, proving the old tail slot
    // was removed rather than left as a gap.
    let other = register_one(&env, &client);
    assert_eq!(client.get_identity_count(), 2);
    let page = client.get_identities_page(&0, &10);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), identity);
    assert_eq!(page.get(1).unwrap(), other);
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage layout
// ─────────────────────────────────────────────────────────────────────────────

/// The per-identity mappings live in persistent storage, never in the instance
/// entry that every invocation loads.
#[test]
fn mappings_in_persistent_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let bond = Address::generate(&env);
    client.register(&identity, &bond, &true);

    env.as_contract(&contract_id, || {
        let identity_key = DataKey::IdentityToBond(identity.clone());
        let bond_key = DataKey::BondToIdentity(bond.clone());
        let flag_key = DataKey::AllowNonInterface(bond.clone());
        let at_key = DataKey::IdentityAt(0);
        let index_key = DataKey::IdentityIndex(identity.clone());

        assert!(
            env.storage().persistent().has(&identity_key),
            "forward mapping must be persistent"
        );
        assert!(
            !env.storage().instance().has(&identity_key),
            "forward mapping must not be in the instance entry"
        );
        assert!(
            env.storage().persistent().has(&bond_key),
            "reverse mapping must be persistent"
        );
        assert!(
            !env.storage().instance().has(&bond_key),
            "reverse mapping must not be in the instance entry"
        );
        assert!(
            env.storage().persistent().has(&flag_key),
            "opt-out flag must be persistent"
        );
        assert!(
            env.storage().persistent().has(&at_key),
            "index position must be persistent"
        );
        assert!(
            env.storage().persistent().has(&index_key),
            "reverse index must be persistent"
        );

        // Only the fixed-size counter stays in the instance entry.
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::IdentityCount)
            .unwrap();
        assert_eq!(count, 1);
    });
}

/// `remove` drops every per-identity key, including the index pair.
#[test]
fn remove_clears_all_persistent_keys() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let identity = Address::generate(&env);
    let bond = Address::generate(&env);
    client.register(&identity, &bond, &true);
    client.remove(&identity);

    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKey::IdentityToBond(identity.clone())));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKey::BondToIdentity(bond.clone())));
        assert!(!env.storage().persistent().has(&DataKey::IdentityAt(0)));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKey::IdentityIndex(identity.clone())));

        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::IdentityCount)
            .unwrap();
        assert_eq!(count, 0);
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Cost independence
// ─────────────────────────────────────────────────────────────────────────────

/// `is_paused()` reads one instance key, so its cost must depend only on the
/// environment — never on the number of registered identities. Allowed drift:
/// 5%.
#[test]
fn instance_cost_independent_of_registry_size() {
    let env = Env::default();

    // Both registries live in the same environment, so the host's per-access
    // ledger overhead is identical for both measurements.
    let (large, _admin_large) = setup(&env);
    let (small, _admin_small) = setup(&env);

    register_one(&env, &small);
    register_many(&env, &large, 500);
    assert_eq!(large.get_identity_count(), 500);
    assert_eq!(small.get_identity_count(), 1);

    let large_cpu = measure_cpu(&env, || {
        assert!(!large.is_paused());
    });
    let small_cpu = measure_cpu(&env, || {
        assert!(!small.is_paused());
    });

    std::println!(
        "is_paused(): 1 identity = {small_cpu} CPU, 500 identities = {large_cpu} CPU ({}% of the 1-identity cost)",
        ((large_cpu as u128) * 100) / (small_cpu as u128)
    );

    let allowed = small_cpu + small_cpu / 20; // +5%
    assert!(
        large_cpu <= allowed,
        "is_paused() cost must not grow with registry size: \
         registry of 1 = {small_cpu} CPU, registry of 500 = {large_cpu} CPU"
    );
}

/// `register` performs a fixed amount of storage work, so registering into a
/// registry of 500 must cost the same as registering into a registry of 1.
/// Allowed drift: 10%.
#[test]
fn register_cost_constant() {
    let env = Env::default();

    let (large, _admin_large) = setup(&env);
    let (small, _admin_small) = setup(&env);

    register_one(&env, &small);
    register_many(&env, &large, 499);

    // 2nd registration into the small registry.
    let small_identity = Address::generate(&env);
    let small_bond = Address::generate(&env);
    let small_cpu = measure_cpu(&env, || {
        small.register(&small_identity, &small_bond, &true);
    });

    // 500th registration into the large registry.
    let large_identity = Address::generate(&env);
    let large_bond = Address::generate(&env);
    let large_cpu = measure_cpu(&env, || {
        large.register(&large_identity, &large_bond, &true);
    });

    assert_eq!(small.get_identity_count(), 2);
    assert_eq!(large.get_identity_count(), 500);

    std::println!(
        "register(): 2nd = {small_cpu} CPU, 500th = {large_cpu} CPU ({}% of the 2nd-registration cost)",
        ((large_cpu as u128) * 100) / (small_cpu as u128)
    );

    let allowed = small_cpu + small_cpu / 10; // +10%
    assert!(
        large_cpu <= allowed,
        "register cost must not grow with registry size: \
         2nd = {small_cpu} CPU, 500th = {large_cpu} CPU"
    );
}
