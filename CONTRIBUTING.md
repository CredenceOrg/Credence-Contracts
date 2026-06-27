# Contributing to Credence Contracts

Welcome! This document is the single source of truth for contributing to the Credence Contracts workspace. It covers the development workflow, every CI gate you must pass, and the conventions that keep reviews fast.

- [Quick Start](#quick-start)
- [Development Workflow](#development-workflow)
- [CI Gates — Local Equivalents](#ci-gates--local-equivalents)
- [Code Conventions](#code-conventions)
- [Pull Request Process](#pull-request-process)
- [Issue Reporting](#issue-reporting)
- [Resources](#resources)

---

## Quick Start

### Prerequisites

| Requirement | Version / Source |
|---|---|
| Rust toolchain | **`1.89.0`** — pinned in [`rust-toolchain.toml`](rust-toolchain.toml) |
| Target | `wasm32-unknown-unknown` (installed automatically by `rustup`) |
| Components | `rustfmt`, `clippy`, `llvm-tools-preview` |
| Cargo tools | `cargo-llvm-cov`, `cargo-audit`, `cargo-geiger` (see [CI Gates](#ci-gates--local-equivalents)) |
| Soroban CLI | [`cargo install soroban-cli`](https://developers.stellar.org/docs/smart-contracts/getting-started/setup) |

> **Toolchain note:** The repo pins its Rust version in `rust-toolchain.toml`. If you are on a different toolchain, `cargo` commands will automatically use the pinned version. To verify: `rustc --version` should show `1.89.0`.

### Setup

```bash
# Clone the repo
git clone https://github.com/Ugasutun/Credence-Contracts.git
cd Credence-Contracts

# Build the workspace (all crates)
cargo build

# Build WASM targets (Soroban contracts)
cargo build --target wasm32-unknown-unknown --release --locked -p credence_bond -p credence_delegation
```

### Run all tests

```bash
cargo test --workspace
```

---

## Development Workflow

### 1. Pick an issue

All work should be tracked by a GitHub issue. Comment on the issue to let others know you are working on it.

### 2. Create a branch

Branch names follow the pattern:

```
<type>/<short-description>
```

Types:

| Type | Purpose |
|---|---|
| `feature/` | New functionality |
| `fix/` | Bug fixes |
| `docs/` | Documentation-only changes |
| `refactor/` | Code restructuring with no behaviour change |
| `ci/` | CI configuration changes |
| `test/` | Test additions or improvements |

Examples:

- `feature/slash-bond-core`
- `fix/storage-ttl-archival`
- `docs/contributing-and-templates`
- `ci/parallelize-workflows`

### 3. Make your changes

Follow the [code conventions](#code-conventions) and run the [local CI gates](#ci-gates--local-equivalents) before committing.

### 4. Commit messages

We use **conventional commits**:

```
<type>: <imperative summary>

<optional body, wrapped at 72 characters>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `ci`, `chore`.

Examples:

```
feat: implement admin-only slash_bond with partial/full slashing

Adds a new entrypoint that allows the admin to slash a bond by an
absolute amount or by percentage (basis points). Emits Slashed event.
```

```
docs: add CONTRIBUTING.md + issue/PR templates

Documents the build/test/clippy/coverage/fuzz gates and wire-stable error
rule; adds issue and PR templates.
```

### 5. Open a Pull Request

See [Pull Request Process](#pull-request-process).

---

## CI Gates — Local Equivalents

Every CI gate below runs automatically on every push and PR. **Run them locally first** to avoid CI failures.

### 1. Formatting (`contracts-lints.yml`)

```bash
cargo fmt --all -- --check
```

Auto-fix formatting issues:

```bash
cargo fmt --all
```

### 2. Clippy Lints (`contracts-lints.yml`)

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

This is the **standard CI clippy** check. It denies all warnings.

### 3. Security Clippy Lints (`security.yml`)

In addition to the standard lint check, CI runs a **security-focused** clippy pass:

```bash
cargo clippy --all-targets -- \
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
```

> **Why separate from standard clippy?** The security lint set catches patterns that are flagged as warnings (not hard errors) in the Rust ecosystem but are treated as blocking failures in the Credence security pipeline. If you use `unwrap()`, `expect()`, `panic!`, integer arithmetic without overflow checks, or index-based slicing, the security lint job will fail.

### 4. Workspace Tests (`contracts-tests.yml`, `ci.yml`)

```bash
# All workspace tests (unittests, integration tests, doc-tests)
cargo test --workspace

# All targets (includes tests, benches, examples)
cargo test --all-targets
```

### 5. Coverage Gate (`coverage.yml`)

The project enforces **95% line coverage** per crate. This is checked for `credence_bond`, `credence_delegation`, and `timelock`.

**One-time setup:**

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
```

**Run coverage locally:**

```bash
# HTML report (opens in browser)
cargo llvm-cov --package credence_bond --open

# Enforce 95% threshold (same as CI)
cargo llvm-cov --package credence_bond --fail-under-lines 95
cargo llvm-cov --package credence_delegation --fail-under-lines 95
cargo llvm-cov --package timelock --fail-under-lines 95

# LCOV output (for editor integration)
cargo llvm-cov --package credence_bond --lcov --output-path lcov-credence_bond.info
```

> See [`docs/testing.md`](docs/testing.md) for full details on testing and coverage.

### 6. Fuzz Harness (`ci.yml`)

The bond crate includes a property-based fuzz harness that exercises random sequences of bond operations (`create`, `top_up`, `slash`, `withdraw`) and verifies core accounting invariants.

```bash
cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture
```

The `--nocapture` flag ensures proptest prints the failing case and shrinking output.

> See [`docs/fuzz-testing.md`](docs/fuzz-testing.md) for harness internals and failure interpretation.

### 7. Release Build (`ci.yml`)

```bash
cargo build --release
```

### 8. Dependency Vulnerability Scan (`security.yml`)

```bash
# One-time setup
cargo install cargo-audit --version 0.22.0 --locked

# Run audit
cargo audit
```

### 9. Unsafe Code Detection (`security.yml`)

```bash
# One-time setup
cargo install cargo-geiger --version 0.12.0 --locked

# Run geiger
cargo geiger
```

### 10. Error Code Wire Stability

Error codes are **wire-stable** — external systems, indexers, and off-chain clients depend on their numeric discriminants.

Run the wire-stability assertion test:

```bash
cargo test -p credence_errors error_codes_wire
```

> **Policy:** Each `ContractError` variant has a fixed numeric code. Variants must not be renumbered after deployment. New variants may only be appended at the end of their existing category block. See [`docs/error-codes-wire.md`](docs/error-codes-wire.md) for the bump procedure and [`docs/errors.md`](docs/errors.md) for the canonical code listing.

### Quick Reference — Run All Gates

```bash
# Format
cargo fmt --all -- --check

# Clippy (standard)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Clippy (security lints)
cargo clippy --all-targets -- \
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

# Build (debug + release)
cargo build --all-targets
cargo build --release

# Tests
cargo test --workspace

# Fuzz harness
cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture

# Coverage (per-crate)
cargo llvm-cov --package credence_bond --fail-under-lines 95

# Error code wire stability
cargo test -p credence_errors error_codes_wire

# Dependency audit
cargo audit

# Unsafe code detection
cargo geiger
```

---

## Code Conventions

### Storage Key Naming

When adding or modifying storage keys, follow the canonical naming convention documented in [`docs/STORAGE_KEYS.md`](docs/STORAGE_KEYS.md). This ensures consistency across the codebase.

### Storage TTL Policy

Every `persistent().set()` call **must** be immediately followed by `persistent().extend_ttl(...)` in the same call frame. Every public entrypoint must call `bump_instance_ttl(&e)` at entry.

See [`docs/storage-ttl.md`](docs/storage-ttl.md) for the full policy, constants, and the expiry-aware TTL pattern.

### Doctest Style

Every `pub fn` on contract types (e.g. `CredenceBond`) must have at least one `/// # Example` block. Pure Rust helpers should have fully runnable doctests; contract methods that require a Soroban `Env` should use `no_run`.

See [`docs/doctest-style.md`](docs/doctest-style.md) for the full guide.

### Changelog Discipline

If your PR modifies any smart contracts (`contracts/**`), you **must** update the `CHANGELOG.md` file. Add an entry under the `## [Unreleased]` section with the appropriate category heading (`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`).

A CI check reminds you if your PR touches `contracts/**` but misses the `CHANGELOG.md` update.

### Error Code Discipline

- Each `ContractError` variant has a fixed, wire-stable numeric code.
- Never renumber or delete existing variants.
- New variants must be appended at the end of their category block (see [`docs/error-codes-wire.md`](docs/error-codes-wire.md)).
- Run `cargo test -p credence_errors error_codes_wire` to verify discriminant values.

### Architecture Overview

See [`docs/architecture.md`](docs/architecture.md) for a complete crate-by-crate mapping of responsibilities, state layout, events, and backend consumption points.

---

## Pull Request Process

1. **Create a branch** following the naming convention (`<type>/<short-description>`).
2. **Run all CI gates locally** (see [Quick Reference](#quick-reference--run-all-gates)).
3. **Write or update tests** for any new or modified functionality.
4. **Update docs** if you change public APIs, storage layout, error codes, or wire behaviour.
5. **Update `CHANGELOG.md`** if you touch contracts.
6. **Open a PR** against `main` or `develop` with the completed PR template.
7. **Address reviewer feedback** — keep the conversation focused and rebase if needed.

### PR Title Convention

PR titles should follow conventional commits, matching the branch type:

```
feat: implement slash_bond with partial/full slashing
fix: prevent storage archival on hot-path reads
docs: add CONTRIBUTING guide and templates
```

### PR Checklist (included in the PR template)

- [ ] Tests pass (`cargo test --workspace`)
- [ ] Formatting checked (`cargo fmt --all -- --check`)
- [ ] Clippy clean (`cargo clippy --workspace --all-targets --all-features -- -D warnings`)
- [ ] Coverage ≥ 95% for affected crates
- [ ] Fuzz harness passes (`cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture`)
- [ ] Error code wire-stability test passes (`cargo test -p credence_errors error_codes_wire`)
- [ ] Changelog updated (if `contracts/**` touched)
- [ ] Docs updated (if public API, storage, or architecture changed)

---

## Issue Reporting

See the issue templates in `.github/ISSUE_TEMPLATE/`:

- **Bug report** — for crashes, incorrect behaviour, or unexpected panics.
- **Feature request** — for new functionality or improvements.

When filing an issue, include:

- A clear description of the problem or proposal.
- Steps to reproduce (for bugs).
- Relevant environment details (OS, Rust version, commit hash).
- For security vulnerabilities, **do not** file a public issue. Follow the process in [`SECURITY.md`](SECURITY.md).

---

## Resources

| Document | What it covers |
|---|---|
| [`README.md`](README.md) | Project overview, build setup |
| [`docs/architecture.md`](docs/architecture.md) | Crate responsibilities, state layout, events |
| [`docs/testing.md`](docs/testing.md) | Running tests, coverage setup, CI integration |
| [`docs/fuzz-testing.md`](docs/fuzz-testing.md) | Bond fuzz harness internals and failure interpretation |
| [`docs/error-codes-wire.md`](docs/error-codes-wire.md) | Wire-stable error code policy and bump procedure |
| [`docs/errors.md`](docs/errors.md) | Canonical `ContractError` code listing by category |
| [`docs/doctest-style.md`](docs/doctest-style.md) | Doctest authoring guide and conventions |
| [`docs/storage-ttl.md`](docs/storage-ttl.md) | Storage TTL policy and expiry-aware patterns |
| [`docs/STORAGE_KEYS.md`](docs/STORAGE_KEYS.md) | Storage key naming conventions |
| [`docs/security.md`](docs/security.md) | Security model and threat analysis |
| [`CHANGELOG.md`](CHANGELOG.md) | Release history and changelog |
| [`SECURITY.md`](SECURITY.md) | Security vulnerability reporting |
| [`rust-toolchain.toml`](rust-toolchain.toml) | Pinned Rust toolchain version |