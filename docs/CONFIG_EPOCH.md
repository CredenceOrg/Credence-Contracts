# Admin Configuration Epoch — Serialization and Retry Contract

## Audience

Integrators and contributors who call the Admin contract's privileged
entrypoints (`add_admin`, `remove_admin`, `update_admin_role`,
`deactivate_admin`, `reactivate_admin`, `suspend_admin`,
`transfer_ownership`, `accept_ownership`, `set_pause_signer`,
`set_pause_threshold`, `pause`, `unpause`, `approve_pause_proposal`,
`execute_pause_proposal`).

## Why this exists

Privileged configuration, pause, and ownership operations are
read-modify-write flows. Two requests that observe the same state and mutate it
can race: for example, two ownership transfers proposed back-to-back, or a
pause proposal approved while the signer set is changing. The Admin contract
makes the behaviour under that race deterministic and reviewable:

1. **Serialization** — Soroban executes each invocation against a consistent
   ledger snapshot and commits storage atomically. Conflicting requests to the
   same contract are serialised by the ledger; they cannot interleave
   half-way through a mutation.
2. **Conflict detection** — every committed privileged mutation advances a
   monotonic `ConfigEpoch` counter exactly once. Clients can detect that their
   snapshot of governance state is stale and retry.
3. **Atomicity** — rejected, stale, repeated, and failed operations never
   advance the epoch and never leave partial state behind.
4. **Idempotency** — operations that would not change state (same-role update,
   duplicate pause/unpause, duplicate proposal approval) are no-ops.

## Epoch semantics

`ConfigEpoch` starts at `0` after `initialize` and is stored under the
`ConfigEpoch` instance-storage key. It is read via:

```rust
pub fn get_config_epoch(e: Env) -> u64
```

It advances exactly once per *committed* privileged mutation. It does **not**
advance for:

- rejected calls (authorization mismatch, `ContractPaused`, invalid target,
  `StaleAdminEpoch`, `InsufficientApprovals`, …);
- no-op repeats (same-role `update_admin_role`, `pause` while paused,
  `unpause` while unpaused, duplicate proposal approval, re-proposing an
  already-proposed-and-approved action);
- failed executions (e.g. executing a pause proposal before the threshold is
  met leaves the proposal live and the epoch unchanged).

Each of the five multi-step pause flows is covered:

| Operation | Epoch bump |
| --- | --- |
| `pause` / `unpause` (threshold 0) | once when the state actually changes |
| `pause` / `unpause` (threshold > 0) | once per proposal creation or new approval |
| `approve_pause_proposal` | once per newly recorded approval |
| `execute_pause_proposal` | once per execution that commits (state change and/or proposal cleanup) |

## Client retry contract

1. Read `get_config_epoch()` together with the governance state your flow
   depends on (`get_all_admins`, `get_admins_by_role`, `get_pending_owner`,
   `get_pause_*` state, etc.).
2. Submit the privileged request.
3. If the request is rejected, or the epoch has advanced since your read, a
   concurrent privileged mutation committed. Re-read the state and retry with
   the fresh snapshot.
4. Because failed calls roll back atomically, retrying can never double-apply
   a partial change, and a stale proposal can never complete a flow after a
   newer one superseded it.

## Conflict examples

### Two ownership transfers

`transfer_ownership(A)` followed by `transfer_ownership(B)` serializes
last-writer-wins: the pending owner is `B`, the epoch advances once per call,
and `A` can never call `accept_ownership` (rejected with `NotAdmin`). The
timelock still applies to the winning transfer.

### Pause proposal vs. signer-set change

A pause proposal is only executable while its epoch-derived ID matches the
current ledger epoch (`require_matching_admin_epoch`, `StaleAdminEpoch`), and
execution re-checks the current threshold against the recorded approval count,
so a signer-set or threshold change can never unlock or poison a stale
proposal.

## Test coverage

`contracts/admin/src/test_concurrency_race_safety.rs` locks these invariants at
the integration boundary (generated contract client):

- `config_epoch_advances_exactly_once_per_committed_mutation`
- `unauthorized_mutation_leaves_no_state_and_no_epoch_bump`
- `insufficient_approvals_leave_proposal_intact`
- `repeated_operations_are_idempotent`
- `concurrent_conflicts_are_detectable_and_retryable`
- `stale_ownership_transfer_is_last_writer_wins`
- `paused_rejections_leave_no_partial_state`
