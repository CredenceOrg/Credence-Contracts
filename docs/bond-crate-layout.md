# `credence_bond` Crate Layout

## Canonical source of truth

`contracts/credence_bond/src/lib.rs` is the single authoritative implementation.
It defines the `CredenceBond` Soroban contract compiled to WASM and deployed
on-chain.  Every other file in `src/` is either a helper module pulled in by
`lib.rs` or a `#[cfg(test)]`-only module.

## Module map (production)

| File | Responsibility |
|------|---------------|
| `src/lib.rs` | **Canonical contract** — public entrypoints, storage keys, data model, reentrancy guard, pure-Rust bond helpers |
| `src/types/mod.rs` | Shared contract types (`Attestation`, dedup key) |
| `src/batch.rs` | Batch-operation helpers |
| `src/claims.rs` | Pull-payment claim queue |
| `src/early_exit_penalty.rs` | Time-decayed early-exit penalty calculation |
| `src/events.rs` | Typed event helpers |
| `src/invariants.rs` | Runtime invariant assertions |
| `src/math.rs` | Checked arithmetic primitives |
| `src/migration.rs` | Lazy storage migration (v1 → v2) |
| `src/nonce.rs` | Per-identity replay-prevention nonce |
| `src/rolling_bond.rs` | Rolling-period renewal logic |
| `src/same_ledger_liquidation_guard.rs` | Same-ledger slash guard |
| `src/slash_history.rs` | Persistent slash record log |
| `src/slashing.rs` | `slash_bond` core logic |
| `src/tiered_bond.rs` | Bronze/Silver/Gold/Platinum tier mapping |
| `src/upgrade_auth.rs` | Two-step upgrade authorization |
| `src/weighted_attestation.rs` | Stake-weighted attestation scoring |

## Test modules (`#[cfg(test)]` only — never shipped to WASM)

| File | Purpose |
|------|---------|
| `src/chaos_token.rs` | Mock token that simulates host failures |
| `src/test_bond_drift.rs` | Time-drift invariant tests |
| `src/test_chaos.rs` | Chaos / fault-injection scenarios |
| `src/test_describe.rs` | `describe_config` / `describe_bond` introspection tests |
| `src/test_differential.rs` | **Regression guard** (see below) |
| `src/fork_divergent.rs` | Deliberately-broken probe contract for divergence detection |

## Regression guard: `test_differential`

### Old design (removed in `refactor/consolidate-fork-artifacts`)

The old `tests/differential.rs` registered four live Soroban contracts
(`canonical`, `fork_ours`, `fork_base`, `fork_theirs`) in the same `Env` and
drove them through identical steps, asserting identical output after every step.

**Why it was removed:** `fork_ours`, `fork_base`, and `fork_theirs` were
parallel copies of the canonical implementation.  A fix applied to `lib.rs` but
missed in a fork produced silent divergence that the harness would fail to catch
(since all forks drifted together).  The file was also never compiled
(`autotests = false`) so the risk was real but invisible.

### Current design

`src/test_differential.rs` drives the **single canonical** `CredenceBond`
through scripted lifecycle scenarios.  After each mutating step the live bond
state is compared against a **pinned** `Pinned` struct with hardcoded expected
field values.  There is one authoritative path; drift is structurally impossible.

`fork_divergent` (a minimal contract that returns `Gold` for every bonded amount
≥ 1, rather than the correct tier) is retained solely to prove the comparison
logic can still catch divergence: `deliberate_divergence_is_caught` asserts that
canonical and divergent produce different tiers for amount = 1_000.

## Removed files

The following files were deleted during consolidation:

| File | Reason |
|------|--------|
| `src/fork_base.rs` | Redundant copy of canonical — drift risk |
| `src/fork_ours.rs` | Same |
| `src/fork_theirs.rs` | Same |
| `src/lib_main.rs` | Stale alternate `lib.rs` from a merge artifact |
| `src/nonce.rs_main.rs` | Malformed duplicate of `nonce.rs` |
| `Cargo.toml_main.toml` | Stale Cargo manifest from a merge artifact |
| `tests/differential.rs` | Replaced by `src/test_differential.rs` |
| Root `base.rs`, `ours.rs`, `theirs.rs`, `lib_main.rs`, `lib_base.rs` | Repository-root merge artifacts |
| Root `test_*.rs`, `types.rs`, `weighted_attestation.rs`, `early_exit_penalty.rs` | Orphan root-level files never wired into any crate |

## Wire-stability invariant

Storage keys (`DataKey` variants) are encoded by **variant name + field shape**,
not declaration order.  Renaming or changing field count moves the key and
orphans existing ledger entries.  Appending new variants and reordering are
safe.  See the `DataKey` doc comment in `lib.rs`.
