# Testing & Coverage

## Running tests

```bash
# All crates
cargo test

# Single crate
cargo test -p credence_bond
cargo test -p credence_delegation
cargo test -p timelock
```

## Coverage (cargo-llvm-cov)

The project targets **95% line coverage** per crate, enforced in CI via
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov).

### One-time setup

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
```

### Run locally

```bash
# HTML report (opens in browser)
cargo llvm-cov --package credence_bond --open

# Enforce threshold (same check as CI)
cargo llvm-cov --package credence_bond --fail-under-lines 95
cargo llvm-cov --package credence_delegation --fail-under-lines 95
cargo llvm-cov --package timelock --fail-under-lines 95

# LCOV output (for editor integration)
cargo llvm-cov --package credence_bond --lcov --output-path lcov.info
```

### CI workflow

`.github/workflows/coverage.yml` runs on every push/PR and fails the build
if any of the three primary crates falls below 95% line coverage. LCOV
reports are uploaded as a build artifact (`lcov-reports`).
