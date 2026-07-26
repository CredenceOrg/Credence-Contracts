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
- `contracts/credence_bond/docs/iter-chunks.md` — `vec_chunks` utility for iterating any `Vec`
  in fixed-size chunks for gas budgeting (uses `DEFAULT_CHUNK_SIZE = 50`)
