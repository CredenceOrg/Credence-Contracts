# `testutils` Feature Gate

## Overview

Test-only helpers that were previously compiled unconditionally (and therefore
included in every release WASM binary) are now behind a Cargo feature flag:

```toml
# Cargo.toml — credence_bond
[features]
testutils = ["soroban-sdk/testutils"]
```

The gate used throughout the source code is:

```rust
#[cfg(any(test, feature = "testutils"))]
```

This means the helpers are compiled in two situations:
1. `cargo test` — Rust's built-in test flag always includes them.
2. `--features testutils` — explicit opt-in for off-chain tooling, integration
   harnesses, and benchmarks that link the crate as an `rlib`.

The release WASM (`cargo build --target wasm32-unknown-unknown --release`)
includes **neither** because it uses no `--features` flag and does not run
`cargo test`.

---

## What is gated

| Location | Item | Reason |
|---|---|---|
| `nonce.rs` | `get_grace_window`, `require_not_expired`, `validate_and_consume`, `validate_and_consume_with_grace` | Only used in nonce-expiry tests |
| `rolling_bond.rs` | `can_withdraw_after_notice`, `period_end` | Convenience predicates; test-only |
| `same_ledger_liquidation_guard.rs` | `record_collateral_increase` | Called only from `batch.rs` (also gated) |
| `slash_history.rs` | `get_slash_history`, `get_slash_record`, `get_total_slashed_from_history` | Full-scan helpers; tests only |
| `batch.rs` (whole module) | All of `batch::*` | Entire module is `#![allow(dead_code)]`; never called from production |
| `lib.rs` | `liquidation_reason` pub mod | Doc comment says "so test code can refer to canonical strings" |
| `lib.rs` | `test_access_control` pub mod | Test module accidentally left ungated |

### Items NOT gated (remain in release WASM)

| Location | Item | Reason |
|---|---|---|
| `slash_history.rs` | `append_slash_history` | Called by production `slashing.rs` |
| `slash_history.rs` | `get_slash_count` | Called by `get_slash_history_page` entry-point |
| `slash_history.rs` | `get_slash_history_page` | Public contract entry-point (paginated read) |
| `claims.rs` | All of `claims::*` | `expire_claims_bounded` is a live contract entry-point |
| `nonce.rs` | `get_nonce`, `consume_nonce`, `require_domain_match` | Production auth paths |
| `rolling_bond.rs` | `is_period_ended`, `apply_renewal` | Production bond renewal path |

---

## Dependency changes

```toml
# Before — soroban-sdk testutils always on, proptest in [dependencies]
[dependencies]
soroban-sdk = { version = "22.0", features = ["testutils"] }
proptest = "1.11.0"

# After — testutils only when needed
[dependencies]
soroban-sdk = { version = "22.0" }          # no testutils in release

[dev-dependencies]
proptest = "1.11.0"                          # only compiled for tests
soroban-sdk = { version = "22.0", features = ["testutils"] }

[features]
testutils = ["soroban-sdk/testutils"]        # off-chain harnesses opt in
gas-bench = ["soroban-sdk/testutils"]        # benchmark harness (unchanged)
```

---

## Using the feature in downstream crates

Off-chain tools (CLIs, integration harnesses, benchmarks) that link
`credence_bond` as an `rlib` and need test-only helpers:

```toml
# In downstream Cargo.toml
[dev-dependencies]
credence_bond = { path = "...", features = ["testutils"] }
```

Or for a binary that always needs them:

```toml
[dependencies]
credence_bond = { path = "...", features = ["testutils"] }
```

**Never** enable the `testutils` feature when building the on-chain WASM:

```bash
# Correct — no testutils in WASM
cargo build --target wasm32-unknown-unknown --release -p credence_bond

# Wrong — would bloat the WASM with test infrastructure
cargo build --target wasm32-unknown-unknown --release -p credence_bond --features testutils
```

---

## See Also

- `contracts/credence_bond/Cargo.toml` — feature definitions
- `contracts/credence_bond/src/slash_history.rs` — `pub mod testutils` pattern
- `contracts/credence_bond/src/rolling_bond.rs` — `pub mod testutils` pattern
- `contracts/credence_bond/src/nonce.rs` — `mod testutils_helpers` pattern
