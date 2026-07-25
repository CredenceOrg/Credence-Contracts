# Add workspace-level SIGNATURE_DOMAIN uniqueness test

## Summary

A workspace-level integration test loading the `SIGNATURE_DOMAIN` constants from every contract crate and asserting they are all unique. This locks the contract in place, documents expected behaviour through executable examples, and shortens review cycles when signature domain code changes.

## Background

Test coverage in the signature-domain area was thin: `SIGNATURE_DOMAIN` constants were added in issue #751 but no regression guard existed to prevent duplicates or missing definitions. A workspace-level test parsing the source constants and asserting uniqueness fills that gap.

## Changes

### 1. Added `SIGNATURE_DOMAIN` to contracts that were missing it

Four contract crates defined in the workspace Cargo.toml did not yet carry a `SIGNATURE_DOMAIN` constant:

| Crate | Domain |
|---|---|
| `credence_registry` | `"CredenceRegistry"` |
| `credence_treasury` | `"CredenceTreasury"` |
| `credence_multisig` | `"CredenceMultisig"` |
| `timelock` | `"Timelock"` |

Each constant is accompanied by the same rustdoc block used in the existing contracts (`credence_bond`, `admin`, `arbitration`, `credence_delegation`) explaining the cross-contract replay attack mitigation rationale.

### 2. Created workspace integration test

File: `tests/signature_domains_unique.rs`

The test `signature_domains_are_unique_across_contracts`:

- **Loads** every `SIGNATURE_DOMAIN` constant by parsing each contract's `src/lib.rs` (and `src/domain.rs` for `credence_delegation`)
- **Asserts presence**: every contract crate in `CONTRACT_CRATES` must define exactly one domain value
- **Asserts uniqueness**: no two contracts may share the same domain string (cross-contract replay risk)
- **Handles multi-file definitions**: `credence_delegation` defines the same domain in both `lib.rs` and `domain.rs`; the test deduplicates within a crate and only flags *different* values across files in the same crate
- **Clean failure messages**: reports which crates are missing and which values duplicate

### 3. Enabled root integration tests

Added a `[package]` section to the workspace root `Cargo.toml` so that integration tests in the `tests/` directory are compiled and run by `cargo test --workspace` / `cargo test --all-targets`. The root package has no library or binary target — only the integration test binary.

## Threat model

### Attack scenario: cross-contract signature replay

Without domain separation, a signature created for contract A could be replayed against contract B if both contracts share the same nonce namespace and similar signature verification logic. The attack is possible when:

1. **Shared nonce namespace**: multiple contracts use similar nonce tracking mechanisms
2. **Similar signature verification**: contracts implement comparable signature validation
3. **Missing domain binding**: signatures are not explicitly bound to a specific contract

### Impact

- **Unauthorized operations**: execute privileged operations in a contract the attacker shouldn't have access to
- **Privilege escalation**: use a signature from a lower-privilege contract to access higher-privilege functions
- **State corruption**: cause unintended state changes by replaying operations in different contexts

### Mitigation

By ensuring every contract defines a unique `SIGNATURE_DOMAIN`, we establish the foundation for future signature domain integration. When signatures include these domain identifiers in their payload hash, they become cryptographically bound to their intended contract.

## Test details

- **Test file**: `tests/signature_domains_unique.rs` (no external crate dependencies, uses only `std`)
- **Test function**: `signature_domains_are_unique_across_contracts`
- **Contracts checked**: `credence_bond`, `credence_delegation`, `credence_registry`, `credence_treasury`, `credence_multisig`, `timelock`, `arbitration`, `admin`
- **Happy path**: all 8 contracts define a unique domain string → passes
- **Sad path 1 (missing)**: a contract is missing `SIGNATURE_DOMAIN` → fails with list of missing crates
- **Sad path 2 (duplicate)**: two contracts share the same domain → fails with list of duplicates
- **Sad path 3 (internal mismatch)**: a crate defines different values across files → fails with details

## Verification

```bash
# Build for WASM (no regressions)
cargo build --target wasm32-unknown-unknown --release

# Run workspace tests (includes the new test)
cargo test --workspace

# Run all targets (includes integration tests)
cargo test --all-targets

# Lint
cargo clippy --workspace --all-targets -- -D warnings
```

## Backwards compatibility

This change is **fully backwards compatible**:

- Existing `SIGNATURE_DOMAIN` constants unchanged
- No changes to function signatures, storage layout, error codes, or validation logic
- No impact on existing contract behaviour
- New constants are private and dead-code-allow'd, same as the existing ones

closes #752