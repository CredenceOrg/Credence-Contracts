# CI Linting Guide for Contract Contributors

> Maintainer-expected linter commands before a contract PR is reviewed.

This document lists the **exact commands** maintainers expect you to run before
opening a PR that touches contracts. Running them locally prevents CI failures
and keeps reviews focused on logic, not style.

---

## Package Gating Strategy

Not every workspace crate compiles cleanly yet. CI enforces quality gates only
on the crates listed in
[`scripts/ci-gated-packages.txt`](../scripts/ci-gated-packages.txt).

The [`contracts-ci.yml`](../.github/workflows/contracts-ci.yml) workflow reads
that file and fans out matrix jobs for **fmt**, **clippy**, **test**, **WASM
build + size budget**, and **no-bare-panic** checks.

### How to gate a new crate

1. Make the crate pass all gates locally (see the commands below).
2. Add one line with the exact Cargo package name to
   `scripts/ci-gated-packages.txt`.
3. Open a PR — CI will automatically include the new crate.

### Currently gated packages

| Package                | Type     |
|------------------------|----------|
| `credence_arbitration` | contract |
| `timelock`             | contract |
| `dispute_resolution`   | contract |
| `testutils`            | library  |

---

## Quick Reference — Run All Lints

```bash
# 1. Formatting
cargo fmt --all -- --check

# 2. Clippy (per gated package)
for pkg in $(grep -v '^\s*#' scripts/ci-gated-packages.txt | grep -v '^\s*$'); do
  cargo clippy -p "$pkg" --all-targets --locked -- -D warnings
done

# 3. Security-focused clippy
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

# 4. Tests (per gated package)
for pkg in $(grep -v '^\s*#' scripts/ci-gated-packages.txt | grep -v '^\s*$'); do
  cargo test -p "$pkg" --locked
done

# 5. WASM build + size check (contract crates only)
rustup target add wasm32v1-none
for pkg in credence_arbitration timelock dispute_resolution; do
  cargo build -p "$pkg" --target wasm32v1-none --release
done
bash scripts/check_wasm_size.sh

# 6. No bare panic!() ratchet
python3 scripts/check_no_panic.py

# 7. Error code wire stability
cargo test -p credence_errors error_codes_wire

# 8. Dependency audit
cargo audit
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

**CI job:** `contracts-ci.yml` → `fmt`

---

## 2. Standard Clippy Lints

**Command (per gated package):**
```bash
cargo clippy -p <package> --all-targets --locked -- -D warnings
```

**What it checks:** The full Rust lint suite (correctness, style, complexity,
perf) on every target (lib, tests, benches, examples).

**Tips:**
- Use `#![allow(clippy::some_lint)]` sparingly and only with a comment
  explaining why the lint is suppressed.
- Prefer `cargo clippy --fix` for auto-fixable lints when safe.

**CI job:** `contracts-ci.yml` → `clippy`

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

> **Note:** `clippy::integer_arithmetic` was renamed to
> `clippy::arithmetic_side_effects` in recent Rust toolchains.

**CI job:** `security.yml`

---

## 4. Tests

**Command (per gated package):**
```bash
cargo test -p <package> --locked
```

**What it checks:** All unit tests, integration tests, and doc-tests for the
gated crate.

**Tips:**
- Use `just test-one <crate> [test_name]` for targeted runs during development.
- Run the fuzz harness explicitly for the bond crate:
  ```bash
  cargo test -p credence_bond fuzz::test_bond_fuzz -- --nocapture
  ```

**CI job:** `contracts-ci.yml` → `test`

---

## 5. WASM Build + Size Budget

**Commands:**
```bash
rustup target add wasm32v1-none
cargo build -p <contract> --target wasm32v1-none --release
bash scripts/check_wasm_size.sh
```

**What it checks:** Each contract crate compiles to a valid WASM binary that
fits within the per-contract size budget defined in
[`scripts/wasm-size-budget.toml`](../scripts/wasm-size-budget.toml).

**CI job:** `contracts-ci.yml` → `wasm`

---

## 6. No Bare panic!() Ratchet

**Command:**
```bash
python3 scripts/check_no_panic.py
```

**What it checks:** No *new* bare `panic!(…)` calls in contract release code.
Existing debt is tracked in `scripts/panic_baseline.txt`; the checker is a
ratchet that prevents the debt from growing.

**CI job:** `contracts-ci.yml` → `no-bare-panic`

---

## 7. Error Code Wire Stability

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

## 8. Dependency Vulnerability Scan

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

**CI job:** `security.yml`

---

## Common Issues and Fixes

### "error: unused import"
Run `cargo fix --allow-dirty` or remove the import manually.

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

---

## Resources

- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — full development workflow
- [`docs/testing.md`](testing.md) — test and coverage setup
- [`docs/error-codes-wire.md`](error-codes-wire.md) — error code wire-stability
- [`docs/errors.md`](errors.md) — canonical error code listing
- [`docs/no-dynamic-strings.md`](no-dynamic-strings.md) — dynamic string prohibition
- [`scripts/ci-gated-packages.txt`](../scripts/ci-gated-packages.txt) — gated package list
