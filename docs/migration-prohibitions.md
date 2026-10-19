# Migration Prohibitions — What We Forbid During Migrations

**Audience:** Contributor  
**Purpose:** Every migration (storage format, event schema, error code, signature scheme) carries risk. This document lists the operations that are **always forbidden** during a migration, explains why they break the system, and shows how to avoid them. Reviewers use this list to gate PRs that touch storage, events, error enums, or wire formats.

---

## 1. Renumbering existing error codes

### Rule

Never change the numeric discriminant of an existing `ContractError` variant. The error codes are wire-stable: indexers, off-chain clients, and support tooling key on the numeric value, not the Rust name.

### What is forbidden

```rust
// ❌ FORBIDDEN — renumbering breaks downstream decoders
pub enum ContractError {
    AlreadyInitialized = 1,  // was 2 — breaks every client that matched code 2
    NotInitialized = 2,      // was 1 — silently shifts semantics
}
```

### What is allowed

```rust
// ✅ Append-only — existing codes never move
pub enum ContractError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    // ... existing codes unchanged ...
    NewVariant = 130,  // appended at end of current category block
}
```

### How to check

```bash
cargo test -p credence_errors error_codes_wire
```

See [`docs/error-codes-wire.md`](error-codes-wire.md) for the bump procedure.

---

## 2. Changing existing storage key semantics

### Rule

Once a `DataKey` variant is deployed and has live entries on-chain, its meaning and the type of the stored value must never change. Adding new variants is allowed; mutating existing ones is not.

### What is forbidden

```rust
// ❌ FORBIDDEN — `Bond` used to store `IdentityBond`, now stores `u64`
pub enum DataKey {
    Bond,                     // new type is incompatible with existing entries
    Admin,
}
```

### What is allowed

```rust
// ✅ New variant for the new format; old reads go through migration
pub enum DataKey {
    Bond,                     // unchanged — still IdentityBond
    Admin,                    // unchanged
    BondV2,                   // new variant for new format
}
```

If the existing variant type must change, add a **lazy migration** (see `contracts/credence_bond/src/migration.rs`):
- Read the old entry.
- Convert to the new format.
- Write under the **same key** (or a new key, deprecating the old).
- The migration must be idempotent (safe to call on every read) and must not panic on missing entries.

### How to check

Search for existing `DataKey` usage in the contract's `storage.rs` and verify new fields are appended, not injected. Use `grep -r "DataKey" contracts/<crate>/src/` to find all storage read/write sites.

---

## 3. Changing event topic positions or types

### Rule

Once an event is deployed, every field's position in the topic array and its Soroban type are frozen. Indexers hard-code these offsets and types. Changing them produces silent data corruption off-chain.

### What is forbidden

```rust
// ❌ FORBIDDEN — topic[2] was `Address`, now is `i128`
e.events().publish(
    (Symbol::new(&e, "bond_created"), identity, amount),
    (duration,),
);
```

### What is allowed

```rust
// ✅ New event name for the new structure; old event still emitted
e.events().publish(
    (Symbol::new(&e, "bond_created_v2"), identity, amount),
    (duration, is_rolling),
);
// Legacy consumers still see the old event
e.events().publish(
    (Symbol::new(&e, "bond_created"), identity),
    (amount, duration),
);
```

### How to check

Every migration that touches events must:
1. Define a new event name (`*_v2`) if topics or types change.
2. Emit both the old and new event for the duration of the migration.
3. Document the deprecation timeline in the PR and `CHANGELOG.md`.

See [`docs/EVENT_INDEXING_MIGRATION.md`](EVENT_INDEXING_MIGRATION.md) for the full dual-emission pattern.

---

## 4. Renumbering wire-stable enum discriminants

### Rule

Any enum whose discriminant is encoded on-chain or in signed payloads must use append-only growth. Renumbering invalidates existing entries or signatures.

### What is forbidden

```rust
// ❌ FORBIDDEN — old on-chain entries encoded with Ed25519=0 now map to Secp256r1
pub enum SchemeTag {
    Ed25519 = 1,     // was 0 — breaks every existing signature
    Secp256r1 = 0,   // was 1 — map collision
}
```

### What is allowed

```rust
// ✅ Append-only
pub enum SchemeTag {
    Ed25519 = 0,
    Secp256r1 = 1,
    MLDSA44 = 2,
    NewScheme = 3,  // safe append
}
```

### Examples in this codebase

| Enum | Where | Wire-stable because |
|------|-------|---------------------|
| `ContractError` | `credence_errors` | Indexers match on numeric code |
| `SchemeTag` | `credence_delegation` | Encoded in signed payload |
| `BondTier` | `credence_bond` | Stored in `IdentityBond` |

### How to check

Assert discriminant values in a test that fails on renumbering. See `contracts/credence_errors/tests/error_codes_wire.rs` for the canonical pattern.

---

## 5. Using `format!` or dynamic symbols in production code

### Rule

Production contract code (anything compiled by `cargo build --target wasm32-unknown-unknown --release`) must never call `format!`, `format_args!`, `write!`, or `writeln!`. These macros bloat WASM size, allocate at runtime, and — worst — let caller-controlled data become event topics or revert strings.

### What is forbidden

```rust
// ❌ FORBIDDEN — dynamic event topic
e.events().publish(
    (Symbol::new(&e, &format!("slash:{}", reason)),),
    (amount,),
);
```

### What is allowed

```rust
// ✅ Static symbol for a known closed set
let topic = match reason {
    SlashReason::Fraud => Symbol::new(&e, "slash_fraud"),
    SlashReason::Downtime => Symbol::new(&e, "slash_downtime"),
};
e.events().publish((topic,), (amount,));
```

### How to check

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace `clippy.toml` bans the format macros; per-crate `#![cfg_attr(... deny(clippy::disallowed_macros))]` enforces it at compile time. See [`docs/no-dynamic-strings.md`](no-dynamic-strings.md) for the full threat model.

---

## 6. Introducing `std::` calls in `#![no_std]` crates

### Rule

Every contract crate is `#![no_std]`. A migration must not introduce `std::` imports, `std::` paths, or dependencies that pull in the Rust standard library. WASM contracts cannot link `std`.

### What is forbidden

```rust
// ❌ FORBIDDEN — std:: not available in WASM
use std::collections::HashMap;
```

### What is allowed

```rust
// ✅ alloc:: types and Soroban SDK primitives
use soroban_sdk::{Vec, Map, Address};
use alloc::vec;
```

### How to check

```bash
cargo build --target wasm32-unknown-unknown --release --locked -p credence_bond
cargo build --target wasm32-unknown-unknown --release --locked -p credence_delegation
```

If the build succeeds, no `std` dependency leaked in.

---

## 7. Adding non-append-only fields to serialised structs

### Rule

When adding a field to a struct that is stored on-chain or serialised into signed payloads, the new field must be appended **after** all existing fields. Inserting fields in the middle changes the binary layout and breaks deserialisation of existing entries.

### What is forbidden

```rust
// ❌ FORBIDDEN — `new_field` inserted before existing field changes layout
pub struct IdentityBond {
    pub admin: Address,
    pub new_field: u64,        // inserted — breaks existing deserialisation
    pub bonded_amount: i128,   // was field 1, now field 2
}
```

### What is allowed

```rust
// ✅ New field appended at the end; old entries deserialise with default
pub struct IdentityBond {
    pub admin: Address,
    pub bonded_amount: i128,
    // ... existing fields unchanged ...
    pub new_field: u64,        // appended — safe
}
```

If the struct is stored with an old format that lacks the new field, the lazy migration pattern in `migration.rs` reads the old entry and writes it back with defaults for missing fields. This is idempotent and safe to call on every read.

### How to check

Compare the struct field order in the current `types/` module with the PR diff. New fields must appear at the end.

---

## 8. Skipping dual-emission for event migrations

### Rule

When event topics, types, or data positions change, the migration **must** emit both the old event and the new event simultaneously for a documented transition period. No indexer should break because the old event vanished overnight.

### What is forbidden

```rust
// ❌ FORBIDDEN — old consumers stop receiving events immediately
e.events().publish(
    (Symbol::new(&e, "bond_created_v2"), identity, amount),
    (duration,),
);
// No old `bond_created` event emitted
```

### What is allowed

```rust
// ✅ Dual emission — both old and new consumers work
events::emit_bond_created(&e, &identity, amount, duration, is_rolling);
events::emit_bond_created_v2(&e, &identity, amount, duration, is_rolling, bond_start);
```

### Changelog requirement

Every event migration must add a `CHANGELOG.md` entry under `Changed` describing:
- Which event(s) gained a new version.
- The dual-emission period (start date, planned removal date).
- How indexers should migrate (subscribe to `*_v2`, drop `*` after cutoff).

### How to check

Grep the diff for the old event name. If the old publish call is removed and no changelog entry documents the removal, the change is forbidden.

---

## 9. Silent storage key changes without migration code

### Rule

If a storage key's value type changes, or a new key replaces an old one, the migration must include runnable migration code (lazy or one-shot). Without it, existing on-chain entries become unreachable or deserialise incorrectly.

### What is forbidden

```rust
// ❌ FORBIDDEN — old Admin entries become garbage after the rename
pub enum DataKey {
    AdminV1,  // was Admin — old entries under "Admin" are never migrated
}
```

### What is allowed

```rust
// ✅ Lazy migration reads old key, writes under new key
pub fn migrate_admin_key(e: &Env) {
    if let Some(admin) = e.storage().instance().get::<DataKey, Address>(&DataKey::Admin) {
        e.storage().instance().set(&DataKey::AdminV1, &admin);
        e.storage().instance().remove(&DataKey::Admin);
    }
}
```

Add the migration call to the contract's entrypoint (e.g., at the top of `__constructor` or the first non-view function invoked after upgrade).

### How to check

For every storage key that is added or modified in the diff, verify there is accompanying `migration.rs` or inline code that migrates live entries. Untouched keys should remain unmodified.

---

## 10. Removing deprecated items without a documented grace period

### Rule

Deprecated error codes, events, storage keys, or entrypoints must not be removed from the codebase until:
1. A deprecation notice has been published for at least one release cycle.
2. The `CHANGELOG.md` records the deprecation and planned removal.
3. No indexer or off-chain consumer depends on the item.

### What is forbidden

```rust
// ❌ FORBIDDEN — removing DeprecatedVariant breaks clients that handle it
// pub enum ContractError {
//     DeprecatedVariant = 99,  // deleted — breaks match arms in client code
// }
```

### What is allowed

```rust
// ✅ Keep the variant, add doc-comment deprecation notice
/// DEPRECATED since v2.1.0 — will be removed in v3.0.0.
/// Use `ReplacementVariant` instead.
DeprecatedVariant = 99,
```

### How to check

A diff that only deletes enum variants, event emissions, or storage keys (without a documented grace period in `CHANGELOG.md`) must be rejected. Keep the dead code with a doc-comment deprecation notice until the grace period expires.

---

## Summary checklist for reviewers

Every migration PR must pass this checklist before merging:

- [ ] No existing error code renumbered (run `cargo test -p credence_errors error_codes_wire`).
- [ ] No existing storage key semantics changed (new keys use new variants).
- [ ] No event topic positions, types, or data fields changed (new events use `*_v2` names).
- [ ] No wire-stable enum discriminants renumbered (append-only).
- [ ] No `format!` / `format_args!` / `write!` / `writeln!` in production code.
- [ ] No `std::` calls in `#![no_std]` crates (WASM build succeeds).
- [ ] New struct fields appended at end, not inserted mid-struct.
- [ ] Dual emission active for old + new events during migration.
- [ ] Storage key migrations have runnable code (lazy or one-shot).
- [ ] Deprecated items kept with doc-comment notice for at least one release cycle.
- [ ] `CHANGELOG.md` updated with migration details.
- [ ] WASM release build passes (`cargo build --target wasm32-unknown-unknown --release --locked`).
- [ ] Lint and tests pass (`cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`).

When in doubt, prefer append-only growth over in-place mutation. Every in-place change is a potential silent breakage for indexers, clients, or on-chain entries.

---

## References

- [`docs/error-codes-wire.md`](error-codes-wire.md) — Error code stability policy and bump procedure
- [`docs/errors.md`](errors.md) — Canonical error code listing
- [`docs/EVENT_INDEXING_MIGRATION.md`](EVENT_INDEXING_MIGRATION.md) — Event schema migration with dual emission
- [`docs/EVENT_INDEXING.md`](EVENT_INDEXING.md) — Event indexing stability guarantees
- [`docs/no-dynamic-strings.md`](no-dynamic-strings.md) — Format macro ban and threat model
- [`docs/signature-scheme-upgrade.md`](signature-scheme-upgrade.md) — Scheme tag wire stability
- [`docs/known-simplifications.md`](known-simplifications.md) — Intentional simplifications vs production paths
- [`contracts/credence_bond/src/migration.rs`](../contracts/credence_bond/src/migration.rs) — Lazy migration example
- [`docs/UPGRADE.md`](UPGRADE.md) — Contract upgrade procedure