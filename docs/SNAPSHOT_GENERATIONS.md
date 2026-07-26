# Snapshot Generations — Semantics & Bump Triggers

## Overview

This workspace uses several distinct snapshot strategies to guard against
different kinds of regressions. Each "generation" pins a different artifact,
fails CI on a different class of change, and has its own refresh protocol.

| Gen | What it pins | Artifact | Refresh command |
|-----|-------------|----------|-----------------|
| 1 | Contract storage layout | `insta` JSON snapshots (`.snap` files) | `INSTA_UPDATE=always cargo test -p credence_delegation test_pause_proposal_lifecycle_snapshots` |
| 2 | DataKey / UpgradeKey XDR encoding | `EXPECTED` hex constant in `tests/datakey_fingerprint.rs` | `cargo test -p <contract> --test datakey_fingerprint -- --nocapture`, then copy output into `EXPECTED` |
| 3 | WASM `contractspecv0` section (ABI) | `tests/spec_xdr/<contract>.v<N>.hex` + `CONTRACT_SPEC_VERSION` | Bump `CONTRACT_SPEC_VERSION` in `lib.rs`, rebuild with `cargo build --target wasm32-unknown-unknown --release`, then copy the new hex |
| 4 | On-chain `BondStatusSnapshot` return type | Struct definition in `contracts/credence_bond/src/status_snapshot.rs` | No pinned file; downstream backends must update their deserialization code |

---

## Generation 1 — `insta` Storage Snapshots

### Purpose

Serialise the full pause-related ledger state after each major lifecycle step
so that any unintended shift in the storage layout is caught by CI.

### Where

- Generator: `contracts/credence_delegation/src/test_pause_snapshots.rs` (`test_pause_proposal_lifecycle_snapshots`)
- Snapshots: `contracts/credence_delegation/test_snapshots/test_pausable_state/*.snap`

### When the generation bumps

- Renaming a pause-related `DataKey` variant
- Adding or removing fields in a pause-related struct
- Altering the cardinality or order of lifecycle steps in the test

### Refresh

```sh
INSTA_UPDATE=always cargo test -p credence_delegation test_pause_proposal_lifecycle_snapshots
```

Review every changed `.snap` file in the diff — each modified line represents
a storage key or value that moved.

---

## Generation 2 — DataKey Fingerprint Tests

### Purpose

Pin the XDR encoding of *every* `DataKey` (and `UpgradeKey`) variant per
contract. A Soroban `#[contracttype]` enum is keyed by variant **name**
(not declaration order), so renaming or retyping a variant silently orphans
all live ledger entries stored under that key. The fingerprint test catches
that before a release ships.

### Where

| Contract | Test file |
|----------|-----------|
| `credence_delegation` | `tests/datakey_fingerprint.rs` |
| `credence_bond` | `tests/datakey_fingerprint.rs` (DataKey + UpgradeKey) |
| `arbitration` | `tests/datakey_fingerprint.rs` |
| `admin` | `tests/datakey_fingerprint.rs` |

### When the generation bumps

- Renaming a `DataKey` or `UpgradeKey` variant
- Changing a variant's field count or field types
- Adding a new variant (new key is fine, but must be added to the test)

### Refresh

```sh
cargo test -p credence_delegation --test datakey_fingerprint -- --nocapture
```

Copy the printed `---- DataKey fingerprints ----` block into the `EXPECTED`
constant. **Review every changed line** — each one is a storage key that
moved. See [docs/datakey-fingerprint.md](datakey-fingerprint.md) for the
underlying Soroban encoding rules.

---

## Generation 3 — Spec XDR Regression (ABI Snapshot)

### Purpose

The Soroban compiler embeds every public type, function, and event schema as
XDR in the WASM `contractspecv0` custom section. The spec XDR test pins that
byte stream and requires an explicit `CONTRACT_SPEC_VERSION` bump whenever
the ABI changes.

This is the only generation that carries a **semantic version label**: the
pinned manifest is `{CONTRACT_SPEC_VERSION}:{hex}`, and CI checks both that
the hex matches the current binary and that the version prefix was
incremented.

### Where

- Test: `contracts/credence_delegation/tests/spec_xdr_regression.rs`
- Snapshot: `contracts/credence_delegation/tests/spec_xdr/credence_delegation.v<N>.hex`
- Version constant: `credence_delegation::CONTRACT_SPEC_VERSION` (currently `2`)

### When the generation bumps

- Adding, removing, or renaming a public contract function
- Changing a function's parameter types or return type
- Adding, removing, or renaming an event
- Changing a `#[contracttype]` struct or enum used in the public interface

### Refresh

1. **Increment** `CONTRACT_SPEC_VERSION` in `contracts/credence_delegation/src/lib.rs`.
2. **Rebuild** the release WASM:
   ```sh
   cargo build -p credence_delegation --target wasm32-unknown-unknown --release
   ```
3. **Copy the new hex** from the test failure output into `tests/spec_xdr/credence_delegation.v<N>.hex` (create a new file with the bumped version number; the `include_str!` path must match).
4. **Update** `EXPECTED_VERSIONED_MANIFEST` if the include path changed.

---

## Generation 4 — On-chain `BondStatusSnapshot`

### Purpose

A read-only contract entrypoint that returns a stable, flat struct for
backend ingestion. Unlike the other generations this is a **production API**
surface — there is no pinned file to refresh, but changing the struct layout
breaks every downstream consumer that deserialises the response.

### Where

- Definition: `contracts/credence_bond/src/status_snapshot.rs`
- Tests: `contracts/credence_bond/src/test_status_snapshot.rs`
- Doc: [docs/status-snapshot.md](status-snapshot.md)

### When the generation bumps

- Adding, removing, or renaming a field in `BondStatusSnapshot`
- Changing a field's type
- Changing the semantics of an existing field (e.g. `available_balance`
  switches from gross to net)

### Protocol

No automated guard exists today. The contributor must:

1. Notify downstream teams of the schema change.
2. Update [docs/status-snapshot.md](status-snapshot.md) with the new struct
   and example output.
3. Bump the last-updated date or an ad-hoc version comment in the struct
   definition.

---

## Cost-Estimate Baselines (not a generation)

`test_budget_helper.rs` and `test_budget_ceilings.rs` capture
`env.cost_estimate().budget()` after key operations. These are **performance
regression guards**, not storage or ABI stability checks. They are listed
here only to distinguish them from the four generations above.

- `contracts/credence_delegation/src/test_budget_helper.rs`
- `contracts/credence_delegation/src/test_budget_ceilings.rs`

---

## Related

- [docs/datakey-fingerprint.md](datakey-fingerprint.md) — Soroban encoding rules and upgrade hazards
- [docs/pause-state-snapshots.md](pause-state-snapshots.md) — `insta` snapshot workflow for the pausable module
- [docs/status-snapshot.md](status-snapshot.md) — On-chain `BondStatusSnapshot` reference
- [docs/testing.md](testing.md) — General test organisation and conventions
- Source: `contracts/credence_delegation/tests/spec_xdr_regression.rs`
- Source: `contracts/credence_bond/src/status_snapshot.rs`
