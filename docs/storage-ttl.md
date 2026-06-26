# Storage TTL policy

This document describes the storage TTL strategy used across all Credence contracts.

## Overview

Soroban stores ledger entries with a finite TTL. Entries that are not periodically bumped
get archived and become inaccessible. Every contract that writes to `instance()` storage
must bump the TTL after each write so that live contract state is never silently archived.

## Coverage by contract

| Contract | Storage tier | Constant | Helper location | Bumped after every write? |
|---|---|---|---|---|
| `credence_bond` | Instance | `consts::INSTANCE_TTL_EXTEND_TO` | `lib.rs::bump_instance_ttl` | ✅ |
| `credence_delegation` | Instance | `consts::INSTANCE_TTL_EXTEND_TO` | `lib.rs::bump_instance_ttl` | ✅ |
| `credence_delegation` | Persistent (Delegation/Nonce) | `consts::MAX_TTL` | `nonce.rs::bump_delegation_ttl` | ✅ |
| `credence_treasury` | Instance | `consts::INSTANCE_TTL_EXTEND_TO` | `treasury.rs::bump_instance_ttl` | ✅ |
| `credence_multisig` | Instance | `consts::INSTANCE_TTL_EXTEND_TO` | `multisig.rs::bump_instance_ttl` | ✅ |
| `timelock` | Instance | `consts::INSTANCE_TTL_EXTEND_TO` | `lib.rs::bump_instance_ttl` | ✅ |
| `arbitration` | Instance | `consts::INSTANCE_TTL_EXTEND_TO` | `lib.rs::bump_instance_ttl` | ✅ |
| `admin` | Instance | `consts::INSTANCE_TTL_EXTEND_TO` | `lib.rs::bump_instance_ttl` | ✅ |

## Constants

Each contract defines its own `src/consts.rs` containing:

```rust
/// Ledger TTL to extend instance storage to on every write (~1 year at 5 s/ledger).
pub const INSTANCE_TTL_EXTEND_TO: u32 = 31_536_000;

/// Threshold below which a TTL bump is triggered.
pub const INSTANCE_TTL_THRESHOLD: u32 = INSTANCE_TTL_EXTEND_TO / 2;
```

`credence_bond` also defines `MIN_NONCE_TTL`. `credence_delegation` additionally defines
`LEDGER_BUMP_BUFFER` and `MAX_TTL` for its persistent nonce/delegation storage.

`INSTANCE_TTL_EXTEND_TO` is the single authoritative value per contract (~1 year at 5 s/ledger).
The Soroban runtime silently clamps this to `max_entry_ttl`, so the value represents an intent,
not a guarantee.

## The bump helper

Each contract contains a private `bump_instance_ttl(e: &Env)` function:

```rust
fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(consts::INSTANCE_TTL_THRESHOLD, consts::INSTANCE_TTL_EXTEND_TO);
}
```

This follows the Soroban pattern: only bumps if the current TTL is below `threshold`
(= half of `extend_to`). No-op if the TTL is already high enough.

## Write lifecycle (CLAUDE.md §State Write Lifecycle)

Per the project write lifecycle, `bump_instance_ttl` is called at **step 6** — immediately
after the final `instance().set()` call in a state-mutating entrypoint, before emitting
events and running invariant checks:

```
5. WRITE:  e.storage().instance().set(&key, &new_state)
6. TTL:    bump_instance_ttl(&e)   ← here
7. EMIT:   e.events().publish(...)
8. CHECK:  invariants::assert_self_consistent(&e)
```

For functions containing multiple `set()` calls (e.g. `initialize`), one `bump_instance_ttl`
call after the last write is sufficient — the single call covers the whole instance entry.

## Delegation persistent storage

`credence_delegation` uses a separate TTL strategy for its persistent `Delegation` and
`Nonce` entries: TTL is derived from the delegation's `expires_at` timestamp plus
`LEDGER_BUMP_BUFFER`, capped at `MAX_TTL`. See `nonce.rs` for details.

## Testing

### Instance storage

Each of the five newly covered contracts ships a `src/test_ttl.rs` that:
- Calls a state-mutating entrypoint
- Asserts `e.as_contract(&contract_id, || Instance::get_ttl(&e.storage().instance())) > 0`
- Sets `li.max_entry_ttl = INSTANCE_TTL_EXTEND_TO + 1` for deterministic results

`credence_delegation` instance TTL tests are in `test_delegation_ttl.rs` (tests 9–10).

### Persistent storage

`credence_delegation` persistent TTL is covered by tests 1–8 in `test_delegation_ttl.rs`.

### `credence_bond`

`credence_bond` instance TTL was already fully covered. No new tests needed.

## Archival and recovery

If an entry is archived (e.g. a very small `max_entry_ttl` network configuration):

- Admin intervention: the admin can restore critical entries from off-chain backups
  and write them back into contract `instance()` storage with a fresh TTL.
- Dedicated `restore_*` helpers may be added in a future iteration.

## Caveats

- The Soroban runtime clamps requested TTLs to the current `max_entry_ttl`. Tests must
  set `li.max_entry_ttl` explicitly for deterministic TTL assertions.
- `pausable.rs` is copy-shared across contracts. TTL bumping is **not** added inside
  `pausable.rs`; each contract's own entrypoints handle TTL so policy stays per-contract.
- Read-path TTL bumping (bump on `.get()` calls) is a separate enhancement; surface as
  a separate issue if desired.
