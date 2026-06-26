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
| `revoked_at` | `u64` | The ledger timestamp when the delegation was revoked; `0` while not revoked. |
| `scheme` | `u8` | The signature scheme used to create the delegation. Currently returns 0 (Ed25519) as it is not persisted in storage. |

## Grace window and authority semantics

The admin-configurable `revocation_grace_period` (default `0`) controls:

1. **Audit status** — when `grace > 0`, delegations report `InGrace` for `expires_at <= now <= expires_at + grace`.
2. **Late revocation** — owners may revoke within that same window after expiry when `grace > 0`.

`is_valid` and `is_valid_delegate` remain a **hard cliff** at `expires_at`. `InGrace` is informational only and does **not** re-grant delegate authority.

When `grace = 0` (default), status jumps directly to `Expired` at `expires_at`, and post-expiry revocation remains permitted at any time (legacy behaviour).

## Configuration

```rust
pub fn set_revocation_grace_period(e: Env, admin: Address, grace_seconds: u64)
pub fn get_revocation_grace_period(e: Env) -> u64
```

## Usage for Indexers

Indexers should use this view to track validity, explicit lifecycle status, and remaining lifetime without reimplementing grace logic locally.

> [!NOTE]
> This is a read-only view and does not require authentication. It utilizes `e.storage().persistent().get` and does not call `require_auth()`.
