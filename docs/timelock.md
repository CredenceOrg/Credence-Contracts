# Timelock for Protocol Changes

> For a concise crate-level entrypoint summary and backend checklist, see [Crate: timelock](./credence-timelock.md).

## Overview

The Timelock contract enforces a mandatory delay before high-impact administrative changes take effect. This protects users by ensuring that proposed changes are visible on-chain for a defined period before they can be executed, giving the community time to review and respond.

## Architecture

```
Admin ──queue_call / queue_operation──> Timelock ──(wait min_delay)──> execute_operation
                                           ^
Admin ──cancel_operation───────────────────┘
```

**Lifecycle of an operation:**

1. Admin calls `queue_operation(proposer, op_hash, delay)` (hash-only) or `queue_call(proposer, target, func, args, salt, delay)` (call-bound). The operation is recorded with an ETA of `now + delay` (where delay >= min_delay) and an `expires_at` grace window. The admin may configure the grace window with `set_grace_period(admin, seconds)`; the default is 86,400 seconds, and a change applies only to operations or admin proposals created afterward.
2. The operation sits in a pending state until the ETA passes.
3. After the ETA, anyone can call `execute_operation(op_id)` to execute it, provided it's within the grace window (`now <= expires_at`). A call-bound operation invokes its queued `(target, func, args)`; a hash-only operation only records that it executed. Executing after `expires_at` fails with `ContractError::OperationExpired`.
4. At any point before execution, including after expiry, the admin can call `cancel_operation(op_id)` to discard it.
1. Admin calls `queue_call(proposer, target, func, args, salt, delay)` (call-bound) or `queue_operation(proposer, op_hash, delay)` (legacy hash-only). The operation is recorded with an ETA of `now + delay` (where `delay >= min_delay_seconds()`) and an `expires_at` grace window.
2. The operation sits in a pending state until the ETA passes.
3. After the ETA, anyone can call `execute_operation(op_id)` to execute it, provided it is within the grace window (`eta <= now <= expires_at`). A call-bound operation invokes its queued `(target, func, args)`; a hash-only operation only records that it executed.
4. At any point before execution, the admin can call `cancel_operation(admin, op_id)` to discard it.

## Call-bound operations

A hash-only operation enforces a delay on **nothing**: the delayed action still
has to be performed by the admin key directly, and no on-chain link exists
between the queued hash and that action. Call-bound operations close that gap.

```
Admin ──queue_call(target, func, args, salt, delay)──> Timelock
                                                   │  (wait, then anyone executes)
                                                   ▼
                                     target.func(args) as the timelock
Admin ──queue_call(proposer, target, func, args, salt, delay)──> Timelock
                                                                    │  (wait, then anyone executes)
                                                                    ▼
                                                      target.func(args) as the timelock
```

### Hash pre-image

`op_hash` is computed on-chain by `hash_call(target, func, args, salt)`:

```
op_hash = sha256(xdr(timelock_address) ‖ xdr(target) ‖ xdr(func) ‖ xdr(args) ‖ xdr(salt))
```

Pre-image field order (matching `TimelockContract::hash_call` in `contracts/timelock/src/lib.rs`):

1. `timelock_address` (`Address`) — `e.current_contract_address()`, binding the hash to this timelock instance.
2. `target` (`Address`) — the contract to invoke on execution.
3. `func` (`Symbol`) — the function name to call on `target`.
4. `args` (`Vec<Val>`) — the arguments passed to `target.func`.
5. `salt` (`BytesN<32>`) — caller-supplied 32-byte salt, allowing identical `(target, func, args)` calls to produce distinct `op_hash` values across separate executions.

Notes:
- Each field is length-delimited by its XDR encoding (`to_xdr`), so changing any byte of `timelock_address`, `target`, `func`, `args`, or `salt` produces a different hash.
- `network_id` is **not** included in `hash_call`'s pre-image (domain separation is provided by `timelock_address` and the caller-supplied `salt`).
- Observers and off-chain tools can call `hash_call(target, func, args, salt)` directly to verify the `op_hash` of a queued operation against the `CallSpec` returned by `get_call(op_id)`.

### Execute ordering

Any address may call `execute_operation(op_id)` once the delay has elapsed. On a call-bound operation, `execute_operation`:

1. checks that status is `Pending`, `now >= eta`, `now <= expires_at`, and `ExecutedOp(op_hash)` is not already set,
2. writes `ExecutedOp(op_hash) = true` and `status = Executed` (to both persistent `CallBoundOperation` storage and the instance `Operation` wrapper) **before** the external call (checks-effects-interactions),
3. emits `operation_executed` with topic `("operation_executed", op_id)` and body `op_hash`,
4. invokes `target.func(args)` through `e.invoke_contract`,
5. emits `call_executed` with topic `("call_executed", op_id)` and body `(target, func)`.

If the inner call panics, the entire transaction — including the status update, replay key, and events — is rolled back, so the operation stays `Pending` and remains executable until `expires_at`.

### Integration: who authorizes the timelock?

The timelock is a plain caller as far as the target contract is concerned. A
target that guards an entry point with `admin.require_auth()` must therefore
hold the **timelock address as its admin** (or list it among its authorized
callers). `cancel_operation` on the timelock remains the admin's escape hatch,
so the timelock's own admin key can still stop a queued call before its ETA.
Migrating existing contracts to name the timelock as their admin is a per
contract decision and is not performed by the timelock itself.

### Storage

Call-bound operations are stored in `persistent()` storage under
`DataKey::CallBoundOperation(op_id)` with TTL extended on write and read
(`CALL_BOUND_OPERATION_TTL = 3600` ledgers). A `QueuedOperation` metadata
wrapper is also written to `instance()` storage under `DataKey::Operation(op_id)`
for backward compatibility with `get_operation`. A queued `args` vector can be
arbitrarily large, so the full `CallBoundOperation` lives in persistent storage
rather than instance storage.

Execution time boundaries are deterministic and inclusive across the full lifecycle window `[eta, expires_at]`:

| Timestamp Boundary | Condition | Result / Error Code | Operation Status |
|-------------------|-----------|----------------------|------------------|
| `now = eta - 1` | `now < eta` | Fails: `ContractError::TimelockNotReady` | `Pending` |
| `now = eta` | `now == eta` | Succeeds (lower inclusive boundary) | `Executed` |
| `now = eta + 1` | `now > eta && now < expires_at` | Succeeds | `Executed` |
| `now = expires_at - 1` | `now < expires_at` | Succeeds | `Executed` |
| `now = expires_at` | `now == expires_at` | Succeeds (upper inclusive boundary) | `Executed` |
| `now = expires_at + 1` | `now > expires_at` | Fails: `ContractError::OperationExpired` | `Pending` |

### Grace Window Semantics

- **Default Grace Duration**: `GRACE_PERIOD = 86_400` seconds (24 hours).
- **Expiration Calculation**: `expires_at = eta + get_grace_period()`.
- **Configuration**: `set_grace_period(admin, seconds)` affects operations queued afterward; existing expiration timestamps do not change.
- **Inclusive Range**: An operation is executable on any ledger timestamp `now` satisfying `eta <= now <= expires_at`.
- **Regression Suite**: Fully tested in `contracts/timelock/src/test_timelock.rs` covering all boundary conditions (`eta - 1`, `eta`, `eta + 1`, `expires_at - 1`, `expires_at`, `expires_at + 1`).

## Event Ordering and State Machine

Every operation moves through exactly one of two terminal paths. `Pending` is
the only non-terminal status; `Executed` and `Cancelled` are permanent and
cannot transition to any other status.

```
                    ┌────────────────────┐
                    │       Pending       │◄── created by queue_operation
                    └──────────┬──────────┘
              eta <= now       │      admin calls
              <= expires_at    │      cancel_operation
                    │          │      (any time before execution)
                    ▼          ▼
          ┌──────────────┐ ┌──────────────┐
          │   Executed   │ │  Cancelled   │
          └──────────────┘ └──────────────┘
             (terminal)        (terminal)
```

### Valid event sequences

| # | Sequence | Notes |
|---|----------|-------|
| 1 | `operation_queued` → `operation_executed` | Normal path once `now` reaches `[eta, expires_at]`. |
| 2 | `operation_queued` → `operation_cancelled` | Admin discards the operation before it executes. |

`operation_queued` always precedes any other event for a given `op_id`,
and exactly one of `operation_executed` or `operation_cancelled` may follow
it — never both, and never more than once.

### Invalid transitions

These transitions are rejected by the contract and can never appear in the
on-chain event log for a single `op_id`:

| Attempted transition | Rejected by | Error |
|-----------------------|-------------|-------|
| `Executed` → `Executed` (double execution) | `execute_operation` status check | `ContractError::ProposalAlreadyExecuted` |
| `Executed` → `Cancelled` | `cancel_operation` status check | `ContractError::ProposalAlreadyExecuted` |
| `Cancelled` → `Executed` | `execute_operation` status check | `ContractError::ProposalAlreadyExecuted` |
| `Cancelled` → `Cancelled` (double cancel) | `cancel_operation` status check | `ContractError::ProposalAlreadyExecuted` |
| `Pending` → `Executed` before `eta` | `execute_operation` ETA check | `ContractError::TimelockNotReady` |
| `Pending` → `Executed` after `expires_at` | `execute_operation` expiry check | `ContractError::OperationExpired` |
| Re-queueing an already-executed `op_hash` | `queue_operation` replay guard | `ContractError::ProposalAlreadyExecuted` |

Both status checks in `execute_operation` and `cancel_operation` test for
`op.status != OperationStatus::Pending`, so `Executed` and `Cancelled` are
indistinguishable to a caller trying to act on them again — either terminal
status surfaces the same `ProposalAlreadyExecuted` error.

## Function Reference

### `initialize(admin)`

Sets up the contract. Must be called once.

| Parameter | Type    | Description                                       |
|-----------|---------|---------------------------------------------------|
| `admin`   | Address | Address authorized to queue and cancel operations |

### Admin rotation

Admin rotation now follows a delayed, two-step flow; the former immediate
`transfer_admin(new_admin)` entrypoint has been removed. A current admin
compromised before this change could transfer control to an attacker in one
transaction, bypassing the delay enforced for queued operations. Rotation now
uses the same minimum delay and grace window as those operations, while
requiring the proposed admin to authenticate the acceptance.

1. The current admin calls `propose_admin(new_admin)`. The proposal records
   `eta = now + min_delay_seconds()` and `expires_at = eta + get_grace_period()`.
2. The current admin retains control and may cancel the proposal during the
   delay by calling `cancel_admin_proposal()`.
3. The proposed admin calls `accept_admin()` at any time in the inclusive
   window `[eta, expires_at]`. Acceptance updates the admin and clears the
   pending proposal.
4. If the proposal expires, it can no longer be accepted. The current admin
   may replace it by proposing again.

`propose_admin` rejects the current admin address and the timelock contract's
own address. Acceptance before ETA fails with `ContractError::TimelockNotReady`;
acceptance after expiry fails with `ContractError::SignatureExpired`.

### `queue_operation(proposer, op_hash, delay) -> u64`

Queues a new operation. Returns a unique operation ID.

| Parameter  | Type       | Description                                  |
|------------|------------|----------------------------------------------|
| `proposer` | Address    | Must be the admin                            |
| `op_hash`  | BytesN<32> | Deterministic hash of the operation payload  |
| `delay`    | u64        | Seconds to delay (>= min_delay_seconds())    |

### `queue_call(proposer, target, func, args, salt, delay) -> u64`

Queues a call-bound operation. Returns the operation ID; the derived `op_hash`
is computed via `hash_call(target, func, args, salt)` and is available on the
returned operation and in the `operation_queued` event.

| Parameter  | Type          | Description                                       |
|------------|---------------|---------------------------------------------------|
| `proposer` | Address       | Must be the admin                                 |
| `target`   | Address       | Contract to invoke on execution                   |
| `func`     | Symbol        | Function to call on `target`                      |
| `args`     | Vec<Val>      | Arguments, encoded exactly as they will be passed |
| `salt`     | BytesN<32>    | Salt included in the operation hash                |
| `delay`    | u64           | Seconds to delay (>= min_delay_seconds())         |
| `salt`     | BytesN<32>    | Caller-supplied 32-byte salt for hash uniqueness  |
| `delay`    | u64           | Seconds to delay (`>= min_delay_seconds()`)       |

### `queue_calls(proposer, calls) -> Vec<u64>`

Queues a batch of `CallRequest` values, each containing `target`, `func`,
`args`, `salt`, and `delay`. Batches must contain 1 to 20 requests. The batch
is atomic: an invalid request reverts all queued operations. Each operation
emits the usual `operation_queued` event.

### Grace-period configuration

- `get_grace_period() -> u64` returns the configured grace period (86,400
  seconds by default).
- `set_grace_period(admin, seconds)` updates the period for future queue
  operations and emits `grace_period_updated(previous, new)`.
- Zero is allowed and makes the ETA the only executable timestamp.

### `hash_call(target, func, args, salt) -> BytesN<32>`

Recomputes the call hash with the same pre-image the contract uses
(`sha256(xdr(timelock_address) || xdr(target) || xdr(func) || xdr(args) || xdr(salt))`),
for off-chain verification against a queued operation's `op_hash`.

| Parameter | Type       | Description                                      |
|-----------|------------|--------------------------------------------------|
| `target`  | Address    | Target contract address                          |
| `func`    | Symbol     | Function name on `target`                        |
| `args`    | Vec<Val>   | Encoded function arguments                       |
| `salt`    | BytesN<32> | 32-byte salt used when queueing the call         |

### `execute_operation(op_id)`

Executes a pending operation. Callable by **any address** (permissionless) once the current timestamp is in the inclusive window `[eta, expires_at]`. For call-bound operations, invokes the queued `(target, func, args)`.

| Parameter | Type | Description                                          |
|-----------|------|------------------------------------------------------|
| `op_id`   | u64  | ID returned by `queue_call` or `queue_operation`     |

### `cancel_operation(admin, op_id)`

Cancels a pending operation.

| Parameter | Type    | Description                          |
|-----------|---------|--------------------------------------|
| `admin`   | Address | Must be the admin address            |
| `op_id`   | u64     | ID of the operation to cancel        |

### Query Functions

- `get_operation(op_id) -> Option<QueuedOperation>`
- `get_operation_count() -> u64`
- `get_operations_page(offset, limit) -> Vec<QueuedOperation>`
- `get_call(op_id) -> Option<CallSpec>` — `None` for hash-only operations
- `is_operation_executed(op_hash) -> bool`
- `get_admin() -> Address`
- `get_pending_admin() -> Option<PendingAdmin>`

### Operation listings

`get_operations_page(offset, limit)` returns the queued operations that
`get_operation` would return, without requiring the caller to know the ids in
advance. Both reads live in `persistent()` storage under
`DataKey::Operation(op_id)` and bump that entry's TTL, so paging through the
queue keeps the entries alive exactly as repeated `get_operation` calls do.

| Rule | Behaviour |
|------|-----------|
| Id space | Ids start at `0` and are allocated one per queued operation. `get_operation_count()` returns the next id, so valid ids are `0..count`. |
| Ordering | Ascending id order. The order is stable for a fixed set of stored entries. |
| `offset` | An index into the id space, not into the returned vector. The examined slice is `offset..min(offset + effective_limit, count)`. `offset >= count` returns an empty vector. |
| `limit == 0` | Uses `MAX_PAGE_LIMIT` (200), matching the admin contract. |
| `limit > MAX_PAGE_LIMIT` | Silently clamped to `MAX_PAGE_LIMIT`; the larger value is never honoured. |
| Missing ids | Ids with no stored entry (never persisted, or expired from persistent storage) are skipped and nothing is fabricated. A page may therefore contain fewer than `effective_limit` entries while later ids still exist. |
| Status | `Executed` and `Cancelled` operations remain in the listing with their terminal status. |

The listing is read-only apart from the per-entry TTL bump; it never mutates
operation state.

Admin rotation entrypoints:

- `propose_admin(new_admin)`
- `accept_admin()`
- `cancel_admin_proposal()`

## Events

| Event                 | Topic Data       | Body Data                           |
|-----------------------|------------------|-------------------------------------|
| `operation_queued`    | (tag, op_id)     | (proposer, op_hash, eta, expires_at)|
| `operation_executed`  | (tag, op_id)     | `op_hash`                           |
| `call_executed`       | (tag, op_id)     | `(target, func)` (emitted after the target invocation on call-bound operations) |
| `operation_cancelled` | (tag, op_id)     | op_hash                             |
| `admin_proposed`      | (tag)            | (current_admin, new_admin, eta, expires_at) |
| `admin_transferred`   | (tag)            | (old_admin, new_admin)              |
| `admin_proposal_cancelled` | (tag)       | (current_admin, proposed_admin)     |
| `grace_period_updated` | (tag)           | (previous, new)                      |

## Security Considerations

- The minimum delay must be greater than zero, enforced by `min_delay_seconds()`.
- Operations cannot be executed before the ETA. The contract checks `now >= eta` at execution time.
- Execution is valid through `expires_at` and invalid strictly after it (`now > expires_at`), returning `ContractError::OperationExpired`.
- Executed operations are tracked by `op_hash` to prevent replay attacks.
- Cancelled and already-executed operations are permanently locked and cannot be re-used.
- The contract uses `checked_add` for all arithmetic to prevent overflow.
- Only the admin may queue, and the queued call is visible (and hash-verifiable)
  for the whole delay, so what executes is exactly what was reviewed.
- Effects (status and replay key) are written before the inner call, so a
  reverting target leaves the operation pending instead of consuming it.
