# Crate: timelock

**Path:** `contracts/timelock`

## Overview

The timelock crate enforces a mandatory delay for high-impact administrative changes. It supports both **call-bound operations** (`queue_call`, which bind a concrete `(target, func, args, salt)` invocation to the queued hash and execute it on-chain) and legacy **hash-only operations** (`queue_operation`).

For the full call-bound lifecycle, hash pre-image specification, state machine, event schemas, and security model, see [Timelock for Protocol Changes](./timelock.md).

## Entrypoints

| Entrypoint | Required role | Notes |
| :--------- | :------------ | :---- |
| `initialize(admin)` | Admin | Stores the timelock admin for the deployment. Must be called once. |
| `queue_call(proposer, target, func, args, salt, delay) -> u64` | Admin | Queues a call-bound operation; computes `op_hash` via `hash_call` and stores the call in persistent storage. |
| `queue_operation(proposer, op_hash, delay) -> u64` | Admin | Legacy hash-only variant; records an `eta` and expiry without binding a contract call. |
| `hash_call(target, func, args, salt) -> BytesN<32>` | None | Public view function that computes `sha256(xdr(timelock_address) \|\| xdr(target) \|\| xdr(func) \|\| xdr(args) \|\| xdr(salt))`. |
| `execute_operation(op_id)` | Public | Executes a pending operation once `eta <= now <= expires_at`; invokes `(target, func, args)` for call-bound operations. |
| `cancel_operation(admin, op_id)` | Admin | Cancels a pending operation before execution. |
| `get_operation(op_id) -> Option<QueuedOperation>` | None | Read-only helper for retrieving queued operation metadata. |
| `get_call(op_id) -> Option<CallSpec>` | None | Read-only helper for retrieving the bound `(target, func, args, salt)` of a call-bound operation (`None` for hash-only). |
| `get_operation_count() -> u64` | None | Returns the number of allocated operation IDs (`0..count`). |
| `get_operations_page(offset, limit) -> Vec<QueuedOperation>` | None | Returns a paginated slice of queued operations in ascending ID order. |
| `is_operation_executed(op_hash) -> bool` | None | Read-only helper for replay-state checks. |
| `get_admin() -> Address` | None | Returns the current admin address. |
| `propose_admin(new_admin)` | Admin | Starts the delayed two-step admin rotation. |
| `accept_admin()` | Pending admin | Completes admin rotation within `[eta, expires_at]`. |
| `cancel_admin_proposal()` | Admin | Cancels an active pending admin proposal. |
| `get_pending_admin() -> Option<PendingAdmin>` | None | Returns the pending admin proposal, if any. |

## Required roles

- **Admin**: Can initialize the contract, queue (`queue_call` / `queue_operation`) and cancel operations, and propose or cancel admin rotation.
- **Pending admin**: Can call `accept_admin()` once the rotation delay elapses.
- **Public**: Any address can call `execute_operation(op_id)` once the timelock window `[eta, expires_at]` opens, and any address can call `hash_call` or read-only query functions.

## Backend integration notes

- Prefer `queue_call` over `queue_operation` so the delayed action is cryptographically bound to `(target, func, args, salt)` and executed directly by the timelock contract.
- Provide a unique 32-byte `salt` per call-bound operation if the same `(target, func, args)` needs to be queued again later, since executed `op_hash` values are permanently blocked from replay.
- Precompute or verify hashes off-chain using `hash_call(target, func, args, salt)` (see [Hash pre-image](./timelock.md#hash-pre-image)).
- Queue operations with `delay >= min_delay_seconds()` (86,400 seconds) and preserve the returned `op_id` for execution or cancellation.
- The contract enforces a 24-hour grace period (`GRACE_PERIOD = 86_400` seconds) after `eta`; expired operations cannot be executed and must be re-queued.
- Index `operation_queued`, `operation_executed`, `call_executed`, and `operation_cancelled` (plus admin rotation events) for UI and alerting workflows. See [Events](./timelock.md#events) for exact topic and body layouts.
