# Snapshot Generations — Semantics and When They Bump

## Audience: Operator (liquidation keepers)

This document explains what a **snapshot generation** is in the bond
liquidation scanner, what it protects against, and exactly when the generation
changes. It is written for operators running keeper software that paginates
through `scan_liquidation_candidates` on the `credence_bond` contract.

> **Not to be confused with** [`status-snapshot.md`](status-snapshot.md), which
> documents the read-only `get_bond_status_snapshot()` view. That "snapshot" is a
> point-in-time struct for one bond. A "snapshot generation" here is the
> registry-size token that keeps a *multi-page* liquidation scan consistent.

---

## What a snapshot generation is

The liquidation scanner walks a keeper-maintained registry of bond holders one
bounded page at a time (see [`liquidation.md`](liquidation.md) for the
mechanics). Because a full scan spans several transactions, the registry could
change *between* pages — a bond created or fully withdrawn mid-scan.

The **snapshot generation is the active registry size** recorded at the moment a
scan begins. The active registry size is the count of currently-active bond
holders, returned by `get_registry_size()` and echoed back in every
`ScanResult.registry_size`:

```text
snapshot_generation := active_registry_size at the start of the scan
                     == ScanResult.registry_size of the first page
```

The keeper captures this value from the first page and passes it back, unchanged,
on every subsequent page. If the registry size the contract sees no longer
matches the value the keeper is carrying, the scan is stale and the call is
rejected with `SnapshotGenerationMismatch` (contract error `#235`).

The guard only fires for continuation pages. The first page of a pass
(`cursor == 0`) always establishes a fresh generation, so any value may be
passed there — keepers conventionally pass `0`.

---

## When the generation bumps

The generation is *not* a counter that increments by one. It **is** the active
registry size, so it changes whenever the active registry size changes. Two
contract entrypoints move it:

| Event | Entrypoint | Effect on active registry size | Effect on generation |
|---|---|---|---|
| A new bond holder is registered | `register_bond_holder` | `+1` | Bumps (any in-flight scan is now stale) |
| A previously-inactive holder is reactivated | `register_bond_holder` (re-add) | `+1` | Bumps |
| A holder is deregistered (bond withdrawn/liquidated) | `deregister_bond_holder` | `−1` (floored at 0) | Bumps |

What does **not** bump the generation:

- Re-registering an address that is already active — `register_bond_holder` is
  idempotent, so the size is unchanged.
- Deregistering an address that is already inactive or absent — no-op.
- Running a scan page. Reads never mutate the registry size.
- The passage of ledgers or wall-clock time. Unlike the ledger-bucket
  [operator](OPERATOR_EPOCHS.md), [admin](ADMIN_EPOCHS.md), and
  [signer](SIGNER_EPOCHS.md) epochs, the snapshot generation advances only on
  registry mutation, never on a schedule.

Because the token is the registry size itself, two different mutations that
happen to leave the size unchanged (one register plus one deregister within the
same page gap) will *not* be detected. The generation guard is a cheap
consistency check against size drift, not a cryptographic proof that the exact
same set of holders is present. This is intentional; see
[known-simplifications.md](known-simplifications.md).

---

## Keeper workflow

```text
# Page 0 establishes the generation.
page = scan_liquidation_candidates(keeper, cursor=0, max_iter=50, min_ratio, gen=0)
gen  = page.registry_size          # capture the snapshot generation

while not page.done:
    page = scan_liquidation_candidates(
        keeper,
        cursor   = page.next_cursor,
        max_iter = 50,
        min_ratio,
        gen,                        # carry the SAME generation every page
    )
    # ... act on page.candidates ...
```

If any continuation page returns `SnapshotGenerationMismatch`, the registry
mutated mid-scan. **Restart the pass from `cursor = 0`** to capture a fresh
generation; do not try to resume the old one.

---

## Concrete example

Assume five active bond holders and a page size of 2.

### Successful multi-page scan (generation stable)

1. Keeper calls `scan_liquidation_candidates(keeper, 0, 2, 0, 0)`.
   - Contract records `active_registry_size = 5`.
   - Returns `next_cursor = 2`, `registry_size = 5`, `done = false`.
   - Keeper captures `gen = 5`.
2. Keeper calls `scan_liquidation_candidates(keeper, 2, 2, 0, 5)`.
   - Requested generation `5` matches current size `5`. Page succeeds.
   - Returns `next_cursor = 4`, `done = false`.
3. Keeper calls `scan_liquidation_candidates(keeper, 4, 2, 0, 5)`.
   - Still `5`. Page succeeds, `done = true`, `next_cursor = 0`.

### Stale-generation rejection (registry shrank mid-scan)

1. Keeper calls `scan_liquidation_candidates(keeper, 0, 2, 0, 0)`.
   - Returns `registry_size = 5`. Keeper captures `gen = 5`.
2. A bond is fully withdrawn; the contract calls `deregister_bond_holder`.
   - Active registry size drops to `4`. **The generation has bumped.**
3. Keeper calls `scan_liquidation_candidates(keeper, 2, 2, 0, 5)`.
   - Requested generation `5` ≠ current size `4`.
   - The call **panics with `SnapshotGenerationMismatch` (`#235`)**.
4. Resolution: the keeper restarts from `cursor = 0`, receives the new
   `registry_size = 4`, and paginates with `gen = 4`.

---

## Error reference

| Error | Code | Meaning | Caller-fixable? |
|---|---|---|---|
| `SnapshotGenerationMismatch` | `235` | The requested snapshot generation does not match the current active registry size; the registry mutated mid-scan. | No — restart the pass from `cursor = 0` with a fresh generation. |
| `CursorOutOfRange` | `226` | `cursor >= registry_slots`; the page start is past the end of the registry. | Yes — supply a cursor within range. |

The generation guard is defined by `require_matching_snapshot_generation` in
`contracts/credence_bond/src/liquidation_scanner.rs`, which rejects any
continuation page whose requested generation differs from the current one.

---

## Cross-references

- [liquidation.md](liquidation.md) — liquidation mechanics and the keeper scan loop
- [status-snapshot.md](status-snapshot.md) — the unrelated per-bond `get_bond_status_snapshot()` view
- [OPERATOR_EPOCHS.md](OPERATOR_EPOCHS.md), [ADMIN_EPOCHS.md](ADMIN_EPOCHS.md), [SIGNER_EPOCHS.md](SIGNER_EPOCHS.md) — ledger-bucket epoch guards (time-based, in contrast to the mutation-based generation here)
- [errors.md](errors.md) — full error enum, including `SnapshotGenerationMismatch` and `CursorOutOfRange`
- [known-simplifications.md](known-simplifications.md) — why the generation tracks size rather than set membership
- Source: `contracts/credence_bond/src/liquidation_scanner.rs`
