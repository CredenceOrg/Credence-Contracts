# Timelock for Protocol Changes

## Overview

The Timelock contract enforces a mandatory delay before high-impact administrative changes take effect. This protects users by ensuring that proposed changes are visible on-chain for a defined period before they can be executed, giving the community time to review and respond.

## Architecture

```
Admin ──queue_operation──> Timelock ──(wait min_delay)──> execute_operation
                               ^
Admin ──cancel_operation───────┘
```

**Lifecycle of an operation:**

1. Admin calls `queue_operation(proposer, op_hash, delay)`. The operation is recorded with an ETA of `now + delay` (where delay >= min_delay) and an `expires_at` grace window.
2. The operation sits in a pending state until the ETA passes.
3. After the ETA, anyone can call `execute_operation(op_id)` to execute it, provided it's within the grace window (`now <= expires_at`).
4. At any point before execution, the admin can call `cancel_operation(op_id)` to discard it.

Execution time boundaries are deterministic and inclusive:

- `now = eta - 1`: execution must fail.
- `now = eta`: execution is allowed.
- `now = expires_at`: execution is still allowed.
- `now = expires_at + 1`: execution must fail.

## Function Reference

### `initialize(admin)`

Sets up the contract. Must be called once.

| Parameter | Type    | Description                                       |
|-----------|---------|---------------------------------------------------|
| `admin`   | Address | Address authorized to queue and cancel operations |

### `queue_operation(proposer, op_hash, delay) -> u64`

Queues a new operation. Returns a unique operation ID.

| Parameter  | Type       | Description                                  |
|------------|------------|----------------------------------------------|
| `proposer` | Address    | Must be the admin                            |
| `op_hash`  | BytesN<32> | Deterministic hash of the operation payload  |
| `delay`    | u64        | Seconds to delay (>= min_delay_seconds())    |

### `execute_operation(op_id)`

Executes a pending operation. Requires the current timestamp to be at or past the operation's ETA and before `expires_at`.

| Parameter | Type | Description                  |
|-----------|------|------------------------------|
| `op_id`   | u64  | ID returned by queue_operation |

### `cancel_operation(admin, op_id)`

Cancels a pending operation.

| Parameter | Type    | Description                          |
|-----------|---------|--------------------------------------|
| `admin`   | Address | Must be the admin address            |
| `op_id`   | u64     | ID of the operation to cancel        |

### Query Functions

- `get_operation(op_id) -> Option<QueuedOperation>`
- `is_operation_executed(op_hash) -> bool`
- `get_admin() -> Address`

## Events

| Event                 | Topic Data       | Body Data                           |
|-----------------------|------------------|-------------------------------------|
| `operation_queued`    | (tag, op_id)     | (proposer, op_hash, eta, expires_at)|
| `operation_executed`  | (tag, op_id)     | op_hash                             |
| `operation_cancelled` | (tag, op_id)     | op_hash                             |

## Security Considerations

- The minimum delay must be greater than zero, enforced by `min_delay_seconds()`.
- Operations cannot be executed before the ETA. The contract checks `now >= eta` at execution time.
- Execution is valid through `expires_at` and invalid strictly after it (`now > expires_at`).
- Executed operations are tracked by `op_hash` to prevent replay attacks.
- Cancelled and already-executed operations are permanently locked and cannot be re-used.
- The contract uses `checked_add` for all arithmetic to prevent overflow.
