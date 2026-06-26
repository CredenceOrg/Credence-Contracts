//! Indexer-side invariant tests.
//!
//! Downstream indexers never see contract storage — they see only the event
//! stream. This suite asserts the **replay invariant**: an indexer that folds a
//! pure `apply(event, state)` over the captured event history of a bond MUST
//! reconstruct exactly the `IdentityBond` that lives on-chain. Any divergence
//! means the contract is either missing an event or emitting a mismatched
//! payload — both of which silently break every indexer.
//!
//! The replayer contract (which events carry which fields, and how each mutates
//! state) is documented in `docs/indexer-replay-contract.md` and mirrored by the
//! `/// # Replay semantics` blocks on each emitter in `src/events.rs`.
//!
//! Scope: these tests cover **non-rolling** bonds. The current event schema does
//! not carry `notice_period_duration`, and rolling-bond lifecycle fields
//! (`withdrawal_requested_at`) are only partially evented, so a rolling bond is
//! not fully reconstructable from events alone — a known gap called out in the
//! replay-contract doc. For non-rolling bonds those two fields are always `0`,
//! so full-struct equality is exact.

use credence_bond::{CredenceBond, CredenceBondClient, IdentityBond};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Symbol, TryFromVal, Val,
};

// ---------------------------------------------------------------------------
// Replayer — the indexer model under test.
//
// `BondEvent` is the decoded, indexer-relevant projection of the on-chain event
// stream. `apply` is intentionally a **pure** function of (state, event): no
// `Env`, no storage, no clock. That purity is the whole point — it is exactly
// what a real off-chain indexer can run.
// ---------------------------------------------------------------------------

/// The bond-lifecycle events an indexer must understand to rebuild state. Every
/// other event in the stream (tier_changed, attester_registered, …) is
/// informational for reconstruction and decodes to `Ignored`.
#[derive(Clone, Debug)]
enum BondEvent {
    /// `bond_created_v2` — the genesis event. Establishes the bond's identity
    /// and immutable terms.
    Created {
        identity: Address,
        amount: i128,
        start: u64,
        duration: u64,
        is_rolling: bool,
    },
    /// `bond_increased_v2` — a top-up. Carries the authoritative new total.
    Increased { new_total: i128 },
    /// `bond_withdrawn_v2` — a withdrawal. Carries the authoritative remaining
    /// balance, so partial/early/full withdrawals all replay identically.
    Withdrawn { remaining: i128 },
    /// `bond_slashed_v2` — a slash. Carries the cumulative total slashed.
    Slashed { total_slashed: i128 },
    /// Any event not material to reconstruction.
    Ignored,
}

/// Pure replay step: fold this over the decoded event stream to rebuild state.
///
/// Returns `None` until the genesis `Created` event is seen; every later event
/// mutates the established bond. The function never reads a clock or storage —
/// it depends only on its two arguments, which is what makes it a faithful model
/// of an indexer.
fn apply(state: Option<IdentityBond>, event: &BondEvent) -> Option<IdentityBond> {
    match (state, event) {
        // Genesis. Slashed starts at 0; a fresh bond is always active. The two
        // fields not carried by events (withdrawal_requested_at,
        // notice_period_duration) are 0 for non-rolling bonds — see module docs.
        (
            None,
            BondEvent::Created {
                identity,
                amount,
                start,
                duration,
                is_rolling,
            },
        ) => Some(IdentityBond {
            identity: identity.clone(),
            bonded_amount: *amount,
            bond_start: *start,
            bond_duration: *duration,
            slashed_amount: 0,
            active: true,
            is_rolling: *is_rolling,
            withdrawal_requested_at: 0,
            notice_period_duration: 0,
        }),
        // A second Created would mean two genesis events for one bond — a bug.
        (Some(_), BondEvent::Created { .. }) => panic!("duplicate bond_created_v2 in stream"),
        // Events before genesis are unreconstructable; the stream is malformed.
        (None, _) => None,

        (Some(mut b), BondEvent::Increased { new_total }) => {
            b.bonded_amount = *new_total;
            Some(b)
        }
        (Some(mut b), BondEvent::Withdrawn { remaining }) => {
            b.bonded_amount = *remaining;
            Some(b)
        }
        (Some(mut b), BondEvent::Slashed { total_slashed }) => {
            b.slashed_amount = *total_slashed;
            Some(b)
        }
        (Some(b), BondEvent::Ignored) => Some(b),
    }
}

/// Fold the replayer over a decoded stream.
fn replay(events: &[BondEvent]) -> Option<IdentityBond> {
    events.iter().fold(None, apply)
}

// ---------------------------------------------------------------------------
// Event-stream capture / decode.
// ---------------------------------------------------------------------------

/// Decode a single typed field out of an event `Val`.
fn dec<T: TryFromVal<Env, Val>>(env: &Env, v: &Val) -> T {
    T::try_from_val(env, v)
        .ok()
        .expect("event field failed to decode to the expected type")
}

/// Decode the full captured event stream into the indexer projection.
///
/// This is deliberately tolerant: unknown topics decode to [`BondEvent::Ignored`]
/// rather than panicking, exactly as a forward-compatible indexer would skip
/// events it does not model.
fn capture(env: &Env) -> Vec<BondEvent> {
    let created = Symbol::new(env, "bond_created_v2");
    let increased = Symbol::new(env, "bond_increased_v2");
    let withdrawn = Symbol::new(env, "bond_withdrawn_v2");
    let slashed = Symbol::new(env, "bond_slashed_v2");

    env.events()
        .all()
        .iter()
        .map(|(_contract, topics, data)| {
            let topic0: Symbol = dec(env, &topics.get(0).expect("event has no topic"));
            if topic0 == created {
                // topics: [sym, identity, amount, start]; data: (duration, is_rolling, end)
                let (duration, is_rolling, _end): (u64, bool, u64) = dec(env, &data);
                BondEvent::Created {
                    identity: dec(env, &topics.get(1).unwrap()),
                    amount: dec(env, &topics.get(2).unwrap()),
                    start: dec(env, &topics.get(3).unwrap()),
                    duration,
                    is_rolling,
                }
            } else if topic0 == increased {
                // topics: [sym, identity, added, new_total, ts]
                BondEvent::Increased {
                    new_total: dec(env, &topics.get(3).unwrap()),
                }
            } else if topic0 == withdrawn {
                // topics: [sym, identity, amount, remaining, ts]
                BondEvent::Withdrawn {
                    remaining: dec(env, &topics.get(3).unwrap()),
                }
            } else if topic0 == slashed {
                // topics: [sym, identity, slash_amount, total_slashed, ts, admin]
                BondEvent::Slashed {
                    total_slashed: dec(env, &topics.get(3).unwrap()),
                }
            } else {
                BondEvent::Ignored
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Test harness.
// ---------------------------------------------------------------------------

struct Fixture {
    env: Env,
    client: CredenceBondClient<'static>,
    admin: Address,
    identity: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    client.initialize(&admin);
    Fixture {
        env,
        client,
        admin,
        identity,
    }
}

/// The core invariant: replaying the captured stream equals on-chain state.
fn assert_replay_matches(f: &Fixture) {
    let reconstructed = replay(&capture(&f.env));
    let on_chain = f.client.get_identity_state();
    assert_eq!(
        reconstructed.expect("stream did not reconstruct any bond"),
        on_chain,
        "indexer replay diverged from on-chain state"
    );
}

// --- Scenario 1: bare creation -------------------------------------------------
#[test]
fn scenario_create_only() {
    let f = setup();
    f.env.ledger().set_timestamp(1_000);
    f.client
        .create_bond(&f.identity, &5_000_i128, &10_000_u64, &false, &0_u64);
    assert_replay_matches(&f);
}

// --- Scenario 2: creation then a single top-up --------------------------------
#[test]
fn scenario_create_then_topup() {
    let f = setup();
    f.client
        .create_bond(&f.identity, &5_000_i128, &10_000_u64, &false, &0_u64);
    f.client.top_up(&f.identity, &2_500_i128);
    assert_replay_matches(&f);
}

// --- Scenario 3: multiple top-ups accumulate ----------------------------------
#[test]
fn scenario_multiple_topups() {
    let f = setup();
    f.client
        .create_bond(&f.identity, &1_000_i128, &10_000_u64, &false, &0_u64);
    f.client.top_up(&f.identity, &1_000_i128);
    f.client.top_up(&f.identity, &3_000_i128);
    assert_replay_matches(&f);
}

// --- Scenario 4: creation then a slash ----------------------------------------
#[test]
fn scenario_create_then_slash() {
    let f = setup();
    f.client
        .create_bond(&f.identity, &10_000_i128, &10_000_u64, &false, &0_u64);
    f.client.slash_bond(&f.admin, &4_000_i128);
    assert_replay_matches(&f);
}

// --- Scenario 5: full lifecycle — top-up, slash, then post-lockup withdrawal ---
#[test]
fn scenario_full_lifecycle() {
    let f = setup();
    f.env.ledger().set_timestamp(0);
    f.client
        .create_bond(&f.identity, &10_000_i128, &1_000_u64, &false, &0_u64);
    f.client.top_up(&identity, &5_000_i128);
    f.client.slash_bond(&f.admin, &2_000_i128);
    // Advance past the lock-up so the standard withdraw path is allowed.
    f.env.ledger().set_timestamp(2_000);
    f.client.withdraw(&identity, &1_000_i128);
    assert_replay_matches(&f);
}

// --- Scenario 6: early withdrawal with penalty --------------------------------
#[test]
fn scenario_withdraw_early() {
    let f = setup();
    let treasury = Address::generate(&f.env);
    f.client
        .set_early_exit_config(&f.admin, &treasury, &500_u32);
    f.env.ledger().set_timestamp(0);
    f.client
        .create_bond(&f.identity, &10_000_i128, &10_000_u64, &false, &0_u64);
    f.env.ledger().set_timestamp(1_000); // still within lock-up
    f.client.withdraw_early(&identity, &1_000_i128);
    assert_replay_matches(&f);
}

// --- Validation: dropping an event must make replay diverge -------------------
//
// This is the negative control required by the issue: if the indexer were to
// miss the `bond_increased_v2` (top_up) event, its reconstructed balance must no
// longer match on-chain state. A test suite where dropping an event still
// "passes" would be asserting nothing.
#[test]
fn dropping_topup_event_diverges() {
    let f = setup();
    f.client
        .create_bond(&f.identity, &5_000_i128, &10_000_u64, &false, &0_u64);
    f.client.top_up(&identity, &2_500_i128);

    let full_stream = capture(&f.env);
    // Sanity: the intact stream reconstructs correctly.
    assert_eq!(
        replay(&full_stream).expect("intact stream should reconstruct"),
        f.client.get_identity_state()
    );

    // Drop the first Increased (top_up) event, as a lossy indexer might.
    let mut lossy = Vec::new();
    let mut dropped = false;
    for ev in full_stream {
        if !dropped && matches!(ev, BondEvent::Increased { .. }) {
            dropped = true;
            continue;
        }
        lossy.push(ev);
    }
    assert!(
        dropped,
        "test setup error: no top_up event was present to drop"
    );

    let reconstructed = replay(&lossy).expect("stream still has genesis event");
    assert_ne!(
        reconstructed,
        f.client.get_identity_state(),
        "dropping the top_up event should make replay diverge from on-chain state"
    );
}
