# Credence Contracts

Soroban smart contracts for the Credence economic trust protocol. This workspace holds the identity bond and delegation contracts.

## About

Part of [Credence](../README.md). Contracts run on the Stellar network via Soroban. The bond contract is the source of truth for staked amounts and is consumed by the backend reputation engine.

## Prerequisites

- Rust 1.84+ (with `wasm32-unknown-unknown`: `rustup target add wasm32-unknown-unknown`)
- [Soroban CLI](https://developers.stellar.org/docs/smart-contracts/getting-started/setup) (`cargo install soroban-cli`)

## Setup

From the repo root:

```bash
cd credence-contracts
cargo build
```

For Soroban (WASM) build:

```bash
cargo build --target wasm32-unknown-unknown --release -p credence_bond
```

## Tests

```bash
cargo test -p credence_bond
cargo test -p credence_delegation
```

## Linting

Run the contracts-only formatting and lint checks locally before opening a PR:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The dedicated CI workflow at `.github/workflows/contracts-lints.yml` runs the same checks.

## Project layout

- `contracts/credence_bond/` — Identity bond contract
  - `create_bond()` / `top_up()` / `withdraw()` / `withdraw_early()`
  - Rolling bonds: `request_withdrawal()` and `renew_if_rolling()`
  - Tiering: `get_tier()` with auto-upgrade/downgrade events
  - Slashing: `slash()` with available-balance enforcement
  - Emergency: `set_emergency_config()`, `set_emergency_mode()`, `emergency_withdraw()`
  - Emergency audit: `get_latest_emergency_record_id()`, `get_emergency_record()`
- `contracts/credence_delegation/` — Delegation contract
- `docs/` — Feature docs (`rolling-bonds.md`, `early-exit.md`, `slashing.md`, `tier-system.md`, `delegation.md`, `emergency.md`)

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
