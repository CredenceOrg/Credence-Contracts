# Paginated Reads — `MAX_QUERY_LIMIT`

## Overview

Several on-chain collections can grow without a strict upper bound at the
application level (attestation lists, slash history, pending claims). Reading
an entire collection in a single Soroban invocation risks hitting the
instruction-budget limit when the collection is large, and allows a
sufficiently active user to cause out-of-budget reverts for any downstream
contract that calls the original unbounded getters.

To prevent this, every collection-read entry-point now accepts an
`(offset: u32, limit: u32)` pair, and `limit` is **silently clamped** to the
constant `MAX_QUERY_LIMIT = 200` defined in `parameters.rs`.

---

## The `MAX_QUERY_LIMIT` Constant

```rust
// contracts/credence_bond/src/parameters.rs
pub const MAX_QUERY_LIMIT: u32 = 200;
```

This is the single source of truth for all paginated reads in `credence_bond`.
The value `200` aligns with `liquidation_scanner::MAX_ITER_HARD_CAP` so all
collection-read caps stay consistent across the codebase.

**Do not duplicate this constant.** Import it as `crate::parameters::MAX_QUERY_LIMIT`
wherever you need it.

---

## Paginated Entry-Points

### 1. `get_subject_attestations_page`

```
get_subject_attestations_page(
    subject: Address,
    offset:  u32,
    limit:   u32,   // clamped to MAX_QUERY_LIMIT
) -> Vec<u64>       // attestation IDs
```

Returns up to `min(limit, MAX_QUERY_LIMIT)` attestation IDs for `subject`
starting at `offset`. Returns an empty vec when `offset >= total`.

The **original** `get_subject_attestations(subject) -> Vec<u64>` is preserved
for backwards compatibility; it still returns the full list.

### 2. `get_slash_history_page`

```
get_slash_history_page(
    identity: Address,
    offset:   u32,
    limit:    u32,   // clamped to MAX_QUERY_LIMIT
) -> Vec<SlashRecord>
```

Returns up to `min(limit, MAX_QUERY_LIMIT)` `SlashRecord` entries for
`identity` starting at `offset`. Returns an empty vec when `offset >= total`.

### 3. `get_pending_claims_paginated` / `get_pending_claims_count`

Internal module helpers in `claims.rs` (not a contract entry-point):

```rust
// O(1) total count
pub fn get_pending_claims_count(e: &Env, user: &Address) -> u32;

// Bounded read — pure, does NOT process or remove claims
pub fn get_pending_claims_paginated(
    e: &Env,
    user: &Address,
    offset: u32,
    limit:  u32,   // clamped to MAX_QUERY_LIMIT
) -> Vec<PendingClaim>;
```

---

## Pagination Pattern

All three helpers use the same stateless `(offset, limit)` cursor that callers
maintain off-chain (or across transactions). There is no on-chain cursor state
for reads — only the writer-side liquidation scanner keeps on-chain cursors.

```text
// Generic off-chain / keeper loop
offset = 0
loop:
    page = contract.get_XXX_page(subject_or_identity, offset, 50)
    if page is empty: break
    process(page)
    offset += page.len()
```

Key properties:

| Property | Value |
|---|---|
| Hard cap per call | `MAX_QUERY_LIMIT = 200` |
| Caller-visible when limit is clamped | No — clamping is silent |
| `limit = 0` behaviour | Treated as `MAX_QUERY_LIMIT` |
| `offset >= total` behaviour | Returns empty vec, no panic |
| Backwards-compatible | Yes — original getters untouched |
| Mutates state | No — read-only |

---

## Deterministic ordering

Every list-returning read guarantees a **stable total order**: for a fixed
collection, walking it page by page returns each entry exactly once (no
duplicates, no omissions) and the concatenated result is the ascending order of
that entry's natural key. The order never depends on map-iteration order or on
any transient storage layout.

The guarantee holds because each collection is backed by an append-only
structure whose insertion order already coincides with a monotonically
increasing key, so no explicit re-sort is required on the read path.

| List-returning API | Backing store | Ordering key | Order |
|---|---|---|---|
| `claims::get_pending_claims_paginated` (offset/limit) | `Vec<PendingClaim>` at `DataKey::PendingClaims(user)` | `claim_id` | ascending |
| `claims::get_pending_claims_page` (cursor) | same `Vec<PendingClaim>` | `claim_id` | ascending |
| `get_subject_attestations` | `Vec<u64>` of attestation ids | attestation id | ascending |
| `slash_history` index reads (`get_slash_history_page`, `testutils::get_slash_history`) | `SlashRecord(identity, index)` keyed records, `index` in `0..count` | record index | ascending (== insertion / timestamp) |
| `liquidation_scanner::scan_liquidation_candidates` | `Vec<Address>` bond-holder registry | registry index | ascending (append-only, never reordered) |
| `iter_chunks::vec_chunks` | any caller-supplied `Vec<T>` | source position | source order (order-preserving) |

Why the key order is stable:

- `claim_id` is drawn from `DataKey::ClaimCounter`, a monotonically increasing
  counter, and new claims are only ever `push_back`ed. The removal paths
  (`process_claims`, `cleanup_expired_claims`, `expire_claims_bounded`) rebuild
  the vector while preserving relative order, so the surviving claims stay sorted
  ascending by `claim_id`. The cursor read (`get_pending_claims_page`) relies on
  exactly this invariant when it skips claims with `claim_id <= start_after`.
- Slash records are stored under `SlashRecord(identity, index)` where `index`
  increments per append; reads iterate `0..count`, i.e. strictly ascending index,
  which is also non-decreasing by `timestamp`.
- The scanner registry is a `Vec<Address>` that only appends and marks entries
  inactive; entries are never removed or reordered, so a registry index is a
  stable cursor across scans.
- `vec_chunks` copies a contiguous window of the source and never sorts, so it
  reproduces the source order exactly.

Offset/limit and index reads are complete regardless of the stored order (they
address entries by position), whereas the cursor read additionally requires the
ascending-`claim_id` invariant above to avoid skipping entries. Regression tests
in `test_ordering_guarantees.rs` lock in all three properties (no duplicates, no
omissions, ascending-key order), including an offset/limit walk over a
deliberately scrambled storage vector and an out-of-order `vec_chunks` source.

---

## Migration Guide

If you previously called the unbounded `get_subject_attestations` and relied
on receiving all IDs in one call, switch to the paged variant:

```rust
// Before (may time out for large subjects)
let all_ids = client.get_subject_attestations(&subject);

// After (safe for any collection size)
let mut offset = 0u32;
let mut all_ids: Vec<u64> = Vec::new();
loop {
    let page = client.get_subject_attestations_page(&subject, &offset, &200_u32);
    if page.is_empty() { break; }
    all_ids.extend(page);
    offset += page.len() as u32;
}
```

The same pattern applies to `get_slash_history_page`.

---

## See Also

- `contracts/credence_bond/src/parameters.rs` — `MAX_QUERY_LIMIT` definition
- `contracts/credence_bond/src/liquidation_scanner.rs` — `MAX_ITER_HARD_CAP` (matching cap for scanner)
- `contracts/credence_bond/src/test_pagination.rs` — test suite for all paginated reads
- `contracts/credence_bond/src/test_ordering_guarantees.rs` — regression tests for the deterministic-ordering guarantee (no duplicates / no omissions / stable key order)
- `contracts/credence_bond/docs/iter-chunks.md` — `vec_chunks` utility for iterating any `Vec`
  in fixed-size chunks for gas budgeting (uses `DEFAULT_CHUNK_SIZE = 50`)
