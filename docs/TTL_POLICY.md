# TTL Policy

## Overview
This document describes the time‑to‑live (TTL) policy for keys stored in the contract storage.  It explains which keys are
* persistent* (never expire), *instance* (expire when the contract instance is removed), and *temporary* (expire after a
configurable number of blocks).  The policy is important for contributors to understand how storage costs are incurred and
how to design state that is safe to delete.

## Key Categories

| Key type | Storage behaviour | Typical use case | Example key name |
|----------|-------------------|------------------|-----------------|
| **Persistent** | Never removed by the runtime | Global configuration, immutable constants | `GLOBAL_CONFIG`
| **Instance** | Removed when the contract instance is destroyed | Per‑user state that should be cleaned up on withdrawal | `USER_BALANCE_{user_id}`
| **Temporary** | Expired after a TTL (configurable via `set_ttl`) | Short‑lived data such as pending approvals or rate‑limit counters | `PENDING_APPROVAL_{txid}`

## Setting a TTL

```rust
use soroban_sdk::{contractimpl, Env, Symbol, Vec, BytesN, Bytes, String, Map, Symbol, Vec, BytesN, Bytes, String, Map, Symbol};

#[contractimpl]
impl MyContract {
    pub fn set_ttl(env: Env, key: Symbol, ttl: u64) {
        env.storage().set_ttl(key, ttl);
    }
}
```

The `ttl` is expressed in **blocks**.  A value of `0` means the key never expires.

## Why the Policy Matters

* **Gas costs** – Expiring keys reduce the storage footprint and lower the gas needed for future writes.
* **Security** – Temporary keys that hold sensitive data (e.g., one‑time tokens) should not persist indefinitely.
* **Compliance** – Some use‑cases require that data be deleted after a certain period (e.g., GDPR‑style data retention).

## Practical Example

```rust
// Store a one‑time approval that expires after 10 blocks
let key = Symbol::from_str("PENDING_APPROVAL_12345");
let ttl = 10u64;
env.storage().set_ttl(key, ttl);
```

After 10 blocks, the key will be automatically purged by the runtime.

---

> **Tip**: Use `env.storage().get_ttl(key)` to inspect the remaining TTL during debugging.

## References

- [Soroban SDK Storage API](https://docs.rs/soroban-sdk/latest/soroban_sdk/storage/struct.Storage.html)
- [Soroban Runtime Storage](https://developers.stellar.org/docs/learn/soroban/contract-storage/)
