# Timelock for Parameter Changes

## Overview

The Timelock contract enforces a mandatory delay before protocol parameter changes take effect. This protects users by ensuring that proposed changes are visible on-chain for a defined period before they can be executed, giving the community time to review and respond.

## Threat Model

Without a timelock, a compromised or malicious admin could instantly change protocol parameters (fee rates, slashing percentages, cooldown periods) to extract user funds. The timelock introduces a window during which governance can intervene and cancel harmful changes.

## Architecture

```
Admin ──propose_change──> Timelock ──(wait min_delay)──> execute_change
                              ^
Governance ──cancel_change────┘
```

**Roles:**

- **Admin** can propose new parameter values and execute them after the delay elapses.
- **Governance** can cancel any pending change before it is executed.

**Lifecycle of a parameter change:**

1. Admin calls `propose_change(parameter_key, new_value)`. The change is recorded with an ETA of `now + min_delay`.
2. The change sits in a pending state until the ETA passes.
3. After the ETA, admin calls `execute_change(change_id)` to finalize.
4. At any point before execution, governance can call `cancel_change(change_id)` to discard it.

## Function Reference

### `initialize(admin, governance, min_delay)`

Sets up the contract. Must be called once.

| Parameter    | Type    | Description                                        |
|--------------|---------|----------------------------------------------------|
| `admin`      | Address | Address authorized to propose and execute changes  |
| `governance` | Address | Address authorized to cancel pending changes       |
| `min_delay`  | u64     | Minimum seconds between proposal and execution     |

### `propose_change(proposer, parameter_key, new_value) -> u64`

Queues a new parameter change. Returns a unique change ID.

| Parameter       | Type    | Description                            |
|-----------------|---------|----------------------------------------|
| `proposer`      | Address | Must be the admin                      |
| `parameter_key` | Symbol  | Identifier for the parameter to change |
| `new_value`     | i128    | The proposed new value                 |

### `execute_change(change_id)`

Executes a pending change. Requires the current timestamp to be at or past the change's ETA.

| Parameter   | Type | Description                  |
|-------------|------|------------------------------|
| `change_id` | u64  | ID returned by propose_change |

### `cancel_change(canceller, change_id)`

Cancels a pending change.

| Parameter   | Type    | Description                          |
|-------------|---------|--------------------------------------|
| `canceller` | Address | Must be the governance address       |
| `change_id` | u64     | ID of the change to cancel           |

### `update_min_delay(new_delay)`

Updates the minimum delay. Admin only.

| Parameter   | Type | Description                |
|-------------|------|----------------------------|
| `new_delay` | u64  | New delay in seconds (> 0) |

### Query Functions

- `get_change(change_id) -> ParameterChange`
- `get_min_delay() -> u64`
- `get_admin() -> Address`
- `get_governance() -> Address`

## Events

| Event               | Topic Data       | Body Data                           |
|---------------------|------------------|-------------------------------------|
| `timelock_initialized` | (tag,)        | (admin, governance, min_delay)      |
| `change_proposed`   | (tag, change_id) | (parameter_key, new_value, eta)     |
| `change_executed`   | (tag, change_id) | (parameter_key, new_value)          |
| `change_cancelled`  | (tag, change_id) | parameter_key                       |
| `delay_updated`     | (tag,)           | (old_delay, new_delay)              |

## Integration Guide

To connect the timelock to other protocol contracts:

1. When a protocol parameter needs updating (e.g., cooldown period, fee rate), the admin calls `propose_change` on the Timelock contract with the parameter key and new value.
2. After the delay elapses, the admin calls `execute_change`. The calling contract (or a separate keeper) should read the executed change and apply the new value to the target contract.
3. If a change is deemed harmful, governance calls `cancel_change` before execution.

The timelock itself stores the change records. The actual application of values to target contracts is the responsibility of the integration layer. This decoupling keeps the timelock contract focused and reusable.

## Security Considerations

- The minimum delay must be greater than zero. A zero delay would defeat the purpose of the timelock.
- Only one address can propose/execute (admin) and only one can cancel (governance). Separation of powers ensures that a compromised admin cannot both propose and suppress cancellation.
- Changes cannot be executed before the ETA. The contract checks `now >= eta` at execution time.
- Cancelled and already-executed changes are permanently locked and cannot be re-used.
- The contract uses `checked_add` for all arithmetic to prevent overflow.
