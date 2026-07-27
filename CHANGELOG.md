# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Lease scope guard** (`require_matching_lease_scope`): defence-in-depth check that a lease's scope bitmask covers the requested operation. Adds `Lease` / `lease_op` primitives and typed `LeaseScopeMismatch` / `LeaseExpired` errors in `credence_errors` (Closes #847).
- **Expired-lease guard tests**: lock Fresh / Expiring soon / Expired behaviour for `require_no_expired_lease` (Closes #845).

### Fixed

- **Arbitration dispute guard**: Added a typed `ArbitrationError::OngoingDispute` guard to reject new disputes while a creator already has an unresolved dispute in progress, closing a defense-in-depth re-entry gap in the arbitration lifecycle. (Closes #850.)

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

- **`credence_errors` discriminant & match coverage**: Resolved a build-breaking
  duplicate discriminant between `NoPendingAdmin = 118` and the recently-added
  `TimestampInFuture = 118`.  `TimestampInFuture` was moved into the Delegation
  error block (new code `513`) and the `ErrorCategory::category()` and
  `ErrorExt::is_recoverable()` match arms were updated accordingly.  The wire-
  stability assertion in `tests/error_codes_wire.rs` was bumped from `118` to
  `513`.  These changes are safe because no contract could have been deployed
  with the broken `118` value (the workspace did not compile prior to this fix).

- **Missing `require_contract_uninitialized` helper**: Added
  `pub fn require_contract_uninitialized(e: &Env, already_initialized: bool)` to
  `credence_errors` so the 8 call sites in `timelock`, `credence_delegation`,
  `credence_bond`, `admin`, `credence_registry`, `credence_treasury`,
  `credence_multisig`, and `templates` that reference it via
  `credence_errors::require_contract_uninitialized` resolve.  The helper is the
  reference implementation tested by `credence_errors::test_errors`.

- **`verify_no_future_ledger` invocation in `domain.rs:249`**: Replaced the
  function-style call (which the macro form requires `Result`-returning
  contexts to satisfy) with an inline `panic_with_error!` so
  `check_payload_age` continues to use its existing panic-based guard.

- **`DelegatedActionPayload` missing `signature_domain` field**: Added the
  required `signature_domain: String::from_str(&e, "CredenceDelegation")` to the
  initializers in `contracts/credence_delegation/src/test_payload_staleness.rs`
  and `contracts/credence_delegation/tests/nonce_replay.rs` so they match the
  post-#906 struct layout.

- **`credence_bond` duplicate declarations**: Removed duplicate
  `mod storage;`, `mod idempotency;` declarations and 8 duplicate `DataKey`
  variants (`Paused`, `PauseSigner`, `PauseSignerCount`, `PauseThreshold`,
  `PauseProposalCounter`, `PauseApproval`, `PauseApprovalCount`,
  `PauseProposal`).  The new `IdempotencyKey(Bytes)` and `BorrowFrozen` variants
  are kept.

### Added

- **Timelock Timeout Test**: Added explicit timeout regression coverage for time-locked operation execution after the grace period (`timelock`).
- **Pause Signer Invariant**: Added invariant test for PauseSignerCount to prevent drift (`credence_delegation`).
- **Slash Bond Core**: Implemented admin-only `slash_bond` functionality with partial/full slashing and event emission.
- **Treasury Guardrails**: Added comprehensive tests and functionality for liquidity floor and slippage protection mechanisms in treasury withdrawals (`credence_treasury`).
- **Batch Bond Atomicity**: Enhanced batch operations with explicit empty batch handling and `MAX_BATCH_BOND_SIZE` enforcement (`credence_bond`).
- **Lease-Signature Helper Tests**: Added Valid / Corrupted / Revoked scenario
  coverage for the `verify_delegated_signature` lease-signature helper in
  `contracts/credence_delegation/src/test_lease_signature.rs` (closes #854).
  Coverage includes Ed25519 success path, Secp256r1 / MLDSA44 with registered
  accepting / rejecting verifiers, unknown-scheme rejection (codes 99, 255),
  verifier-revocation scenarios (never-registered, register-then-unregister),
  garbled-bytes corruption cases (zero, all-ones), and a panicking-verifier
  guard. The new module is wired into `lib.rs` under the existing
  `#[cfg(test)] mod test_*` block.

### Changed

- **SafeERC20 Migration**: Replaced direct `TokenClient` calls with safe wrapper functions to support non-compliant ERC20 tokens across the protocol.
- **Protocol Fixes**: Resolved compilation errors, completed `top_up` and `extend_duration` with overflow protection.
- **Event Indexing**: Migrated lifecycle events to V2 for optimized off-chain indexing.