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

## Accrual helper template

[`contracts/templates/accrual_helper_template.rs`](../contracts/templates/accrual_helper_template.rs)
is a copyable pseudocode pattern for idempotently accruing interest to the
current ledger timestamp. It outlines the timestamp check, checked interest
math, debt and reserve updates, and the final timestamp write. The file is kept
outside `src/`, so it is not compiled as part of the `templates` crate; its
placeholder module declaration and commented code must be adapted before use.

To adopt it:

1. Copy the helper into your contract as `src/accrual.rs` and declare the module
   from `src/lib.rs` (`mod accrual;` or `pub mod accrual;`, depending on your API).
2. Remove the template's placeholder `pub module accrual_helper_template` line
   and replace the pseudocode with your contract's storage access, error handling,
   interest model, and checked-arithmetic helpers.
3. Add the keys used by your implementation to your `DataKey` enum. The example
   refers to `LastAccrualTimestamp` (`u64`), `InterestRatePerSecond` (`i128`),
   `TotalDebt` (`i128`), and optionally `TotalReserves` (`i128`) when interest
   fees are tracked. Choose appropriate defaults and initialization rules for
   each key; these names are examples, not keys exported by the template crate.
4. Call `ensure_accrued(&e)` at the start of each borrow or repay path that must
   use fresh debt state, before reading or changing principal.

## Accrual test templates

[`contracts/templates/accrual_tests_template.rs`](../contracts/templates/accrual_tests_template.rs)
contains skeleton tests for borrow-triggered accrual, repay-triggered accrual,
and avoiding duplicate accrual in one operation. It deliberately leaves the
fixture, client methods, storage readers, rate configuration, and expected math
as comments because those details depend on the adopting contract.

To adopt it, copy the relevant tests into `src/test_accrual.rs` and include that
module under `#[cfg(test)]`, or place them in an integration test under `tests/`.
Replace `YourLendingClient`, `setup`, and the commented operations with the real
contract API, and calculate expectations with the same documented rounding and
checked-arithmetic rules as production. The tests use the same example keys as
the helper: `LastAccrualTimestamp`, `InterestRatePerSecond`, `TotalDebt`, and
optionally `TotalReserves`. Add a test-only accrual counter only if needed for
the no-double-accrual case; it is not a required production storage key.

## Build & test

```bash
# Native test build
cargo test -p templates

# WASM build
cargo build --target wasm32v1-none --release -p templates
```
