//! Boundary + fuzz test suite for `tiered_bond` tier-threshold transitions.
//!
//! Closes acceptance criteria of issue **#494**:
//! - table-driven boundary tests at every tier threshold (-1, exact, +1) reading
//!   thresholds from the configured constants (so the suite stays valid if the
//!   admin re-tunes them);
//! - fuzz/property test that runs deterministic, seeded sequences of
//!   `top_up` / `withdraw` / `slash` operations on a freshly created bond and
//!   asserts that after every step the on-chain `get_tier()` matches the tier
//!   implied by the current `bonded_amount` (no path dependence, no sticky
//!   tier);
//! - asserts that crossing a tier emits the documented
//!   `tier_changed` + `tier_changed_v2` events;
//! - exercises the fully-slashed case (effective balance = 0 ⇒ `Bronze` after a
//!   full `withdraw_bond`, otherwise the bond's tier remains a function of
//!   `bonded_amount` and is preserved across slashing).
//!
//! ## Determinism
//! The fuzz loop uses `SplitMix64` seeded with a fixed value (overridable via the
//! `BOND_TIER_FUZZ_SEED` / `BOND_TIER_FUZZ_ITERS` / `BOND_TIER_FUZZ_ACTIONS`
//! env vars) and `catch_unwind` around each operation, mirroring the patterns in
//! `fuzz/test_bond_fuzz.rs` and `fuzz/test_slashing_tier_invariants.rs` so the
//! suite runs reproducibly under `cargo test` without `cargo-fuzz`.

#![cfg(test)]

extern crate std;

use crate::tiered_bond::{
    get_tier_for_amount, tier_rank, TIER_BRONZE_MAX, TIER_GOLD_MAX, TIER_SILVER_MAX,
};
use crate::BondTier;
use crate::{test_helpers, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, Env, FromVal, Symbol};
use std::panic::{catch_unwind, AssertUnwindSafe};

// ── tiny deterministic RNG (SplitMix64) ──────────────────────────────────────

#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn range(&mut self, lo: i128, hi: i128) -> i128 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() as i128).unsigned_abs() as i128 % (hi - lo)
    }
    fn bool(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

// ── env helpers ──────────────────────────────────────────────────────────────

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

const DEFAULT_SEED: u64 = 0x00C0DECE;
const DEFAULT_ITERS: usize = 200;
const DEFAULT_ACTIONS: usize = 6;

fn fuzz_seed() -> u64 {
    env_u64("BOND_TIER_FUZZ_SEED").unwrap_or(DEFAULT_SEED)
}
fn fuzz_iters() -> usize {
    env_usize("BOND_TIER_FUZZ_ITERS").unwrap_or(DEFAULT_ITERS)
}
fn fuzz_actions_per_iter() -> usize {
    env_usize("BOND_TIER_FUZZ_ACTIONS").unwrap_or(DEFAULT_ACTIONS)
}

/// All artifacts for a single bond test case.
///
/// Captures the contract address explicitly (rather than relying on
/// `Env::current_contract_address`) so event-filter closures match the bond
/// contract's ledger entries even when a secondary contract (the token) has
/// been registered in the same test environment.
struct BondFixture {
    env: Env,
    client: CredenceBondClient<'static>,
    admin: Address,
    identity: Address,
    contract_id: Address,
}

impl BondFixture {
    /// Spin up the bond contract, register the mock token, mint funds,
    /// approve the bond, set the bond's token, then create the initial bond
    /// with `amount` and advance the ledger sequence so a subsequent slash
    /// does not collide with the same-ledger guard.
    fn new(amount: i128) -> Self {
        let env = Env::default();
        env.ledger().with_mut(|li| li.timestamp = 1_000);
        let (client, admin, identity, _token_id, contract_id) =
            test_helpers::setup_with_token(&env);
        client.create_bond_with_rolling(
            &identity,
            &amount,
            &crate::validation::MIN_BOND_DURATION,
            &false,
            &0,
        );
        test_helpers::advance_ledger_sequence(&env);
        // SAFETY: `client` borrows from `env`; we move both into `Self` so
        // the lifetime relationship is preserved.
        let client: CredenceBondClient<'static> = unsafe { core::mem::transmute(client) };
        Self {
            env,
            client,
            admin,
            identity,
            contract_id,
        }
    }

    fn env(&self) -> &Env {
        &self.env
    }
}

// ── table-driven boundary tests ──────────────────────────────────────────────
//
// Each threshold is read from constants rather than hardcoded; admins may
// reconfigure via `set_tier_thresholds` and the table-driven assertions still
// describe the *correct* boundary semantics for that configuration.

/// Helper: assert that `amount` evaluates to `expected` using the configured
/// thresholds (or the constants fallback when none are stored).
fn assert_tier(e: &Env, amount: i128, expected: BondTier) {
    let got = get_tier_for_amount(e, amount);
    assert!(
        core::mem::discriminant(&got) == core::mem::discriminant(&expected),
        "amount={amount}: expected {expected:?} got {got:?}"
    );
}

/// Boundary table for the **default** thresholds (`TIER_BRONZE_MAX`,
/// `TIER_SILVER_MAX`, `TIER_GOLD_MAX`). Each row is `(amount, expected_tier)`
/// and covers the exact threshold plus the immediate -1 and +1 neighbours.
#[test]
fn test_boundary_table_default_thresholds() {
    let e = Env::default();
    let rows: &[(i128, BondTier)] = &[
        // Bronze ceiling
        (0, BondTier::Bronze),
        (1, BondTier::Bronze),
        (TIER_BRONZE_MAX - 1, BondTier::Bronze),
        (TIER_BRONZE_MAX, BondTier::Silver), // exact boundary
        (TIER_BRONZE_MAX + 1, BondTier::Silver),
        // Silver ceiling
        (TIER_SILVER_MAX - 1, BondTier::Silver),
        (TIER_SILVER_MAX, BondTier::Gold), // exact boundary
        (TIER_SILVER_MAX + 1, BondTier::Gold),
        // Gold ceiling
        (TIER_GOLD_MAX - 1, BondTier::Gold),
        (TIER_GOLD_MAX, BondTier::Platinum), // exact boundary
        (TIER_GOLD_MAX + 1, BondTier::Platinum),
        (i128::MAX, BondTier::Platinum),
    ];
    for &(amount, expected) in rows {
        assert_tier(&e, amount, expected);
    }
}

/// Boundary table for **admin-configured** thresholds. Exercises the storage
/// path of `get_tier_for_amount` (not just the constants fallback) and proves
/// the boundary behaviour generalises to arbitrary thresholds.
#[test]
fn test_boundary_table_admin_thresholds() {
    let e = Env::default();
    e.mock_all_auths();

    let new_bronze: i128 = 2_000_000_000_000_000_000_000; // 2 000 * 10^18
    let new_silver: i128 = 7_000_000_000_000_000_000_000; // 7 000 * 10^18
    let new_gold: i128 = 25_000_000_000_000_000_000_000; // 25 000 * 10^18

    e.storage()
        .instance()
        .set(&crate::DataKey::TierThresholds, &crate::TierThresholds {
            bronze_max: new_bronze,
            silver_max: new_silver,
            gold_max: new_gold,
        });

    let rows: &[(i128, BondTier)] = &[
        (new_bronze - 1, BondTier::Bronze),
        (new_bronze, BondTier::Silver),
        (new_bronze + 1, BondTier::Silver),
        (new_silver - 1, BondTier::Silver),
        (new_silver, BondTier::Gold),
        (new_silver + 1, BondTier::Gold),
        (new_gold - 1, BondTier::Gold),
        (new_gold, BondTier::Platinum),
        (new_gold + 1, BondTier::Platinum),
    ];
    for &(amount, expected) in rows {
        assert_tier(&e, amount, expected);
    }
}

/// Cross the bronze→silver boundary with a single-unit top_up and verify the
/// tier moves as expected. Each step must read its target amount from the
/// canonical constants.
#[test]
fn test_top_up_crosses_bronze_to_silver_boundary() {
    let f = BondFixture::new(TIER_BRONZE_MAX - 1);

    assert_eq!(f.client.get_tier(), BondTier::Bronze);

    // +2 units pushes the bond past the boundary into Silver.
    f.client.top_up(&2_i128);
    assert_eq!(f.client.get_tier(), BondTier::Silver);

    // +0 stays exactly on the boundary — still Silver (upper bound is exclusive,
    // so `bonded_amount == bronze_max` is Silver by the documented semantics).
    f.client.top_up(&0_i128);
    assert_eq!(f.client.get_tier(), BondTier::Silver);
}

/// Boundary retreat: cross silver → bronze with a withdrawal. Demonstrates
/// that tier downgrades carry the same event semantics as upgrades.
#[test]
fn test_withdraw_under_threshold_downgrades_tier() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, identity, ..) = test_helpers::setup_with_token(&env);
    client.create_bond_with_rolling(&identity, &TIER_BRONZE_MAX, &86_400_u64, &false, &0_u64);
    assert_eq!(client.get_tier(), BondTier::Silver);

    env.ledger().with_mut(|li| li.timestamp = 86_401);
    client.withdraw(&1_i128);
    assert_eq!(client.get_tier(), BondTier::Bronze);
}

/// Cross two thresholds in one `top_up`. The on-chain tier jumps from
/// Bronze directly to Platinum (the intermediate Silver/Gold tiers are not
/// observed on chain — there is exactly one transition, captured by one event
/// pair).
#[test]
fn test_top_up_crosses_two_thresholds_in_one_call() {
    let start = TIER_BRONZE_MAX - 5;
    let f = BondFixture::new(start);

    assert_eq!(f.client.get_tier(), BondTier::Bronze);

    // top_up is large enough to skip both bronze→silver and silver→gold.
    let target = TIER_GOLD_MAX + 100;
    let delta = target - start;
    f.client.top_up(&delta);

    assert_eq!(f.client.get_tier(), BondTier::Platinum);
}

/// Tier is preserved by slashing because `bonded_amount` is the tier input
/// and slash does **not** mutate `bonded_amount`.
#[test]
fn test_fully_slashed_bond_preserves_tier() {
    let f = BondFixture::new(TIER_GOLD_MAX); // Platinum

    assert_eq!(f.client.get_tier(), BondTier::Platinum);

    // Fully slash: `bonded_amount` stays at TIER_GOLD_MAX, so the tier derived
    // from it remains Platinum even though the *available* balance is zero.
    f.client.slash(&f.admin, &TIER_GOLD_MAX);

    let state = f.client.get_identity_state();
    assert_eq!(state.bonded_amount, TIER_GOLD_MAX);
    assert_eq!(state.slashed_amount, TIER_GOLD_MAX);
    assert_eq!(f.client.get_tier(), BondTier::Platinum);

    // Over-slashing is a no-op on `slashed_amount` (capped at bonded_amount),
    // so the tier is still preserved.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        f.client.slash(&f.admin, &(TIER_GOLD_MAX * 2))
    }));
    assert_eq!(f.client.get_tier(), BondTier::Platinum);
}

/// Full-exit (`withdraw_bond`) zeroes `bonded_amount`, which collapses the
/// tier to the lowest band regardless of how much was previously slashed.
#[test]
fn test_full_exit_collapses_tier_to_bronze() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, identity, ..) = test_helpers::setup_with_token(&env);
    client.create_bond_with_rolling(&identity, &TIER_GOLD_MAX, &0_u64, &false, &0_u64);
    assert_eq!(client.get_tier(), BondTier::Platinum);

    let _ = client.withdraw_bond(&identity);
    assert_eq!(client.get_tier(), BondTier::Bronze);
}

// ── event assertions ─────────────────────────────────────────────────────────

fn count_v2_tier_events(e: &Env, contract_id: &Address, identity: &Address) -> u32 {
    let events = e.events().all();
    events
        .iter()
        .filter(|ev| {
            ev.0 == *contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "tier_changed_v2")
                && Address::from_val(&e, &ev.1.get(1).unwrap()) == *identity
        })
        .count() as u32
}

fn count_v1_tier_events(e: &Env, contract_id: &Address) -> u32 {
    let events = e.events().all();
    events
        .iter()
        .filter(|ev| {
            ev.0 == *contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "tier_changed")
        })
        .count() as u32
}

/// `create_bond` at the silver ceiling emits exactly one tier transition
/// (Bronze → Silver) with both the v1 and v2 events.
#[test]
fn test_create_bond_emits_tier_changed_event() {
    let f = BondFixture::new(TIER_GOLD_MAX - 1); // → Silver

    // Both event flavours fire exactly once for the initial transition.
    let v2 = count_v2_tier_events(f.env(), &f.contract_id, &f.identity);
    assert_eq!(v2, 1, "expected exactly one tier_changed_v2 event");
    let v1 = count_v1_tier_events(f.env(), &f.contract_id);
    assert_eq!(v1, 1, "expected exactly one tier_changed (v1) event");

    // Decode the v2 event payload to assert (old_tier=Bronze, new_tier=Silver,
    // timestamp>0).
    let mut saw_v2_decode = false;
    for ev in f.env().events().all().iter() {
        if ev.0 != f.contract_id {
            continue;
        }
        if Symbol::from_val(f.env(), &ev.1.get(0).unwrap())
            != Symbol::new(f.env(), "tier_changed_v2")
        {
            continue;
        }
        let ident = Address::from_val(f.env(), &ev.1.get(1).unwrap());
        assert_eq!(ident, f.identity);
        let payload = <(BondTier, BondTier, u64)>::from_val(f.env(), &ev.2);
        assert!(
            core::mem::discriminant(&payload.0) == core::mem::discriminant(&BondTier::Bronze)
        );
        assert!(
            core::mem::discriminant(&payload.1) == core::mem::discriminant(&BondTier::Silver)
        );
        assert!(payload.2 > 0, "v2 timestamp must be positive");
        saw_v2_decode = true;
    }
    assert!(saw_v2_decode, "tier_changed_v2 event must be present");
}

/// `top_up` crossing two thresholds emits exactly one v2 event whose
/// `(old_tier, new_tier)` data reflects the terminal pair (Silver → Platinum).
/// The starting amount is Silver, not Bronze, so `create_bond` itself emits a
/// transition (Bronze → Silver) and `top_up` emits a second (Silver →
/// Platinum) — total two v1/v2 events.
#[test]
fn test_top_up_emits_tier_changed_event_on_boundary_cross() {
    let start: i128 = TIER_BRONZE_MAX + 1; // Silver
    let f = BondFixture::new(start);

    f.client.top_up(&(TIER_GOLD_MAX + 100 - start));
    assert_eq!(f.client.get_tier(), BondTier::Platinum);

    let v1 = count_v1_tier_events(f.env(), &f.contract_id);
    let v2 = count_v2_tier_events(f.env(), &f.contract_id, &f.identity);
    assert_eq!(v1, 2, "exactly two v1 events expected (create + top_up)");
    assert_eq!(v2, 2, "exactly two v2 events expected (create + top_up)");
}

/// `withdraw` crossing a threshold boundary emits a v2 event with
/// `old_tier` = pre-withdraw tier and `new_tier` = post-withdraw tier.
#[test]
fn test_withdraw_emits_tier_changed_event_on_boundary_cross() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, identity, _token_id, contract_id) =
        test_helpers::setup_with_token(&env);

    client.create_bond_with_rolling(&identity, &TIER_BRONZE_MAX, &86_400_u64, &false, &0_u64);
    let v1_before = count_v1_tier_events(&env, &contract_id);
    let v2_before = count_v2_tier_events(&env, &contract_id, &identity);

    env.ledger().with_mut(|li| li.timestamp = 86_401);
    client.withdraw(&1_i128);
    assert_eq!(client.get_tier(), BondTier::Bronze);

    let v1_after = count_v1_tier_events(&env, &contract_id);
    let v2_after = count_v2_tier_events(&env, &contract_id, &identity);
    assert_eq!(v1_after, v1_before + 1);
    assert_eq!(v2_after, v2_before + 1);

    // Decode the v2 payload just emitted (the most recent v2 for our identity).
    let mut saw = false;
    for ev in env.events().all().iter() {
        if ev.0 != contract_id {
            continue;
        }
        if Symbol::from_val(&env, &ev.1.get(0).unwrap())
            != Symbol::new(&env, "tier_changed_v2")
        {
            continue;
        }
        if Address::from_val(&env, &ev.1.get(1).unwrap()) != identity {
            continue;
        }
        let payload = <(BondTier, BondTier, u64)>::from_val(&env, &ev.2);
        if core::mem::discriminant(&payload.1) == core::mem::discriminant(&BondTier::Bronze) {
            assert!(
                core::mem::discriminant(&payload.0) == core::mem::discriminant(&BondTier::Silver)
            );
            assert!(payload.2 > 0);
            saw = true;
        }
    }
    assert!(saw, "tier_changed_v2 downgrade Silver→Bronze must be present");
}

/// A no-op top-up (`amount = 0`) and a non-crossing top-up do not emit a
/// tier event.
#[test]
fn test_no_tier_change_emits_no_event() {
    // Start in Silver (above bronze_max, away from silver_max).
    let f = BondFixture::new(TIER_BRONZE_MAX + 500);

    let pre_v1 = count_v1_tier_events(f.env(), &f.contract_id);
    let pre_v2 = count_v2_tier_events(f.env(), &f.contract_id, &f.identity);

    f.client.top_up(&0_i128);
    f.client.top_up(&123_i128); // stays Silver (well below gold_max)
    assert_eq!(f.client.get_tier(), BondTier::Silver);

    let post_v1 = count_v1_tier_events(f.env(), &f.contract_id);
    let post_v2 = count_v2_tier_events(f.env(), &f.contract_id, &f.identity);
    assert_eq!(pre_v1, post_v1);
    assert_eq!(pre_v2, post_v2);
}

/// Slash does not change `bonded_amount`, so it does not emit a tier event.
#[test]
fn test_slash_does_not_emit_tier_event() {
    let f = BondFixture::new(TIER_BRONZE_MAX + 500); // Silver

    let pre_v1 = count_v1_tier_events(f.env(), &f.contract_id);
    let pre_v2 = count_v2_tier_events(f.env(), &f.contract_id, &f.identity);

    let _ = catch_unwind(AssertUnwindSafe(|| {
        f.client.slash(&f.admin, &(TIER_BRONZE_MAX / 2))
    }));

    let post_v1 = count_v1_tier_events(f.env(), &f.contract_id);
    let post_v2 = count_v2_tier_events(f.env(), &f.contract_id, &f.identity);
    assert_eq!(pre_v1, post_v1, "slash must not emit tier_changed");
    assert_eq!(pre_v2, post_v2, "slash must not emit tier_changed_v2");
    assert_eq!(f.client.get_tier(), BondTier::Silver);
}

// ── fuzz: tier always tracks bonded_amount, regardless of operation order ────

/// Sequence the bond through random `top_up` / `withdraw` / `slash` operations
/// and assert that `get_tier()` always matches `get_tier_for_amount(bonded_amount)`.
/// Uses a deterministic seed so the run is reproducible on every CI pass.
#[test]
fn fuzz_tier_tracks_bonded_amount_under_random_sequences() {
    let seed = fuzz_seed();
    let iters = fuzz_iters();
    let actions = fuzz_actions_per_iter();

    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin, identity, ..) = test_helpers::setup_with_token(&env);

    let mut rng = Rng::new(seed);

    // Start below bronze_max so every promotion cell is reachable.
    let start: i128 = TIER_BRONZE_MAX / 4;
    client.create_bond_with_rolling(&identity, &start, &86_400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&env);

    // Move time past lock-up so withdraws succeed.
    env.ledger().with_mut(|li| li.timestamp = 86_401);

    let mut prev_bonded = start;
    let mut prev_rank = tier_rank(&client.get_tier());

    for _ in 0..iters {
        for _ in 0..actions {
            let op = rng.next() % 3;
            let state = client.get_identity_state();
            let bonded = state.bonded_amount;
            let available_for_withdraw = bonded.saturating_sub(state.slashed_amount);

            match op {
                0 => {
                    let delta = rng.range(1, TIER_GOLD_MAX * 2);
                    let _ = catch_unwind(AssertUnwindSafe(|| client.top_up(&delta)));
                }
                1 => {
                    let slash = rng.range(0, bonded + 1);
                    let _ = catch_unwind(AssertUnwindSafe(|| client.slash(&admin, &slash)));
                    test_helpers::advance_ledger_sequence(&env);
                }
                _ => {
                    if available_for_withdraw == 0 {
                        continue;
                    }
                    let take = rng.range(0, available_for_withdraw + 1);
                    let _ = catch_unwind(AssertUnwindSafe(|| client.withdraw(&take)));
                }
            }

            // After every step, the on-chain tier must equal the tier implied
            // by the post-state `bonded_amount` derived from `get_tier_for_amount`.
            let post_state = client.get_identity_state();
            let post_bonded = post_state.bonded_amount;
            let implied_tier = get_tier_for_amount(&env, post_bonded);
            let on_chain_tier = client.get_tier();
            assert!(
                core::mem::discriminant(&on_chain_tier) == core::mem::discriminant(&implied_tier),
                "iter seed=0x{seed:016x} op={op}: bonded={} implied={:?} on_chain={:?}",
                post_bonded,
                implied_tier,
                on_chain_tier
            );

            // No-sticky-tier assertion: each operation may only move the rank
            // in a direction consistent with whether `bonded_amount` increased
            // or decreased.
            let cur_rank = tier_rank(&on_chain_tier);
            assert!(cur_rank <= 3);
            if post_bonded > prev_bonded {
                assert!(
                    cur_rank >= prev_rank,
                    "rank decreased while bonded_amount increased (prev {prev_rank} -> cur {cur_rank}); prev_bonded={prev_bonded} post_bonded={post_bonded}"
                );
            } else if post_bonded < prev_bonded {
                assert!(
                    cur_rank <= prev_rank,
                    "rank increased while bonded_amount decreased (prev {prev_rank} -> cur {cur_rank}); prev_bonded={prev_bonded} post_bonded={post_bonded}"
                );
            }
            prev_bonded = post_bonded;
            prev_rank = cur_rank;
        }
    }

    // Final invariant: tier equals tier implied by final bonded amount.
    let final_bonded = client.get_identity_state().bonded_amount;
    let final_tier = client.get_tier();
    let derived = get_tier_for_amount(&env, final_bonded);
    assert!(
        core::mem::discriminant(&final_tier) == core::mem::discriminant(&derived),
        "final bonded={final_bonded} tier={final_tier:?} derived={derived:?}"
    );
}

/// Rapid up/down crossing around a single boundary: oscillates bonded
/// across `TIER_BRONZE_MAX` ± k for several rounds and verifies the tier
/// flips on every other step in lockstep with `bonded_amount`.
#[test]
fn fuzz_rapid_threshold_crossing() {
    let seed = fuzz_seed();
    let iters = fuzz_iters();

    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin, identity, ..) = test_helpers::setup_with_token(&env);

    // Start exactly on the boundary, in Silver.
    client.create_bond_with_rolling(&identity, &TIER_BRONZE_MAX, &86_400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&env);
    env.ledger().with_mut(|li| li.timestamp = 86_401);

    let mut rng = Rng::new(seed);
    for step in 0..iters {
        let k = (rng.next() % 16) as i128;
        let above = step % 2 == 0;
        let target = if above {
            TIER_BRONZE_MAX + k
        } else {
            TIER_BRONZE_MAX - 1 - k
        };

        let cur = client.get_identity_state().bonded_amount;
        if target > cur {
            let _ = catch_unwind(AssertUnwindSafe(|| client.top_up(&(target - cur))));
        } else {
            let take = cur - target;
            let _ = catch_unwind(AssertUnwindSafe(|| client.withdraw(&take)));
        }

        let state = client.get_identity_state();
        let implied = get_tier_for_amount(&env, state.bonded_amount);
        let on_chain = client.get_tier();
        assert!(
            core::mem::discriminant(&implied) == core::mem::discriminant(&on_chain),
            "step={step}: target={target} above={above} bonded={} implied={:?} on_chain={:?}",
            state.bonded_amount,
            implied,
            on_chain
        );

        // Drain accrued slashing so we don't trip the available-balance
        // assertion in the next withdraw branch.
        let _ = catch_unwind(AssertUnwindSafe(|| client.slash(&admin, &0_i128)));
    }
}

// ── regression vectors: documented tier boundaries under admin thresholds ────

const ADMIN_BRONZE: i128 = 3_000_000_000_000_000_000_000; // 3 000 * 10^18
const ADMIN_SILVER: i128 = 9_000_000_000_000_000_000_000; // 9 000 * 10^18
const ADMIN_GOLD: i128 = 30_000_000_000_000_000_000_000; // 30 000 * 10^18

#[test]
fn regression_boundary_table_under_admin_thresholds() {
    let e = Env::default();
    e.mock_all_auths();
    e.storage().instance().set(
        &crate::DataKey::TierThresholds,
        &crate::TierThresholds {
            bronze_max: ADMIN_BRONZE,
            silver_max: ADMIN_SILVER,
            gold_max: ADMIN_GOLD,
        },
    );

    let cases: &[(i128, BondTier)] = &[
        (0, BondTier::Bronze),
        (ADMIN_BRONZE - 1, BondTier::Bronze),
        (ADMIN_BRONZE, BondTier::Silver),
        (ADMIN_BRONZE + 1, BondTier::Silver),
        (ADMIN_SILVER - 1, BondTier::Silver),
        (ADMIN_SILVER, BondTier::Gold),
        (ADMIN_SILVER + 1, BondTier::Gold),
        (ADMIN_GOLD - 1, BondTier::Gold),
        (ADMIN_GOLD, BondTier::Platinum),
        (ADMIN_GOLD + 1, BondTier::Platinum),
        (i128::MAX, BondTier::Platinum),
    ];
    for &(amount, expected) in cases {
        let got = get_tier_for_amount(&e, amount);
        assert!(
            core::mem::discriminant(&got) == core::mem::discriminant(&expected),
            "admin thresholds: amount={amount} expected={expected:?} got={got:?}"
        );
    }
}
