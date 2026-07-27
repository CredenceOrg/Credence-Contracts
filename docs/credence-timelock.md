# Crate: timelock

**Path:** `contracts/timelock`

## Overview

The timelock crate enforces a mandatory delay for high-impact administrative changes. It is intended to give the community time to react before an operation becomes executable.

## Entrypoints

| Entrypoint | Required role | Notes |
| :--------- | :------------ | :---- |
| `initialize` | Admin | Stores the timelock admin for the deployment. |
| `queue_operation` | Admin | Queues an operation with a minimum delay and records an `eta` plus an expiry. |
| `execute_operation` | Public | Executes a pending operation once the ledger time reaches `eta` and before it expires. |
| `cancel_operation` | Admin | Cancels a pending operation before execution. |
| `get_operation` | None | Read-only helper for retrieving queued operation details. |
| `is_operation_executed` | None | Read-only helper for replay-state checks. |

## Required roles

- **Admin**: Can initialize, queue, and cancel operations.
- **Public**: Any address can attempt execution after the timelock window opens, but the call still fails if the operation is not ready or already executed.

## Backend integration notes

- Queue operations with a delay of at least the minimum configured value and preserve the returned `op_id` for later execution or cancellation.
- The contract enforces a grace period after `eta`; if execution is missed, the operation expires and must be re-queued.
- Backends should index the `operation_queued`, `operation_executed`, and `operation_cancelled` events for UI and alerting workflows.
- A repeated `op_hash` cannot be executed twice, so indexers should track executed hashes rather than only operation IDs.
