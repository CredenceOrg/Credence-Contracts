# How the Crates Fit Together

This document maps every Crate in the Credence workspace, its upstream dependencies, and why it exists. It is written for contributors who need to understand the dependency graph before adding or modifying a contract.

See also: [architecture.md](architecture.md) for per-crate internals and [cross-contract-call-graph.md](cross-contract-call-graph.md) for runtime call paths.

---

## Dependency Graph

```
credence_errors  ◄──── shared by all other crates
   ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲
   │    │    │    │    │    │    │    │    │    │    │    │
credence_math  testutils  credence_admin_cli  (no Credence deps)
      │                │
      │           ┌────┴─────────────────────────────────────────────┐
      │           │  contracts/* (all contracts depend on credence_errors)   │
      │           │  ┌─────────────────────────────────────────────────┐ │
      │           │  │ credence_bond ──► credence_math, timelock     │ │
      │           │  │ credence_delegation                           │ │
      │           │  │ credence_registry                             │ │
      │           │  │ credence_treasury                             │ │
      │           │  │ timelock                                      │ │
      │           │  │ arbitration (credence_arbitration)            │ │
      │           │  │ admin                                         │ │
      │           │  │ credence_multisig                             │ │
      │           │  │ templates                                     │ │
      │           │  └─────────────────────────────────────────────────┘ │
      │           └─────────────────────────────────────────────────────┘
      │
      └──► crates/credence_admin_cli (standalone CLI, no Credence deps)
```

### Layer 0 — Shared Infrastructure

| Crate | Path | Deps | Purpose |
|---|---|---|---|
| `credence_errors` | `contracts/credence_errors/` | `soroban-sdk` | Canonical `ContractError` enum used by every contract via `panic_with_error!`. Wire-stable codes; never renumber a variant. |
| `credence_math` | `contracts/credence_math/` | `soroban-sdk`, `credence_errors`, `ethnum` | Overflow-safe arithmetic (`add_i128`, `mul_i128`, `split_bps`, `ceil_div_i128`). Pure library — no state, no events. |
| `testutils` | `crates/testutils/` | `soroban-sdk` (testutils feature) | Shared test harness re-exported by Soroban. Used as a dev-dependency in contracts that need mock auth and test helpers. |
| `credence_admin_cli` | `crates/credence_admin_cli/` | `soroban-client`, `stellar-baselib`, `clap`, `anyhow`, `serde`, `tokio` | Off-chain CLI tool for admin operations (not a Soroban contract). |

### Layer 1 — Standalone Contracts (depend only on `credence_errors`)

These crates have no dependency on other Credence contracts. Each is a self-contained `cdylib` Soroban contract.

| Crate | Path | Key Entry Points |
|---|---|---|
| `admin` | `contracts/admin/` | `initialize()`, `add_admin()`, `remove_admin()`, `transfer_ownership()` |
| `credence_multisig` | `contracts/credence_multisig/` | `create_proposal()`, `approve()`, `execute()` |
| `credence_registry` | `contracts/credence_registry/` | `register_identity()`, `get_bond_contract()`, `activate_identity()` |
| `credence_treasury` | `contracts/credence_treasury/` | `receive_fees()`, `create_withdrawal_proposal()`, `approve_withdrawal()` |
| `timelock` | `contracts/timelock/` | `queue_operation()`, `execute()`, `cancel()` |
| `credence_arbitration` | `contracts/arbitration/` | `create_dispute()`, `cast_vote()`, `resolve_dispute()` |
| `templates` | `contracts/templates/` | Minimal template for new contracts (scaffold with `credence_errors`) |

### Layer 2 — The Core Bond Contract (depends on `credence_errors` + `credence_math` + `timelock`)

`credence_bond` is the protocol's primary contract. It is the only contract that depends on two other Credence crates.

**Cargo.toml deps** (`contracts/credence_bond/Cargo.toml`):

```toml
[dependencies]
credence_errors = { path = "../credence_errors" }
credence_math   = { path = "../credence_math" }
timelock        = { path = "../timelock" }
soroban-sdk = "22.0"
```

**Why it depends on `credence_math`:** The bond contract performs basis-point splits (`split_bps`) for fee calculations and checked `i128` arithmetic for bond amounts and supply tracking. Rather than reimplementing these, it imports `credence_math::split_bps` and `credence_math::add_i128`. See `contracts/credence_bond/src/math.rs` (which re-exports `credence_math` functions) and `contracts/credence_bond/src/governance_approval.rs` (which imports `credence_math::BPS_DENOMINATOR` directly).

**`timelock` dependency:** The bond contract lists `timelock` as a regular dependency, though its runtime source files do not directly import from it today. The pause/pausable mechanism is implemented locally via `pausable.rs` (copied pattern, not imported from the timelock crate). The dependency may be reserved for future time-gated operations or shared pause logic.

**Example usage of a `credence_math` function inside the bond contract** (from `contracts/credence_bond/src/governance_approval.rs`):

```rust
use credence_math::BPS_DENOMINATOR;
```

And through the re-export at `contracts/credence_bond/src/math.rs`:

```rust
use credence_math::{add_i128, split_bps};
```

### Layer 3 — Delegation (depends on `credence_errors` + cross-contract calls to `credence_bond`)

`credence_delegation` is a standalone contract for delegated attestation rights. It also depends on `credence_bond` as a **dev-dependency** for cross-contract auth-tree fuzz tests (not for on-chain compilation).

**Cargo.toml deps** (`contracts/credence_delegation/Cargo.toml`):

```toml
[dependencies]
soroban-sdk      = "22.0"
credence_errors  = { path = "../credence_errors" }

[dev-dependencies]
credence_bond = { path = "../credence_bond" }   # fuzz test only, not on-chain
```

**Why the dev-dependency exists:** The delegation contract's cross-contract auth-tree fuzz tests drive the bond contract via the Soroban test framework. The bond contract is compiled as an `rlib` (not `cdylib`) so it can be linked by the test harness. See `[lib]` in `contracts/credence_bond/Cargo.toml`:

```toml
crate-type = ["cdylib", "rlib"]
```

The auth-tree fuzz tests live in `contracts/credence_delegation/tests/auth_tree_fuzz.rs`.

---

## Why This Structure?

### `credence_errors` is a leaf for a reason
Every contract uses `panic_with_error!` to revert with a typed `ContractError`. If contracts had their own error enums, off-chain indexers and SDK consumers would need a different error decoder per contract. A single shared enum keeps wire formats stable and dashboard queries simple. See [error-codes-wire.md](error-codes-wire.md).

### `credence_math` is a leaf to avoid arithmetic drift
Overflow-unsafe arithmetic (e.g., Rust's default `+`) silently wraps on release builds, which can silently corrupt bond amounts. `credence_math` forces every arithmetic operation to carry a descriptive panic message, and uses checked operations (`checked_add`, `checked_mul`) that revert cleanly via `ContractError::Arithmetic` if they overflow.

### `timelock` is a contract, not a library
`timelock` is a deployable `cdylib` contract. Its `pausable` module is copied per contract rather than imported — each contract has its own `src/pausable.rs`.
### `credence_bond` has the most Credence dependencies

The bond contract is the only contract that depends on more than one other Credence crate (`credence_errors`, `credence_math`, and `timelock`). No other deployed contract in the workspace depends on a Credence sibling crate at runtime.

### `credence_delegation` uses `credence_bond` as a dev-dep only
The bond contract's `rlib` target allows test harnesses to link against it directly. This enables the cross-contract auth-tree fuzz tests in `contracts/credence_delegation/tests/auth_tree_fuzz.rs` without deploying both contracts to a test network. The `credence_bond` dep is **not** included in the WASM artifact.

---

## Build Verification

To verify the dependency graph compiles for WASM:

```bash
cargo build --target wasm32-unknown-unknown --release -p credence_bond
```

To verify a specific crate's tests pass:

```bash
cargo test -p credence_bond
cargo test -p credence_delegation
```

To check the full workspace with warnings as errors:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```