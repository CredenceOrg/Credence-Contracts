# Storage TTL Policy

This document describes the storage TTL strategy used across all Credence contracts. Every storage entry must have its TTL actively managed to prevent silent archival under Soroban's ledger expiration model.

---

## Instance storage policy

**Constant**: `STORAGE_TTL_EXTEND_TO = 31_536_000` (~1 year at 5 s/ledger)

**Threshold**: `STORAGE_TTL_EXTEND_TO / 2` (~6 months)

**Rule**: Every public entrypoint (reads **and** writes alike) calls `bump_instance_ttl(&e)` as its first statement. This ensures the instance storage block — which holds admin config, bond state, balances, and lookup tables — remains accessible for as long as the contract receives any traffic.

The helper used in each contract:

```rust
const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}
```

---

## Persistent storage — audit records

**Constant**: `PERSISTENT_TTL_MAX = 3_110_400` (the Soroban network cap, ~6 months at 5 s/ledger)

**Rule**: Every `persistent().set()` is immediately followed (same call frame) by `persistent().extend_ttl(key, PERSISTENT_TTL_MAX / 2, PERSISTENT_TTL_MAX)`. This applies to reads as well, so records survive read-only traversal.

Applies to:
- `credence_bond` slash history (`SlashRecord`, `SlashCount`)
- `credence_bond` emergency audit trail (`Record`, `Transition`, `RecordSeq`, `TransitionSeq`)

If `PERSISTENT_TTL_MAX` ever changes on mainnet, update the single constant in `credence_bond/src/lib.rs`; all callers derive from it via `crate::PERSISTENT_TTL_MAX`.

---

## Persistent storage — expiry-bound records

**Pattern**: Expiry-aware TTL computed from the record's own `expires_at` timestamp.

```rust
// Convert expires_at (Unix seconds) → ledger count, add buffer, cap at max.
fn ttl_for_claim(e: &Env, expires_at: u64) -> u32 {
    if expires_at == 0 { return PERSISTENT_TTL_MAX; }
    let remaining_secs = expires_at.saturating_sub(e.ledger().timestamp());
    let ledgers = (remaining_secs / SECONDS_PER_LEDGER) as u32;
    ledgers.saturating_add(LEDGER_BUMP_BUFFER).min(PERSISTENT_TTL_MAX)
}
```

**Buffer**: `LEDGER_BUMP_BUFFER = 17_280` (~1 day at 5 s/ledger)

Applies to:
- `credence_bond` claims (`ClaimCounter`, `ClaimById`, `PendingClaims`, `ClaimableAmount`) — in `claims.rs`
- `credence_delegation` delegations and nonces — in `nonce.rs` (expiry-aware `bump_delegation_ttl` / `bump_nonce_ttl`)

---

## Contract-by-contract reference

| Contract | Storage tier | TTL strategy | Helper location |
|---|---|---|---|
| `credence_bond` | instance | `STORAGE_TTL_EXTEND_TO` / `bump_instance_ttl` | `lib.rs` |
| `credence_bond` | persistent (slash history) | `PERSISTENT_TTL_MAX` | `slash_history.rs` |
| `credence_bond` | persistent (emergency) | `PERSISTENT_TTL_MAX` | `emergency.rs` |
| `credence_bond` | persistent (claims) | Expiry-aware, `PERSISTENT_TTL_MAX` cap | `claims.rs` |
| `credence_delegation` | instance | `STORAGE_TTL_EXTEND_TO` / `bump_instance_ttl` | `lib.rs` |
| `credence_delegation` | persistent (delegations) | Expiry-aware / `bump_delegation_ttl` | `nonce.rs` |
| `credence_delegation` | persistent (nonces) | Expiry-aware / `bump_nonce_ttl` | `nonce.rs` |
| `credence_registry` | instance | `STORAGE_TTL_EXTEND_TO` / `bump_instance_ttl` | `lib.rs` |
| `admin` | instance | `STORAGE_TTL_EXTEND_TO` / `bump_instance_ttl` | `lib.rs` |
| `credence_treasury` | instance | `STORAGE_TTL_EXTEND_TO` / `bump_instance_ttl` | `treasury.rs` |
| `arbitration` | instance | `STORAGE_TTL_EXTEND_TO` / `bump_instance_ttl` | `lib.rs` |
| `credence_multisig` | instance | `STORAGE_TTL_EXTEND_TO` / `bump_instance_ttl` | `multisig.rs` |
| `timelock` | instance | `STORAGE_TTL_EXTEND_TO` / `bump_instance_ttl` | `lib.rs` |
| `credence_errors` | — | No storage | — |
| `credence_math` | — | No storage | — |

---

## Testing

Every newly covered storage path has at least one regression test that:
1. Calls the mutating function.
2. Advances the ledger via `e.ledger().with_mut(|li| { li.sequence_number += N; li.timestamp += T; })`.
3. Reads the data back and asserts it returns the correct value.

Test locations:
- `credence_bond/src/test_slashing.rs` — `SlashRecord` + `SlashCount` survive ledger advancement
- `credence_bond/src/test_emergency.rs` — `Record` + `Transition` persist after TTL window
- `credence_bond/src/test_claim_expiry_sweep.rs` — `ClaimCounter` + `PendingClaims` survive
- `credence_delegation/src/test_delegation_ttl.rs` — canonical model (delegation + nonce TTL)

---

## Soroban TTL clamping

The network silently clamps `extend_to` to `max_entry_ttl`. Tests must set `max_entry_ttl` explicitly:

```rust
e.ledger().set(LedgerInfo {
    timestamp: ...,
    protocol_version: 22,
    sequence_number: ...,
    network_id: Default::default(),
    base_reserve: 10,
    min_temp_entry_ttl: 1,
    min_persistent_entry_ttl: 1,
    max_entry_ttl: 3_110_400,
});
```

---

## Out of scope (follow-up)

- Archival-restore helpers (`restore_*` functions) — not included in this release. If an entry becomes archived, it requires admin intervention or a dedicated restore path.
- Per-key TTL customisation beyond the two tiers documented above.
