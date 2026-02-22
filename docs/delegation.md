# Delegation System

Soroban contract enabling bond owners to delegate attestation and management rights to other addresses.

## Overview

The `CredenceDelegation` contract stores delegations keyed by `(owner, delegate, DelegationType)`. Each delegation carries an expiry timestamp and can be revoked by the owner at any time.

## Types

### DelegationType

| Variant       | Description                              |
|---------------|------------------------------------------|
| Attestation   | Delegate can attest on behalf of owner   |
| Management    | Delegate can manage bonds on behalf of owner |

### Delegation

| Field            | Type            | Description                      |
|------------------|-----------------|----------------------------------|
| owner            | Address         | Bond owner granting delegation   |
| delegate         | Address         | Address receiving delegated rights |
| delegation_type  | DelegationType  | Kind of delegation               |
| expires_at       | u64             | Ledger timestamp when delegation expires |
| revoked          | bool            | Whether the delegation was revoked |

## Contract Functions

### `initialize(admin: Address)`

Set the contract admin. Can only be called once.

### `delegate(owner, delegate, delegation_type, expires_at) -> Delegation`

Create a delegation. Requires owner authorization. `expires_at` must be a future timestamp. Emits a `delegation_created` event.

### `revoke_delegation(owner, delegate, delegation_type)`

Revoke an active delegation. Requires owner authorization. Panics if the delegation does not exist or is already revoked. Emits a `delegation_revoked` event.

### `get_delegation(owner, delegate, delegation_type) -> Delegation`

Retrieve a stored delegation. Panics if not found.

### `is_valid_delegate(owner, delegate, delegation_type) -> bool`

Returns `true` if the delegation exists, is not revoked, and has not expired. Returns `false` otherwise (including when no delegation exists).

## Events

| Event                | Data        | Emitted when              |
|----------------------|-------------|---------------------------|
| delegation_created   | Delegation  | A new delegation is stored |
| delegation_revoked   | Delegation  | A delegation is revoked    |

## Security

- Only the owner can create or revoke their delegations (`require_auth`).
- Delegations are time-bound; expired delegations are treated as invalid.
- Double initialization is rejected.
- Double revocation is rejected.
- Each `(owner, delegate, type)` tuple maps to exactly one delegation record.

## Usage

```bash
# Build
cargo build -p credence_delegation

# Test
cargo test -p credence_delegation
```
