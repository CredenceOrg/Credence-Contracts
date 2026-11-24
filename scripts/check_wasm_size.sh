#!/usr/bin/env bash
# check_wasm_size.sh - verify each contract wasm file is under its size budget.
#
# Budgets are read from scripts/wasm-size-budget.toml (per-contract limits with
# a default_kb fallback). Pass a single positional argument to override all
# limits for a one-off check, e.g. ./scripts/check_wasm_size.sh 60
#
# GATED_FILE mode (issue #1742)
# ─────────────────────────────
# When the GATED_FILE environment variable is set to the path of a package list
# (e.g. scripts/ci-gated-packages.txt), the script additionally verifies that
# every *contract* crate in that list (crate-type contains cdylib) has produced
# a .wasm artifact in WASM_DIR.  A missing artifact causes a non-zero exit even
# if no existing .wasm files exceed their size budget.
#
# This catches silent failures where a crate rename or a build flag change
# produces no .wasm output but the script previously exited 0.
#
# Example:
#   GATED_FILE=scripts/ci-gated-packages.txt \
#   WASM_DIR=target/wasm32v1-none/release \
#   bash scripts/check_wasm_size.sh
#
# Default behaviour (GATED_FILE unset) is fully backward compatible.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUDGET_FILE="${WASM_BUDGET_FILE:-$SCRIPT_DIR/wasm-size-budget.toml}"
# Allow the caller to override the WASM output directory.  The default is the
# legacy wasm32-unknown-unknown path; CI passes wasm32v1-none's path via the
# WASM_DIR env variable when building with the current Soroban/Stellar tooling.
WASM_DIR="${WASM_DIR:-$REPO_ROOT/target/wasm32-unknown-unknown/release}"
GLOBAL_OVERRIDE_KB="${1:-}"
GATED_FILE="${GATED_FILE:-}"

read_default_kb() {
  if [[ -n "$GLOBAL_OVERRIDE_KB" ]]; then
    echo "$GLOBAL_OVERRIDE_KB"
    return
  fi
  if [[ -f "$BUDGET_FILE" ]]; then
    local val
    val=$(grep -E '^default_kb[[:space:]]*=' "$BUDGET_FILE" | head -1 | sed -E 's/.*=[[:space:]]*([0-9]+).*/\1/')
    if [[ -n "$val" ]]; then
      echo "$val"
      return
    fi
  fi
  echo "64"
}

get_contract_limit_kb() {
  local contract="$1"
  local default_kb="$2"

  if [[ -n "$GLOBAL_OVERRIDE_KB" ]]; then
    echo "$GLOBAL_OVERRIDE_KB"
    return
  fi

  if [[ ! -f "$BUDGET_FILE" ]]; then
    echo "$default_kb"
    return
  fi

  local limit
  limit=$(awk -v name="$contract" '
    /^\[contracts\]/ { in_section=1; next }
    /^\[/ { in_section=0 }
    in_section && $1 == name "=" {
      gsub(/[^0-9]/, "", $3)
      print $3
      exit
    }
  ' "$BUDGET_FILE")

  if [[ -n "$limit" ]]; then
    echo "$limit"
  else
    echo "$default_kb"
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# is_contract_crate: return 0 when the crate at $REPO_ROOT/contracts/<name>
# declares crate-type = ["cdylib"] (i.e. it is a deployable Soroban contract).
# Non-contract helper crates (testutils, etc.) return 1.
# ─────────────────────────────────────────────────────────────────────────────
is_contract_crate() {
  local pkg="$1"
  # Search both contracts/ and crates/ subdirectories.
  for search_dir in "$REPO_ROOT/contracts" "$REPO_ROOT/crates"; do
    local cargo_toml="$search_dir/$pkg/Cargo.toml"
    if [[ -f "$cargo_toml" ]]; then
      if grep -q 'cdylib' "$cargo_toml" 2>/dev/null; then
        return 0
      fi
      return 1
    fi
  done
  # Package not found in either directory — skip silently.
  return 1
}

DEFAULT_KB="$(read_default_kb)"
echo "Checking wasm size budgets (default ${DEFAULT_KB}KB; config: ${BUDGET_FILE})"

# ─────────────────────────────────────────────────────────────────────────────
# GATED_FILE mode: check that every gated contract crate has a .wasm artifact.
# ─────────────────────────────────────────────────────────────────────────────
failed=0

if [[ -n "$GATED_FILE" ]]; then
  if [[ ! -f "$GATED_FILE" ]]; then
    echo "[ERROR] GATED_FILE not found: ${GATED_FILE}"
    exit 1
  fi

  echo "GATED_FILE mode: verifying expected wasm artifacts from ${GATED_FILE}"

  while IFS= read -r pkg || [[ -n "$pkg" ]]; do
    # Strip comments and blank lines (same logic as contracts-ci.yml).
    [[ "$pkg" =~ ^[[:space:]]*# ]] && continue
    [[ -z "${pkg// }" ]] && continue

    # Only check crates that actually produce a deployable wasm (cdylib).
    if ! is_contract_crate "$pkg"; then
      echo "[SKIP] ${pkg}: not a cdylib contract crate"
      continue
    fi

    # Wasm artifact name: Cargo replaces hyphens with underscores.
    local_pkg="${pkg//-/_}"
    expected_wasm="$WASM_DIR/${local_pkg}.wasm"

    if [[ ! -f "$expected_wasm" ]]; then
      echo "[MISSING] ${pkg}: expected wasm not found at ${expected_wasm}"
      failed=1
    else
      echo "[FOUND]   ${pkg}: wasm artifact present at ${expected_wasm}"
    fi
  done < "$GATED_FILE"

  if (( failed != 0 )); then
    echo "[ERROR] One or more gated contracts have no wasm artifact in ${WASM_DIR}/"
    exit 1
  fi
fi

# ─────────────────────────────────────────────────────────────────────────────
# Default mode: check sizes of all existing .wasm files.
# ─────────────────────────────────────────────────────────────────────────────
shopt -s nullglob
wasm_files=("$WASM_DIR"/*.wasm)
if [ ${#wasm_files[@]} -eq 0 ]; then
  echo "[ERROR] No wasm files found in ${WASM_DIR}/"
  exit 1
fi

for wasm in "${wasm_files[@]}"; do
  contract="$(basename "$wasm" .wasm)"
  limit_kb="$(get_contract_limit_kb "$contract" "$DEFAULT_KB")"
  limit_bytes=$((limit_kb * 1024))
  size=$(wc -c < "$wasm")
  size_kb=$((size / 1024))

  if (( size > limit_bytes )); then
    echo "[FAIL] ${contract}: ${size_kb}KB (${size} bytes) exceeds limit of ${limit_kb}KB"
    failed=1
  else
    echo "[PASS] ${contract}: ${size_kb}KB (${size} bytes) within limit of ${limit_kb}KB"
  fi
done

if (( failed != 0 )); then
  exit 1
fi

exit 0
