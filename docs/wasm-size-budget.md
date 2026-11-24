# Wasm Size Budget

Soroban contracts have a hard on-chain size limit of ~64 KiB per contract Wasm. Exceeding this limit prevents upgrades and can cause runtime failures.

## Enforced limits

CI enforces per-contract ceilings via [`.github/workflows/wasm-size.yml`](../.github/workflows/wasm-size.yml), which builds release Wasm and runs [`scripts/check_wasm_size.sh`](../scripts/check_wasm_size.sh). Budgets are defined in [`scripts/wasm-size-budget.toml`](../scripts/wasm-size-budget.toml).

| Contract | Budget (KiB) | Budget (bytes) |
| --- | ---: | ---: |
| `admin` | 64 | 65 536 |
| `bounty_escrow` | 64 | 65 536 |
| `credence_arbitration` | 64 | 65 536 |
| `credence_bond` | 64 | 65 536 |
| `credence_delegation` | 64 | 65 536 |
| `credence_multisig` | 64 | 65 536 |
| `credence_treasury` | 64 | 65 536 |
| `bounty-escrow` | 64 | 65 536 |
| `credence_registry` | 64 | 65 536 |
| `fixed_duration_bond` | 64 | 65 536 |
| `templates` | 64 | 65 536 |
| `timelock` | 64 | 65 536 |

The listed values are explicit ceilings, not measured artifact sizes. The default ceiling for any contract not listed explicitly is **64 KiB** (65 536 bytes); measured sizes and growth headroom should be recorded when the corresponding wasm artifacts are available.

## How it works

1. The **WASM Size Budget** workflow builds all deployable contracts with `cargo build --target wasm32-unknown-unknown --release --locked -p <contract>…` (pinned toolchain from [`rust-toolchain.toml`](../rust-toolchain.toml)).
2. `scripts/check_wasm_size.sh` scans `target/wasm32-unknown-unknown/release/*.wasm` by default (or the directory in `WASM_DIR`) and fails if any artifact exceeds its configured budget.
3. Debug symbols are stripped via the workspace `profile.release.strip = "symbols"` setting, so only deployable code size is measured.

Pass a single argument to override all limits for a one-off local check, e.g. `./scripts/check_wasm_size.sh 60` for a 60 KiB ceiling.

## GATED_FILE mode — fail on missing wasm

When the `GATED_FILE` environment variable points to a package list (e.g.
`scripts/ci-gated-packages.txt`), `check_wasm_size.sh` additionally verifies
that every **contract** crate in that list (those whose `Cargo.toml` declares
`crate-type = ["cdylib"]`) has produced a `.wasm` artifact in `WASM_DIR`.

A missing artifact causes a non-zero exit even if no existing `.wasm` files
exceed their size budget. This catches silent failures where a crate rename or
a build flag change produces no WASM output.

Non-contract helper crates (e.g. `testutils`) are detected via the absence of
`cdylib` in their `Cargo.toml` and are silently skipped.

```bash
# Verify all gated contract crates built successfully and are within budget
GATED_FILE=scripts/ci-gated-packages.txt \
WASM_DIR=target/wasm32v1-none/release \
bash scripts/check_wasm_size.sh
```

Verification that the flag works:

```bash
GATED_FILE=scripts/ci-gated-packages.txt WASM_DIR=/nonexistent bash scripts/check_wasm_size.sh
echo $?   # prints non-zero
```

Default behaviour (GATED_FILE unset) is fully backward compatible.

## Local validation

```bash
cargo build --target wasm32-unknown-unknown --release --locked \
  -p admin -p credence_arbitration -p credence_bond -p credence_delegation \
  -p credence_multisig -p credence_treasury -p templates -p timelock
chmod +x scripts/check_wasm_size.sh
./scripts/check_wasm_size.sh
```

The script prints pass/fail for each contract. See also [wasm-reproducibility.md](wasm-reproducibility.md) for hash pinning of bond and delegation artifacts.

## Adjusting a budget

If a contract legitimately needs more space:

1. Raise its entry under `[contracts]` in `scripts/wasm-size-budget.toml`.
2. Update the table in this document.
3. Land the change in the same PR as the binary growth, with reviewer approval.
