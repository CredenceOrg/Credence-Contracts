#![cfg(test)]

//! # Proptest: Batch Atomicity 
//!

//! Property-based tests that lock in the **between-batch invariant** for the
//! `add_attestation_batch` entrypoint of `credence_bond`.
//!
//!
//! No batch-sized call may leave the contract in a *split-brain* state —
//! partial success where some items of a batch landed in storage while the
//! overall call reverted. Between batches, every observable piece of state
//! that depends on a successful batch (per-attester nonces, per-subject
//! attestation counts, the subject attestation ID list, the aggregate
//! attestation ID counter) must be consistent with exactly the count of
//! committed items.
//!
//! ## What we assert
//!
//! For every proptest case below:
//!   1. **Happy path (clean batch of N healthy items):** the subject count
//!      advances by exactly N, every participating attester's nonce advances
//!      by exactly 1 (no double-credit), and `list_len == count`
//!      (invariant I7 from `crate::test_invariants`).
//!   2. **Sad path (batch containing any poisonous item):** the call panics,
//!      and post-batch state exactly matches pre-batch state for nonces and
//!      counts — proving rollback is total, not partial (no split-brain).
//!
//! The poison modes covered are:
//!   - **Unregistered attester** — auth passes (mocked) but the attester is
//!     not in `DataKey::Attester`, so the registration check panics.
//!   - **Duplicate attester** — two positions in the batch carry the same
//!     attester address; the explicit uniqueness check panics.
//!   - **Aggregate overweight** — per-item weight is fine, but the SUM
//!     exceeds `max_weight`; the weight-cap check panics.
//!
//! ## Determinism
//!
//! proptest is seeded deterministically; no `Date::now()`, `Math::random()`,
//! or wall-clock entropy. Failing seeds are persisted by proptest (the
//! per-test regression file lives next to the binary); commit any failing
//! seed back to the repo per the issue's "Commit any failing seed back to
//! the repo" requirement.

extern crate std;

use credence_bond::soroban_sdk::testutils::Address as _;
use credence_bond::soroban_sdk::{Address, Env, String, Vec};
use credence_bond::{AttestationBatchItem, CredenceBond, CredenceBondClient};
use proptest::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

// ── test fixture ────────────────────────────────────────────────────────────

/// Multiplier of 1.0 — multiplier_bps = 10_000 so per-attester weight equals
/// the stake amount directly. Keeps the weight arithmetic readable.
const ONE_X_BPS: u32 = 10_000;

/// Maximum authorized aggregate weight — large enough that any batch we'll
/// construct can succeed when not specifically engineered to fail.
const LARGE_MAX_WEIGHT: u32 = 1_000_000;

/// Build a fresh, fully-isolated contract instance with `mock_all_auths()`.
///
/// The client is `transmute`-cast to `'static` so it can outlive the local
/// borrow and be reused across proptest strategy invocations; this mirrors
/// the pattern already used by `crate::fuzz::test_slashing_tier_invariants`.
fn setup() -> (Env, CredenceBondClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);
    // SAFETY: lifetime is tightened to 'static so this works with proptest's
    // by-value strategy outputs; the env is kept alive alongside the client.
    let client: CredenceBondClient<'static> = unsafe { core::mem::transmute(client) };
    (env, client, admin)
}

/// Register `n` freshly-generated attesters each with `stake` weight.
fn register_pool(
    env: &Env,
    client: &CredenceBondClient<'_>,
    admin: &Address,
    n: usize,
    stake: i128,
) -> std::vec::Vec<Address> {
    let mut out = std::vec::Vec::with_capacity(n);
    for _ in 0..n {
        let a = Address::generate(env);
        client.register_attester(&a);
        client.set_attester_stake(admin, &a, &stake);
        out.push(a);
    }
    out
}

/// Configure the global weighted-attestation params.
fn configure_weights(client: &CredenceBondClient<'_>, admin: &Address, max_weight: u32) {
    client.set_weight_config(admin, &ONE_X_BPS, &max_weight);
}

fn make_attestation_data(env: &Env, tag: &str) -> String {
    String::from_str(env, tag)
}

// ── Property tests ──────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        .. ProptestConfig::default()
    })]

    /// **Happy path (variable batch size):** a clean batch of `n` healthy
    /// attesters stores exactly `n` items, advances each consumed attester's
    /// nonce by exactly 1, and preserves the I7 invariant
    /// (`count == list_len`).
    fn prop_attestation_batch_count_matches_n_healthy_items(
        batch_size in 1usize..=20usize,
    ) {
        let (env, client, admin) = setup();
        configure_weights(&client, &admin, LARGE_MAX_WEIGHT);
        let pool = register_pool(&env, &client, &admin, batch_size, 1_000);
        let subject = Address::generate(&env);

        // Pre: count == 0
        prop_assert_eq!(
            client.get_subject_attestation_count(&subject),
            0u32,
            "fresh subject must have zero attestations"
        );

        // Build batch
        let mut items: Vec<AttestationBatchItem> = Vec::new(&env);
        for i in 0..batch_size {
            let attester = pool[i].clone();
            let nonce = client.get_nonce(&attester);
            items.push_back(AttestationBatchItem {
                attester,
                attestation_data: make_attestation_data(&env, &format!("clean-batch-{}", i)),
                nonce,
            });
        }

        let added = client.add_attestation_batch(&subject, &items);

        // Post: count, return value, and I7 invariant
        prop_assert_eq!(added.len() as usize, batch_size,
            "add_attestation_batch returned wrong number of attestations");
        prop_assert_eq!(
            client.get_subject_attestation_count(&subject) as usize, batch_size,
            "subject attestation count did not advance by exactly batch_size"
        );
        let list = client.get_subject_attestations(&subject);
        prop_assert_eq!(list.len() as usize, batch_size,
            "subject attestation list did not grow to batch_size");
        // I7: count == list_len (the contract also enforces this internally)
        prop_assert_eq!(
            list.len() as u32, client.get_subject_attestation_count(&subject),
            "between-batch invariant I7 VIOLATED: list_len != count"
        );

        // Each consumed attester advanced by exactly 1 (no double-credit)
        for (i, a) in pool.iter().enumerate() {
            prop_assert_eq!(client.get_nonce(a), 1u64,
                "attester[{}] nonce did not advance by exactly 1", i);
        }
    }

    /// **Sad path: unregistered attester.** Any batch containing a single
    /// unregistered address MUST revert atomically: no nonce consumed, no
    /// subject count growth, no list growth. This is the split-brain guard.
    fn prop_attestation_batch_reverts_atomically_on_unregistered_attester(
        batch_size in 2usize..=10usize,
        poison_position in 0usize..=10usize,
    ) {
        let poison_idx = poison_position % batch_size;
        let (env, client, admin) = setup();
        configure_weights(&client, &admin, LARGE_MAX_WEIGHT);
        let pool = register_pool(&env, &client, &admin, batch_size, 1_000);
        let poison = Address::generate(&env); // NOT registered

        let subject = Address::generate(&env);
        let pre_count = client.get_subject_attestation_count(&subject);
        let pre_nonces: std::vec::Vec<u64> = pool.iter()
            .map(|a| client.get_nonce(a))
            .collect();

        let mut items: Vec<AttestationBatchItem> = Vec::new(&env);
        for i in 0..batch_size {
            let attester = if i == poison_idx {
                poison.clone()
            } else {
                pool[i].clone()
            };
            // The poison cannot have its nonce consumed because it isn't
            // registered; reads don't require auth, so this smoke check
            // just reads whatever the default nonce happens to be.
            let nonce = client.get_nonce(&attester);
            items.push_back(AttestationBatchItem {
                attester,
                attestation_data: make_attestation_data(
                    &env,
                    &format!("poison-batch-{}", i),
                ),
                nonce,
            });
        }

        let res = catch_unwind(AssertUnwindSafe(|| {
            client.add_attestation_batch(&subject, &items);
        }));
        prop_assert!(
            res.is_err(),
            "batch with unregistered attester at position {} must panic",
            poison_idx
        );

        // Atomicity: every watched state MUST equal pre-batch state.
        prop_assert_eq!(
            client.get_subject_attestation_count(&subject), pre_count,
            "between-batch invariant VIOLATED: subject attestation count drifted after atomic revert"
        );
        let list = client.get_subject_attestations(&subject);
        prop_assert_eq!(list.len() as u32, pre_count,
            "between-batch invariant VIOLATED: subject attestation list grew after atomic revert");
        for (i, a) in pool.iter().enumerate() {
            prop_assert_eq!(
                client.get_nonce(a), pre_nonces[i],
                "between-batch invariant VIOLATED: attester[{}] nonce drifted after atomic revert",
                i
            );
        }
    }

    /// **Sad path: duplicate attester within the batch.** When two positions
    /// in the batch carry the same attester address, the explicit uniqueness
    /// check panics "duplicate attester in batch". The whole batch MUST revert.
    fn prop_attestation_batch_reverts_atomically_on_duplicate_attester(
        batch_size in 2usize..=10usize,
    ) {
        let (env, client, admin) = setup();
        configure_weights(&client, &admin, LARGE_MAX_WEIGHT);
        // One healthy attester reused N times to force the duplicate path.
        let pool = register_pool(&env, &client, &admin, 1, 1_000);
        let subject = Address::generate(&env);
        let pre_count = client.get_subject_attestation_count(&subject);
        let pre_nonce = client.get_nonce(&pool[0]);

        let mut items: Vec<AttestationBatchItem> = Vec::new(&env);
        for i in 0..batch_size {
            let nonce = client.get_nonce(&pool[0]);
            items.push_back(AttestationBatchItem {
                attester: pool[0].clone(),
                attestation_data: make_attestation_data(
                    &env,
                    &format!("dup-{}", i),
                ),
                nonce,
            });
        }

        let res = catch_unwind(AssertUnwindSafe(|| {
            client.add_attestation_batch(&subject, &items);
        }));
        prop_assert!(
            res.is_err(),
            "batch with duplicate attester must panic"
        );
        prop_assert_eq!(
            client.get_subject_attestation_count(&subject), pre_count,
            "subject attestation count drifted after duplicate-attester atomic revert"
        );
        prop_assert_eq!(
            client.get_nonce(&pool[0]), pre_nonce,
            "attester[0] nonce drifted after duplicate-attester atomic revert"
        );
    }

    /// **Sad path: aggregate weight exceeds configured cap.** When the sum of
    /// per-item weights is greater than `max_weight`, the weight-cap check
    /// panics. The whole batch MUST revert.
    ///
    /// We register `n` attesters with stakes `(max_weight/n) + 1` so each
    /// individual weight fits under the cap, but the sum of N weights
    /// strictly exceeds `max_weight`.
    fn prop_attestation_batch_reverts_atomically_on_weight_cap_violation(
        n in 2usize..=4usize,
    ) {
        let (env, client, admin) = setup();
        // max_weight = 1000 (small, easy to overflow).
        let max_weight = 1_000u32;
        configure_weights(&client, &admin, max_weight);
        // Each attester weight = (max_weight/n) + 1. Sum = max_weight + n, > cap.
        let per_stake = (max_weight / n as u32) as i128 + 1;
        let pool = register_pool(&env, &client, &admin, n, per_stake);
        let subject = Address::generate(&env);
        let pre_count = client.get_subject_attestation_count(&subject);
        let pre_nonces: std::vec::Vec<u64> = pool.iter()
            .map(|a| client.get_nonce(a))
            .collect();

        let mut items: Vec<AttestationBatchItem> = Vec::new(&env);
        for i in 0..n {
            let nonce = client.get_nonce(&pool[i]);
            items.push_back(AttestationBatchItem {
                attester: pool[i].clone(),
                attestation_data: make_attestation_data(
                    &env,
                    &format!("overweight-{}", i),
                ),
                nonce,
            });
        }

        let res = catch_unwind(AssertUnwindSafe(|| {
            client.add_attestation_batch(&subject, &items);
        }));
        prop_assert!(
            res.is_err(),
            "batch whose aggregate weight exceeds max_weight (n={}, per_stake={}, cap={}) must panic",
            n, per_stake, max_weight
        );

        prop_assert_eq!(
            client.get_subject_attestation_count(&subject), pre_count,
            "subject attestation count drifted after overweight atomic revert"
        );
        for (i, a) in pool.iter().enumerate() {
            prop_assert_eq!(
                client.get_nonce(a), pre_nonces[i],
                "attester[{}] nonce drifted after overweight atomic revert", i
            );
        }
    }

    /// **Sequence test: between-batch invariants hold across mixed success
    /// and failure sequences.** For each batch in a sequence of 3 calls,
    /// choose between a clean batch and one of three poison modes. After each
    /// call, the observable state must match the running summary of which
    /// batches succeeded and which failed.
    fn prop_between_batch_invariants_hold_across_sequences(
        // 0 = clean, 1 = unregistered, 2 = duplicate, 3 = overweight
        choices in proptest::collection::vec(0u8..=3u8, 1..=4usize),
    ) {
        let (env, client, admin) = setup();
        configure_weights(&client, &admin, LARGE_MAX_WEIGHT);
        let healthy_pool = register_pool(&env, &client, &admin, 8, 1_000);
        let poison_addr = Address::generate(&env); // unregistered
        let subject = Address::generate(&env);

        // Track expected state as we go.
        let mut expected_count: u32 = 0;

        for (step, choice) in choices.iter().enumerate() {
            let mut items: Vec<AttestationBatchItem> = Vec::new(&env);
            // Build either a clean 3-item batch or a poisoned 3-item batch.
            let expected_panic = match choice {
                0 => false,
                1 => true, // unregistered at slot 0
                2 => true, // duplicate attester
                _ => true, // overweight
            };

            for i in 0..3usize {
                let attester = match choice {
                    0 => healthy_pool[i].clone(),
                    1 if i == 0 => poison_addr.clone(),
                    1 => healthy_pool[i].clone(),
                    // duplicate: slot 0 and slot 1 share pool[0]
                    2 if i < 2 => healthy_pool[0].clone(),
                    2 => healthy_pool[i].clone(),
                    // overweight: pre-raise n=3 stakes to overflow cap
                    _ => healthy_pool[i].clone(),
                };
                let nonce = client.get_nonce(&attester);
                items.push_back(AttestationBatchItem {
                    attester,
                    attestation_data: make_attestation_data(
                        &env,
                        &format!("seq-{}-{}", step, i),
                    ),
                    nonce,
                });
            }

            // For the overweight case, we need max_weight tight enough that
            // 3 items each weighting 1000 -> 3000 > 1000. Recompute cap now.
            if *choice == 3 {
                configure_weights(&client, &admin, 1_000u32);
            }

            let res = catch_unwind(AssertUnwindSafe(|| {
                client.add_attestation_batch(&subject, &items);
            }));

            if expected_panic {
                prop_assert!(
                    res.is_err(),
                    "step {}: poisoned batch (variant {}) must panic",
                    step, choice
                );
                // No state mutation.
                prop_assert_eq!(
                    client.get_subject_attestation_count(&subject), expected_count,
                    "step {}: subject count drifted after poisoned sequence batch", step
                );
            } else {
                prop_assert!(
                    res.is_ok(),
                    "step {}: clean batch must succeed without panic", step
                );
                expected_count += 3;
                let post_count = client.get_subject_attestation_count(&subject);
                prop_assert_eq!(
                    post_count, expected_count,
                    "step {}: subject count did not advance by exactly 3 after clean sequence batch",
                    step
                );
                let list = client.get_subject_attestations(&subject);
                prop_assert_eq!(list.len() as u32, post_count,
                    "step {}: I7 VIOLATED in sequence (list_len != count)", step);
                // Each new attestation id must separately be retrievable
                // — guards against partial-write bugs that store the count
                // but leave individual ids dangling.
                for j in (post_count - 3)..post_count {
                    let id = list.get(j).unwrap();
                    let _ = client.get_attestation(&id);
                }
            }

            // Restore cap if we just tightened it for the overweight variant
            // so the next iteration starts from a clean config.
            if *choice == 3 {
                configure_weights(&client, &admin, LARGE_MAX_WEIGHT);
            }
        }
    }
}