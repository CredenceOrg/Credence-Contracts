## Fix #376 — Add storage TTL bumping for `Delegation` and `Nonce` entries

### Problem

`DataKey::Delegation` and `DataKey::Nonce` entries were stored in **instance
storage**, which has a single shared TTL for the entire contract instance.
Neither key type had its TTL extended individually. If a `Nonce` entry expired
and was archived, it would restore to `0`, breaking the monotonic nonce
guarantee and enabling replay attacks against long-lived management delegations.

### Changes

**`nonce.rs`**
- Added TTL constants: `LEDGER_BUMP_BUFFER = 17_280` (~1 day), `MIN_NONCE_TTL = 518_400` (~30 days), `MAX_TTL = 3_110_400` (~6 months).
- Added `bump_delegation_ttl(env, key, expires_at)` — bumps a `Delegation` key's TTL to `(expires_at − now) / 5s + LEDGER_BUMP_BUFFER`, capped at `MAX_TTL`.
- Added `bump_nonce_ttl(env, key, expires_at)` — bumps a `Nonce` key's TTL to at least `MIN_NONCE_TTL`, extended further if a delegation's `expires_at` exceeds that floor.
- Both helpers guard with `has()` before calling `extend_ttl` to avoid `MissingValue` errors on first write.
- Migrated all `Nonce` reads/writes from `instance()` to `persistent()` storage.

**`lib.rs`**
- Migrated all `Delegation` reads/writes from `instance()` to `persistent()` storage.
- `store_delegation`: calls `bump_delegation_ttl` after write; calls `bump_nonce_ttl` to ensure the nonce entry's TTL covers the delegation's lifetime.
- `mark_delegation_revoked`: calls `bump_delegation_ttl` after write.
- `get_delegation`, `is_valid_delegate`, `get_attestation_status`: call `bump_delegation_ttl` on every read.

**`test_delegation_ttl.rs`** (new file, 8 tests)
- TTL set on write (`delegate`)
- TTL refreshed on `get_delegation`
- TTL refreshed on `is_valid_delegate`
- Nonce TTL set on `consume_nonce`
- Nonce TTL covers delegation lifetime
- Nonce TTL refreshed on `get_nonce`
- TTL bumped on `revoke_delegation`
- TTL capped at `MAX_TTL` for far-future expiries

### Security guarantee

After this change, a `Nonce` entry for any owner who has ever issued a
delegation will remain in persistent storage for at least `MIN_NONCE_TTL`
ledgers (~30 days) beyond the last interaction, and for at least as long as
the longest active delegation. Archival of a nonce entry while a delegation
is still valid is no longer possible under normal operation.

### Testing

```
cargo test -p credence_delegation
```

All 55 tests pass (27 pre-existing + 8 new TTL tests + existing domain/pausable suites).
