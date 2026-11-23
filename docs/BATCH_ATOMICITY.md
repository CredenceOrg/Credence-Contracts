# Batch Atomicity: All-or-Nothing vs. Partial Progress

This document states the project's stance on how multi-item operations should
handle partial failure, inventories every real multi-item entrypoint against
that stance, and flags the places where the shipped contract doesn't (yet)
match the design intent described elsewhere in `docs/`.

Audience: contributors adding a new multi-item entrypoint, and reviewers
checking whether an existing one behaves the way its doc comment claims.

## The base guarantee: one call is one atomic unit

Every Soroban contract invocation is atomic at the host level: if the
invocation panics or returns an error, none of its state mutations persist —
the whole transaction reverts as if it never happened. This project already
relies on that guarantee in several places:

- `docs/chaos-testing.md` — "**Atomic revert** — when any inner call panics, no state mutation persists."
- `docs/bond-token-custody.md` — "If the token transfer fails, the transaction reverts and the earlier state update is rolled back atomically by Soroban."
- `docs/reentrancy.md` — "In Soroban's execution model, panics typically cause transaction rollback, so the lock state would revert anyway."

That base guarantee is not a design choice this codebase makes — it's free,
and every entrypoint gets it. The actual design question this doc answers is
narrower: **when one call processes a *collection* of items, does a single
bad item fail the whole call, or does the call skip it and keep going?**
Both patterns are used here, deliberately, for different reasons.

## Pattern A: all-or-nothing (fail-fast)

The call runs its validation passes over the *entire* input before writing
any state. If anything in the batch is invalid, the whole call panics/errors
and — via the base guarantee above — nothing is written.

**Use this when:**
- The batch represents a single caller-authorized economic action (one
  signature, one intent), not an unrelated pile of independent work items.
- The item count is small and hard-capped up front, so a full validation
  pass is cheap and bounded.
- Partial completion would itself be a bug: e.g. accepting 3 of 5 attestations
  toward a weight cap would leave the weight-cap invariant meaningless,
  because which 3 succeeded is caller-unpredictable.

**Production example — `add_attestation_batch`**
(`contracts/credence_bond/src/lib.rs:998-1141`, unconditionally compiled,
real deployed entrypoint):

```rust
pub fn add_attestation_batch(
    e: Env,
    subject: Address,
    items: Vec<AttestationBatchItem>,
) -> Vec<Attestation>
```

The function runs five sequential read-only passes over `items` — size/empty
check, duplicate-attester check, per-item auth/nonce/registration, storage
dedup check, weight-cap accumulation — and only after *all five* pass does a
sixth pass perform any writes. A panic in any of the first five leaves
storage byte-for-byte unchanged. See `docs/attestation-batching.md` for the
full constraint list (batch size ≤ `MAX_BATCH_ATTESTATION_SIZE` = 64, weight
caps, dedup keys).

One inconsistency worth knowing about: the duplicate-attester check
(`lib.rs:1017`) raises a bare `panic!("duplicate attester in batch")`
instead of a typed `ContractError`, unlike every other failure path in the
same function (`ContractError::EmptyBatch`, `BatchTooLarge`,
`UnauthorizedAttester`, `DuplicateAttestation`, `AttestationWeightExceedsMax`
— see `contracts/credence_errors/src/lib.rs:299-317`). It's still
all-or-nothing (the call still reverts), just without a typed error a caller
can match on. Tracked by the CI check added in #715
(`scripts/check_no_panic.py`), which will flag this line if it's touched
without being converted to `panic_with_error!`.

## Pattern B: bounded partial progress

The call processes at most `max_iter` items, skipping ones that don't apply
(already handled, not yet due, out of scope) rather than failing the call
over them, and returns however far it got. Callers are expected to invoke it
repeatedly — often permissionlessly, by anyone — until the underlying
backlog is drained.

**Use this when:**
- The workload is unbounded or grows over time (it's a maintenance sweep
  over accumulated state, not a single caller's one-shot action).
- The operation is permissionless / keeper-driven — no single caller can be
  relied on to submit a bounded, pre-validated batch.
- Forcing full completion in one call risks exceeding the Soroban
  instruction budget, or would let one bad/irrelevant item at the front of
  the collection block everything behind it forever.

**Production example — `expire_claims`**
(`contracts/credence_bond/src/lib.rs:2341-2343`, wrapping
`claims::expire_claims_bounded` at `contracts/credence_bond/src/claims.rs:615-699`;
unconditionally compiled, real deployed entrypoint):

```rust
pub fn expire_claims(e: Env, user: Address, max_iter: u32) -> u32
```

Callable by anyone, for any user. Scans up to `max_iter` (hard-capped at
`MAX_BATCH_CLAIMS = 50`) of the user's pending claims; claims past their
`expires_at` are dropped, everything else (unexpired, already-processed, or
`expires_at == 0` permanent claims) is kept as-is. It never panics because
some claims in the window were invalid or not-yet-expired — it just skips
them. See `docs/batch-operations.md`'s "Permissionless Claim Expiry Sweep"
section for the full behavior and gas-safety notes (accurate as written).

**Caveat to know about:** this is a *front-window* sweep, not a persisted
cursor. Every call rescans from index 0 of the user's claim vector. Because
only expired items are removed and everything else stays in place, an
expired claim sitting past index `max_iter` is unreachable until everything
ahead of it has been pruned or expires too. For a single user's claim list
(bounded, slow-growing) this is fine; it would not be an adequate pattern
for scanning an unbounded, fast-growing collection — that's what the cursor
design below exists for.

## Designed, but not currently shipped

Two more multi-item constructs exist in the source tree with the same
all-or-nothing / partial-progress design intent as above, but **neither is
part of the compiled contract in any configuration** right now. Listing them
here so this doc doesn't understate what's actually deployed, and so nobody
re-derives this the hard way during an audit.

### `create_batch_bonds` (all-or-nothing design) — test/testutils-only

`contracts/credence_bond/src/batch.rs`, declared at
`contracts/credence_bond/src/lib.rs:3-4`:

```rust
#[cfg(any(test, feature = "testutils"))]
mod batch;
```

The `testutils` feature is opt-in (`contracts/credence_bond/Cargo.toml:71`,
not part of any default feature set), so a standard release build
(`cargo build --target wasm32-unknown-unknown --release`, no
`--features testutils`) excludes this module entirely — `create_batch_bonds`
does not exist in the deployed contract's callable surface.
`docs/batch-operations.md` describes `create_batch_bonds` /
`validate_batch_bonds` / `get_batch_total_amount` as if they were live
client-callable functions; that doc has been annotated with a note pointing
here rather than rewritten, since the *design* it describes (two-phase
validate-then-write, matching Pattern A above) is accurate for what the code
does when it does run under `cargo test`.

Also worth flagging while it's fresh: the write phase
(`contracts/credence_bond/src/batch.rs:153-177`) stores every bond in a
batch under the same non-identity-scoped `DataKey::Bond` key instead of a
per-identity key, so a multi-bond batch currently overwrites rather than
creates N independent bonds even under `--features testutils`. Not a
production risk today (the module isn't reachable in a release build), but
worth fixing before this module is ever wired up for real.

### `scan_liquidation_candidates` (bounded partial-progress, cursor-based design) — fully dead code

`contracts/credence_bond/src/liquidation_scanner.rs` is **not declared as a
module anywhere in the crate** (`grep -rn "mod liquidation_scanner"` across
`contracts/credence_bond/src/` returns nothing). Unlike `batch.rs`, this
isn't gated behind a feature flag that excludes it from release only — it
is not part of the module tree under *any* build, including
`cargo test`. Its own test files (`test_liquidation_scanner.rs`,
`test_liquidation_rounding.rs`) are equally undeclared and therefore also
don't compile as part of the crate.

The design in that file is the more robust answer to the "unbounded
collection, permissionless caller" problem than the front-window sweep
`expire_claims` uses: a `cursor: u32` argument plus a **tamper-resistant,
on-chain-persisted cursor per keeper** (`ScanKey::KeeperCursor(keeper)`),
so a scan interrupted after any call resumes exactly where it left off, and
a keeper can't skip positions by forging a cursor value (`advance_keeper_cursor`
only accepts `0` or a value strictly greater than the current cursor, capped
at the registry length). See the module's own design doc at the top of the
file (`liquidation_scanner.rs:1-36`) for the intended keeper workflow.

Practical implication: **there is currently no production keeper-liquidation
sweep entrypoint at all.** The only liquidation path that ships is
`liquidate()` (`contracts/credence_bond/src/lib.rs:2168`), which acts on one
bond per call and is not a scan/sweep. Wiring `liquidation_scanner` up (or
deciding it should be deleted) is a separate follow-up, not something this
doc's issue covers — but any future work in this area should reuse its
cursor pattern rather than re-inventing a front-window sweep, since a
front-window sweep can permanently starve items behind an ever-growing front
if new candidates keep landing faster than sweeps drain them.

### `process_claims` / `get_pending_claims_paginated` — compiled, but unreachable

`contracts/credence_bond/src/claims.rs:345` and `:739`. Unlike
`liquidation_scanner`, the `claims` module itself is unconditionally
compiled (`mod claims;`, `lib.rs:5`), so these functions do exist in the
compiled artifact — but neither has a `#[contractimpl]` wrapper exposing it
as a contract entrypoint, so no client call can reach them. `expire_claims`
(Pattern B above) is the only claims-sweep entrypoint that's actually
callable.

## Decision guide for new entrypoints

| Question | All-or-nothing (Pattern A) | Bounded partial progress (Pattern B) |
|---|---|---|
| Who submits the batch? | The caller, as one authorized action | Anyone, permissionlessly, repeatedly |
| Is the item count bounded by the caller's input? | Yes, and cheaply validated up front | No — backed by an accumulating on-chain collection |
| What does "half done" mean here? | Meaningless / a bug (see weight-cap example above) | Normal, expected, and resumable |
| Cursor needed? | No | Prefer a persisted, tamper-resistant cursor (`liquidation_scanner` design) over a front-window rescan (`expire_claims` design) if the collection can grow faster than sweeps drain it |
| Reference implementation | `add_attestation_batch` (`lib.rs:998`) | `expire_claims` (`lib.rs:2341`) for small/slow collections; `liquidation_scanner`'s cursor design (currently unwired, see above) as the template for large/fast ones |

## Read-only pagination is a separate concern

Read-only paginated getters — `get_pending_claims_page`
(`claims.rs:275`), `get_subject_attestations_page`, `get_slash_history_page`
(both in `lib.rs`), and the equivalents in other contracts
(`get_arbitrators_page` in `arbitration`, `get_identities_page` in
`credence_registry`) — don't mutate state, so none of the atomicity
questions above apply to them. They're plain `(page, next_cursor)` or
`(offset, limit)` reads, listed here only to avoid confusing them with the
mutating sweeps above.

## Related docs

- `docs/attestation-batching.md` — full constraint list for `add_attestation_batch`.
- `docs/batch-operations.md` — design of `create_batch_bonds` (test/testutils-only, see above) and the accurate, shippable `expire_claims` write-up.
- `docs/known-simplifications.md` — other intentional limitations and production-path notes.
- `contracts/credence_errors/src/lib.rs` — typed error catalogue, including the batch-specific variants (`BatchTooLarge`, `EmptyBatch`, `DuplicateAttestation`, `CursorOutOfRange`).
