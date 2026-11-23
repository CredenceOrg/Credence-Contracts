# Admin & Arbitration Pagination — Design Document

**Issue:** #1298 — [Quality][Medium] admin and arbitration controls: pagination and cursor semantics

## Overview

This change adds deterministic, bounded, cursor-paginated read methods to the
Admin and Arbitration contracts. Without pagination, `get_all_admins` and
`get_admins_by_role` return unbounded `Vec<Address>` that will eventually exceed
Soroban's per-transaction read budget as the admin set grows. The Arbitration
contract already had `get_arbitrators_page` but lacked a proper error for invalid
cursors.

## Changes

### Admin Contract

#### New Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_all_admins_page` | `(cursor: u32, limit: u32) → (Vec<Address>, Option<u32>)` | Cursor-paginated read of all admin addresses |
| `get_admins_by_role_page` | `(role: AdminRole, cursor: u32, limit: u32) → (Vec<Address>, Option<u32>)` | Cursor-paginated read of admin addresses for a specific role |

#### Deprecated Methods

| Method | Replacement |
|--------|-------------|
| `get_all_admins` | `get_all_admins_page` |
| `get_admins_by_role` | `get_admins_by_role_page` |

The deprecated methods remain functional for backward compatibility. They are
annotated with `#[deprecated]` to guide callers toward the bounded variants.

#### Constants

```rust
/// Hard cap on the page size for paginated admin reads.
const MAX_PAGE_LIMIT: u32 = 200;
```

### Arbitration Contract

#### Modified Methods

| Method | Change |
|--------|--------|
| `get_arbitrators_page` | Return type changed from `(Vec<Address>, Option<u32>)` to `Result<(Vec<Address>, Option<u32>), ArbitrationError>` |

#### New Error Variant

```rust
/// Pagination cursor is out of range (cursor >= registry_len).
CursorOutOfRange = 16,
```

Previously, passing `cursor >= registry_len` silently returned an empty page
with `None` cursor, allowing callers to synthesize a completed-scan response
without scanning any entries. Now it returns `Err(CursorOutOfRange)`.

## Cursor Semantics

### Index-Based Cursor

Both contracts use **index-based cursors** (`u32`):

1. `cursor = 0` — start from the first element
2. `cursor = N` — start from the Nth element (0-based index)
3. `cursor >= total_count` — **rejected** with `CursorOutOfRange` (arbitration) or
   returns empty page with `None` cursor (admin, for backward compatibility)

### Page Limit Clamping

| Contract | Default (limit=0) | Hard Cap |
|----------|-------------------|----------|
| Admin | `MAX_PAGE_LIMIT` (200) | `MAX_PAGE_LIMIT` (200) |
| Arbitration | `DEFAULT_MAX_ITER` (50) | `MAX_ITER_HARD_CAP` (200) |

A `limit` larger than the hard cap is silently clamped. A `limit` of 0 uses the
default value.

### Return Contract

```
(page, next_cursor)
```

- `next_cursor = Some(next_index)` — more results may remain; feed `next_index`
  back as `cursor` to continue.
- `next_cursor = None` — the page exhausted the remaining set; no further pages.

### Deterministic Ordering

Results are returned in **insertion order** (the order elements were added to
the underlying `Vec`). This ordering is stable and deterministic: two calls
with the same `cursor` and `limit` always return identical results (absent
concurrent mutations).

## Invariants

1. **No skipped or duplicated items:** Concatenating all pages (following
   `next_cursor` until `None`) reproduces the full set in insertion order.
2. **Bounded reads:** Every read is bounded by `min(limit, MAX_PAGE_LIMIT)`,
   ensuring the contract never exceeds Soroban's per-transaction resource
   budget.
3. **Stale cursor rejection (Arbitration):** Passing `cursor >= len` returns
   `Err(CursorOutOfRange)` rather than a silent empty page. This prevents
   callers from fabricating a "scan complete" response without work.
4. **No state mutation:** All pagination methods are pure reads — they do not
   modify storage, emit events, or require authorization.

## Failure Behavior

| Condition | Admin | Arbitration |
|-----------|-------|-------------|
| `cursor >= total` | Returns `(empty, None)` | Returns `Err(CursorOutOfRange)` |
| `limit > hard_cap` | Clamped to `MAX_PAGE_LIMIT` | Clamped to `MAX_ITER_HARD_CAP` |
| `limit == 0` | Uses `MAX_PAGE_LIMIT` (200) | Uses `DEFAULT_MAX_ITER` (50) |
| Empty set, `cursor = 0` | Returns `(empty, None)` | Returns `Err(CursorOutOfRange)` |

## Security Assumptions

1. **Read-only boundary:** Pagination methods require no authorization and
   perform no state mutations. An unprivileged caller can enumerate admins and
   arbitrators.
2. **Admin list is append-only in practice:** Admins are added and removed via
   privileged entrypoints. Between paginated reads, the set may change (admins
   added/removed), but this is safe — pagination walks the current snapshot.
3. **Cursor is not on-chain state:** Cursors are purely client-maintained
   (stateless reads). No on-chain cursor state is stored or validated beyond
   the range check.
4. **Deterministic ordering:** The underlying `Vec<Address>` is maintained in
   insertion order with compaction on removal, ensuring stable pagination.

## Migration / Rollback

- **Backward compatible:** `get_all_admins` and `get_admins_by_role` remain
  functional (deprecated, not removed). Existing callers continue to work.
- **Arbitration breaking change:** `get_arbitrators_page` now returns
  `Result<(Vec<Address>, Option<u32>), ArbitrationError>` instead of
  `(Vec<Address>, Option<u32>)`. Callers must update to handle the `Result`
  type. The `try_` prefixed client method wraps the error in an additional
  `Result` layer.

## Test Coverage

### Admin Pagination Tests (16 tests)

- Empty set (cursor past end)
- Single admin
- Boundary exact fit (limit == count)
- Multi-page walk (page 1 → page 2)
- Cursor past end
- Limit clamped to cap
- Zero limit uses default
- Deterministic ordering
- Empty role (get_admins_by_role_page)
- Single role admin
- Multi-page role pagination
- Role cursor past end
- Role limit clamped
- Full walk reassembles deprecated list
- Role pagination matches deprecated list
- Admin count matches pagination total

### Arbitration Pagination Tests (12 tests)

- Empty registry cursor zero → CursorOutOfRange
- Cursor past end → CursorOutOfRange
- Cursor at boundary (cursor == len) → CursorOutOfRange
- Single item page
- Limit zero uses default
- Full walk reassembles
- Deterministic ordering
- Exact boundary split (3 pages of 2)
- After unregister compacts
- Registry and pagination integration (existing, updated)
- Limit clamped to cap (200)
- Unregister removal correctness
