# Contract Template

Canonical Soroban contract template for the Credence workspace. Copy this crate when starting a new contract to get the correct structure, patterns, and test harness out of the box.

## Overview

`TemplateContract` is a minimal but complete Soroban contract that demonstrates every pattern required by Credence contracts:

- Admin-gated initialisation with a double-init guard
- Typed `DataKey` enum for all storage slots
- `#[contracttype]` structs for on-chain data
- `require_auth()` on every mutating entry point
- `Symbol`-keyed event emission
- Ledger-timestamp recording on writes
- `#![no_std]` + `soroban_sdk` imports

## Types

### `DataKey`

| Variant          | Description                              |
|------------------|------------------------------------------|
| `Admin`          | Stores the contract administrator        |
| `Record(Address)`| Stores a `Record` keyed by owner address |

### `Record`

| Field        | Type  | Description                                  |
|--------------|-------|----------------------------------------------|
| `value`      | i128  | Arbitrary numeric value set by the admin     |
| `updated_at` | u64   | Ledger timestamp of the last write           |

## Contract Functions

### `initialize(admin: Address)`

Set the contract admin. Panics with `"already initialized"` if called more than once. Emits an `initialized` event.

### `set_record(owner: Address, value: i128)`

Store or overwrite a `Record` for `owner`. Requires admin authorization. Records the current ledger timestamp in `updated_at`. Emits a `record_set` event.

### `remove_record(owner: Address)`

Delete the record for `owner`. Requires admin authorization. No-op if the record does not exist. Emits a `record_removed` event.

### `get_record(owner: Address) -> Record`

Return the record for `owner`. Panics with `"record not found"` if none exists.

### `has_record(owner: Address) -> bool`

Return `true` if a record exists for `owner`, `false` otherwise.

### `get_admin() -> Address`

Return the current admin address. Panics with `"not initialized"` if the contract has not been initialised.

## Events

| Event            | Topic key          | Data    | Emitted when                    |
|------------------|--------------------|---------|---------------------------------|
| `initialized`    | `initialized`      | admin   | Contract is initialised         |
| `record_set`     | `record_set, owner`| value   | A record is created or updated  |
| `record_removed` | `record_removed, owner` | `()` | A record is deleted            |

## Security

- Double initialisation is rejected (`"already initialized"`).
- All mutating entry points require admin `require_auth()`.
- `get_record` panics rather than returning a default, preventing silent misuse.
- `get_admin` panics on uninitialised contracts, preventing silent misuse.

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
cargo build --target wasm32-unknown-unknown --release -p templates
```
