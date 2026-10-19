# Gas Benchmark Notes — Hot-Path Contract Calls

> Summarizes the most expensive hot paths and the benchmark methodology used to
> compare them. Date: 2026-07-27.

## Overview

This document catalogues the contract entry points that dominate Soroban
resource consumption and describes how we measure, compare, and optimize them.
The methodology is source-level storage operation counting (the primary
cost driver in Soroban) supplemented by on-chain budget metering where available.

## Methodology

### Storage Operation Counting

Soroban resource fees are dominated by **storage host operations** (reads,
writes, TTL extensions). Each storage interaction is counted by key:

- **Read**: `e.storage().persistent().get()`, `e.storage().instance().get()`
- **Write**: `e.storage().persistent().set()`, `e.storage().instance().set()`
- **TTL bump**: `extend_ttl()` calls

Counting is **success-path only** (panic/revert paths are excluded — they
don't represent sustainable contract throughput).

### Budget Metering (when available)

For contracts compiled with the active workspace, `env.budget()` can be
queried before and after a call to obtain CPU instructions, memory bytes,
and other host-charged resources. See [`bond_gas_benchmarks.md`](bond_gas_benchmarks.md)
for the bond crate's budget profile.

### Reproducibility

All benchmarks are reproducible from the workspace root:

```bash
cargo test --workspace
```

Individual gas-profile assertion tests live alongside the hot-path code and
serve as **static guards** — they fail CI if a refactor silently increases the
declared storage budget.

---

## Hot-Path Catalogue

> **"Hot path" definition**: Any public entry point that is called at scale
> during normal protocol operation (not admin-only or infrequent transitions).

### 1. `credence_bond` — Core Bond Operations

| Entry point | Frequency | Storage ops (success) | Notes |
|---|---|---|---|
| `create_bond` | High | 3 reads / 2 writes | One-time per bond |
| `top_up` | Medium | 2 reads / 1 write | Collateral increase |
| `withdraw_bond` | Medium | 3 reads / 2 writes | Post-lockup withdrawal |
| `withdraw_early` | Low | 3 reads / 2 writes | With penalty calculation |
| `slash_bond` | Low | 3 reads / 2 writes | Admin-only |
| `is_bond_active` | Very High | 1 read | Read-only, zero writes |

**Optimization rules (applied in #369):**
- Read each storage key at most once per call.
- Mutate loaded `IdentityBond` in place instead of constructing a new one.
- Write the bond key once.
- Preserve checks-effects-interactions ordering before optional callback calls.

See [`bond_gas_benchmarks.md`](bond_gas_benchmarks.md) for the full
storage operation budget before/after optimization.

### 2. `credence_delegation` — Delegation Lifecycle

| Entry point | Frequency | Storage ops (success) | Notes |
|---|---|---|---|
| `delegate` | High | 2 reads / 2 writes | Plus nonce bump |
| `execute_delegated_delegate` | High | 3 reads / 2 writes | Relayer path, includes payload verification |
| `revoke_delegation` | Medium | 2 reads / 1 write | Plus nonce bump |
| `is_valid_delegate` | Very High | 1 read | Zero-write check |
| `get_delegation_summary` | High | 1 read | Indexer query |

**Nonce costs:**
- `consume_nonce`: 1 persistent read + 1 persistent write per call.
- `get_nonce`: 1 persistent read (only writes TTL if key exists).
- `invalidate_nonce_range`: 1 persistent read + 1 persistent write.

**TTL cost note:** Delegation entries have a TTL bound to `expires_at`.
Nonce entries have a minimum TTL of `MIN_NONCE_TTL` (518,400 ledgers ≈ 30 days).
Every read/write to these keys calls `extend_ttl`, adding one storage host
operation per access.

### 3. `credence_registry` — Identity Registration

| Entry point | Frequency | Storage ops (success) | Notes |
|---|---|---|---|
| `register` | Medium | 3 reads / 3 writes | Identity + reverse + list update |
| `register_trustless` | Medium | 4 reads / 3 writes | Plus code-hash verification |
| `get_bond_contract` | High | 1 read | Zero-write lookup |
| `get_identity` | High | 1 read | Reverse lookup |
| `is_registered` | Very High | 1 read | Boolean check |
| `get_identities_page` | Medium | 1 read | Bounded pagination (≤ 200) |

### 4. `timelock` — Time-locked Operations

| Entry point | Frequency | Storage ops (success) | Notes |
|---|---|---|---|
| `queue` | Low | 2 reads / 1 write | Proposal creation |
| `execute` | Low | 3 reads / 2 writes | After delay passes |
| `cancel` | Low | 2 reads / 1 write | Before execution |

### 5. `credence_treasury` — Asset Management

| Entry point | Frequency | Storage ops (success) | Notes |
|---|---|---|---|
| `deposit` | Medium | 2 reads / 1 write | Asset custody |
| `withdraw` | Low | 3 reads / 2 writes | Slippage + liquidity checks |
| `get_balance` | High | 1 read | Read-only |

---

## Benchmark Framework

### Gas Profile Tests

Each hot-path crate includes a `gas_profile_tests` module (or equivalent)
that asserts the declared storage budget. These tests are intentionally
simple — they count storage keys accessed and fail CI if a refactor
silently increases the budget.

Example (conceptual):

```rust
#[test]
fn withdraw_bond_storage_budget() {
    // Assert: bond key read once + write once + 2 TTL bumps.
    // If this fails, a refactor added an unexpected storage operation.
    assert_eq!(WITHDRAW_BOND_STORAGE_BUDGET, expected_ops_count);
}
```

### Running Benchmarks

```bash
# Debug build (fast compile, less representative)
cargo test --workspace gas_profile

# Release build (representative optimization)
cargo test --release --workspace gas_profile
```

### CI Integration

The gas-profile assertion tests run as part of `cargo test --workspace` in
`contracts-tests.yml`. A failure indicates an unintentional storage budget
regression.

---

## Optimization Patterns

### 1. Read-once, mutate-in-place, write-once
Load the bond/delegation record once, mutate its fields, write once.
Avoid constructing a new struct and writing every field.

### 2. Check before write
Use `.has()` to verify a key exists before writing to avoid unnecessary
write operations (especially for TTL bumps).

### 3. Bounded pagination
Return at most `MAX_PAGE_SIZE` elements per call to keep reads bounded
regardless of total storage size. See `get_identities_page` in
`credence_registry`.

### 4. Avoid redundant TTL bumps
If the same key is read and then written in the same call frame, bump TTL
once after the write rather than after both read and write. The net TTL
extension is the same, but one fewer host operation is charged.

---

## Resources

- [`bond_gas_benchmarks.md`](bond_gas_benchmarks.md) — detailed bond storage budget
- [`gas-report.txt`](../gas-report.txt) — WASM size comparisons
- [`docs/wasm-size-budget.md`](wasm-size-budget.md) — WASM binary size constraints
- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — development workflow and CI gates
