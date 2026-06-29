# Contract Upgrade Procedure

This document provides end-to-end instructions for Operators to execute a contract upgrade on the Credence protocol via the Universal Upgradeable Proxy Standard (UUPS) and Timelock mechanism.

## Network Passphrases

Before invoking the CLI commands, ensure your environment is configured with the correct network passphrase:

- **Testnet**: `Test SDF Network ; September 2015`
- **Pubnet**: `Public Global Stellar Network ; September 2015`

## Upgrade Flow (via Timelock)

Most high-impact upgrades require routing through the `timelock` contract to enforce a mandatory delay, giving the community time to review the proposed `new_implementation`.

### 1. Build the New Implementation

Compile the contract to WebAssembly, ensuring deterministic builds and `#![no_std]` compliance.

```bash
cargo build --target wasm32-unknown-unknown --release --locked -p credence_bond
```

### 2. Deploy the New Implementation

Deploy the newly built Wasm to the network to obtain its contract address. *Note: this does not update the proxy yet.*

```bash
NEW_IMPL_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/credence_bond.wasm \
  --source admin \
  --network testnet)
echo "New Implementation: $NEW_IMPL_ID"
```

### 3. Queue the Upgrade in Timelock

The `queue_operation` function in the Timelock contract requires a deterministic hash of the upgrade payload and a delay.

**Timelock Delays**:
- Upgrades generally enforce a **minimum delay** (`min_delay_seconds`), typically 24 hours (86,400 seconds) for standard administrative actions, matching the admin rotation timelock.
- Ensure your `--delay` argument satisfies the on-chain `min_delay_seconds()` requirement.

```bash
soroban contract invoke \
  --id <TIMELOCK_CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- \
  queue_operation \
  --proposer <ADMIN_ADDRESS> \
  --op_hash <UPGRADE_PAYLOAD_HASH> \
  --delay 86400
```
*Note the returned `op_id`.*

### 4. Wait for the Timelock Delay

The operation is now in a pending state until `now >= eta`. Execution must fail if attempted beforehand.

### 5. Execute the Upgrade

Once the ETA passes (and before `expires_at`), anyone can execute the queued operation through the Timelock. This effectively delegates the call to the proxy's `execute_upgrade` method.

```bash
soroban contract invoke \
  --id <TIMELOCK_CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- \
  execute_operation \
  --op_id <OP_ID>
```

## Emergency Direct Upgrades

If the upgrade admin role is held directly by an EOA (Externally Owned Account) rather than the Timelock, or during an active emergency mode, authorized Upgraders can bypass the queue and execute directly on the proxy.

```bash
soroban contract invoke \
  --id <PROXY_CONTRACT_ID> \
  --source upgrader \
  --network testnet \
  -- \
  execute_upgrade \
  --executor <UPGRADER_ADDRESS> \
  --new_implementation $NEW_IMPL_ID \
  --proposal_id 0
```

## Admin Rotation

For administrative safety, transferring the Upgrade Admin role uses a two-step flow protected by a 24-hour timelock.

1. **Propose**:
   ```bash
   soroban contract invoke \
     --id <PROXY_CONTRACT_ID> \
     --source old_admin \
     --network testnet \
     -- transfer_upgrade_admin --admin <OLD_ADMIN> --new_admin <NEW_ADMIN>
   ```

2. **Accept** (after 24 hours):
   ```bash
   soroban contract invoke \
     --id <PROXY_CONTRACT_ID> \
     --source new_admin \
     --network testnet \
     -- accept_upgrade_admin --caller <NEW_ADMIN>
   ```
