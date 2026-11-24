# Credence Contracts

Soroban smart contracts for the Credence economic trust protocol on Stellar: bonds, attestations, arbitration, dispute resolution, timelocks and treasury.

**Live demo (frontend):** https://credence-frontend-fawn.vercel.app

## Testnet deployment

| Contract | Contract ID (Stellar testnet) |
|---|---|
| `timelock` | [`CAKAQJWZN5O2BTSWKUAJZNJIF6ZPBZ2UEZM6H4IIVTC675JXHICCGM4K`](https://stellar.expert/explorer/testnet/contract/CAKAQJWZN5O2BTSWKUAJZNJIF6ZPBZ2UEZM6H4IIVTC675JXHICCGM4K) |
| `credence_arbitration` | [`CCIED32YR7ZB5MBDKRCT7HYFWAH2AAKFH43HL4IREGSUJ2U3ZHECCL7T`](https://stellar.expert/explorer/testnet/contract/CCIED32YR7ZB5MBDKRCT7HYFWAH2AAKFH43HL4IREGSUJ2U3ZHECCL7T) |
| `dispute_resolution` | [`CCKOGX3BZOH7VSMTIVPITZU6S32NY4PETJIRVQ6SA5WY7PABLMU3KXAH`](https://stellar.expert/explorer/testnet/contract/CCKOGX3BZOH7VSMTIVPITZU6S32NY4PETJIRVQ6SA5WY7PABLMU3KXAH) |

## Workspace layout

The Cargo workspace contains 17 packages. Contract and shared-support crates live under `contracts/`; shared interfaces, test utilities and the admin CLI live under `crates/`.

| Package | Path | Purpose |
|---|---|---|
| `admin` | `contracts/admin` | Privileged configuration, pause and ownership controls |
| `credence_arbitration` | `contracts/arbitration` | Weighted arbitration voting for dispute resolution |
| `bounty-escrow` | `contracts/bounty-escrow` | Risk-monitoring analytics for escrowed bounty funds |
| `credence_bond` | `contracts/credence_bond` | Identity bonds, lifecycle, fees, withdrawals and slashing |
| `credence_delegation` | `contracts/credence_delegation` | Delegation of attestation and management rights |
| `credence_errors` | `contracts/credence_errors` | Shared error types for all Credence smart contracts |
| `credence_math` | `contracts/credence_math` | Shared overflow-safe arithmetic helpers |
| `credence_multisig` | `contracts/credence_multisig` | Threshold-based multi-signature governance and administration |
| `credence_registry` | `contracts/credence_registry` | Maps identities to bond contracts |
| `credence_treasury` | `contracts/credence_treasury` | Protocol fees, slashed funds and multi-signature withdrawals |
| `dispute_resolution` | `contracts/dispute_resolution` | Dispute records, resolution and lifecycle transitions |
| `fixed_duration_bond` | `contracts/fixed_duration_bond` | Bonds that lock USDC for a predetermined duration |
| `templates` | `contracts/templates` | Canonical starting point for new Soroban contracts |
| `timelock` | `contracts/timelock` | Delays protocol parameter changes before execution |
| `interfaces` | `crates/interfaces` | Shared contract-agnostic interfaces and constants |
| `testutils` | `crates/testutils` | Shared test addresses and collection helpers |
| `credence_admin_cli` | `crates/credence_admin_cli` | Command-line tool for Credence admin operations |

## Build

Build the Rust workspace:

```bash
cargo build --workspace --locked
```

Build the Soroban contracts:

```bash
stellar contract build
stellar contract build --package timelock
```

Deploy a built contract to testnet:

```bash
stellar contract deploy --wasm target/wasm32v1-none/release/<crate>.wasm --source <identity> --network testnet
```

## Test

Run all workspace tests:

```bash
cargo test --workspace --locked
```

The packages currently enabled in the contracts CI test gate are listed in `scripts/ci-gated-packages.txt`. Run their tests individually:

```bash
cargo test -p dispute_resolution --locked
cargo test -p testutils --locked
```

For test setup, coverage commands and the broader test matrix, see [the testing guide](docs/testing.md) and [the CI guide](docs/CI.md).

## Lint

Check formatting across the workspace:

```bash
cargo fmt --all -- --check
```

Run Clippy for the packages currently enabled in the contracts CI gate:

```bash
cargo clippy -p dispute_resolution --all-targets --locked -- -D warnings
cargo clippy -p testutils --all-targets --locked -- -D warnings
```

Run the panic ratchet used by CI:

```bash
python3 scripts/check_no_panic.py
```

See the [CI guide](docs/CI.md) for the full local check list, including security-focused Clippy lints, WASM build and size checks, dependency audit and unsafe-code scanning.

## Contribute

1. Check the workspace and crate-specific guidance in [the documentation index](docs/README.md).
2. Read [the CI guide](docs/CI.md) and [the testing guide](docs/testing.md) before changing code.
3. Run the relevant tests and checks above, and include their results in your pull request.
4. Keep each pull request focused on one issue and link it in the PR description.
