# WASM Reproducibility

Credence treats Soroban WASM output as a supply-chain artifact. The goal is that a clean build of the bond and delegation contracts produces the same bytes every time when the source, lockfile, and toolchain are unchanged.

## What CI checks

The workflow at [`.github/workflows/wasm-repro.yml`](../.github/workflows/wasm-repro.yml) performs two clean builds with `--locked` and compares SHA-256 digests for:

- `target/wasm32-unknown-unknown/release/credence_bond.wasm`
- `target/wasm32-unknown-unknown/release/credence_delegation.wasm`

If either hash differs between the two passes, the workflow fails immediately.

## Toolchain pin

The repository pins Rust in [`rust-toolchain.toml`](../rust-toolchain.toml) to keep CI and local builds on the same compiler and target set:

- `channel = "1.85.1"`
- `targets = ["wasm32-unknown-unknown"]`
- `components = ["rustfmt", "clippy", "llvm-tools-preview"]`

That pin is the first line of defense against drift from a moving `stable` toolchain.

## Local reproduction

Run the same build twice from a clean working tree:

```bash
cargo build --release --target wasm32-unknown-unknown --locked -p credence_bond -p credence_delegation
sha256sum target/wasm32-unknown-unknown/release/credence_bond.wasm \
  target/wasm32-unknown-unknown/release/credence_delegation.wasm
```

Then clean the target directory, rebuild, and compare the digests. A passing CI run ends with the hash comparison step succeeding and no diff output.

## Security notes

- A hash mismatch is treated as a build-integrity failure, not a warning.
- The check is designed to catch accidental nondeterminism and build-time injection before deployment.
- Use the pinned toolchain and `--locked` builds when validating contract changes locally.

## CI output shape

A successful run should show the two build passes completing and the final diff step exiting cleanly. A failure should surface a mismatched digest for one or both WASM artifacts and stop the merge.