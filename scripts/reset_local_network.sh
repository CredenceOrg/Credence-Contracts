#!/usr/bin/env bash
# reset_local_network.sh
#
# One-command local Soroban network reset.
# Tears down any running local node, wipes its ledger state, and starts a
# fresh instance. Safe to re-run at any time (idempotent).
#
# Usage:
#   sh scripts/reset_local_network.sh
#   npm run reset-local-network
#
# Requirements:
#   - Soroban CLI  (cargo install soroban-cli)
#   - Docker (used by `soroban network` to run the local validator)

set -euo pipefail

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

info()  { printf '\033[0;34m[reset-local-network]\033[0m %s\n' "$*"; }
ok()    { printf '\033[0;32m[reset-local-network]\033[0m %s\n' "$*"; }
warn()  { printf '\033[0;33m[reset-local-network]\033[0m %s\n' "$*"; }
die()   { printf '\033[0;31m[reset-local-network] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

command -v soroban >/dev/null 2>&1 \
  || die "'soroban' CLI not found. Install with: cargo install soroban-cli"

command -v docker >/dev/null 2>&1 \
  || die "'docker' not found. Docker is required to run the local Soroban node."

# ---------------------------------------------------------------------------
# Stop any running local network
# ---------------------------------------------------------------------------

info "Stopping any running local Soroban network…"
if soroban network ls 2>/dev/null | grep -q "^local"; then
  soroban network rm local 2>/dev/null || true
  ok "Removed existing 'local' network config."
else
  info "No existing 'local' network config found — skipping removal."
fi

# Stop the Docker container if it is still running
CONTAINER_NAME="soroban-local"
if docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^${CONTAINER_NAME}$"; then
  info "Stopping container '${CONTAINER_NAME}'…"
  docker stop "${CONTAINER_NAME}" >/dev/null
  ok "Container stopped."
fi

# Remove the container so state is fully wiped
if docker ps -a --format '{{.Names}}' 2>/dev/null | grep -q "^${CONTAINER_NAME}$"; then
  info "Removing container '${CONTAINER_NAME}'…"
  docker rm "${CONTAINER_NAME}" >/dev/null
  ok "Container removed."
fi

# ---------------------------------------------------------------------------
# Start a fresh local network
# ---------------------------------------------------------------------------

info "Starting fresh local Soroban network…"
soroban network start local \
  --docker-name "${CONTAINER_NAME}" \
  2>&1 | sed 's/^/  /'

ok "Local Soroban network is running."
info "RPC endpoint : http://localhost:8000/soroban/rpc"
info "Network name : local"
info ""
info "To use this network with the CLI:"
info "  soroban contract deploy --network local --source <KEY> --wasm <PATH>"
