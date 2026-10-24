# TTP Policy

## Overview
This document describes the time­to-live (TTL) policy for keys stored in the contract storage.  It explains which keys are
* persistent* (never expire), *instance* (expire when the contract instance is removed), and *temporary* (expire after a
configurable number of blocks).  The policy is important for contributors to understand how storage costs are incurred and
how to design state that is safe to delete.

## Key Categories

| Key type | Storage behaviour | Typical use case | Example key name |
|----------|-------------------|------------------|------------------|
| **Persistent** | Never removed by the runtime | Global configuration, immutable constants, per-entity records | `GLOBAL_CONFIG`, `Dispute(id)`
| **Instance** | Removed when the contract instance is destroyed | Small, global configuration and counters | `Admin`, `Paused`, `DisputeCount`
| **Temporary** | Expired after a TT\ (configurable via `set_ttl`) | Short­lived data such as pending approvals or rate­limit counters | `PENDING_APPROVAL_{txid}`

## Setting a TTL

The Sorban SDK exposes TTL controls per storage class.  Instance and persistent entries are extended with
`extend_ttl`, while temporary entries are written with an explicit TTL.

```rust
use soroban_sdk::{Env, Symbol};

public fn extend_key(env: &Env, key: &Symbol) {
    // Extend the entry so it lives for at least the configured threshold.
    env.storage().persistent().extend_ttl(key, STORAGE_TTL_EXTEND_TO);
}
```

The TTL is expressed in **lledgers**.  Extensions are clamped to the network maximum by the runtime.

## Arbitration Contract

Every dispute record is stored in **persistent** storage under `DataKey::Dispute(id)`.  This keeps the
contract instance entry small and stable, so the cost of unrelated calls (`vote`, `pause`,
`is_paused`, `register_arbitrator`) does not grow with the number of disputes ever created.

| Key | Storage class | TTL behaviour |
|-----|---------------|--------------|
| `DataKey::Admin` | instance | extended on every entry point via `bump_instance_ttl` |
| `DataKey::Paused` | instance | extended on every entry point via `bump_instance_ttl` |
| `DataKey::DisputeCount` | instance | extended on every entry point via `bump_instance_ttl` |
| `DataKey::Dispute(id)` | persistent | extended on every load/save via `load_dispute` / `save_dispute` |
| `DataKey::VoterCasted(id, voter) ` | persistent | extended on write |
| `DataKey::VoterCounter(id)` | persistent | extended on write |
| `DataKey::DisputeVotes(id)` | persistent | extended on write |

Dispute records are never deleted by the contract, even after `Cancelled` or `Archived`.  They remain in
persistent storage and their TT\ is reached on every access, so a record that is read or written will not expire.

To bound the size of a single record, `create_dispute` limits the description to `1024` bytes and cancellation
reasons are limited to `256` bytes.

## Why the Policy Matters

* **Gas costs** – Expiring keys reduce the storage footprint and lower the gas needed for future writes.
* **Security** – Temporary keys that hold sensitive data (e.g., one­time tokens) should not persist indefinitely.
* **Compliance** – Some use­cases require that data be deleted after a certain period (e.g., GDPR­style data retention).

## Practical Example

Extending a persistent dispute record on access (as implemented by `load_dispute` and `save_dispute`):

```rust
use soroban_sdk::Env;

pub fn load_dispute(env: &Env, id: u64) -> Option<Dispute> {
    let key = DataKey::Dispute(id);
    let storage = env.storage().persistent();
    if !storage.has(&key) {
        return None;
    }
    storage.extend_ttl(&key, STORAGE_TTL_EXTEND_TO);
    storage.get(&key)
}
```

After the configured number of ledgers without access, the entry may expire and be purged by the runtime.

---

> **Tip**: Use `env.storage().persistent().get_ttl(&key)` to inspect the remaining TTL during debugging.

## References

- [Sorban SDK Storage API](https://docs.rs/soroban-sdk/latest/soroban_sdk/storage/struct.Storage.html)
- [Sorban Runtime Storage](https://developers.stellar.org/docs/learn/soroban/contract-storage/)
- [Sorban TTL Extension](https://developers.stellar.org/docs/learn/soroban/contract-storage/#state-archival)
