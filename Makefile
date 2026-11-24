# Makefile — local CI mirror for Credence Contracts
#
# Mirrors the five jobs in .github/workflows/contracts-ci.yml exactly:
#   fmt          cargo fmt --all -- --check  (whole workspace)
#   test         cargo test -p <pkg> --locked  (each gated package)
#   clippy       cargo clippy -p <pkg> --all-targets --locked -- -D warnings
#   wasm         wasm32v1-none release build + size-budget check (contract crates)
#   panic-check  python3 scripts/check_no_panic.py
#   ci-local     runs all five in the same order as CI
#
# Package names are read from scripts/ci-gated-packages.txt (comment and blank
# lines stripped) — the single source of truth shared with the workflow.
#
# Usage:
#   make ci-local          # run the full local gate
#   make fmt               # formatting check only
#   make test              # tests only
#   make clippy            # clippy only
#   make wasm              # WASM build + size check only
#   make panic-check       # bare-panic ratchet only
#   make fmt-fix           # auto-fix formatting
#
# Prerequisites: a working `cargo` installation with the `rustfmt`, `clippy`
# components and the `wasm32v1-none` target installed (see docs/CI.md).

PACKAGES_FILE := scripts/ci-gated-packages.txt

# Strip comment lines (starting with #) and blank lines from the packages file.
# This mirrors the shell `[[ "$pkg" =~ ^#.*$ || -z "$pkg" ]] && continue` used
# in the CI workflow.
GATED_PKGS := $(shell grep -v '^\s*\#' $(PACKAGES_FILE) | grep -v '^\s*$$' | sed 's/[[:space:]]//g')

# Contract crates are all gated packages except testutils (which links std and
# is not a WASM contract — matches the CI `[[ "$pkg" == "testutils" ]] && continue`).
WASM_PKGS := $(filter-out testutils,$(GATED_PKGS))

.PHONY: fmt fmt-fix test clippy wasm panic-check ci-local

# ---------------------------------------------------------------------------
# fmt — whole-workspace formatting check
# ---------------------------------------------------------------------------
fmt:
	@echo "==> cargo fmt --all -- --check"
	cargo fmt --all -- --check

# ---------------------------------------------------------------------------
# fmt-fix — auto-fix formatting (not part of ci-local)
# ---------------------------------------------------------------------------
fmt-fix:
	@echo "==> cargo fmt --all"
	cargo fmt --all

# ---------------------------------------------------------------------------
# test — cargo test -p <pkg> --locked for each gated package
# ---------------------------------------------------------------------------
test:
	@echo "==> Running tests for gated packages: $(GATED_PKGS)"
	@for pkg in $(GATED_PKGS); do \
		echo "--- cargo test -p $$pkg --locked"; \
		cargo test -p "$$pkg" --locked || exit 1; \
	done

# ---------------------------------------------------------------------------
# clippy — per-package clippy for each gated package
# ---------------------------------------------------------------------------
clippy:
	@echo "==> Running clippy for gated packages: $(GATED_PKGS)"
	@for pkg in $(GATED_PKGS); do \
		echo "--- cargo clippy -p $$pkg --all-targets --locked -- -D warnings"; \
		cargo clippy -p "$$pkg" --all-targets --locked -- -D warnings || exit 1; \
	done

# ---------------------------------------------------------------------------
# wasm — wasm32v1-none release build + size-budget check
# testutils is excluded (std crate, not a contract).
# ---------------------------------------------------------------------------
wasm:
	@echo "==> Building WASM for contract crates: $(WASM_PKGS)"
	@for pkg in $(WASM_PKGS); do \
		echo "--- cargo build -p $$pkg --target wasm32v1-none --release --locked"; \
		cargo build -p "$$pkg" --target wasm32v1-none --release --locked || exit 1; \
	done
	@echo "==> Checking WASM size budgets"
	WASM_DIR=target/wasm32v1-none/release bash scripts/check_wasm_size.sh

# ---------------------------------------------------------------------------
# panic-check — bare-panic ratchet
# ---------------------------------------------------------------------------
panic-check:
	@echo "==> python3 scripts/check_no_panic.py"
	python3 scripts/check_no_panic.py

# ---------------------------------------------------------------------------
# ci-local — run all five checks in CI order
# ---------------------------------------------------------------------------
ci-local: fmt test clippy wasm panic-check
	@echo ""
	@echo "✓ ci-local passed: fmt, test, clippy, wasm, panic-check all green."
