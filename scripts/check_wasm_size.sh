#!/usr/bin/env bash
# check_wasm_size.sh - verify each contract wasm file is under its size budget.
#
# Budgets are read from scripts/wasm-size-budget.toml (per-contract limits with
# a default_kb fallback). Pass a single positional argument to override all
# limits for a one-off check, e.g. ./scripts/check_wasm_size.sh 60
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUDGET_FILE="${WASM_BUDGET_FILE:-$SCRIPT_DIR/wasm-size-budget.toml}"
WASM_DIR="$REPO_ROOT/target/wasm32-unknown-unknown/release"
GLOBAL_OVERRIDE_KB="${1:-}"

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

DEFAULT_KB="$(read_default_kb)"
echo "Checking wasm size budgets (default ${DEFAULT_KB}KB; config: ${BUDGET_FILE})"

shopt -s nullglob
wasm_files=("$WASM_DIR"/*.wasm)
if [ ${#wasm_files[@]} -eq 0 ]; then
  echo "[ERROR] No wasm files found in ${WASM_DIR}/"
  exit 1
fi

failed=0
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
