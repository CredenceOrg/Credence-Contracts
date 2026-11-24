# Contract Template

Canonical Soroban contract template for the Credence workspace. Copy this crate when starting a new contract to get the correct structure, patterns, and test harness out of the box.

## Overview

`TemplateContract` is a minimal but complete Soroban contract that demonstrates every pattern required by Credence contracts:

- Authenticated initialisation with a double-init guard
- Typed `DataKey` enum for all storage slots
- `#[contracttype]` structs for on-chain data
- `require_auth()` on initialization and admin-only mutating entry points
- `Symbol`-keyed event emission
- Ledger-timestamp recording on writes
- `#![no_std]` + `soroban_sdk` imports

## Types

### `DataKey`

| Variant           | Description                              |
| ----------------- | ---------------------------------------- |
| `Admin`           | Stores the contract administrator        |
| `Record(Address)` | Stores a `Record` keyed by owner address |

### `Record`

| Field        | Type | Description                                                  |
| ------------ | ---- | ------------------------------------------------------------ |
| `value`      | i128 | Arbitrary numeric value set by the admin                     |
| `updated_at` | u64  | Ledger timestamp of the last write                           |
| `expires_at` | u64  | Ledger timestamp when the record expires (0 = never expires) |

## Contract Functions

### `initialize(admin: Address)`

Set the contract admin. Requires authorization from the proposed admin. Returns the typed `AlreadyInitialized` error if called more than once. Emits an `initialized` event.

### `set_record(owner: Address, value: i128, expires_at: u64)`

Store or overwrite a `Record` for `owner`. Requires admin authorization. Rejects nonzero `expires_at` values that are less than or equal to the current ledger timestamp with `ExpiryInPast`. Records the current ledger timestamp in `updated_at`. Emits a `record_set` event.

### `remove_record(owner: Address)`

Delete the record for `owner`. Requires admin authorization. No-op if the record does not exist. Emits a `record_removed` event.

### `get_record(owner: Address) -> Record`

Return the record for `owner`. Returns `BondNotFound` if none exists, or `SignatureExpired` if the current timestamp is >= `expires_at`. This is a read-only operation: it never purges storage. A failed invocation rolls back all writes, so cleanup must not be combined with an erroring read.

### `has_record(owner: Address) -> bool`

Return `true` if an active record exists for `owner`, `false` otherwise. This read-only view does not purge expired records.

### `purge_expired(owner: Address)`

Permissionlessly remove an expired record and emit `record_purged`. Permissionless cleanup is safe because it can only remove data that is already expired. Returns `BondNotFound` if the record does not exist and `LockupNotExpired` if it is not yet expired. Keep cleanup in this separate successful invocation: Soroban rolls back writes if an invocation panics.

### `is_expired(owner: Address) -> bool`

Return `true` if a record exists for `owner` but is currently expired.

## How to add TTL/expiry

This template demonstrates an expiry pattern with read-only views and explicit cleanup:

1. Store an `expires_at` (`u64`) timestamp alongside the data.
2. Provide an `is_expired` read view for indexers and clients.
3. Validate on write that a nonzero expiry is strictly later than the current ledger timestamp.
4. In `get_` and `has_` methods, check expiry without modifying storage. Have a separate, successful `purge_expired` invocation remove expired data and emit an event. Do not delete and then panic: the panic reverts the deletion along with the rest of the invocation.

### `get_admin() -> Address`

Return the current admin address through the `Governable` interface. Returns the typed `NotInitialized` error if the contract has not been initialised. `set_admin` is also exported through `Governable` and requires current-admin authorization.

## Events

| Event            | Topic key               | Data  | Emitted when                           |
| ---------------- | ----------------------- | ----- | -------------------------------------- |
| `initialized`    | `initialized`           | admin | Contract is initialised                |
| `record_set`     | `record_set, owner`     | value | A record is created or updated         |
| `record_removed` | `record_removed, owner` | `()`  | A record is deleted                    |
| `record_purged`  | `record_purged, owner`  | `()`  | An expired record is explicitly purged |

## Security

- Double initialisation is rejected (`"already initialized"`).
- `initialize` requires the proposed admin's `require_auth()`; admin mutations require current-admin authorization.
- `purge_expired` is intentionally permissionless and can only delete already-expired data.
- Reads are side-effect-free; expired records remain stored until explicitly purged or overwritten.
- Missing/uninitialized state and expiry violations use typed contract errors.

## Using this template

1. Copy `contracts/templates/` to `contracts/<your_contract>/`.
2. Rename the package in `Cargo.toml` and update the workspace `members` list in the root `Cargo.toml`.
3. Rename `TemplateContract` and `TemplateContractClient` throughout.
4. Replace `DataKey`, `Record`, and entry points with your contract's logic.
5. Keep the test harness structure — add tests for every new entry point.

## Build & test

```bash
# Native test build
cargo test -p templates

# WASM build
cargo build --target wasm32v1-none --release -p templates
```
