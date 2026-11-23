# Delegation Summary View

The `get_delegation_summary` view function provides a comprehensive summary of a delegation's state for indexers and off-chain tools.

## Entrypoint

```rust
pub fn get_delegation_summary(
    e: Env,
    owner: Address,
    delegate: Address,
    delegation_type: DelegationType,
) -> DelegationSummary
```

## `DelegationSummary` Struct

| Field | Type | Description |
|-------|------|-------------|
| `is_valid` | `bool` | `true` if the delegation is NOT revoked AND the current ledger timestamp is strictly less than `expires_at`. **Does not** treat `InGrace` as valid for authorization. |
| `status` | `DelegationStatus` | Explicit lifecycle: `Active`, `InGrace`, `Expired`, or `Revoked`. |
| `time_to_expiry` | `u64` | The remaining lifetime of the delegation in seconds (`expires_at - now`). Returns 0 if expired. |
| `delegation_type` | `DelegationType` | The type of delegation (`Attestation` or `Management`). |
| `revoked_at` | `u64` | The ledger timestamp when the delegation was revoked; `0` while not revoked. Persisted on the `Delegation` record — see [Revocation timestamp semantics](#revocation-timestamp-semantics). |
| `scheme` | `u8` | The signature scheme used to create the delegation (`0` = Ed25519). Persisted from the delegated-action payload; the direct `delegate()` path always stores `0`. |

## Grace window and authority semantics

The admin-configurable `revocation_grace_period` (default `300` seconds / 5 minutes) controls:

1. **Audit status** — when `grace > 0`, delegations report `InGrace` for `expires_at <= now <= expires_at + grace`.
2. **Late revocation** — owners may revoke within that same window after expiry when `grace > 0`.

`is_valid` and `is_valid_delegate` remain a **hard cliff** at `expires_at`. `InGrace` is informational only and does **not** re-grant delegate authority.

When `grace` is at its default of `300` seconds, delegations enter `InGrace` for 5 minutes after `expires_at`, and late revocation is permitted within that window. Set the grace period explicitly to `0` to restore the legacy hard-cliff behaviour (immediate `Expired` status at `expires_at`, unlimited post-expiry revocation).

## Revocation timestamp semantics

`revoked_at` records *when* a delegation was pulled, not merely *that* it was. This
distinction is forensically important for delegated-authority systems: it answers
"was this action signed before or after the owner revoked?", which a boolean
`revoked` flag cannot.

### How it is set

`revoked_at` is set to `e.ledger().timestamp()` at the moment of revocation. All
revoke paths share a single internal writer (`mark_delegation_revoked`), so they
produce consistent values:

- `revoke_delegation` (direct, owner-authenticated)
- `revoke_attestation` (direct, attester-authenticated)
- `execute_delegated_revoke` (relayer-friendly, payload-authenticated)
- `execute_delegated_revoke_attest` (relayer-friendly attestation revoke)

The same writer also sets `revoked = true` and re-publishes the full `Delegation`
(including `revoked_at`) in the `delegation_revoked` event, so indexers receive the
timestamp without an extra read.

### The `0` sentinel and idempotency

- `revoked_at == 0` means **never revoked**. A live delegation always reads `0`.
- Revocation is **not idempotent**: a second revoke panics with `AlreadyRevoked`
  (`#502`) *before* any write, so the **first** `revoked_at` is preserved and never
  overwritten.
- `revoked_at` is independent of `expires_at`. A delegation can be revoked before,
  at, or (within the grace window) after expiry; `is_valid` is a hard cliff at
  `expires_at` regardless of `revoked_at`.

### Back-compat read convention (v1 → v2 entries)

`revoked_at` (and `scheme`) were added in the v2 `Delegation` layout. Entries
persisted before the upgrade are read back through the legacy decoder, which fills
`revoked_at = 0` and `scheme = 0`. Therefore:

- A pre-v2 entry that was **never revoked** reads `revoked_at = 0` — correct.
- A pre-v2 entry that **was revoked** before the upgrade also reads `revoked_at = 0`.
  Consumers must treat `revoked == true && revoked_at == 0` as "revoked at an
  unknown (pre-upgrade) time", not as "not revoked". For never-revoked entries the
  pair is always `revoked == false && revoked_at == 0`.

## Configuration

```rust
pub fn set_revocation_grace_period(e: Env, admin: Address, grace_seconds: u64)
pub fn get_revocation_grace_period(e: Env) -> u64
```

## Usage for Indexers

Indexers should use this view to track validity, explicit lifecycle status, and remaining lifetime without reimplementing grace logic locally.

> [!NOTE]
> This is a read-only view and does not require authentication. It utilizes `e.storage().persistent().get` and does not call `require_auth()`.
