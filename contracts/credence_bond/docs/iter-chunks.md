# Fixed-Size Chunk Iteration — `vec_chunks`

## Overview

Iterating an unbounded `Vec` inside a single Soroban invocation risks hitting
the CPU-instruction or memory budget as the collection grows. The chunk pattern
solves this by processing a fixed-size window per transaction and resuming from
a caller-maintained offset in the next call.

`vec_chunks` in `src/iter_chunks.rs` provides this primitive. It is a pure,
`#![no_std]`-compatible utility with no storage access — it only borrows the
`Vec` and the `Env`.

---

## The `DEFAULT_CHUNK_SIZE` Constant

```rust
// contracts/credence_bond/src/parameters.rs
pub const DEFAULT_CHUNK_SIZE: u32 = 50;
```

This is the single source of truth for the default chunk size across
`credence_bond`. The value 50 keeps each chunk comfortably inside the Soroban
instruction budget even for moderately expensive per-item work.

**Do not hard-code `50`.** Import it as `crate::parameters::DEFAULT_CHUNK_SIZE`
so the whole codebase stays in sync if the default ever changes.

---

## API

```rust
// contracts/credence_bond/src/iter_chunks.rs
pub fn vec_chunks<T>(
    e:          &Env,
    source:     &Vec<T>,
    offset:     u32,
    chunk_size: u32,
) -> (Vec<T>, Option<u32>)
```

### Arguments

| Parameter    | Type      | Description |
|---|---|---|
| `e`          | `&Env`    | Soroban environment (required by `soroban_sdk::Vec`). |
| `source`     | `&Vec<T>` | The vector to iterate over. **Not mutated.** |
| `offset`     | `u32`     | Zero-based start index of this chunk. |
| `chunk_size` | `u32`     | Max items per chunk. Pass `0` to use `DEFAULT_CHUNK_SIZE`. |

### Returns

`(chunk, next_offset)` where:

* `chunk` — `Vec<T>` of at most `chunk_size` elements starting at `offset`.
  Empty when `offset >= source.len()`.
* `next_offset` — `Some(offset + chunk.len())` when more elements remain;
  `None` when the chunk reached the end of `source`.

### Panics

Never panics. An out-of-range `offset` returns an empty chunk.

---

## Usage Pattern

### Within a contract entrypoint (single transaction)

```rust
use crate::iter_chunks::vec_chunks;
use crate::parameters::DEFAULT_CHUNK_SIZE;

// Process exactly one chunk per call; caller passes `offset` as an argument.
pub fn process_page(e: Env, items: Vec<u64>, offset: u32) -> Option<u32> {
    let (chunk, next) = vec_chunks(&e, &items, offset, DEFAULT_CHUNK_SIZE);
    for i in 0..chunk.len() {
        let item = chunk.get(i).unwrap();
        // … per-item work …
        let _ = item;
    }
    next   // return to caller so they know the next offset
}
```

### Off-chain / keeper loop (multiple transactions)

```text
offset = 0
loop:
    next = contract.process_page(items, offset)
    if next is None: break
    offset = next
```

### Inline loop (when the full vec fits in one budget)

```rust
use crate::iter_chunks::vec_chunks;

let mut offset = 0u32;
loop {
    let (chunk, next) = vec_chunks(&e, &my_vec, offset, 20);
    if chunk.is_empty() { break; }
    // … handle chunk …
    match next {
        Some(n) => offset = n,
        None    => break,
    }
}
```

---

## Key Properties

| Property | Value |
|---|---|
| Default chunk size | `DEFAULT_CHUNK_SIZE = 50` |
| `chunk_size = 0` behaviour | Uses `DEFAULT_CHUNK_SIZE` |
| `offset >= len` behaviour | Returns empty chunk, `next = None` |
| Mutates `source` | No — read-only borrow |
| `no_std` compatible | Yes |
| Storage access | None |
| Works with any `Vec<T>` | Yes, as long as `T` round-trips through `soroban_sdk::Val` |

---

## Choosing a `chunk_size`

| Per-item cost | Recommended `chunk_size` |
|---|---|
| Cheap (arithmetic, comparisons) | 100–200 |
| Moderate (one storage read per item) | 50 (`DEFAULT_CHUNK_SIZE`) |
| Expensive (cross-contract call per item) | 5–10 |

When in doubt, use `DEFAULT_CHUNK_SIZE` and benchmark with
`env.cost_estimate().budget()` in your test harness.

---

## See Also

- `contracts/credence_bond/src/iter_chunks.rs` — implementation and inline tests
- `contracts/credence_bond/src/parameters.rs` — `DEFAULT_CHUNK_SIZE` definition
- `contracts/credence_bond/docs/pagination.md` — related `(offset, limit)` read pattern
- `contracts/credence_bond/src/liquidation_scanner.rs` — real-world chunk loop
  using `MAX_ITER_HARD_CAP` (the same principle applied to liquidation scanning)
