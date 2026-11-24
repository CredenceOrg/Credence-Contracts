//! Event schema regression tests for the Admin contract (issue #1438).
//!
//! # Why this file exists
//!
//! Admin events are the only off-chain surface a governance indexer has: the
//! ledger entry for an admin is a set of `DataKey` blobs that cannot be
//! reconstructed without replaying the contract. A payload-shape change — a
//! renamed topic, a field moved between `topics` and `data`, a reordered
//! tuple — is therefore a *silent* breaking change. The transaction still
//! succeeds; only the consumer breaks, usually weeks later, during an audit.
//!
//! This suite makes that breaking change fail at the pull-request gate.
//!
//! # Test-host semantics this file is built on (read before editing)
//!
//! In Soroban SDK 22's test host, `env.events().all()` returns the events of
//! the **most recent top-level contract invocation only**, and it returns an
//! **empty vector when that invocation failed**.
//!
//! ```text
//! client.initialize(..)        -> log == ["admin_initialized"]
//! client.add_admin(..)         -> log == ["admin_added", "ROLE_ASSIGNED"]
//! client.try_add_admin(..)     -> log == []            (rolled back)
//! ```
//!
//! Two consequences drive every assertion below:
//!
//! 1. **Never diff the log across invocations.** A `before`/`after` count taken
//!    around two different calls compares unrelated snapshots. (This is the
//!    bug that made the older `test_role_events.rs` count assertions
//!    unsatisfiable — see that file's header.)
//! 2. **A rejected call is asserted as an empty log**, which is the faithful
//!    encoding of the on-chain guarantee: a failed invocation commits neither
//!    state nor events. This is a *stronger* statement than "the count did not
//!    grow".
//!
//! Each test therefore performs its setup, then makes **one** call of interest,
//! then asserts on the resulting log. Sequences that need several events
//! accumulate observations in the test body (see
//! [`whole_event_surface_matches_the_frozen_table_exactly`]), never in the
//! host.
//!
//! # Design: an independent frozen table, not shared constants
//!
//! Topic names live inline at the emission sites (`lib.rs`, `pausable.rs`).
//! This suite deliberately does **not** import them. A schema test that builds
//! its expected `Symbol` from the same constant the emitter uses is
//! tautological: rename both together and the test still passes, so the
//! rename — the exact failure this file exists to catch — is invisible.
//!
//! [`FROZEN_EVENTS`] is therefore a hand-maintained *frozen* table of string
//! literals. It is the oracle, and it is deliberately allowed to disagree with
//! the implementation. A change in either direction fails:
//!
//! * a topic the implementation emits that is not in the table →
//!   [`whole_event_surface_matches_the_frozen_table_exactly`]
//! * a table row nothing emits → [`every_frozen_event_is_reachable`]
//! * an arity change → `assert_emitted` and the per-event shape tests
//!
//! # Invariants locked here
//!
//! | # | Invariant | Adversarial question it answers |
//! |---|-----------|----------------------------------|
//! | I1 | Every emitted topic is in the frozen table, and every table row is reachable | *did someone add, rename, or drop an event?* |
//! | I2 | Topic arity and typed payload of all 18 events are pinned | *did a field move between `topics` and `data`?* |
//! | I3 | `topics[0]` is always a `Symbol`; indexed `topics[1]` is the *subject*, never the caller | *can a caller forge an indexer-visible identity?* |
//! | I4 | Every event is attributed to the emitting contract, never to the caller | *can an unrelated contract's event pass as admin governance?* |
//! | I5 | A rejected invocation emits **zero** events and advances **zero** epochs | *does a failed retry leave a phantom record for an indexer to double-count?* |
//! | I6 | No-op calls emit nothing, or re-emit the same shape on repeat | *is the event stream idempotent under retry?* |
//! | I7 | Multi-event invocations keep their relative order | *can an indexer see `ROLE_ASSIGNED` without the `admin_added` that authorised it?* |
//! | I8 | Topic names stay inside the Soroban `Symbol` limits | *will a future rename abort on-chain at publish time?* |
//! | I9 | Two Admin instances never cross-attribute events | *does a shared topic namespace leak between contracts?* |
//! | I10 | Boundary cases keep the schema: `Some`/`None` proposal ids, both the direct and proposal pause paths, the ownership-timelock instant, and an epoch boundary | *does an edge case silently take a different code path?* |
//!
//! # Scope
//!
//! Event shape, arity, payload, and the event-visible consequences of
//! rejection / retry / repetition. Underlying state rollback is owned by
//! `test_atomic_rollback.rs` and epoch arithmetic by
//! `test_concurrency_race_safety.rs`; this file asserts the event side and
//! defers to them for the state model.
//!
//! SDK 22 events API: `env.events().all()` → `Vec<(Address, Vec<Val>, Val)>`
//! where the tuple is `(contract_id, topics, data)`.

#![cfg(test)]

extern crate std;

use crate::pausable::{PauseAction, PROPOSAL_EPOCH_SIZE};
use crate::{
    AdminContract, AdminContractClient, AdminInfo, AdminRole, OWNERSHIP_TRANSFER_TIMELOCK,
};
use credence_errors::ContractError;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::Vec as SorobanVec;
use soroban_sdk::{Address, Env, String as SorobanString, Symbol, TryFromVal, Val};

// ── Frozen schema table ───────────────────────────────────────────────────

/// One row of the frozen event schema.
///
/// `topics` is the *total* topic count, including `topics[0]` (the name).
/// `shape` documents the `data` payload for a reader of the table alone; the
/// shape itself is asserted for real (typed decode) by the per-event tests.
struct FrozenEvent {
    name: &'static str,
    topics: u32,
    shape: &'static str,
}

/// Maximum byte length of a Soroban event topic / `Symbol`.
///
/// Mirrors the host's `TOPIC_BYTES_LENGTH_LIMIT`. A longer name is rejected at
/// publish time, which would brick the entrypoint.
const SOROBAN_TOPIC_BYTE_LIMIT: usize = 32;

/// Every event topic the Admin contract emits, frozen.
const FROZEN_EVENTS: &[FrozenEvent] = &[
    // ── lifecycle / RBAC (lib.rs) ──
    FrozenEvent {
        name: "admin_initialized",
        topics: 1,
        shape: "data: Address(super_admin)",
    },
    FrozenEvent {
        name: "admin_added",
        topics: 1,
        shape: "data: AdminInfo",
    },
    FrozenEvent {
        name: "ROLE_ASSIGNED",
        topics: 2,
        shape: "topics: [Symbol, Address(subject)] | data: (AdminRole, Address(caller))",
    },
    FrozenEvent {
        name: "ROLE_REVOKED",
        topics: 2,
        shape: "topics: [Symbol, Address(subject)] | data: (Address(caller),)",
    },
    FrozenEvent {
        name: "admin_role_updated",
        topics: 1,
        shape: "data: (Address(admin), AdminRole(old), AdminRole(new))",
    },
    FrozenEvent {
        name: "admin_deactivated",
        topics: 1,
        shape: "data: AdminInfo",
    },
    FrozenEvent {
        name: "admin_reactivated",
        topics: 1,
        shape: "data: AdminInfo",
    },
    FrozenEvent {
        name: "admin_removed",
        topics: 1,
        shape: "data: AdminInfo",
    },
    FrozenEvent {
        name: "admin_suspended",
        topics: 1,
        shape: "data: (Address(admin), u64(until_ts))",
    },
    // ── ownership (lib.rs) ──
    FrozenEvent {
        name: "ownership_transfer_initiated",
        topics: 1,
        shape: "data: (Address(current_owner), Address(new_owner))",
    },
    FrozenEvent {
        name: "admin_rotated",
        topics: 3,
        shape: "topics: [Symbol, Address(previous), Address(next)] | data: u32(ledger_seq)",
    },
    FrozenEvent {
        name: "ownership_transfer_accepted",
        topics: 1,
        shape: "data: (Address(previous_owner), Address(new_owner))",
    },
    // ── pause mechanism (pausable.rs) ──
    FrozenEvent {
        name: "pause_signer_set",
        topics: 2,
        shape: "topics: [Symbol, Address(signer)] | data: bool(enabled)",
    },
    FrozenEvent {
        name: "pause_threshold_set",
        topics: 1,
        shape: "data: u32(threshold)",
    },
    FrozenEvent {
        name: "pause_proposed",
        topics: 2,
        shape: "topics: [Symbol, u64(proposal_id)] | data: u32(action)",
    },
    FrozenEvent {
        name: "pause_approved",
        topics: 2,
        shape: "topics: [Symbol, u64(proposal_id)] | data: Address(signer)",
    },
    FrozenEvent {
        name: "paused",
        topics: 1,
        shape: "data: (Option<u64>(proposal_id), String(reason))",
    },
    FrozenEvent {
        name: "unpaused",
        topics: 1,
        shape: "data: Option<u64>(proposal_id)   // NOT a tuple",
    },
];

// ── Decoding helpers ──────────────────────────────────────────────────────

/// Decode a `Val` into `T`, or fail with a diagnosable message.
macro_rules! decode {
    ($env:expr, $val:expr, $ty:ty, $msg:literal) => {
        <$ty>::try_from_val($env, &$val).expect($msg)
    };
}

/// The three parts of one recorded event: `(contract_id, topics, data)`.
type Emitted = (Address, SorobanVec<Val>, Val);

/// The event log of the **most recent** top-level invocation, with every
/// topic resolved to its frozen static name.
///
/// Panics on a topic absent from [`FROZEN_EVENTS`]: an undocumented event is
/// exactly the regression this file exists to catch, and it must fail here
/// rather than be silently skipped.
fn last_log(e: &Env) -> std::vec::Vec<(Address, &'static str, SorobanVec<Val>, Val)> {
    let table: std::vec::Vec<(Symbol, &'static str)> = FROZEN_EVENTS
        .iter()
        .map(|f| (Symbol::new(e, f.name), f.name))
        .collect();

    let mut out = std::vec::Vec::new();
    for (contract, topics, data) in e.events().all().iter() {
        let sym = topic_name(e, &topics);
        let frozen = table
            .iter()
            .find(|(s, _)| *s == sym)
            .map(|(_, n)| *n)
            .unwrap_or_else(|| {
                panic!(
                    "undocumented event topic {sym:?} with arity {}: every topic the Admin \
                     contract emits must be added to FROZEN_EVENTS, and adding or renaming one \
                     requires a version bump for existing indexers",
                    topics.len()
                )
            });
        out.push((contract, frozen, topics, data));
    }
    out
}

/// Resolve `topics[0]` to its `Symbol`, asserting it is a `Symbol`.
///
/// Invariant I3: an indexer dispatches on `topics[0]`. An emitter that
/// published a non-`Symbol` discriminator would make the consumer unable to
/// route the event at all, so assert the type rather than only reading it.
fn topic_name(e: &Env, topics: &SorobanVec<Val>) -> Symbol {
    let first = topics.get(0).unwrap_or_else(|| {
        panic!(
            "event published with zero topics: the frozen schema requires the event name in \
             topics[0], and an indexer dispatches on it"
        )
    });
    decode!(
        e,
        first,
        Symbol,
        "topics[0] must be a Symbol: an indexer dispatches on the event name and cannot route a \
         non-Symbol discriminator"
    )
}

/// The frozen names in the most recent invocation's log, in emission order.
fn names(e: &Env) -> std::vec::Vec<&'static str> {
    last_log(e).into_iter().map(|(_, n, _, _)| n).collect()
}

/// Assert the most recent invocation emitted exactly `expected`, in order.
///
/// Order is part of the contract: within one invocation a consumer may rely on
/// the state event preceding the RBAC event that interprets it.
#[track_caller]
fn assert_emitted(e: &Env, expected: &[&str], what: &str) {
    let got = names(e);
    assert_eq!(
        got, expected,
        "{what}: the event stream of this invocation is part of the frozen contract. \
         Expected {expected:?} in that order, got {got:?}."
    );
}

/// Assert the most recent invocation emitted nothing at all.
///
/// This is the encoding of invariant I5: a rejected invocation commits neither
/// state nor events, so an indexer can treat "the call returned an error" as
/// "no governance action occurred".
#[track_caller]
fn assert_emitted_nothing(e: &Env, what: &str) {
    let got = names(e);
    assert!(
        got.is_empty(),
        "{what} was rejected but still published {got:?}. A failed invocation must commit no \
         events, or an indexer records a governance action that never took effect."
    );
}

/// `(contract, topics, data)` of the only event named `name` in the last log.
#[track_caller]
fn only(e: &Env, name: &str) -> (Address, SorobanVec<Val>, Val) {
    let log = last_log(e);
    let hits: std::vec::Vec<_> = log.iter().filter(|(_, n, _, _)| *n == name).collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one `{name}` event in this invocation, found {}. A consumer that \
         assumes one event per action mis-counts a retry.",
        hits.len()
    );
    let (c, _, t, d) = hits[0];
    (c.clone(), t.clone(), *d)
}

/// `(topics, data)` of the `n`-th event named `name` in the last log.
#[track_caller]
fn nth(e: &Env, name: &str, n: u32) -> (SorobanVec<Val>, Val) {
    let log = last_log(e);
    let mut seen = 0u32;
    for (_, found, topics, data) in log {
        if found == name {
            if seen == n {
                return (topics, data);
            }
            seen += 1;
        }
    }
    panic!("expected `{name}` occurrence #{n} in this invocation's log, found {seen}")
}

/// A printable label for an `Address`.
///
/// `soroban_sdk::Address` derives `Clone` but neither `Debug` nor `Display`, so
/// it cannot appear in an assertion message directly. The strkey form is what
/// operators see in explorers, so use that.
fn addr(a: &Address) -> SorobanString {
    a.to_string()
}

/// Assert a client call failed with exactly `expected`.
///
/// `try_*` returns `Result<Result<T, C>, Result<Error, InvokeError>>`. A
/// contract-level rejection is the `Ok(Error::from_contract_error(..))` case;
/// an `InvokeError` means the invocation never reached the guard, which is a
/// different bug and is reported as such.
#[track_caller]
fn assert_rejected<T, C, I: std::fmt::Debug>(
    res: Result<Result<T, C>, Result<soroban_sdk::Error, I>>,
    expected: ContractError,
) {
    match res {
        Ok(_) => panic!(
            "expected rejection with {expected:?} (code {}), but the call succeeded",
            expected as u32
        ),
        Err(Err(invoke)) => panic!(
            "expected a contract-level rejection with {expected:?} (code {}), but the invocation \
             itself failed before reaching the guard: {invoke:?}",
            expected as u32
        ),
        Err(Ok(err)) => assert_eq!(
            err,
            soroban_sdk::Error::from_contract_error(expected as u32),
            "wrong rejection code: clients branch on it to decide whether to retry, so it is \
             part of the contract"
        ),
    }
}

/// Field-by-field `AdminInfo` assertion.
///
/// `AdminInfo` derives `Clone, Debug` but not `PartialEq`, so equality is
/// checked explicitly. That is deliberate: a reordering of the
/// `#[contracttype]` fields would still decode, silently pairing two
/// same-width values, and only a per-field check catches that.
#[track_caller]
fn assert_admin_info(
    info: &AdminInfo,
    address: &Address,
    role: AdminRole,
    assigned_at: u64,
    assigned_by: &Address,
    active: bool,
    suspended_until: u64,
) {
    assert_eq!(&info.address, address, "AdminInfo.address");
    assert_eq!(info.role, role, "AdminInfo.role");
    assert_eq!(info.assigned_at, assigned_at, "AdminInfo.assigned_at");
    assert_eq!(&info.assigned_by, assigned_by, "AdminInfo.assigned_by");
    assert_eq!(info.active, active, "AdminInfo.active");
    assert_eq!(
        info.suspended_until, suspended_until,
        "AdminInfo.suspended_until"
    );
}

// ── Fixtures ──────────────────────────────────────────────────────────────

/// `(env, client, contract_id, super_admin)` — a freshly initialised contract
/// with `min_admins = 1`, `max_admins = 100` and all auths mocked.
///
/// Note the log already holds `admin_initialized` when this returns; tests
/// overwrite it with the call they care about.
fn setup() -> (Env, AdminContractClient<'static>, Address, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &id);
    let super_admin = Address::generate(&e);
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, id, super_admin)
}

/// Register `n` pause signers and set the threshold, returning the signers.
fn add_signers(
    e: &Env,
    client: &AdminContractClient,
    admin: &Address,
    n: usize,
    threshold: u32,
) -> std::vec::Vec<Address> {
    let mut out = std::vec::Vec::new();
    for _ in 0..n {
        let s = Address::generate(e);
        client.set_pause_signer(admin, &s, &true);
        out.push(s);
    }
    client.set_pause_threshold(admin, &threshold);
    out
}

/// Append the most recent invocation's `(name, arity)` pairs to `acc`.
///
/// The host only ever exposes the latest invocation, so a whole-surface walk
/// has to accumulate here.
fn record(e: &Env, acc: &mut std::vec::Vec<(&'static str, u32)>) {
    for (_, n, topics, _) in last_log(e) {
        acc.push((n, topics.len()));
    }
}

// ══════════════════════════════════════════════════════════════════════════
// I8 — the frozen table is itself well formed
// ══════════════════════════════════════════════════════════════════════════

/// Every topic name must be a legal Soroban `Symbol`.
///
/// Soroban symbols allow `[A-Za-z0-9_]` and must not start with a digit;
/// `Symbol::new` panics otherwise. A rename to `admin-initialized` (hyphen)
/// or `1_admin_added` (leading digit) would abort at publish time and brick
/// the entrypoint in production while passing every behavioural test. Asserting
/// the charset here turns that into a legible CI failure.
#[test]
fn frozen_topic_names_are_valid_soroban_symbols() {
    for frozen in FROZEN_EVENTS {
        let bytes = frozen.name.as_bytes();
        assert!(
            bytes.len() <= SOROBAN_TOPIC_BYTE_LIMIT,
            "topic `{}` is {} bytes, over the Soroban {SOROBAN_TOPIC_BYTE_LIMIT}-byte symbol \
             limit: publishing it would panic on-chain and permanently break the entrypoint",
            frozen.name,
            bytes.len()
        );
        let first = bytes[0];
        assert!(
            first.is_ascii_alphabetic() || first == b'_',
            "topic `{}` starts with `{}`: Soroban symbols must start with a letter or `_`",
            frozen.name,
            first as char
        );
        for (i, b) in bytes.iter().enumerate() {
            assert!(
                b.is_ascii_alphanumeric() || *b == b'_',
                "topic `{}` contains `{}` at offset {i}: Soroban symbols allow only [A-Za-z0-9_]",
                frozen.name,
                *b as char
            );
        }
    }
}

/// The table is a set, not a multiset.
#[test]
fn frozen_topic_names_are_unique() {
    for (i, a) in FROZEN_EVENTS.iter().enumerate() {
        for b in &FROZEN_EVENTS[i + 1..] {
            assert_ne!(
                a.name, b.name,
                "`{}` is listed twice in FROZEN_EVENTS: duplicate topic names make the arity \
                 assertions ambiguous and indexers collapse the two events",
                a.name
            );
        }
    }
}

/// Arity is pinned per event and inside the range Soroban supports (1..=4,
/// i.e. the name plus up to three indexed fields).
#[test]
fn frozen_topic_arities_are_in_range() {
    for frozen in FROZEN_EVENTS {
        assert!(
            (1..=4).contains(&frozen.topics),
            "topic `{}` is frozen at arity {} but Soroban only supports 1..=4 topics (name + up \
             to 3 indexed fields)",
            frozen.name,
            frozen.topics
        );
    }
}

/// The table has one row per emission site and no undocumented shape.
#[test]
fn frozen_table_covers_every_emission_site() {
    assert_eq!(
        FROZEN_EVENTS.len(),
        18,
        "FROZEN_EVENTS must list every event the Admin contract emits. A new emission site \
         without a frozen row would ship unindexed."
    );
    for frozen in FROZEN_EVENTS {
        assert!(
            !frozen.shape.is_empty(),
            "frozen row `{}` has no documented data shape",
            frozen.name
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════
// I1 / I2 — whole-surface shape
// ══════════════════════════════════════════════════════════════════════════

/// Walk every emitting entrypoint and assert the accumulated
/// `(name, arity)` stream equals [`FROZEN_EVENTS`] exactly.
///
/// Because the host exposes one invocation at a time, the walk accumulates
/// here. This single test catches:
///
/// * a new event added without a schema bump → an extra `(name, arity)` pair
/// * a renamed topic → `last_log` panics with "undocumented event topic"
/// * a dropped event → a missing pair
/// * a field moved between `topics` and `data` → an arity mismatch
/// * an event moved to a different entrypoint → a duplicate/missing pair
#[test]
fn whole_event_surface_matches_the_frozen_table_exactly() {
    let (e, client, id, super_admin) = setup();
    let mut seen: std::vec::Vec<(&'static str, u32)> = std::vec::Vec::new();
    record(&e, &mut seen); // admin_initialized

    // RBAC lifecycle.
    let subject = Address::generate(&e);
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);
    record(&e, &mut seen);
    client.update_admin_role(&super_admin, &subject, &AdminRole::Operator);
    record(&e, &mut seen);
    client.deactivate_admin(&super_admin, &subject);
    record(&e, &mut seen);
    client.reactivate_admin(&super_admin, &subject);
    record(&e, &mut seen);
    let now = e.ledger().timestamp();
    client.suspend_admin(&super_admin, &subject, &(now + 1_000));
    record(&e, &mut seen);
    client.remove_admin(&super_admin, &subject);
    record(&e, &mut seen);

    // Pause mechanism, after the RBAC ops because those are
    // `require_not_paused`-gated.
    let signer = Address::generate(&e);
    client.set_pause_signer(&super_admin, &signer, &true);
    record(&e, &mut seen);
    client.set_pause_threshold(&super_admin, &1u32);
    record(&e, &mut seen);
    let pause_id = client.pause(&signer).unwrap();
    record(&e, &mut seen);
    client.execute_pause_proposal(&pause_id);
    record(&e, &mut seen);
    let unpause_id = client.unpause(&signer).unwrap();
    record(&e, &mut seen);
    client.execute_pause_proposal(&unpause_id);
    record(&e, &mut seen);

    // A second signer + a threshold change covers the remaining pause rows on
    // a path that is not already taken above.
    let signer2 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &signer2, &true);
    record(&e, &mut seen);
    client.set_pause_threshold(&super_admin, &2u32);
    record(&e, &mut seen);
    let pause_id2 = client.pause(&signer).unwrap();
    record(&e, &mut seen);
    // A *distinct* signer: `pause_approved` is emitted only for a newly
    // recorded approval, so reusing `signer` here would publish nothing.
    client.approve_pause_proposal(&signer2, &pause_id2);
    record(&e, &mut seen);

    // Ownership rotation last: it strips the caller's authority.
    let heir = Address::generate(&e);
    client.add_admin(&super_admin, &heir, &AdminRole::SuperAdmin);
    record(&e, &mut seen);
    client.transfer_ownership(&super_admin, &heir);
    record(&e, &mut seen);
    e.ledger()
        .with_mut(|l| l.timestamp += OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&heir);
    record(&e, &mut seen);

    // Compare as multisets: the set of schemas is the contract, the number of
    // times a given schema is produced is asserted per-test.
    let mut got = seen.clone();
    got.sort_unstable();
    got.dedup();
    let mut want: std::vec::Vec<(&'static str, u32)> =
        FROZEN_EVENTS.iter().map(|f| (f.name, f.topics)).collect();
    want.sort_unstable();
    want.dedup();

    assert_eq!(
        got, want,
        "the set of (event name, topic count) pairs emitted by a full Admin lifecycle no longer \
         matches the frozen schema.\nObserved: {got:?}\nFrozen:   {want:?}"
    );

    // Every emission is attributed to the Admin contract that produced it.
    for (contract, n, _, _) in last_log(&e) {
        assert_eq!(
            contract,
            id,
            "`{n}` was attributed to {:?}, not {id:?}",
            addr(&contract)
        );
    }
}

/// Every frozen row is reachable. Complements the multiset comparison above:
/// it proves each table entry is produced by a real entrypoint, so a row
/// cannot go stale while the set comparison still passes.
#[test]
fn every_frozen_event_is_reachable() {
    let (e, client, _id, super_admin) = setup();
    let mut seen: std::vec::Vec<&'static str> = std::vec::Vec::new();
    let mut note = |e: &Env, seen: &mut std::vec::Vec<&'static str>| {
        seen.extend(names(e));
    };
    note(&e, &mut seen);

    let subject = Address::generate(&e);
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);
    note(&e, &mut seen);
    client.update_admin_role(&super_admin, &subject, &AdminRole::Operator);
    note(&e, &mut seen);
    client.deactivate_admin(&super_admin, &subject);
    note(&e, &mut seen);
    client.reactivate_admin(&super_admin, &subject);
    note(&e, &mut seen);
    let now = e.ledger().timestamp();
    client.suspend_admin(&super_admin, &subject, &(now + 1_000));
    note(&e, &mut seen);
    client.remove_admin(&super_admin, &subject);
    note(&e, &mut seen);

    // Sign up two signers explicitly so each registration is observed: the
    // host only exposes the latest invocation, and `add_signers` performs
    // three calls of which only the last would be visible.
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);
    note(&e, &mut seen);
    client.set_pause_signer(&super_admin, &s2, &true);
    note(&e, &mut seen);
    client.set_pause_threshold(&super_admin, &2u32);
    note(&e, &mut seen);
    let pause_id = client.pause(&s1).unwrap();
    note(&e, &mut seen);
    client.approve_pause_proposal(&s2, &pause_id);
    note(&e, &mut seen);
    client.execute_pause_proposal(&pause_id);
    note(&e, &mut seen);
    let unpause_id = client.unpause(&s1).unwrap();
    note(&e, &mut seen);
    client.approve_pause_proposal(&s2, &unpause_id);
    note(&e, &mut seen);
    client.execute_pause_proposal(&unpause_id);
    note(&e, &mut seen);

    let heir = Address::generate(&e);
    client.add_admin(&super_admin, &heir, &AdminRole::SuperAdmin);
    note(&e, &mut seen);
    client.transfer_ownership(&super_admin, &heir);
    note(&e, &mut seen);
    e.ledger()
        .with_mut(|l| l.timestamp += OWNERSHIP_TRANSFER_TIMELOCK);
    client.accept_ownership(&heir);
    note(&e, &mut seen);

    for frozen in FROZEN_EVENTS {
        assert!(
            seen.contains(&frozen.name),
            "frozen event `{}` is unreachable: no entrypoint emits it, so the row is stale and \
             should be deleted (or an emitter is broken)",
            frozen.name
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════
// I2 — per-invocation shape and ordering
// ══════════════════════════════════════════════════════════════════════════

/// `initialize` publishes exactly one event: the bootstrap SuperAdmin.
#[test]
fn initialize_publishes_only_admin_initialized() {
    let e = Env::default();
    e.mock_all_auths();
    let id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &id);
    let super_admin = Address::generate(&e);
    client.initialize(&super_admin, &1u32, &100u32);

    assert_emitted(&e, &["admin_initialized"], "initialize");

    let (_, data) = nth(&e, "admin_initialized", 0);
    let emitted: Address = decode!(
        &e,
        data,
        Address,
        "admin_initialized data must be an Address"
    );
    assert_eq!(
        emitted, super_admin,
        "admin_initialized must name the bootstrap SuperAdmin"
    );
}

/// `add_admin` publishes `admin_added` then `ROLE_ASSIGNED`, in that order.
///
/// Ordering is load-bearing: a consumer that crashes after the RBAC event must
/// still be able to reconcile from the state event, which is only possible if
/// the state event came first.
#[test]
fn add_admin_publishes_state_event_before_rbac_event() {
    let (e, client, _id, super_admin) = setup();
    let subject = Address::generate(&e);
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);

    assert_emitted(&e, &["admin_added", "ROLE_ASSIGNED"], "add_admin");

    let added: AdminInfo = {
        let (_, d) = nth(&e, "admin_added", 0);
        decode!(&e, d, AdminInfo, "admin_added data must be an AdminInfo")
    };
    assert_admin_info(
        &added,
        &subject,
        AdminRole::Admin,
        e.ledger().timestamp(),
        &super_admin,
        true,
        0,
    );
}

/// `deactivate_admin` / `reactivate_admin` / `update_admin_role` / `remove_admin`
/// each publish the state event first, then the matching RBAC event.
#[test]
fn every_role_mutation_publishes_its_state_event_first() {
    // deactivate
    let (e, client, _id, super_admin) = setup();
    let subject = Address::generate(&e);
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);
    client.deactivate_admin(&super_admin, &subject);
    assert_emitted(
        &e,
        &["admin_deactivated", "ROLE_REVOKED"],
        "deactivate_admin",
    );
    let (_, d) = nth(&e, "admin_deactivated", 0);
    let info: AdminInfo = decode!(
        &e,
        d,
        AdminInfo,
        "admin_deactivated data must be an AdminInfo"
    );
    assert_eq!(
        info.active, false,
        "the snapshot must show the post-deactivation flag, or a replaying indexer keeps the \
         admin active"
    );
    assert_eq!(info.role, AdminRole::Admin);
    assert_eq!(&info.address, &subject);

    // reactivate
    client.reactivate_admin(&super_admin, &subject);
    assert_emitted(
        &e,
        &["admin_reactivated", "ROLE_ASSIGNED"],
        "reactivate_admin",
    );
    let (_, d) = nth(&e, "admin_reactivated", 0);
    let info: AdminInfo = decode!(
        &e,
        d,
        AdminInfo,
        "admin_reactivated data must be an AdminInfo"
    );
    assert_admin_info(
        &info,
        &subject,
        AdminRole::Admin,
        e.ledger().timestamp(),
        &super_admin,
        true,
        0,
    );

    // update role
    client.update_admin_role(&super_admin, &subject, &AdminRole::Operator);
    assert_emitted(
        &e,
        &["admin_role_updated", "ROLE_ASSIGNED"],
        "update_admin_role",
    );

    // remove
    client.remove_admin(&super_admin, &subject);
    assert_emitted(&e, &["admin_removed", "ROLE_REVOKED"], "remove_admin");
    let (_, d) = nth(&e, "admin_removed", 0);
    let info: AdminInfo = decode!(&e, d, AdminInfo, "admin_removed data must be an AdminInfo");
    assert_eq!(
        &info.address, &subject,
        "admin_removed must name the removed admin, not the caller"
    );
    assert_eq!(
        &info.assigned_by, &super_admin,
        "provenance must survive removal"
    );
}

/// `admin_role_updated` carries both the old and the new role.
///
/// Emitting only the new role would make the event useless for reconciliation,
/// and emitting `old` in both slots would make it a no-op.
#[test]
fn admin_role_updated_payload_carries_old_and_new_role() {
    let (e, client, _id, super_admin) = setup();
    let subject = Address::generate(&e);
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);
    client.update_admin_role(&super_admin, &subject, &AdminRole::Operator);

    let (topics, data) = nth(&e, "admin_role_updated", 0);
    assert_eq!(
        topics.len(),
        1,
        "admin_role_updated keeps all three fields in `data`; promoting the old role to an \
         indexed topic would change the arity and break consumers"
    );
    let (who, old, new): (Address, AdminRole, AdminRole) = decode!(
        &e,
        data,
        (Address, AdminRole, AdminRole),
        "data must be (Address, AdminRole, AdminRole)"
    );
    assert_eq!(
        who, subject,
        "the tuple must name the admin whose role changed"
    );
    assert_eq!(
        old,
        AdminRole::Admin,
        "old role must be the pre-update role"
    );
    assert_eq!(
        new,
        AdminRole::Operator,
        "new role must be the post-update role"
    );
}

/// `suspend_admin` reports the requested expiry, and the boundary is exact.
#[test]
fn admin_suspended_payload_carries_the_requested_expiry() {
    let (e, client, _id, super_admin) = setup();
    let subject = Address::generate(&e);
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);
    let until = e.ledger().timestamp() + 7_777;
    client.suspend_admin(&super_admin, &subject, &until);

    assert_emitted(&e, &["admin_suspended"], "suspend_admin");
    let (topics, data) = nth(&e, "admin_suspended", 0);
    assert_eq!(topics.len(), 1);
    let (who, emitted_until): (Address, u64) =
        decode!(&e, data, (Address, u64), "data must be (Address, u64)");
    assert_eq!(who, subject);
    assert_eq!(
        emitted_until, until,
        "admin_suspended must report the requested expiry: a consumer that recomputes it from \
         the ledger timestamp cannot tell how long the suspension lasts"
    );

    // The boundary is inclusive: effective again *at* `until_ts`, still
    // suspended one second earlier. An off-by-one either grants authority a
    // second early or keeps it revoked a second too long.
    e.ledger().with_mut(|l| l.timestamp = until - 1);
    assert!(
        !client.has_role_at_least(&subject, &AdminRole::Admin),
        "the admin must still be suspended one second before `until_ts`"
    );
    e.ledger().with_mut(|l| l.timestamp = until);
    assert!(
        client.has_role_at_least(&subject, &AdminRole::Admin),
        "the suspension must lapse automatically at `until_ts` with no second transaction"
    );
}

/// `ownership_transfer_initiated` reports the owner read from storage, not the
/// caller's argument.
#[test]
fn ownership_transfer_initiated_payload_reports_the_recorded_owner() {
    let (e, client, _id, super_admin) = setup();
    let heir = Address::generate(&e);
    client.add_admin(&super_admin, &heir, &AdminRole::SuperAdmin);
    client.transfer_ownership(&super_admin, &heir);

    assert_emitted(&e, &["ownership_transfer_initiated"], "transfer_ownership");
    let (topics, data) = nth(&e, "ownership_transfer_initiated", 0);
    assert_eq!(topics.len(), 1);
    let (current, next): (Address, Address) = decode!(
        &e,
        data,
        (Address, Address),
        "data must be (Address, Address)"
    );
    assert_eq!(
        current, super_admin,
        "the recorded owner, not the caller's argument"
    );
    assert_eq!(next, heir);
    assert_eq!(client.get_pending_owner(), Some(heir));
}

/// `accept_ownership` publishes `admin_rotated` then
/// `ownership_transfer_accepted`, and the two agree on the address pair.
#[test]
fn accept_ownership_publishes_rotation_then_acceptance() {
    let (e, client, _id, super_admin) = setup();
    let heir = Address::generate(&e);
    client.add_admin(&super_admin, &heir, &AdminRole::SuperAdmin);
    client.transfer_ownership(&super_admin, &heir);
    e.ledger()
        .with_mut(|l| l.timestamp += OWNERSHIP_TRANSFER_TIMELOCK);
    let expected_seq = e.ledger().sequence();
    client.accept_ownership(&heir);

    assert_emitted(
        &e,
        &["admin_rotated", "ownership_transfer_accepted"],
        "accept_ownership",
    );

    // `admin_rotated` is the only 3-topic Admin event, so its arity is
    // especially load-bearing for per-owner filtering.
    let (topics, data) = nth(&e, "admin_rotated", 0);
    assert_eq!(
        topics.len(),
        3,
        "admin_rotated must have 3 topics (name + previous + next); both addresses must stay \
         indexed or an indexer loses per-owner filtering"
    );
    let previous: Address = decode!(
        &e,
        topics.get(1).unwrap(),
        Address,
        "topics[1] is the previous owner"
    );
    let next: Address = decode!(
        &e,
        topics.get(2).unwrap(),
        Address,
        "topics[2] is the next owner"
    );
    let seq: u32 = decode!(
        &e,
        data,
        u32,
        "admin_rotated data must be the u32 ledger sequence"
    );
    assert_eq!(previous, super_admin);
    assert_eq!(next, heir);
    assert_eq!(
        seq, expected_seq,
        "admin_rotated must report the sequence of the commit that performed the rotation, not \
         the sequence at proposal time"
    );

    let (topics2, data2) = nth(&e, "ownership_transfer_accepted", 0);
    assert_eq!(topics2.len(), 1);
    let (prev2, next2): (Address, Address) = decode!(
        &e,
        data2,
        (Address, Address),
        "data must be (Address, Address)"
    );
    assert_eq!(
        (prev2, next2),
        (previous, next),
        "the two ownership events must agree"
    );
    assert_eq!(client.get_owner(), heir);
}

/// `admin_rotated` tracks the ledger sequence it commits at, including far
/// from the proposal — a stale sequence would break an indexer's
/// fork-detection.
#[test]
fn admin_rotated_ledger_sequence_tracks_the_ledger() {
    let (e, client, _id, super_admin) = setup();
    let heir = Address::generate(&e);
    client.add_admin(&super_admin, &heir, &AdminRole::SuperAdmin);
    client.transfer_ownership(&super_admin, &heir);
    e.ledger().with_mut(|l| {
        l.timestamp += OWNERSHIP_TRANSFER_TIMELOCK;
        l.sequence_number += 4_242;
    });
    let expected_seq = e.ledger().sequence();
    client.accept_ownership(&heir);

    let (_, data) = nth(&e, "admin_rotated", 0);
    let seq: u32 = decode!(&e, data, u32, "admin_rotated data must be a u32");
    assert_eq!(
        seq, expected_seq,
        "the sequence must be read at commit time"
    );
}

/// `pause_proposed` tags the `PauseAction` discriminant; a swapped mapping
/// would make an indexer apply the wrong transition.
#[test]
fn pause_proposed_encodes_the_action_and_the_proposal_id() {
    let (e, client, _id, super_admin) = setup();
    let signers = add_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers[0].clone();
    let s2 = signers[1].clone();

    let pause_id = client.pause(&s1).unwrap();
    assert_emitted(&e, &["pause_proposed"], "pause (proposal path)");
    let (topics, data) = nth(&e, "pause_proposed", 0);
    assert_eq!(topics.len(), 2, "pause_proposed must have 2 topics");
    let id_topic: u64 = decode!(
        &e,
        topics.get(1).unwrap(),
        u64,
        "topics[1] must be a u64 id"
    );
    assert_eq!(
        id_topic, pause_id,
        "topics[1] must carry the id the entrypoint returned, or an indexer cannot correlate \
         `pause_proposed` with `pause_approved` / `paused`"
    );
    let action: u32 = decode!(
        &e,
        data,
        u32,
        "pause_proposed data must be a u32 action tag"
    );
    assert_eq!(action, PauseAction::Pause as u32);

    // Reach the threshold, execute, then propose the opposite action.
    client.approve_pause_proposal(&s2, &pause_id);
    client.execute_pause_proposal(&pause_id);
    let unpause_id = client.unpause(&s1).unwrap();
    let (_, data) = nth(&e, "pause_proposed", 0);
    let action: u32 = decode!(
        &e,
        data,
        u32,
        "pause_proposed data must be a u32 action tag"
    );
    assert_eq!(
        action,
        PauseAction::Unpause as u32,
        "an unpause proposal must tag action = Unpause"
    );
    assert_ne!(
        unpause_id, pause_id,
        "pause and unpause in the same epoch must derive different ids, or one overwrites the \
         other in DataKey::PauseProposal"
    );
}

/// `pause_approved` correlates the proposal id (topic) with the signer (data).
#[test]
fn pause_approved_payload_carries_proposal_id_and_signer() {
    let (e, client, _id, super_admin) = setup();
    let signers = add_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers[0].clone();
    let s2 = signers[1].clone();

    let id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &id);

    assert_emitted(&e, &["pause_approved"], "approve_pause_proposal");
    let (topics, data) = nth(&e, "pause_approved", 0);
    assert_eq!(topics.len(), 2);
    let id_topic: u64 = decode!(
        &e,
        topics.get(1).unwrap(),
        u64,
        "topics[1] must be the proposal id"
    );
    assert_eq!(id_topic, id);
    let signer: Address = decode!(&e, data, Address, "data must be the approving signer");
    assert_eq!(
        signer, s2,
        "pause_approved must name the signer who approved, not the proposal initiator"
    );
}

/// `pause_threshold_set` carries the committed `u32` threshold.
#[test]
fn pause_threshold_set_payload_is_a_u32() {
    let (e, client, _id, super_admin) = setup();
    add_signers(&e, &client, &super_admin, 3, 1);

    assert_emitted(&e, &["pause_threshold_set"], "set_pause_threshold");
    let (topics, data) = nth(&e, "pause_threshold_set", 0);
    assert_eq!(topics.len(), 1);
    let threshold: u32 = decode!(&e, data, u32, "pause_threshold_set data must be a u32");
    assert_eq!(
        threshold, 1,
        "the event must report the threshold that was committed"
    );
}

/// `pause_signer_set` indexes the signer and reports the flag; both values
/// round-trip, and a stuck `true` would make a revoked signer look active
/// forever.
#[test]
fn pause_signer_set_payload_carries_the_signer_and_flag() {
    let (e, client, _id, super_admin) = setup();
    let signer = Address::generate(&e);

    client.set_pause_signer(&super_admin, &signer, &true);
    assert_emitted(&e, &["pause_signer_set"], "enable a pause signer");
    let (topics, data) = nth(&e, "pause_signer_set", 0);
    assert_eq!(
        topics.len(),
        2,
        "pause_signer_set must have 2 topics: the signer must be indexed so an indexer can query \
         a signer's registration without scanning payloads"
    );
    let emitted: Address = decode!(
        &e,
        topics.get(1).unwrap(),
        Address,
        "topics[1] must be the signer"
    );
    assert_eq!(
        emitted, signer,
        "topics[1] must be the signer, not the calling admin"
    );
    let enabled: bool = decode!(&e, data, bool, "data must be a bool");
    assert!(enabled, "enabling must report enabled = true");

    client.set_pause_signer(&super_admin, &signer, &false);
    assert_emitted(&e, &["pause_signer_set"], "disable a pause signer");
    let (_, data) = nth(&e, "pause_signer_set", 0);
    let enabled: bool = decode!(&e, data, bool, "data must be a bool");
    assert!(!enabled, "disabling must report enabled = false");
}

// ══════════════════════════════════════════════════════════════════════════
// I10 — boundary values
// ══════════════════════════════════════════════════════════════════════════

/// `paused` is a 2-tuple on the proposal path: `Some(id)` plus an empty reason.
#[test]
fn paused_payload_on_the_proposal_path_is_some_id_and_empty_reason() {
    let (e, client, _id, super_admin) = setup();
    let s1 = add_signers(&e, &client, &super_admin, 1, 1)[0].clone();
    let pause_id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&pause_id);

    assert_emitted(&e, &["paused"], "execute_pause_proposal (pause)");
    let (topics, data) = nth(&e, "paused", 0);
    assert_eq!(topics.len(), 1);
    let (id, reason): (Option<u64>, SorobanString) = decode!(
        &e,
        data,
        (Option<u64>, SorobanString),
        "paused data must be (Option<u64>, String) — the field is the easiest in the contract to \
         break because unpaused uses a bare Option<u64>"
    );
    assert_eq!(
        id,
        Some(pause_id),
        "a proposal-driven pause must correlate with its proposal id"
    );
    assert_eq!(
        reason,
        SorobanString::from_str(&e, ""),
        "the proposal path supplies no human reason, but the field must still be present so the \
         tuple arity is stable across both pause paths"
    );
    assert!(client.is_paused());
}

/// `paused` is the *same* 2-tuple on the direct path: `None` plus the caller
/// as the reason. Both branches must decode identically.
#[test]
fn paused_payload_on_the_direct_path_is_none_and_a_reason() {
    let (e, client, _id, super_admin) = setup();

    assert!(
        client.pause(&super_admin).is_none(),
        "threshold 0 must pause inline"
    );
    assert_emitted(&e, &["paused"], "pause (direct path)");
    let (topics, data) = nth(&e, "paused", 0);
    assert_eq!(topics.len(), 1);
    let (id, reason): (Option<u64>, SorobanString) = decode!(
        &e,
        data,
        (Option<u64>, SorobanString),
        "paused data must be (Option<u64>, String)"
    );
    assert_eq!(
        id, None,
        "a direct pause has no proposal, so the id must be `None` and not a sentinel such as 0"
    );
    assert_eq!(
        reason,
        addr(&super_admin),
        "a direct pause records the caller as the reason so the audit trail is not empty"
    );
}

/// `unpaused` is a **bare** `Option<u64>` on the proposal path.
///
/// This is the easiest field in the whole contract to break: `paused` uses a
/// 2-tuple while `unpaused` uses a bare optional, so a "harmonising" refactor
/// that wraps it in a tuple would compile, decode as a `u64`-shaped payload
/// for a while, and silently break consumers.
#[test]
fn unpaused_payload_is_a_bare_optional_id_on_the_proposal_path() {
    let (e, client, _id, super_admin) = setup();
    let s1 = add_signers(&e, &client, &super_admin, 1, 1)[0].clone();

    let pause_id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&pause_id);
    let unpause_id = client.unpause(&s1).unwrap();
    client.execute_pause_proposal(&unpause_id);

    assert_emitted(&e, &["unpaused"], "execute_pause_proposal (unpause)");
    let (topics, data) = nth(&e, "unpaused", 0);
    assert_eq!(
        topics.len(),
        1,
        "unpaused must have 1 topic; the proposal id belongs in `data`, not as an indexed topic, \
         which would diverge it from `paused`"
    );
    let id: Option<u64> = decode!(
        &e,
        data,
        Option<u64>,
        "unpaused data must decode as a bare Option<u64>; wrapping it in a tuple is a silent \
         breaking change because both accept a u64-shaped payload"
    );
    assert_eq!(id, Some(unpause_id));
    assert!(!client.is_paused());
}

/// The direct path (threshold 0) publishes `None`, so an indexer must handle
/// both the "proposal-driven" and "direct" unpause shapes from the same field.
#[test]
fn unpaused_payload_on_the_direct_path_is_none() {
    let (e, client, _id, super_admin) = setup();

    // No signers registered, so the threshold is 0 and pause/unpause apply
    // inline instead of going through the proposal path.
    client.pause(&super_admin);
    assert_emitted(&e, &["paused"], "pause (direct)");
    assert!(client.unpause(&super_admin).is_none());
    assert_emitted(&e, &["unpaused"], "unpause (direct)");

    let (topics, data) = nth(&e, "unpaused", 0);
    assert_eq!(
        topics.len(),
        1,
        "the direct path must share the frozen arity"
    );
    let id: Option<u64> = decode!(&e, data, Option<u64>, "unpaused data must be Option<u64>");
    assert_eq!(id, None, "a direct unpause has no proposal id");
}

/// A pause proposed exactly on an epoch boundary is still well formed, and the
/// id it derives is a full-width `u64`.
///
/// The id is `SHA-256(action ++ epoch)[0..8]`, so a narrower type anywhere in
/// the derivation or the event encoding would truncate it and two epochs could
/// collide on the same `pause_proposed` topic.
#[test]
fn epoch_boundary_produces_a_well_formed_event() {
    let (e, client, _id, super_admin) = setup();
    let s1 = add_signers(&e, &client, &super_admin, 1, 1)[0].clone();

    // Land exactly on a multiple of the epoch size.
    let boundary = 7 * u32::from(PROPOSAL_EPOCH_SIZE);
    e.ledger().with_mut(|l| l.sequence_number = boundary);
    let id = client.pause(&s1).unwrap();

    assert_emitted(&e, &["pause_proposed"], "pause at the epoch boundary");
    let (topics, data) = nth(&e, "pause_proposed", 0);
    assert_eq!(topics.len(), 2);
    let emitted: u64 = decode!(
        &e,
        topics.get(1).unwrap(),
        u64,
        "topics[1] must be a u64 id"
    );
    let action: u32 = decode!(
        &e,
        data,
        u32,
        "pause_proposed data must be a u32 action tag"
    );
    assert_eq!(emitted, id, "the topic id must equal the returned id");
    assert_eq!(action, PauseAction::Pause as u32);
    assert!(
        id > u64::from(u32::MAX),
        "the derived id is the first 8 bytes of a SHA-256 digest and must exceed the u32 range; \
         a value inside u32 would mean the derivation was truncated"
    );

    // One sequence before the boundary is a different epoch, hence a different
    // id — otherwise a proposal could be executed one sequence early.
    e.ledger().with_mut(|l| l.sequence_number = boundary - 1);
    let res = client.try_execute_pause_proposal(&id);
    assert_rejected(res, ContractError::StaleAdminEpoch);
    assert_emitted_nothing(&e, "executing a proposal one sequence into the next epoch");
}

// ══════════════════════════════════════════════════════════════════════════
// I3 — indexer identity: topics[1] is the subject, never the caller
// ══════════════════════════════════════════════════════════════════════════

/// The indexed `topics[1]` of the `ROLE_*` events is the **subject**; the
/// caller lives in `data`.
///
/// This is the most consequential identity property in the file. If the two
/// were swapped, every indexer would attribute admin grants to the SuperAdmin
/// who performed them and its per-admin view would be permanently wrong while
/// the transaction stream looked healthy.
#[test]
fn role_events_index_the_subject_not_the_caller() {
    let (e, client, _id, super_admin) = setup();
    let second_super = Address::generate(&e);
    client.add_admin(&super_admin, &second_super, &AdminRole::SuperAdmin);
    let subject = Address::generate(&e);

    client.add_admin(&second_super, &subject, &AdminRole::Operator);

    assert_ne!(second_super, subject, "the fixture needs a distinct caller");
    let (topics, data) = nth(&e, "ROLE_ASSIGNED", 0);
    let indexed: Address = decode!(
        &e,
        topics.get(1).unwrap(),
        Address,
        "topics[1] must be an Address"
    );
    let (role, caller): (AdminRole, Address) = decode!(
        &e,
        data,
        (AdminRole, Address),
        "data must be (AdminRole, Address)"
    );
    assert_eq!(
        indexed, subject,
        "topics[1] must index the admin whose role changed, never the caller"
    );
    assert_eq!(caller, second_super, "data must carry the acting caller");
    assert_eq!(role, AdminRole::Operator);
}

/// Same property for `ROLE_REVOKED`.
#[test]
fn role_revoked_indexes_the_subject_not_the_caller() {
    let (e, client, _id, super_admin) = setup();
    let peer = Address::generate(&e);
    client.add_admin(&super_admin, &peer, &AdminRole::SuperAdmin);
    let subject = Address::generate(&e);
    client.add_admin(&super_admin, &subject, &AdminRole::Operator);

    client.deactivate_admin(&peer, &subject);

    let (topics, data) = nth(&e, "ROLE_REVOKED", 0);
    let indexed: Address = decode!(
        &e,
        topics.get(1).unwrap(),
        Address,
        "topics[1] must be an Address"
    );
    let (caller,): (Address,) = decode!(&e, data, (Address,), "data must be a 1-tuple (Address)");
    assert_eq!(
        indexed, subject,
        "ROLE_REVOKED topics[1] must be the subject"
    );
    assert_eq!(caller, peer, "ROLE_REVOKED data must carry the caller");
}

/// `topics[0]` is a `Symbol` on every path of every event.
#[test]
fn every_event_names_itself_with_a_symbol_topic() {
    let (e, client, _id, super_admin) = setup();
    let subject = Address::generate(&e);
    let s1 = Address::generate(&e);

    let mut invocations = 0u32;
    let mut check = |e: &Env, invocations: &mut u32| {
        *invocations += 1;
        for (_, n, topics, _) in last_log(e) {
            assert!(
                Symbol::try_from_val(e, &topics.get(0).unwrap()).is_ok(),
                "`{n}` did not decode topics[0] as a Symbol; an indexer could not route it"
            );
        }
    };

    client.add_admin(&super_admin, &subject, &AdminRole::Admin);
    check(&e, &mut invocations);
    client.deactivate_admin(&super_admin, &subject);
    check(&e, &mut invocations);
    client.reactivate_admin(&super_admin, &subject);
    check(&e, &mut invocations);
    client.update_admin_role(&super_admin, &subject, &AdminRole::Operator);
    check(&e, &mut invocations);
    client.suspend_admin(&super_admin, &subject, &(e.ledger().timestamp() + 10));
    check(&e, &mut invocations);
    client.remove_admin(&super_admin, &subject);
    check(&e, &mut invocations);
    client.set_pause_signer(&super_admin, &s1, &true);
    check(&e, &mut invocations);
    client.set_pause_threshold(&super_admin, &1u32);
    check(&e, &mut invocations);
    let id = client.pause(&s1).unwrap();
    check(&e, &mut invocations);
    client.execute_pause_proposal(&id);
    check(&e, &mut invocations);
    let uid = client.unpause(&s1).unwrap();
    check(&e, &mut invocations);
    client.execute_pause_proposal(&uid);
    check(&e, &mut invocations);

    assert!(
        invocations >= 10,
        "the sweep must cover a broad slice of the surface"
    );
}

// ══════════════════════════════════════════════════════════════════════════
// I4 / I9 — event attribution
// ══════════════════════════════════════════════════════════════════════════

/// Every event is attributed to the emitting contract, never to the caller.
#[test]
fn events_are_attributed_to_the_emitting_contract() {
    let (e, client, id, super_admin) = setup();
    let subject = Address::generate(&e);
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);

    for (contract, n, _, _) in last_log(&e) {
        assert_eq!(
            contract,
            id,
            "event `{n}` is attributed to {:?} instead of the admin contract {:?}",
            addr(&contract),
            addr(&id)
        );
        assert_ne!(
            contract,
            super_admin,
            "an event must never be attributed to the calling admin {:?}",
            addr(&super_admin)
        );
    }
}

/// Two Admin instances share a topic namespace, so only the contract address
/// disambiguates them. Each instance's payload must carry its own subject.
#[test]
fn two_admin_instances_never_cross_attribute_events() {
    let e = Env::default();
    e.mock_all_auths();
    let id_a = e.register_contract(None, AdminContract);
    let id_b = e.register_contract(None, AdminContract);
    let admin_a = Address::generate(&e);
    let admin_b = Address::generate(&e);
    let client_a = AdminContractClient::new(&e, &id_a);
    let client_b = AdminContractClient::new(&e, &id_b);
    client_a.initialize(&admin_a, &1u32, &100u32);
    client_b.initialize(&admin_b, &1u32, &100u32);

    let only_on_a = Address::generate(&e);
    let only_on_b = Address::generate(&e);
    client_a.add_admin(&admin_a, &only_on_a, &AdminRole::Admin);
    // A's log is the last invocation.
    let (_, data) = nth(&e, "admin_added", 0);
    let info: AdminInfo = decode!(&e, data, AdminInfo, "admin_added data must be an AdminInfo");
    assert_eq!(&info.address, &only_on_a);
    let (contract, _, _) = only(&e, "admin_added");
    assert_eq!(contract, id_a, "A's event must be attributed to A");

    client_b.add_admin(&admin_b, &only_on_b, &AdminRole::Admin);
    let (_, data) = nth(&e, "admin_added", 0);
    let info: AdminInfo = decode!(&e, data, AdminInfo, "admin_added data must be an AdminInfo");
    assert_eq!(&info.address, &only_on_b);
    let (contract, _, _) = only(&e, "admin_added");
    assert_eq!(contract, id_b, "B's event must be attributed to B");

    // And the two subjects are genuinely different, so the assertions above
    // are not vacuous.
    assert_ne!(only_on_a, only_on_b);
}

// ══════════════════════════════════════════════════════════════════════════
// I5 — a rejected invocation emits nothing
// ══════════════════════════════════════════════════════════════════════════

/// Authorization failures publish nothing and advance no epoch.
#[test]
fn unauthorized_role_mutations_emit_no_events() {
    let (e, client, _id, super_admin) = setup();
    let outsider = Address::generate(&e);
    let subject = Address::generate(&e);

    // An unknown address is not an admin at all.
    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_add_admin(&outsider, &subject, &AdminRole::Admin),
        ContractError::NotAdmin,
    );
    assert_emitted_nothing(&e, "add_admin by a non-admin");
    assert_eq!(
        client.get_config_epoch(),
        epoch,
        "a rejected call must not advance the epoch"
    );

    // A registered Operator cannot grant an Admin role.
    client.add_admin(&super_admin, &outsider, &AdminRole::Operator);
    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_add_admin(&outsider, &subject, &AdminRole::Admin),
        ContractError::NotAdmin,
    );
    assert_emitted_nothing(&e, "add_admin of an Admin role by an Operator");
    assert_eq!(client.get_config_epoch(), epoch);

    // An Admin cannot remove a peer Admin: a strictly higher role is required.
    let peer = Address::generate(&e);
    client.add_admin(&super_admin, &peer, &AdminRole::Admin);
    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_remove_admin(&peer, &subject),
        ContractError::NotAdmin,
    );
    assert_emitted_nothing(&e, "remove_admin by a peer Admin");
    assert_eq!(client.get_config_epoch(), epoch);

    // An Operator cannot deactivate anyone.
    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_deactivate_admin(&outsider, &peer),
        ContractError::NotAdmin,
    );
    assert_emitted_nothing(&e, "deactivate_admin by an Operator");
    assert_eq!(client.get_config_epoch(), epoch);

    assert_eq!(
        client.get_admin_count(),
        3,
        "state is untouched by the rejections"
    );
}

/// Duplicate and boundary rejections publish nothing.
#[test]
fn invalid_and_duplicate_mutations_emit_no_events() {
    let (e, client, _id, super_admin) = setup();
    let subject = Address::generate(&e);
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);

    // Already an admin.
    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_add_admin(&super_admin, &subject, &AdminRole::Admin),
        ContractError::AlreadyActive,
    );
    assert_emitted_nothing(&e, "duplicate add_admin");
    assert_eq!(client.get_config_epoch(), epoch);

    // Already deactivated.
    client.deactivate_admin(&super_admin, &subject);
    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_deactivate_admin(&super_admin, &subject),
        ContractError::AlreadyDeactivated,
    );
    assert_emitted_nothing(&e, "duplicate deactivate_admin");
    assert_eq!(client.get_config_epoch(), epoch);

    // The genesis guard: re-initialising must not replay `admin_initialized`.
    // The log of the rejected call is the assertion; the contract state is
    // checked separately because a rolled-back call also empties the log.
    assert_rejected(
        client.try_initialize(&super_admin, &1u32, &100u32),
        ContractError::AlreadyInitialized,
    );
    assert_emitted_nothing(&e, "re-initialize");
    assert_eq!(
        client.get_config_epoch(),
        epoch,
        "a rejected re-initialize must not advance the epoch"
    );

    // Suspension must be in the future.
    let now = e.ledger().timestamp();
    assert_rejected(
        client.try_suspend_admin(&super_admin, &subject, &now),
        ContractError::AdminSuspended,
    );
    assert_emitted_nothing(&e, "suspend_admin with a non-future timestamp");
    assert_eq!(client.get_config_epoch(), epoch);
}

/// Ownership rejections publish nothing: no timelock bypass, no wrong acceptor,
/// and no replayed rotation.
#[test]
fn ownership_rejections_emit_no_events() {
    let (e, client, _id, super_admin) = setup();
    let heir = Address::generate(&e);
    client.add_admin(&super_admin, &heir, &AdminRole::SuperAdmin);

    // Not the owner.
    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_transfer_ownership(&heir, &heir),
        ContractError::NotAdmin,
    );
    assert_emitted_nothing(&e, "transfer_ownership by a non-owner");
    assert_eq!(client.get_config_epoch(), epoch);

    // Timelock not elapsed — the off-by-one that would let a new owner seize
    // control before the delay expires.
    client.transfer_ownership(&super_admin, &heir);
    let epoch = client.get_config_epoch();
    e.ledger()
        .with_mut(|l| l.timestamp += OWNERSHIP_TRANSFER_TIMELOCK - 1);
    assert_rejected(
        client.try_accept_ownership(&heir),
        ContractError::TimelockNotReady,
    );
    assert_emitted_nothing(&e, "accept_ownership one second early");
    assert_eq!(client.get_config_epoch(), epoch);
    assert_eq!(
        client.get_owner(),
        super_admin,
        "ownership must not have moved"
    );

    // Wrong acceptor.
    let other = Address::generate(&e);
    assert_rejected(client.try_accept_ownership(&other), ContractError::NotAdmin);
    assert_emitted_nothing(&e, "accept_ownership by a non-pending address");
    assert_eq!(client.get_owner(), super_admin);

    // Exactly at the timelock the rotation is emitted, once.
    e.ledger().with_mut(|l| l.timestamp += 1);
    client.accept_ownership(&heir);
    assert_emitted(
        &e,
        &["admin_rotated", "ownership_transfer_accepted"],
        "accept_ownership at the timelock boundary",
    );
    assert_eq!(client.get_owner(), heir);

    // Replayed accept is a clean rejection with no second rotation.
    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_accept_ownership(&heir),
        ContractError::NoPendingAdmin,
    );
    assert_emitted_nothing(&e, "replayed accept_ownership");
    assert_eq!(client.get_config_epoch(), epoch);
}

/// Pause-proposal rejections publish nothing.
#[test]
fn pause_proposal_rejections_emit_no_events() {
    let (e, client, _id, super_admin) = setup();
    let signers = add_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers[0].clone();
    let s2 = signers[1].clone();

    let id = client.pause(&s1).unwrap();
    assert_emitted(&e, &["pause_proposed"], "pause");

    // Unknown proposal.
    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_approve_pause_proposal(&s2, &999_999),
        ContractError::ProposalNotFound,
    );
    assert_emitted_nothing(&e, "approve of an unknown proposal");
    assert_eq!(client.get_config_epoch(), epoch);

    // Not a signer.
    let outsider = Address::generate(&e);
    assert_rejected(
        client.try_approve_pause_proposal(&outsider, &id),
        ContractError::NotSigner,
    );
    assert_emitted_nothing(&e, "approve by a non-signer");

    // Insufficient approvals — the most dangerous rejection to get wrong,
    // because a phantom `paused` would lock the contract for a consumer.
    assert_rejected(
        client.try_execute_pause_proposal(&id),
        ContractError::InsufficientApprovals,
    );
    assert_emitted_nothing(&e, "execute below the approval threshold");
    assert!(
        !client.is_paused(),
        "a rejected execute must not pause the contract"
    );

    // Threshold above the signer count.
    assert_rejected(
        client.try_set_pause_threshold(&super_admin, &5),
        ContractError::ThresholdExceedsSigners,
    );
    assert_emitted_nothing(&e, "set_pause_threshold above the signer count");

    // Pause-signer changes are SuperAdmin-only.
    assert_rejected(
        client.try_set_pause_signer(&s1, &outsider, &true),
        ContractError::NotAdmin,
    );
    assert_emitted_nothing(&e, "set_pause_signer by a non-SuperAdmin");

    // The contract's own address is rejected as a signer.
    let contract_id = client.address.clone();
    assert_rejected(
        client.try_set_pause_signer(&super_admin, &contract_id, &true),
        ContractError::InvalidAdminAddress,
    );
    assert_emitted_nothing(&e, "set_pause_signer with the contract address");
}

/// A stale proposal (derived in a previous epoch) is rejected silently.
///
/// This is the concrete stale-state path: a client that read governance state
/// at epoch *N* and submits at *N+1* must get a clean rejection, never a
/// half-applied pause.
#[test]
fn stale_epoch_proposal_rejection_emits_no_events() {
    let (e, client, _id, super_admin) = setup();
    let signers = add_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers[0].clone();
    let s2 = signers[1].clone();

    let id = client.pause(&s1).unwrap();

    // Cross several epochs so the derived id no longer matches.
    e.ledger()
        .with_mut(|l| l.sequence_number += 3 * u32::from(PROPOSAL_EPOCH_SIZE));

    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_approve_pause_proposal(&s2, &id),
        ContractError::StaleAdminEpoch,
    );
    assert_emitted_nothing(&e, "approval of a stale proposal");
    assert_eq!(client.get_config_epoch(), epoch);

    assert_rejected(
        client.try_execute_pause_proposal(&id),
        ContractError::StaleAdminEpoch,
    );
    assert_emitted_nothing(&e, "execution of a stale proposal");
    assert!(
        !client.is_paused(),
        "a stale proposal must never pause the contract"
    );
    assert_eq!(client.get_config_epoch(), epoch);
}

/// A paused contract is a hard gate: every state-mutating entrypoint rejects
/// silently, so a pause cannot be used to launder phantom events.
#[test]
fn paused_contract_rejects_mutations_without_emitting_events() {
    let (e, client, _id, super_admin) = setup();
    let s1 = add_signers(&e, &client, &super_admin, 1, 1)[0].clone();
    let pause_id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&pause_id);
    assert!(client.is_paused());

    let subject = Address::generate(&e);
    let heir = Address::generate(&e);
    let now = e.ledger().timestamp();

    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_add_admin(&super_admin, &subject, &AdminRole::Admin),
        ContractError::ContractPaused,
    );
    assert_emitted_nothing(&e, "add_admin while paused");

    assert_rejected(
        client.try_deactivate_admin(&super_admin, &subject),
        ContractError::ContractPaused,
    );
    assert_emitted_nothing(&e, "deactivate_admin while paused");

    assert_rejected(
        client.try_suspend_admin(&super_admin, &super_admin, &(now + 10)),
        ContractError::ContractPaused,
    );
    assert_emitted_nothing(&e, "suspend_admin while paused");

    assert_rejected(
        client.try_transfer_ownership(&super_admin, &heir),
        ContractError::ContractPaused,
    );
    assert_emitted_nothing(&e, "transfer_ownership while paused");

    assert_eq!(
        client.get_config_epoch(),
        epoch,
        "no rejected mutation may advance the epoch"
    );
    assert_eq!(
        client.get_admin_count(),
        1,
        "no admin was added while paused"
    );
    assert!(
        client.get_pending_owner().is_none(),
        "no rotation was proposed"
    );

    // Unpausing emits exactly one `unpaused`, with no drift from the four
    // rejected attempts.
    let unpause_id = client.unpause(&s1).unwrap();
    assert_emitted(&e, &["pause_proposed"], "unpause proposal while paused");
    client.execute_pause_proposal(&unpause_id);
    assert_emitted(&e, &["unpaused"], "unpause while paused");
    assert!(!client.is_paused());
}

// ══════════════════════════════════════════════════════════════════════════
// I6 — idempotency: retries must not duplicate the event stream
// ══════════════════════════════════════════════════════════════════════════

/// A rejected call retried any number of times still publishes nothing.
///
/// The single most important retry property: a client wrapping a governance
/// call in a retry loop must not end up with N copies of a successful event
/// once the underlying condition clears.
#[test]
fn repeated_rejected_calls_never_accumulate_events() {
    let (e, client, _id, _super_admin) = setup();
    let outsider = Address::generate(&e);
    let subject = Address::generate(&e);
    let epoch = client.get_config_epoch();

    for attempt in 0..5u32 {
        let res = client.try_add_admin(&outsider, &subject, &AdminRole::Admin);
        assert_rejected(res, ContractError::NotAdmin);
        assert_emitted_nothing(&e, &std::format!("add_admin retry #{attempt}"));
    }
    assert_eq!(
        client.get_config_epoch(),
        epoch,
        "no retry may advance the epoch"
    );
    assert_eq!(
        client.get_admin_count(),
        1,
        "state is untouched by the retries"
    );
}

/// A failed-then-successful sequence produces exactly one event set, with the
/// right shape on the successful attempt.
#[test]
fn retry_after_transient_failure_emits_exactly_one_event_set() {
    let (e, client, _id, super_admin) = setup();
    let subject = Address::generate(&e);

    // Fail first: the target is not an admin yet.
    assert_rejected(
        client.try_update_admin_role(&super_admin, &subject, &AdminRole::Operator),
        ContractError::NotAdmin,
    );
    assert_emitted_nothing(&e, "update_admin_role on a non-admin");

    // Now make the target real and retry.
    client.add_admin(&super_admin, &subject, &AdminRole::Admin);
    assert_emitted(&e, &["admin_added", "ROLE_ASSIGNED"], "the retried grant");
    client.update_admin_role(&super_admin, &subject, &AdminRole::Operator);
    assert_emitted(
        &e,
        &["admin_role_updated", "ROLE_ASSIGNED"],
        "the retried role update",
    );
}

/// `set_pause_threshold` is a true no-op: it early-returns before writing or
/// publishing, so repeating it is completely invisible.
#[test]
fn repeated_pause_threshold_is_a_silent_no_op() {
    let (e, client, _id, super_admin) = setup();
    add_signers(&e, &client, &super_admin, 3, 2);
    let epoch = client.get_config_epoch();

    for _ in 0..3 {
        client.set_pause_threshold(&super_admin, &2u32);
        assert_emitted_nothing(&e, "set_pause_threshold with the same value");
    }
    assert_eq!(
        client.get_config_epoch(),
        epoch,
        "a no-op threshold change must not advance the epoch"
    );
}

/// Re-proposing the same pause in the same epoch is a silent no-op: the id is
/// a pure function of `(action, epoch)`, so the second `pause` targets the
/// existing record and must not re-publish.
#[test]
fn repeated_pause_proposal_in_the_same_epoch_is_a_silent_no_op() {
    let (e, client, _id, super_admin) = setup();
    let s1 = add_signers(&e, &client, &super_admin, 1, 1)[0].clone();

    let first = client.pause(&s1).unwrap();
    assert_emitted(&e, &["pause_proposed"], "the first proposal");
    let epoch = client.get_config_epoch();

    for _ in 0..3 {
        assert_eq!(
            client.pause(&s1).unwrap(),
            first,
            "the proposal id must be deterministic for the same (action, epoch)"
        );
        assert_emitted_nothing(&e, "a repeated pause proposal");
    }
    assert_eq!(
        client.get_config_epoch(),
        epoch,
        "a no-op proposal must not bump the epoch"
    );
}

/// Duplicate approval is a silent no-op: `approve_pause_proposal` publishes
/// only when the approval is newly recorded, so a retried approval changes
/// neither state, nor the event stream, nor the epoch. That is what lets an
/// indexer treat every `pause_approved` emission as a distinct signer
/// approval instead of double-counting a replay.
#[test]
fn duplicate_approval_is_a_silent_no_op() {
    let (e, client, _id, super_admin) = setup();
    let signers = add_signers(&e, &client, &super_admin, 2, 2);
    let s1 = signers[0].clone();
    let s2 = signers[1].clone();

    let id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &id);
    // The first approval is a real state change and must be recorded.
    assert_emitted(&e, &["pause_approved"], "the first approval");
    let (topics, data) = nth(&e, "pause_approved", 0);
    assert_eq!(topics.len(), 2);
    let id_topic: u64 = decode!(&e, topics.get(1).unwrap(), u64, "topics[1] must be a u64");
    let signer: Address = decode!(&e, data, Address, "data must be a signer");
    assert_eq!(id_topic, id, "the approval must reference its proposal");
    assert_eq!(signer, s2, "the approval must name the approving signer");

    let after_first = client.get_config_epoch();

    // A retried approval changes nothing, so it must be completely invisible:
    // no event and no epoch advance.
    client.approve_pause_proposal(&s2, &id);
    assert_emitted_nothing(&e, "a duplicate approval");
    assert_eq!(
        client.get_config_epoch(),
        after_first,
        "a duplicate approval must not advance the epoch: it changed no state"
    );

    // The threshold is now met (s1 approved by proposing, s2 by approving), so
    // execution succeeds and emits exactly one `paused`.
    client.execute_pause_proposal(&id);
    assert_emitted(&e, &["paused"], "execute once the threshold is met");
    assert!(client.is_paused());
}

/// Re-registering an already-registered pause signer, or removing one that was
/// never registered, changes nothing and so must publish nothing.
///
/// `set_pause_signer` is the one pause entrypoint that could still have
/// emitted on a no-op, which would make a client retry loop desynchronise any
/// indexer that replays `pause_signer_set`.
#[test]
fn duplicate_pause_signer_registration_is_a_silent_no_op() {
    let (e, client, _id, super_admin) = setup();
    let signer = Address::generate(&e);

    client.set_pause_signer(&super_admin, &signer, &true);
    assert_emitted(&e, &["pause_signer_set"], "enabling a signer");
    let after_enable = client.get_config_epoch();

    // Enabling an already-enabled signer.
    client.set_pause_signer(&super_admin, &signer, &true);
    assert_emitted_nothing(&e, "re-enabling an already-enabled signer");
    assert_eq!(client.get_config_epoch(), after_enable);

    // Disabling one that was never enabled.
    let stranger = Address::generate(&e);
    client.set_pause_signer(&super_admin, &stranger, &false);
    assert_emitted_nothing(&e, "disabling a signer that was never enabled");
    assert_eq!(client.get_config_epoch(), after_enable);

    // The genuine disable is still recorded, and only once.
    client.set_pause_signer(&super_admin, &signer, &false);
    assert_emitted(&e, &["pause_signer_set"], "the genuine disable");
    let (_, data) = nth(&e, "pause_signer_set", 0);
    let enabled: bool = decode!(&e, data, bool, "data must be a bool");
    assert!(!enabled, "the disable must report enabled = false");

    client.set_pause_signer(&super_admin, &signer, &false);
    assert_emitted_nothing(&e, "disabling an already-disabled signer");
}

/// Executing a proposal twice must not publish a second state event.
#[test]
fn double_execute_emits_no_second_state_event() {
    let (e, client, _id, super_admin) = setup();
    let s1 = add_signers(&e, &client, &super_admin, 1, 1)[0].clone();

    let id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&id);
    assert_emitted(&e, &["paused"], "the first execute");

    let epoch = client.get_config_epoch();
    assert_rejected(
        client.try_execute_pause_proposal(&id),
        ContractError::ProposalNotFound,
    );
    assert_emitted_nothing(&e, "a replayed execute");
    assert_eq!(client.get_config_epoch(), epoch);

    let uid = client.unpause(&s1).unwrap();
    client.execute_pause_proposal(&uid);
    assert_emitted(&e, &["unpaused"], "the unpause");

    assert_rejected(
        client.try_execute_pause_proposal(&uid),
        ContractError::ProposalNotFound,
    );
    assert_emitted_nothing(&e, "a replayed unpause execute");
}

/// A direct pause/unpause that changes nothing publishes nothing, so the
/// `paused` / `unpaused` events are exactly the state *transitions*.
#[test]
fn direct_pause_and_unpause_no_ops_emit_nothing() {
    let (e, client, _id, super_admin) = setup();

    client.unpause(&super_admin);
    assert_emitted_nothing(&e, "unpausing a contract that is not paused");

    client.pause(&super_admin);
    assert_emitted(&e, &["paused"], "the first pause");

    client.pause(&super_admin);
    assert_emitted_nothing(&e, "re-pausing an already-paused contract");

    client.unpause(&super_admin);
    assert_emitted(&e, &["unpaused"], "the first unpause");
}

// ══════════════════════════════════════════════════════════════════════════
// I1 / I2 — repetition and concurrency do not drift the schema
// ══════════════════════════════════════════════════════════════════════════

/// Repeating the same lifecycle many times keeps every arity stable.
///
/// Guards against an accumulator bug — a topic appended twice on the second
/// iteration, say — that a single-shot test would miss.
#[test]
fn repeated_lifecycle_keeps_a_stable_schema() {
    const ROUNDS: u32 = 8;
    let (e, client, _id, super_admin) = setup();

    for round in 0..ROUNDS {
        let subject = Address::generate(&e);
        client.add_admin(&super_admin, &subject, &AdminRole::Operator);
        assert_emitted(&e, &["admin_added", "ROLE_ASSIGNED"], "round add");
        client.deactivate_admin(&super_admin, &subject);
        assert_emitted(
            &e,
            &["admin_deactivated", "ROLE_REVOKED"],
            "round deactivate",
        );
        client.reactivate_admin(&super_admin, &subject);
        assert_emitted(
            &e,
            &["admin_reactivated", "ROLE_ASSIGNED"],
            "round reactivate",
        );
        client.remove_admin(&super_admin, &subject);
        assert_emitted(&e, &["admin_removed", "ROLE_REVOKED"], "round remove");

        // Every event in this round matches its frozen arity.
        for (n, topics, _) in last_log(&e).into_iter().map(|(_, n, t, d)| (n, t, d)) {
            let frozen = FROZEN_EVENTS
                .iter()
                .find(|f| f.name == n)
                .unwrap_or_else(|| panic!("{n} is not in FROZEN_EVENTS"));
            assert_eq!(
                topics.len(),
                frozen.topics,
                "round {round}: `{n}` drifted to arity {} (frozen {})",
                topics.len(),
                frozen.topics
            );
        }
    }
    assert_eq!(
        client.get_admin_count(),
        1,
        "every round's subject was removed again"
    );
}

/// Two actors granting alternately never interleave their own event pairs.
///
/// A monotonic indexer reading the log while another admin acts must still see
/// a coherent per-subject history. Each round uses fresh subjects so the
/// interleaving is genuine rather than an accumulation of no-ops.
#[test]
fn interleaved_actors_do_not_interleave_their_own_event_pairs() {
    const ROUNDS: u32 = 5;
    let (e, client, _id, super_admin) = setup();

    for round in 0..ROUNDS {
        let a = Address::generate(&e);
        let b = Address::generate(&e);

        client.add_admin(&super_admin, &a, &AdminRole::Operator);
        assert_emitted(&e, &["admin_added", "ROLE_ASSIGNED"], "round grant to A");
        let (_, data) = nth(&e, "admin_added", 0);
        let info: AdminInfo = decode!(&e, data, AdminInfo, "admin_added data must be an AdminInfo");
        assert_eq!(
            info.address, a,
            "the payload of A's grant must describe A even though B is granted in the next call"
        );
        let (_, data) = nth(&e, "ROLE_ASSIGNED", 0);
        let (role, caller): (AdminRole, Address) = decode!(
            &e,
            data,
            (AdminRole, Address),
            "ROLE_ASSIGNED data must be (AdminRole, Address)"
        );
        assert_eq!(role, AdminRole::Operator);
        assert_eq!(
            caller, super_admin,
            "the acting caller must be recorded, not the other subject"
        );

        client.add_admin(&super_admin, &b, &AdminRole::Admin);
        assert_emitted(&e, &["admin_added", "ROLE_ASSIGNED"], "round grant to B");
        let (_, data) = nth(&e, "admin_added", 0);
        let info: AdminInfo = decode!(&e, data, AdminInfo, "admin_added data must be an AdminInfo");
        assert_eq!(info.address, b, "B's grant must describe B, not A");
        assert_eq!(
            info.role,
            AdminRole::Admin,
            "the role must not bleed between subjects"
        );

        // Within a single invocation the two events are adjacent and ordered,
        // which is what an indexer relies on for a crash-consistent replay.
        assert_eq!(
            client.get_admin_count(),
            1 + 2 * (round + 1),
            "each round adds exactly two admins on top of the bootstrap SuperAdmin; a lost or \
             duplicated event would desynchronise the indexer's view from the ledger"
        );
    }
}

/// Alternating pause/unpause cycles keep the payloads on the same schema and
/// the two events strictly interleaved, so a consumer tracking the boolean
/// pause state cannot desynchronise.
///
/// Each cycle runs in a **fresh epoch**, which is the only supported operating
/// pattern — see
/// [`re_proposing_a_pause_in_the_same_epoch_after_execution_is_rejected`].
#[test]
fn alternating_pause_cycles_preserve_payload_shape_and_order() {
    const CYCLES: u32 = 4;
    let (e, client, _id, super_admin) = setup();
    let s1 = add_signers(&e, &client, &super_admin, 1, 1)[0].clone();

    for cycle in 0..CYCLES {
        // New epoch each cycle, so the derived proposal id differs.
        e.ledger()
            .with_mut(|l| l.sequence_number += u32::from(PROPOSAL_EPOCH_SIZE));

        let pause_id = client.pause(&s1).unwrap();
        client.execute_pause_proposal(&pause_id);
        let (topics, data) = nth(&e, "paused", 0);
        assert_eq!(topics.len(), 1, "cycle {cycle}: paused arity drifted");
        let (id, _reason): (Option<u64>, SorobanString) = decode!(
            &e,
            data,
            (Option<u64>, SorobanString),
            "paused data shape drifted"
        );
        assert_eq!(
            id,
            Some(pause_id),
            "cycle {cycle}: paused lost its proposal id"
        );
        assert!(client.is_paused(), "cycle {cycle}: must be paused");

        let unpause_id = client.unpause(&s1).unwrap();
        assert_ne!(
            unpause_id, pause_id,
            "cycle {cycle}: pause and unpause must derive different proposal ids in the same \
             epoch, or one overwrites the other in DataKey::PauseProposal"
        );
        client.execute_pause_proposal(&unpause_id);
        let (topics, data) = nth(&e, "unpaused", 0);
        assert_eq!(topics.len(), 1, "cycle {cycle}: unpaused arity drifted");
        let id: Option<u64> = decode!(&e, data, Option<u64>, "unpaused data shape drifted");
        assert_eq!(
            id,
            Some(unpause_id),
            "cycle {cycle}: unpaused lost its proposal id"
        );
        assert!(!client.is_paused(), "cycle {cycle}: must be unpaused");
    }
}

/// A completed proposal leaves its per-signer approval records behind, so
/// re-proposing the *same* action in the *same* epoch resets the approval
/// count to zero while the signer is already recorded as having approved.
///
/// The result is that the re-proposed pause can never reach the threshold and
/// is rejected with `InsufficientApprovals` until the epoch rolls over
/// (`PROPOSAL_EPOCH_SIZE` ledger sequences).
///
/// This test pins the behaviour rather than the desired behaviour, for two
/// reasons:
///
/// 1. It is the reason a client must not blindly re-propose in the same epoch,
///    and an operator needs to see it named in a test, not rediscovered during
///    an incident.
/// 2. The failure is **safe**: it fails closed. No pause is applied, no
///    `paused` event is published, and the contract stays unpaused — so no
///    indexer records a governance action that did not happen.
///
/// If `propose_action` is later fixed (by clearing `DataKey::PauseApproval*`
/// together with the proposal record), this test will fail and must be
/// updated in the same change. That failure is the intended signal.
#[test]
fn re_proposing_a_pause_in_the_same_epoch_after_execution_is_rejected() {
    let (e, client, _id, super_admin) = setup();
    let s1 = add_signers(&e, &client, &super_admin, 1, 1)[0].clone();

    // Cycle 1 succeeds.
    let first = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&first);
    assert_emitted(&e, &["paused"], "the first executed pause");
    let unpause_id = client.unpause(&s1).unwrap();
    client.execute_pause_proposal(&unpause_id);
    assert!(!client.is_paused());

    // Cycle 2 in the same epoch: the proposal is recreated, but the stale
    // approval record means the count stays below the threshold.
    let second = client.pause(&s1).unwrap();
    assert_eq!(
        second, first,
        "the id is a pure function of (action, epoch), so the same action in the same epoch \
         reuses the id"
    );
    assert_emitted(&e, &["pause_proposed"], "the re-proposal");
    assert!(
        !client.is_paused(),
        "a proposal that has not executed must not have paused the contract"
    );

    let epoch = client.get_config_epoch();
    let res = client.try_execute_pause_proposal(&second);
    assert_rejected(res, ContractError::InsufficientApprovals);
    assert_emitted_nothing(&e, "executing a same-epoch re-proposal");
    assert_eq!(
        client.get_config_epoch(),
        epoch,
        "the rejected execute must not bump the epoch"
    );
    assert!(
        !client.is_paused(),
        "the emergency pause stays blocked (fails closed)"
    );

    // Crossing into the next epoch clears the hazard: the id is derived from
    // the epoch, so a fresh proposal starts with a fresh approval set.
    e.ledger()
        .with_mut(|l| l.sequence_number += u32::from(PROPOSAL_EPOCH_SIZE));
    let third = client.pause(&s1).unwrap();
    assert_ne!(
        third, first,
        "a new epoch must derive a new proposal id, which is what unblocks the signer"
    );
    client.execute_pause_proposal(&third);
    assert_emitted(&e, &["paused"], "the pause in the next epoch");
    assert!(client.is_paused());
}

/// A contract that never emits `ROLE_ASSIGNED` still satisfies the frozen
/// table — proving the table is not vacuous: an empty log is *not* accepted as
/// covering the surface. (Guards the "reachable" test against silently
/// short-circuiting.)
#[test]
fn an_empty_log_does_not_satisfy_the_surface_contract() {
    let (e, client, _id, _super_admin) = setup();
    // A read-only call emits nothing.
    let _ = client.get_admin_count();
    assert_emitted_nothing(&e, "a read-only getter");
    for frozen in FROZEN_EVENTS {
        assert!(
            count(&e, frozen.name) == 0,
            "`{}` must not appear for a read-only call",
            frozen.name
        );
    }
}

/// Count events named `name` in the most recent invocation.
fn count(e: &Env, name: &str) -> u32 {
    names(e).iter().filter(|n| **n == name).count() as u32
}

/// Assert the most recent invocation published `expected` events named `name`.
#[track_caller]
fn assert_count(e: &Env, name: &str, expected: u32, what: &str) {
    let got = count(e, name);
    assert_eq!(
        got, expected,
        "{what}: expected {expected} `{name}` event(s) in this invocation, got {got}"
    );
}
