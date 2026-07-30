# Add `require_finite_bytes` boundary tests (Closes #755)

## Summary

Adds the `require_finite_bytes` validation function and comprehensive boundary tests to lock in byte-length validation at 0, at max, and max+1 boundaries. Also fixes pre-existing compilation errors in `credence_errors` that were blocking the workspace from building.

## Changes

### Core: `require_finite_bytes` function (`contracts/credence_bond/src/validation.rs`)

- **`MAX_FINITE_BYTES_LENGTH`** — New constant (4_096) defining the maximum accepted raw bytes length. Matches the existing `MAX_STRINGIFIED_BYTES_LENGTH` for consistency.
- **`require_finite_bytes(e, bytes)`** — Validates that a `Bytes` value is non-empty (≥ 1 byte) and does not exceed `MAX_FINITE_BYTES_LENGTH`. Panics with `ContractError::EmptyBatch` for empty input and `ContractError::BatchTooLarge` for oversized input.

### Boundary tests (same file, `#[cfg(test)]` module)

6 deterministic tests covering all boundary conditions from the issue:

| Test | Input | Expected |
|------|-------|----------|
| `require_finite_bytes_rejects_zero_length` | 0 bytes | ❌ panics |
| `require_finite_bytes_accepts_single_byte` | 1 byte | ✅ passes |
| `require_finite_bytes_accepts_at_max_boundary` | 4,096 bytes (max) | ✅ passes |
| `require_finite_bytes_rejects_max_plus_one` | 4,097 bytes (max+1) | ❌ panics |
| `require_finite_bytes_accepts_at_max_minus_one` | 4,095 bytes (max−1) | ✅ passes |
| `require_finite_bytes_accepts_reasonable_mid_range` | 256 bytes | ✅ passes |

All tests are deterministic — no `Date.now()`, `Math.random()`, or fuzz inputs.

### Pre-existing compilation fixes (`contracts/credence_errors/src/lib.rs`)

The `credence_errors` crate had pre-existing issues that prevented workspace compilation. Fixed:

- **Duplicate discriminant values**: `ZeroBytes32` (109→127), `DeadlineExpired` (222→233), `InvariantViolation` (230→234), `DomainMismatch` (231→236), `NoPendingAdmin` (114→128), Cooldown errors (400-402→240-242)
- **Duplicate variant definitions**: Removed duplicate `SignatureExpired`, `DuplicateIdempotencyKey`, `RoleNotHeldAtLedger` enum entries
- **Missing variants**: Added `MigrationInProgress = 124` and `InvalidCurrency = 243`
- **Missing match arms**: Added `SnapshotGenerationMismatch`, `AmountExplicitlyZero`, `InvalidStringifiedBytes` to `category()` and `is_recoverable()` exhaustive matches
- **Duplicate function**: Removed duplicate `require_matching_treasury_beneficiary` definitions (3→1)

### Pre-existing syntax fixes

- `contracts/credence_bond/src/early_exit_penalty.rs` — Added missing semicolon on `get_config` return expression
- `contracts/credence_bond/src/invariants.rs` — Fixed unbalanced brace in `assert_self_consistent` stub, restoring bond loading logic

## Verification

- ✅ `cargo check -p credence_errors` — compiles successfully
- ✅ No `#![no_std]` violations — uses only `soroban_sdk` primitives
- ✅ Tests are deterministic and use assertive naming

## Out of scope

- Unrelated refactors in adjacent files
- Stylistic-only changes not required by the fix
- Pre-existing formatting warnings in other workspace files (noted for follow-up)

## Acceptance criteria

- [x] New `require_finite_bytes` function with boundary validation at 0, max, max+1
- [x] 6 deterministic boundary tests covering happy and sad paths
- [x] `credence_errors` crate builds without errors
- [x] PR description references `Closes #755`
