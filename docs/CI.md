# CI Guide for Contract Contributors

> Maintainer-expected commands before a contract PR is reviewed.

This document describes the CI gate, lists the commands to run locally before
opening a PR, and explains how to add a new crate to the gate.

---

## The Contracts CI Gate (`contracts-ci.yml`)

The primary CI gate is `.github/workflows/contracts-ci.yml`. It reads the list
of gated packages from **`scripts/ci-gated-packages.txt`** and runs five jobs
for every PR and push to `main`/`master`:

| Job | What it does |
|---|---|
| `fmt` | `cargo fmt --all -- --check` — whole-workspace format check |
| `test` | `cargo test -p <pkg> --locked` for each gated package |
| `clippy` | `cargo clippy -p <pkg> --all-targets --locked -- -D warnings` for each gated package |
| `wasm` | `cargo build -p <pkg> --target wasm32v1-none --release --locked` for each gated contract crate, then `scripts/check_wasm_size.sh` |
| `no-bare-panic` | `python3 scripts/check_no_panic.py` — ratchet: no new bare `panic!()` in contract release code |

### Gated packages (scripts/ci-gated-packages.txt)

The gated packages as of this writing are:

```
credence_arbitration
timelock
dispute_resolution
testutils
```

### How to add a crate to the gate

1. Ensure `cargo test -p <name> --locked` passes locally.
2. Ensure `cargo clippy -p <name> --all-targets --locked -- -D warnings` passes locally.
3. If it is a contract crate (`#![no_std]` + `cdylib`), ensure it builds for
   `wasm32v1-none`:
   ```bash
   rustup target add wasm32v1-none
   cargo build -p <name> --target wasm32v1-none --release --locked
   ```
   Then verify it is within budget:
   ```bash
   WASM_DIR=target/wasm32v1-none/release bash scripts/check_wasm_size.sh
   ```
   If a higher budget is needed, update `scripts/wasm-size-budget.toml` and
   `docs/wasm-size-budget.md` in the same PR.
4. Add the crate's `[package] name` (one line, no trailing whitespace) to
   `scripts/ci-gated-packages.txt`.
5. Open a PR — the CI picks up the new entry automatically.

---

## Quick Reference — Run All Checks Locally

```bash
# 1. Formatting
cargo fmt --all -- --check

# 2. Per-package clippy (gated packages)
for pkg in credence_arbitration timelock dispute_resolution testutils; do
  cargo clippy -p "$pkg" --all-targets --locked -- -D warnings
done

# 3. Security-focused clippy (security.yml; run on whole workspace)
cargo clippy --all-targets -- \
  -W clippy::arithmetic_side_effects \
  -W clippy::unwrap_used \
  -W clippy::expect_used \
  -W clippy::panic \
  -W clippy::todo \
  -W clippy::unimplemented \
  -W clippy::indexing_slicing \
  -W clippy::cast_possible_truncation \
  -W clippy::cast_sign_loss \
  -D warnings

# 4. Per-package tests (gated packages)
for pkg in credence_arbitration timelock dispute_resolution testutils; do
  cargo test -p "$pkg" --locked
done

# 5. Coverage (per-crate, ≥ 95%) — see section 5 below
cargo llvm-cov --package credence_bond --fail-under-lines 95
cargo llvm-cov --package credence_delegation --fail-under-lines 95
cargo llvm-cov --package timelock --fail-under-lines 95

# 6. Error code wire stability
cargo test -p credence_errors error_codes_wire

# 7. Dependency audit
cargo audit

# 8. Unsafe code detection
cargo geiger

# 9. WASM build + size check (gated contract crates)
rustup target add wasm32v1-none
for pkg in credence_arbitration timelock dispute_resolution; do
  cargo build -p "$pkg" --target wasm32v1-none --release --locked
done
WASM_DIR=target/wasm32v1-none/release bash scripts/check_wasm_size.sh

# 10. No bare panic!()
python3 scripts/check_no_panic.py
```

---

## 1. Formatting

**Command:**
```bash
cargo fmt --all -- --check
```

**What it checks:** Consistent code formatting across the entire workspace.

**Auto-fix:**
```bash
cargo fmt --all
```

**CI job:** `contracts-ci.yml` (`fmt` job)

---

## 2. Per-package Clippy (gated packages)

**Command:**
```bash
cargo clippy -p <pkg> --all-targets --locked -- -D warnings
```

**What it checks:** The full Rust lint suite (correctness, style, complexity,
perf) on every target (lib, tests, benches, examples) for each gated crate.

**Tips:**
- Do not add `#![allow(clippy::all)]` or other blanket lint suppression — use
  targeted, documented `#[allow(reason = "…")]` attributes only.
- Use `cargo clippy --fix` for auto-fixable lints when safe.

**CI job:** `contracts-ci.yml` (`clippy` matrix job)

---

## 3. Security-focused Clippy Lints

**Command:**
```bash
cargo clippy --all-targets -- \
  -W clippy::arithmetic_side_effects \
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

**Note:** The lint was renamed from `clippy::integer_arithmetic` to
`clippy::arithmetic_side_effects` in Rust 1.64. Use the new name;
the old name triggers an "unknown lint" warning on stable Rust.

**What it checks:** Patterns that are warnings in the Rust ecosystem but are
blocking failures for the Credence security pipeline:

| Lint | Risk | Approved alternative |
|---|---|---|
| `unwrap_used` / `expect_used` | Silent panics in production | `unwrap_or_else(\|\| panic_with_error!(…))` |
| `panic` / `todo` / `unimplemented` | Unexpected halting | `panic_with_error!` with wire-stable codes |
| `arithmetic_side_effects` | Unchecked overflow/underflow | `checked_add`, `saturating_add` |
| `indexing_slicing` | Out-of-bounds panic | `.get()` with explicit error handling |
| `cast_possible_truncation` | Silent precision loss | Explicit bounds checks before casting |
| `cast_sign_loss` | Unexpected sign change | `try_from` with error handling |

**CI job:** `security.yml` (`clippy-security` job)

---

## 4. Tests (gated packages)

**Command:**
```bash
cargo test -p <pkg> --locked
```

Run once per gated package. Expected baselines:
- `credence_arbitration` — ≥ 272 tests
- `timelock` — several dozen tests
- `dispute_resolution` — several tests
- `testutils` — several tests

**CI job:** `contracts-ci.yml` (`test` matrix job)

---

## 5. Coverage Gate (≥ 95%)

**One-time setup:**
```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
```

**Commands:**
```bash
cargo llvm-cov --package credence_bond --fail-under-lines 95
cargo llvm-cov --package credence_delegation --fail-under-lines 95
cargo llvm-cov --package timelock --fail-under-lines 95
```

**What it checks:** Per-crate line coverage meets or exceeds 95%.

**Tip:** Generate an HTML report to find uncovered lines:
```bash
cargo llvm-cov --package credence_bond --open
```

**CI job:** `coverage.yml` (pending full setup — see placeholder)

---

## 6. Error Code Wire Stability

**Command:**
```bash
cargo test -p credence_errors error_codes_wire
```

**What it checks:** `ContractError` variant discriminants match their
documented wire-stable codes. Variants must not be renumbered or deleted
after deployment.

**Policy:** See `docs/error-codes-wire.md` for the bump procedure when
adding new errors.

---

## 7. Dependency Vulnerability Scan

**One-time setup:**
```bash
cargo install cargo-audit --version 0.22.0 --locked
```

**Command:**
```bash
cargo audit
```

**What it checks:** Known vulnerabilities in the dependency tree
(advisory database from the RustSec project).

**CI job:** `security.yml` (`cargo-audit` job)

---

## 8. Unsafe Code Detection

**One-time setup:**
```bash
cargo install cargo-geiger --version 0.12.0 --locked
```

**Command:**
```bash
cargo geiger
```

**What it checks:** All usage of `unsafe` blocks in the dependency tree.
The Credence project has a zero-unsafe policy.

**CI job:** `security.yml` (`cargo-geiger` job)

---

## Common Issues and Fixes

### "error: unused import"
Run `cargo fix --allow-dirty` or remove the import manually.

### Blanket `#![allow(clippy::all)]` is not allowed
Remove blanket lint suppression from crate roots. Add targeted
`#[allow(lint_name, reason = "…")]` at the specific item instead.
The `contracts-ci.yml` clippy job will catch blanket suppression because it
cannot differentiate real violations from suppressed ones.

### Coverage below 95%
Generate the HTML report (`cargo llvm-cov --open`) and add tests for the
uncovered lines. Pay special attention to error paths (panics, guard
branches) which are often uncovered.

### "Error(Contract, #N)" in test output
This is expected for tests that exercise panic paths. Use
`#[should_panic(expected = "Error(Contract, #N)")]` to assert the
correct error code.

### Cargo.lock out of date
Run `cargo update` and commit the updated lockfile.

### WASM build fails for wasm32v1-none
Ensure the crate uses `#![no_std]` and that its `[dependencies]` do not
pull in `std`-only crates. The `testutils` feature of `soroban-sdk` must
not be enabled in the release build (`--features testutils` is only for
dev-dependencies).

---

## Resources

- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — full development workflow
- [`docs/testing.md`](testing.md) — test and coverage setup
- [`docs/error-codes-wire.md`](error-codes-wire.md) — error code wire-stability
- [`docs/errors.md`](errors.md) — canonical error code listing
- [`docs/no-dynamic-strings.md`](no-dynamic-strings.md) — dynamic string prohibition
- [`scripts/ci-gated-packages.txt`](../scripts/ci-gated-packages.txt) — authoritative list of gated packages
- [`scripts/wasm-size-budget.toml`](../scripts/wasm-size-budget.toml) — per-contract WASM size budgets
