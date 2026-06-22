# Contributing to Credence Contracts

Thanks for improving the Credence Soroban contracts. This guide mirrors the repository's current CI workflows so a local branch can be checked before it becomes a PR.

## Prerequisites

- Rust toolchain from [`rust-toolchain.toml`](rust-toolchain.toml). At the time of writing it pins Rust `1.89.0` with `rustfmt`, `clippy`, `llvm-tools-preview`, and the `wasm32-unknown-unknown` target.
- Cargo and rustup.
- Optional Soroban CLI for local deployment flows: `cargo install soroban-cli`.
- Optional coverage tool used by CI: `cargo install cargo-llvm-cov --locked`.
- Optional security tools used by the security workflow: `cargo install cargo-audit --version 0.22.0 --locked` and `cargo install cargo-geiger --version 0.12.0 --locked`.

If your local Rust version differs from the pinned file, let rustup install it automatically or run:

```bash
rustup toolchain install 1.89.0 --component rustfmt --component clippy --component llvm-tools-preview --target wasm32-unknown-unknown
```

## Repository map

- [`contracts/`](contracts/) contains the contract crates.
- [`crates/credence_admin_cli`](crates/credence_admin_cli/) contains the admin CLI crate.
- [`docs/architecture.md`](docs/architecture.md) is the canonical architecture and crate map.
- [`docs/testing.md`](docs/testing.md) documents the coverage policy and per-crate test commands.
- [`docs/error-codes-wire.md`](docs/error-codes-wire.md) documents the wire-stable error-code rule.
- [`docs/fuzz-testing.md`](docs/fuzz-testing.md) documents fuzz and property-test expectations.

## Branch and commit conventions

Use short, scoped branch names:

- `feature/<summary>` for user-visible features.
- `fix/<summary>` for bug or security hardening fixes.
- `test/<summary>` for test-only changes.
- `docs/<summary>` for documentation/templates.
- `refactor/<summary>` for behavior-preserving cleanup.
- `ci/<summary>` for workflow/tooling changes.

Prefer Conventional Commits:

```text
docs: add contributor workflow and PR templates
fix(credence_bond): reject invalid withdrawal window
```

## Local setup

```bash
git clone https://github.com/CredenceOrg/Credence-Contracts.git
cd Credence-Contracts
cargo build --all-targets
```

For optimized WASM builds, use the same target named in the README:

```bash
cargo build --target wasm32-unknown-unknown --release --locked -p credence_bond -p credence_delegation
```

For reproducible WASM and hash comparison details, see [`docs/wasm-reproducibility.md`](docs/wasm-reproducibility.md) and [`docs/wasm-size-budget.md`](docs/wasm-size-budget.md).

## Test and CI checklist

Run the narrowest package test while iterating, then run the relevant full gates before opening a PR.

### Core build and test gates

These mirror `.github/workflows/ci.yml` and `.github/workflows/contracts-tests.yml`:

```bash
cargo fmt --all -- --check
cargo build --all-targets
cargo test --all-targets
cargo test --workspace
cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture
cargo build --release
```

### Lint gates

These mirror `.github/workflows/contracts-lints.yml`:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### Coverage gates

CI enforces 95% line coverage per primary crate in `.github/workflows/coverage.yml`:

```bash
cargo llvm-cov --package credence_bond --fail-under-lines 95 --lcov --output-path lcov-credence_bond.info
cargo llvm-cov --package credence_delegation --fail-under-lines 95 --lcov --output-path lcov-credence_delegation.info
cargo llvm-cov --package timelock --fail-under-lines 95 --lcov --output-path lcov-timelock.info
```

See [`docs/testing.md`](docs/testing.md) for HTML reports and editor integration.

### Security gates

The security workflow runs dependency, lint, and unsafe-code scans:

```bash
cargo audit
cargo clippy --all-targets --message-format=json -- \
  -W clippy::integer_arithmetic \
  -W clippy::unwrap_used \
  -W clippy::expect_used \
  -W clippy::panic \
  -W clippy::todo \
  -W clippy::unimplemented \
  -W clippy::indexing_slicing \
  -W clippy::cast_possible_truncation \
  -W clippy::cast_sign_loss \
  -D warnings
cargo geiger
```

`cargo-geiger` is warning-oriented in CI, but contract-owned unsafe code should still be reviewed and justified.

## Wire-stable error-code rule

`ContractError` discriminants are part of the public wire format. Do not renumber existing variants. When adding an error:

1. Append the new variant in the right category in `contracts/credence_errors/src/lib.rs`.
2. Assign the next unused code.
3. Add or update `contracts/credence_errors/tests/error_codes_wire.rs`.
4. Run `cargo test -p credence_errors error_codes_wire`.
5. Update [`docs/errors.md`](docs/errors.md) and [`docs/error-codes-wire.md`](docs/error-codes-wire.md) if the policy or reference changes.

## PR expectations

Before opening a PR:

- Keep contract changes focused and documented.
- Add or update tests for changed behavior, including edge cases and failure paths.
- Preserve or improve the 95% coverage target for affected crates.
- Run `cargo fmt --all -- --check` and the relevant test/lint commands above.
- Update docs when public API, storage, event, deployment, or security assumptions change.
- Include verification commands and output summaries in the PR body.
- If a broad CI gate is blocked by unrelated upstream state, call that out explicitly and include the focused command that verifies your change.

## Issue and PR templates

Use the issue templates in [`.github/ISSUE_TEMPLATE`](.github/ISSUE_TEMPLATE/) for bugs and features, and the PR template in [`.github/pull_request_template.md`](.github/pull_request_template.md) for all pull requests.
