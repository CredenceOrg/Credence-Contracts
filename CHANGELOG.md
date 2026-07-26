# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Lease scope guard** (`require_matching_lease_scope`): defence-in-depth check that a lease's scope bitmask covers the requested operation. Adds `Lease` / `lease_op` primitives and typed `LeaseScopeMismatch` / `LeaseExpired` errors in `credence_errors` (Closes #847).
- **Expired-lease guard tests**: lock Fresh / Expiring soon / Expired behaviour for `require_no_expired_lease` (Closes #845).

### Fixed

- **Distinguish permission errors from state errors in `is_recoverable()`** (closes #912).
  `credence_errors::ContractError::is_recoverable()` had three enum variants
  (`TimestampInFuture`, `DuplicateIdempotencyKey`, `DelegationInactive`) each
  matched twice across the `match`; Rust's first-arm-wins semantics silently
  discarded the later, correct arm. `DelegationInactive` — a terminal *state*
  (delegation already revoked/expired) that no retry can fix — was wrongly
  reported as recoverable, indistinguishable from genuine *permission* errors
  like `NotSigner`/`NotAdmin` that a caller resolves by switching signer/role.
  Off-chain consumers using `is_recoverable()` to choose a WARN (recoverable)
  vs. ERROR (fatal) log level would under-alert on inactive delegations.

- **No-dynamic-strings in production contract code** (closes #713). New workspace
  `clippy.toml` declares `disallowed-macros` for `format`, `format_args`, `write`,
  `writeln` (and their `std::` / `alloc::` / `core::` qualified forms); every
  contract crate now denies `clippy::disallowed_macros` under `cfg_attr(not(any(test,
  feature = "testutils")), ...)` so the lint fires for `cargo build --release` and
  the WASM build but stays silent during `cargo test` and `cargo build
  --features testutils`. Production contract code now requires
  `soroban_sdk::Symbol::new(&e, "fixed")` for on-chain event topics and revert
  surfaces; see `docs/no-dynamic-strings.md` for the threat model and migration
  table. (Closes #713.)

- Tighten storage TTL bumps across all contracts to prevent silent archival of hot-path data (closes #570). Adds `bump_instance_ttl` to every public entrypoint in `credence_registry`, `admin`, `credence_treasury`, `arbitration`, `credence_multisig`, `timelock`, and `credence_delegation`; adds `extend_ttl` after every persistent write (and on reads) in `credence_bond` slash history, emergency audit trail, and claims modules.

### Added

- **Timelock Timeout Test**: Added explicit timeout regression coverage for time-locked operation execution after the grace period (`timelock`).
- **Pause Signer Invariant**: Added invariant test for PauseSignerCount to prevent drift (`credence_delegation`).
- **Slash Bond Core**: Implemented admin-only `slash_bond` functionality with partial/full slashing and event emission.
- **Treasury Guardrails**: Added comprehensive tests and functionality for liquidity floor and slippage protection mechanisms in treasury withdrawals (`credence_treasury`).
- **Batch Bond Atomicity**: Enhanced batch operations with explicit empty batch handling and `MAX_BATCH_BOND_SIZE` enforcement (`credence_bond`).
- **Corridor-Gated Settlement** (closes #911): Added an admin-managed corridor allowlist (`register_corridor` / `remove_corridor` / `is_corridor_registered`) and a new `settle` entrypoint to `credence_treasury` that rejects settlement to any unregistered destination with the new `CorridorNotRegistered` (611) error.

### Changed

- **SafeERC20 Migration**: Replaced direct `TokenClient` calls with safe wrapper functions to support non-compliant ERC20 tokens across the protocol.
- **Protocol Fixes**: Resolved compilation errors, completed `top_up` and `extend_duration` with overflow protection.
- **Event Indexing**: Migrated lifecycle events to V2 for optimized off-chain indexing.
