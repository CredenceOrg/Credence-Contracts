# Contract Upgrade Procedure

Audience: **operators** — anyone who holds the upgrade admin key for a deployed Credence contract and needs to ship a new Wasm binary or rotate the upgrade admin role.

This document covers two independent procedures:

1. [Rotating the upgrade admin key](#upgrade-admin-rotation) — who is authorised to perform upgrades.
2. [Deploying a new Wasm binary](#deploying-a-new-wasm-binary) — swapping the contract logic.

See also:
- [TWO_STEP_ADMIN.md](TWO_STEP_ADMIN.md) — two-step ownership rotation for the bond admin role (separate from the upgrade admin)
- [DEPLOYMENT.md](DEPLOYMENT.md) — initial deploy and cross-contract wiring runbook
- [CONSTRUCTOR_PATTERNS.md](CONSTRUCTOR_PATTERNS.md) — one-shot initialise pattern including `initialize_upgrade_auth`

---

## Background

The bond contract (`credence_bond`) maintains a dedicated **upgrade admin** that is separate from the operational admin used for parameter changes.  The upgrade admin controls who may rotate the upgrade key; it is stored under `DataKey::Upgrade(UpgradeKey::Admin)`.

At initialisation (`initialize` / `initialize_with_registry`) the deployer address is automatically granted the `Upgrader` role and recorded as the upgrade admin.  All subsequent upgrade-admin operations go through the four public entrypoints described below.

The timelock contract can be used to commit to an upgrade hash before execution (see [Timelock-guarded upgrades](#timelock-guarded-upgrades)); this gives the community a mandatory review window.

---

## Network Setup

Export these environment variables before running any CLI commands:

```bash
# Network identifier — "testnet" or "mainnet"
export NETWORK="testnet"

# Soroban CLI identity alias (or path to secret key)
export ADMIN_KEY="upgrade_admin"

# Contract IDs from your deploy_addresses.env
export BOND_CONTRACT_ID="<bond-contract-id>"
export TIMELOCK_CONTRACT_ID="<timelock-contract-id>"    # optional: only needed for timelock flow
```

Network passphrases (needed when constructing payloads outside the CLI):

- **Testnet**: `Test SDF Network ; September 2015`
- **Mainnet**: `Public Global Stellar Network ; September 2015`

---

## Upgrade Admin Rotation

The upgrade admin role uses a two-step handoff with a built-in 24-hour timelock.  Neither the proposer nor the acceptor can shortcut both steps.

### Constraints

| Check | Value |
|---|---|
| Timelock before acceptance | 86 400 s (24 h) |
| Proposal expiry | 604 800 s (7 days) |
| Who can propose | current upgrade admin only |
| Who can accept | the address named in the proposal only |

### Step 1 — Propose the new upgrade admin

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- \
  transfer_upgrade_admin \
  --admin    <CURRENT_UPGRADE_ADMIN_ADDRESS> \
  --new_admin <NEW_UPGRADE_ADMIN_ADDRESS>
```

The contract stores a `PendingAdminTransfer` record containing the proposed address and the current timestamp.  An `upgrade_admin_transfer_started` event is emitted.

Verify the pending admin was stored:

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --network "$NETWORK" \
  -- \
  get_pending_upgrade_admin
# Returns: Some(<NEW_UPGRADE_ADMIN_ADDRESS>)
```

### Step 2 — Wait 24 hours

The acceptance call will revert with `"timelock not elapsed"` until `now >= proposed_at + 86_400`.

### Step 3 — Accept the upgrade admin role

Run this from the *new* admin's key:

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --source new_upgrade_admin \
  --network "$NETWORK" \
  -- \
  accept_upgrade_admin \
  --caller <NEW_UPGRADE_ADMIN_ADDRESS>
```

On success:
- The upgrade admin slot is updated to `NEW_UPGRADE_ADMIN_ADDRESS`.
- The new admin is granted the `Upgrader` role and added to the authorised-upgraders list.
- The pending slot is cleared.
- An `upgrade_admin_transfer_completed` event is emitted.

Verify:

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --network "$NETWORK" \
  -- \
  get_pending_upgrade_admin
# Returns: None
```

### Cancelling a pending transfer

If the proposal is stale or was made in error, the current upgrade admin can cancel it at any time before acceptance:

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- \
  cancel_upgrade_admin_transfer \
  --admin <CURRENT_UPGRADE_ADMIN_ADDRESS>
```

An `upgrade_admin_transfer_cancelled` event is emitted and the pending slot is cleared.

### Error conditions

| Message | Cause |
|---|---|
| `"not upgrade admin"` | `--admin` does not match the stored upgrade admin |
| `"new admin must be different"` | `--new_admin` equals `--admin` |
| `"no pending upgrade admin"` | `accept_upgrade_admin` called with nothing pending |
| `"not pending upgrade admin"` | `--caller` does not match the pending address |
| `"timelock not elapsed"` | Called fewer than 24 h after the proposal |
| `"admin transfer proposal expired"` | Called more than 7 days after the proposal |

---

## Deploying a New Wasm Binary

Soroban contract upgrades work by uploading a new Wasm blob to the network and then calling `Env::deployer().update_current_contract_wasm()` inside the contract.  The upgrade admin role (described above) controls who is authorised to authorise that swap.

### Step 1 — Build the new binary

Always build with `--locked` to pin the exact dependency versions recorded in `Cargo.lock`:

```bash
cargo build \
  --target wasm32-unknown-unknown \
  --release \
  --locked \
  -p credence_bond
```

The artifact is written to:

```
target/wasm32-unknown-unknown/release/credence_bond.wasm
```

Verify the WASM size is within the per-contract budget before proceeding:

```bash
bash scripts/check_wasm_size.sh
```

For a reproducibility hash check (recommended before mainnet deploys), see [docs/wasm-reproducibility.md](wasm-reproducibility.md).

### Step 2 — Upload the Wasm to the network

```bash
NEW_WASM_HASH=$(soroban contract upload \
  --wasm target/wasm32-unknown-unknown/release/credence_bond.wasm \
  --source "$ADMIN_KEY" \
  --network "$NETWORK")

echo "New Wasm hash: $NEW_WASM_HASH"
```

`soroban contract upload` returns the SHA-256 hex hash of the uploaded Wasm.  It does **not** update any running contract; that happens in Step 4.

### Step 3 (optional) — Queue the upgrade in the timelock

Skip this step for testnet hot-fixes.  For mainnet, routing the upgrade hash through the timelock enforces a 24-hour community review window.

See [Timelock-guarded upgrades](#timelock-guarded-upgrades) below.

### Step 4 — Invoke the contract's upgrade entrypoint

The upgrade entrypoint calls `Env::deployer().update_current_contract_wasm()` internally.  It requires the caller to hold the `Upgrader` role assigned during `initialize_upgrade_auth`.

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- \
  upgrade \
  --new_wasm_hash "$NEW_WASM_HASH"
```

> **Note:** the `upgrade` entrypoint is gated by `require_upgrade_auth`.  Callers without the `Upgrader` role receive `"unauthorized upgrade"`.

### Step 5 — Verify

Confirm the contract now executes the new code:

```bash
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --network "$NETWORK" \
  -- \
  version
```

Compare the returned version string against the value compiled into the new binary.

---

## Timelock-Guarded Upgrades

For production upgrades, queue the Wasm hash in the timelock before execution.  This creates an immutable on-chain record that the upgrade was proposed at a known time, giving the community 24 hours to review it before it can be executed.

The timelock enforces:
- `min_delay_seconds() == 86_400` (24 hours, hardcoded)
- `GRACE_PERIOD == 86_400` — the operation expires 24 hours *after* the ETA; it cannot be executed after that window closes

### 1 — Compute the upgrade payload hash

Construct a deterministic `BytesN<32>` SHA-256 hash of the upgrade payload.  For Credence upgrades the payload is the new Wasm hash:

```bash
# Derive a 32-byte hash of the new wasm hash string to use as op_hash.
# In practice this is computed by your deployment tooling; the exact method
# must match whatever the execution step verifies.
OP_HASH=$(echo -n "$NEW_WASM_HASH" | sha256sum | awk '{print $1}')
echo "op_hash: $OP_HASH"
```

### 2 — Queue the operation

Only the timelock's admin can call `queue_operation`:

```bash
OP_ID=$(soroban contract invoke \
  --id "$TIMELOCK_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- \
  queue_operation \
  --proposer <TIMELOCK_ADMIN_ADDRESS> \
  --op_hash  "$OP_HASH" \
  --delay    86400)

echo "Queued as op_id: $OP_ID"
```

The contract stores a `QueuedOperation` with `eta = now + delay` and `expires_at = eta + GRACE_PERIOD`.  An `operation_queued` event is emitted.

Check the queued operation:

```bash
soroban contract invoke \
  --id "$TIMELOCK_CONTRACT_ID" \
  --network "$NETWORK" \
  -- \
  get_operation \
  --op_id "$OP_ID"
```

### 3 — Wait for ETA

The `execute_operation` call reverts with `TimelockNotReady` until `now >= eta`.

### 4 — Execute the timelock operation

```bash
soroban contract invoke \
  --id "$TIMELOCK_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- \
  execute_operation \
  --op_id "$OP_ID"
```

`execute_operation` marks the operation as `Executed` and records `op_hash` in the replay-guard set; the same hash cannot be queued again.  It does **not** call the bond contract directly — the Wasm swap (Step 4 above) must be invoked separately, typically in the same transaction bundle.

### 5 — Execute the Wasm swap

After `execute_operation` succeeds, invoke the `upgrade` entrypoint as in Step 4 of the previous section.

### Cancelling a queued operation

```bash
soroban contract invoke \
  --id "$TIMELOCK_CONTRACT_ID" \
  --source "$ADMIN_KEY" \
  --network "$NETWORK" \
  -- \
  cancel_operation \
  --admin <TIMELOCK_ADMIN_ADDRESS> \
  --op_id "$OP_ID"
```

Only the timelock admin can cancel.

---

## Post-Upgrade Checklist

Run these read-only calls to confirm the upgrade completed successfully.  None of them mutate state.

```bash
# Confirm the new version string matches what was compiled in
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --network "$NETWORK" \
  -- version

# Confirm admin and early-exit config survived the upgrade
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --network "$NETWORK" \
  -- describe_config

# Confirm the upgrade admin is still the expected address
soroban contract invoke \
  --id "$BOND_CONTRACT_ID" \
  --network "$NETWORK" \
  -- get_pending_upgrade_admin
# Expected: None (no transfer in flight)
```

---

## Storage Migration

Soroban upgrades are in-place: all existing instance storage survives the Wasm swap.  The bond contract uses lazy migration (`migrate_v1_to_v2` in `src/migration.rs`) to handle fields added between schema versions.  The migration runs automatically on the first read after an upgrade — no manual data-migration step is required.

If you are introducing a new storage schema version:
1. Add the migration logic to `src/migration.rs`.
2. Call it at the top of every entrypoint that reads the affected storage key.
3. Document the new schema version in this file and in [STORAGE_KEYS.md](STORAGE_KEYS.md).

---

## Quick Reference

| Entrypoint | Contract | Auth | Purpose |
|---|---|---|---|
| `transfer_upgrade_admin` | `credence_bond` | current upgrade admin | Propose a new upgrade admin (starts 24 h timelock) |
| `accept_upgrade_admin` | `credence_bond` | pending upgrade admin (after 24 h) | Complete the admin handoff |
| `get_pending_upgrade_admin` | `credence_bond` | — | Read-only: pending proposal |
| `cancel_upgrade_admin_transfer` | `credence_bond` | current upgrade admin | Cancel a pending proposal |
| `upgrade` | `credence_bond` | Upgrader role | Swap Wasm (requires uploaded hash) |
| `queue_operation` | `timelock` | timelock admin | Commit to upgrade hash with delay |
| `execute_operation` | `timelock` | permissionless (after ETA) | Mark queued op executed |
| `cancel_operation` | `timelock` | timelock admin | Cancel a pending queued op |

---

## Cross-References

| Topic | Document |
|---|---|
| Operational admin rotation (bond admin, not upgrade admin) | [TWO_STEP_ADMIN.md](TWO_STEP_ADMIN.md) |
| Initial deployment and cross-contract wiring | [DEPLOYMENT.md](DEPLOYMENT.md) |
| One-shot `initialize` pattern | [CONSTRUCTOR_PATTERNS.md](CONSTRUCTOR_PATTERNS.md) |
| Storage key naming | [STORAGE_KEYS.md](STORAGE_KEYS.md) |
| WASM size budget and CI gate | [wasm-size-budget.md](wasm-size-budget.md) |
| Reproducible build verification | [wasm-reproducibility.md](wasm-reproducibility.md) |
| Emergency mode and drain | [emergency.md](emergency.md) |
