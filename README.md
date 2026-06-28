# Credence Contracts

Soroban smart contracts for the Credence economic trust protocol. This workspace holds the identity bond and delegation contracts.

## About

Part of [Credence](../README.md). Contracts run on the Stellar network via Soroban. The bond contract is the source of truth for staked amounts and is consumed by the backend reputation engine.

## Prerequisites

- Rust 1.85.1+ (pinned in [`rust-toolchain.toml`](rust-toolchain.toml)); the WASM target is included
- [Soroban CLI](https://developers.stellar.org/docs/smart-contracts/getting-started/setup) (`cargo install soroban-cli`)

## Setup

From the repo root:

```bash
cargo build
```

For Soroban (WASM) build:

```bash
cargo build --target wasm32-unknown-unknown --release --locked -p credence_bond -p credence_delegation
```

For the reproducibility check and the CI hash comparison, see [docs/wasm-reproducibility.md](docs/wasm-reproducibility.md).

## Tests

Run all workspace tests:

```bash
cargo test --workspace
```

Run specific contract tests:

```bash
cargo test -p credence_bond
cargo test -p credence_delegation
```

The dedicated CI workflow at `.github/workflows/contracts-tests.yml` runs the full workspace tests on every PR.

## Linting

Run the contracts-only formatting and lint checks locally before opening a PR:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The dedicated CI workflow at `.github/workflows/contracts-lints.yml` runs the same checks.

## Security scanning

Pull requests run `cargo audit --deny warnings`; dependency vulnerabilities are surfaced in a sticky PR comment and the full JSON report is uploaded as a workflow artifact. See [docs/SECURITY_SCANNING.md](docs/SECURITY_SCANNING.md) for the local command and triage flow.

## Release profile — WASM size

The workspace release profile is tuned to minimize WASM binary size:

```toml
[profile.release]
opt-level = "z"       # Optimize for size
lto = "fat"           # Full link-time optimisation across all crates
codegen-units = 1     # Single codegen unit for maximum inlining
strip = "symbols"     # Strip debug symbols
panic = "abort"       # Omit panic unwind machinery
```

- `opt-level = "z"` — instructs `rustc` to optimise for size rather than speed.
- `lto = "fat"` — enables full cross-crate LTO so the linker can eliminate dead code and inline across crate boundaries.
- `codegen-units = 1` — prevents the compiler from splitting a crate into multiple compilation units, giving the optimiser a whole-crate view.
- `strip = "symbols"` — removes the symbol table from the final `.wasm`.
- `panic = "abort"` — replaces panic unwind landing pads with an immediate `wasm32::unreachable`, saving hundreds of bytes per panic site.

These settings apply workspace-wide. Individual contracts can override them in their own `Cargo.toml` if needed.

## WASM size budget

Release Wasm for every deployable contract must stay within per-contract size ceilings enforced in CI. See [docs/wasm-size-budget.md](docs/wasm-size-budget.md) for the enforced limits and [`.github/workflows/wasm-size.yml`](.github/workflows/wasm-size.yml) for the gate.

## Project layout

- `contracts/credence_bond/` — Identity bond contract
  - `create_bond()` / `top_up()` / `withdraw()` / `withdraw_early()`
  - Rolling bonds: `request_withdrawal()` and `renew_if_rolling()`
  - Tiering: `get_tier()` with auto-upgrade/downgrade events
  - Slashing: `slash()` with available-balance enforcement
  - Emergency: `set_emergency_config()`, `set_emergency_mode()`, `emergency_withdraw()`
  - Emergency audit: `get_latest_emergency_record_id()`, `get_emergency_record()`
  - Lifecycle: [bond state transitions](docs/bond-state-transitions.md)
- `contracts/credence_delegation/` — Delegation contract
- `docs/` — Feature docs (`EVENTS.md`, `rolling-bonds.md`, `early-exit.md`, `slashing.md`, `tier-system.md`, `delegation.md`, `emergency.md`, `UPGRADE.md`)

**Known simplifications:** See [docs/known-simplifications.md](docs/known-simplifications.md) for a complete list of intentional limitations and production paths.

## Deploy (Soroban CLI)

Configure network and deploy:

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/credence_bond.wasm \
  --source <SECRET_KEY> \
  --network <NETWORK>
```

See [Stellar Soroban docs](https://developers.stellar.org/docs/smart-contracts) for auth and network setup.

For the full testnet deploy and cross-contract wiring runbook, see [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).
