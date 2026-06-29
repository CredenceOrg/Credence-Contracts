# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Tighten storage TTL bumps across all contracts to prevent silent archival of hot-path data (closes #570). Adds `bump_instance_ttl` to every public entrypoint in `credence_registry`, `admin`, `credence_treasury`, `arbitration`, `credence_multisig`, `timelock`, and `credence_delegation`; adds `extend_ttl` after every persistent write (and on reads) in `credence_bond` slash history, emergency audit trail, and claims modules.

### Added

- **Timelock Timeout Test**: Added explicit timeout regression coverage for time-locked operation execution after the grace period (`timelock`).
- **Pause Signer Invariant**: Added invariant test for PauseSignerCount to prevent drift (`credence_delegation`).
- **Slash Bond Core**: Implemented admin-only `slash_bond` functionality with partial/full slashing and event emission.
- **Treasury Guardrails**: Added comprehensive tests and functionality for liquidity floor and slippage protection mechanisms in treasury withdrawals (`credence_treasury`).
- **Batch Bond Atomicity**: Enhanced batch operations with explicit empty batch handling and `MAX_BATCH_BOND_SIZE` enforcement (`credence_bond`).

### Changed

- **SafeERC20 Migration**: Replaced direct `TokenClient` calls with safe wrapper functions to support non-compliant ERC20 tokens across the protocol.
- **Protocol Fixes**: Resolved compilation errors, completed `top_up` and `extend_duration` with overflow protection.
- **Event Indexing**: Migrated lifecycle events to V2 for optimized off-chain indexing.
