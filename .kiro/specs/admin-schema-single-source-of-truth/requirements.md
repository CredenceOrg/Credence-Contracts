# Requirements Document

## Introduction

The `contracts/admin` crate defines the Credence admin schema: the `AdminRole` enum, the `AdminInfo` struct, the `DataKey` storage-key enum, and all admin-related event topic strings. Today those definitions and their string literals are duplicated in several other places (`credence_bond/src/events.rs`, multiple `pausable.rs` copies, schema-verification tests) so a single change to the schema requires edits in several files. This feature centralises every admin schema artefact so there is exactly one authoritative location to edit when the schema changes.

## Glossary

- **Admin_Crate**: The `contracts/admin` Rust crate, the canonical home of all admin schema types and constants.
- **AdminRole**: The `#[contracttype]` enum (`SuperAdmin`, `Admin`, `Operator`) defined in the Admin_Crate.
- **AdminInfo**: The `#[contracttype]` struct holding per-admin metadata defined in the Admin_Crate.
- **DataKey**: The `#[contracttype]` enum of ledger storage keys defined in the Admin_Crate.
- **Event_Topic_Constant**: A `const &str` (or equivalent Soroban `Symbol`-compatible constant) that names an on-chain event topic emitted by the Admin_Crate.
- **Downstream_Crate**: Any crate in the workspace that `use`s or duplicates admin schema items — currently `contracts/credence_bond` (events.rs) and any `pausable.rs` copy that duplicates admin event topics.
- **Schema_Test**: A test that verifies the XDR encoding or payload shape of an admin-contract event (e.g. `test_events_schema.rs`, `datakey_fingerprint.rs`).
- **WASM_Target**: The `wasm32-unknown-unknown` compilation target required for all deployed Soroban contracts.
- **Public_API**: The set of types, constants, and functions that the Admin_Crate exports with `pub` visibility.
- **Backwards_Compatible**: A change that does not alter the wire encoding of any existing type or the byte value of any existing event topic string, so existing on-chain data and indexers continue to work without migration.

---

## Requirements

### Requirement 1: Single Definition of AdminRole and AdminInfo

**User Story:** As a downstream engineer, I want `AdminRole` and `AdminInfo` to be exported from exactly one location, so that changing the admin role hierarchy requires editing only the Admin_Crate.

#### Acceptance Criteria

1. THE Admin_Crate SHALL export `AdminRole` and `AdminInfo` with `pub` visibility so every other crate in the workspace can import them without redefining them.
2. WHEN any Downstream_Crate needs `AdminRole` or `AdminInfo`, THE Downstream_Crate SHALL import them from the Admin_Crate rather than declaring its own equivalent types.
3. THE Admin_Crate SHALL NOT contain more than one definition of `AdminRole` and more than one definition of `AdminInfo`.
4. WHEN `cargo build --target wasm32-unknown-unknown --release` is run against the Admin_Crate, THE WASM_Target build SHALL complete without errors.

---

### Requirement 2: Public DataKey Enum

**User Story:** As an operator tool author, I want the `DataKey` storage-key enum to be publicly accessible from the Admin_Crate, so that integration tests and monitoring tools can construct expected storage keys without duplicating the enum.

#### Acceptance Criteria

1. THE Admin_Crate SHALL declare `DataKey` with `pub` visibility.
2. WHEN the `datakey_fingerprint` integration test imports `DataKey` from the Admin_Crate, THE test SHALL compile and pass without an `#[allow(unused_imports)]` or visibility workaround.
3. THE Admin_Crate SHALL ensure that making `DataKey` public does not alter the XDR encoding of any existing variant (i.e. the `datakey_fingerprints_are_pinned` snapshot SHALL remain unchanged).
4. WHEN `cargo clippy --workspace --all-targets -- -D warnings` is run, THE workspace SHALL produce zero warnings related to `DataKey` visibility.

---

### Requirement 3: Centralised Admin Event Topic Constants

**User Story:** As a frontend engineer or indexer author, I want all admin event topic strings defined as named constants in one place, so that renaming a topic causes a compile error everywhere it is used rather than a silent divergence.

#### Acceptance Criteria

1. THE Admin_Crate SHALL define a `pub` constant for each distinct event topic string it emits (e.g. `TOPIC_ADMIN_INITIALIZED`, `TOPIC_ADMIN_ADDED`, `TOPIC_ADMIN_REMOVED`, `TOPIC_ADMIN_ROLE_UPDATED`, `TOPIC_ADMIN_DEACTIVATED`, `TOPIC_ADMIN_REACTIVATED`, `TOPIC_ADMIN_SUSPENDED`, `TOPIC_ADMIN_ROTATED`, `TOPIC_OWNERSHIP_TRANSFER_INITIATED`, `TOPIC_OWNERSHIP_TRANSFER_ACCEPTED`, `TOPIC_ROLE_ASSIGNED`, `TOPIC_ROLE_REVOKED`).
2. WHEN the Admin_Crate emits an event, THE Admin_Crate SHALL reference the corresponding Event_Topic_Constant rather than an inline string literal.
3. WHEN a Downstream_Crate emits or matches an event topic that originates in the admin schema (e.g. `"admin_rotated"` in `credence_bond/src/events.rs`), THE Downstream_Crate SHALL reference the Admin_Crate's exported Event_Topic_Constant rather than a duplicated string literal.
4. THE Admin_Crate SHALL maintain the exact byte value of every Event_Topic_Constant so that Backwards_Compatible behaviour is preserved for existing on-chain indexers.
5. WHEN `cargo test -p admin` is run, THE Schema_Test suite SHALL pass, confirming every event topic constant produces the expected XDR-encoded payload shape.

---

### Requirement 4: Pausable Event Topics Not Duplicated

**User Story:** As a contract maintainer, I want the pause-mechanism event topic strings (`pause_signer_set`, `pause_threshold_set`, `pause_approved`, `paused`, `unpaused`, `pause_proposed`) defined once, so that a rename does not require editing every contract's `pausable.rs` independently.

#### Acceptance Criteria

1. THE Admin_Crate SHALL define `pub` constants for the pause event topics it owns: `TOPIC_PAUSED`, `TOPIC_UNPAUSED`, `TOPIC_PAUSE_PROPOSED`, `TOPIC_PAUSE_APPROVED`, `TOPIC_PAUSE_SIGNER_SET`, `TOPIC_PAUSE_THRESHOLD_SET`.
2. WHEN the Admin_Crate's `pausable.rs` emits a pause event, THE Admin_Crate SHALL reference the corresponding constant rather than an inline string literal.
3. WHERE a Downstream_Crate's `pausable.rs` duplicates admin pause event topic strings that are semantically identical to those in the Admin_Crate, THE Downstream_Crate SHALL import and reuse the Admin_Crate's constants.
4. IF the Admin_Crate is not yet a dependency of a Downstream_Crate that needs these constants, THEN THE Downstream_Crate's `Cargo.toml` SHALL be updated to add the Admin_Crate as a dependency before the constants are referenced.
5. THE Admin_Crate constants SHALL preserve the exact string value of each pause event topic to maintain Backwards_Compatible on-chain behaviour.

---

### Requirement 5: Schema Test Coverage Locked to Constants

**User Story:** As a CI engineer, I want the admin event schema tests to reference the exported constants, so that a constant value change is automatically detected by the test suite rather than requiring manual inspection of string literals.

#### Acceptance Criteria

1. WHEN `test_events_schema.rs` constructs a `Symbol` for an admin event topic, THE test SHALL use the corresponding Admin_Crate Event_Topic_Constant rather than an inline string.
2. THE `datakey_fingerprint.rs` integration test SHALL import `DataKey` via the Admin_Crate's public API.
3. WHEN `cargo test -p admin` is run, ALL Schema_Tests SHALL pass.
4. IF an Event_Topic_Constant value is changed in the Admin_Crate, THEN THE Schema_Test suite SHALL fail fast on the payload-shape assertion, making the breaking change visible before merge.

---

### Requirement 6: Backwards-Compatible Public API

**User Story:** As a downstream contract author, I want the refactoring to preserve the existing public API surface, so that I do not need to update call sites beyond replacing duplicated literals with imported constants.

#### Acceptance Criteria

1. THE Admin_Crate SHALL retain all existing `pub fn` entrypoints (`initialize`, `add_admin`, `remove_admin`, `update_admin_role`, `deactivate_admin`, `reactivate_admin`, `suspend_admin`, `transfer_ownership`, `accept_ownership`, `get_owner`, `get_pending_owner`, `get_admin_info`, `get_admin_role`, `is_admin`, `has_role_at_least`, `check_role_at_ledger`, `get_all_admins`, `get_admins_by_role`, `get_admin_count`, `get_active_admin_count`, `get_config`, `get_role`, `get_required_role_to_assign`, `version`) with unchanged signatures.
2. THE Admin_Crate SHALL NOT change the discriminant values of `AdminRole` (`SuperAdmin = 3`, `Admin = 2`, `Operator = 1`) or the field layout of `AdminInfo`.
3. WHEN `cargo test -p admin` is run after the refactoring, THE existing test suite SHALL pass with no regressions.
4. IF a breaking change to the public API is unavoidable, THEN THE implementation SHALL document the required migration step in a `MIGRATION.md` file at the workspace root.

---

### Requirement 7: Documentation Updated at Point of Observation

**User Story:** As a developer integrating the Admin_Crate, I want the relevant documentation updated in the same PR, so that I can understand the canonical import path for admin schema types without reading the source.

#### Acceptance Criteria

1. THE Admin_Crate's inline Rust doc-comments for `AdminRole`, `AdminInfo`, `DataKey`, and each Event_Topic_Constant SHALL describe the canonical re-export path and state that these are the single source of truth.
2. WHERE a `docs/` page or `README.md` exists for the Admin_Crate or for any Downstream_Crate that is updated as part of this feature, THEN THE documentation SHALL be updated in the same commit to reflect the new import path.
3. THE pull request that lands this feature SHALL reference the originating issue with `Closes #<issue-number>` in its description.

---

### Requirement 8: Lint and Type-Check Pass

**User Story:** As a CI engineer, I want the workspace to pass all existing checks after the refactoring, so that no regressions are introduced.

#### Acceptance Criteria

1. WHEN `cargo clippy --workspace --all-targets -- -D warnings` is run against the workspace after the change, THE clippy check SHALL produce zero new warnings.
2. WHEN `cargo build --target wasm32-unknown-unknown --release` is run for each affected contract crate, THE WASM_Target build SHALL succeed.
3. WHEN `cargo test -p admin` is run, THE test suite SHALL report zero failures.
4. IF the `#![no_std]` constraint is in force for any affected crate, THEN THE refactored code SHALL not introduce any `std::` call or dependency, using `soroban_sdk` primitives exclusively.
