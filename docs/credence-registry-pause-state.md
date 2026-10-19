# Credence Registry Pause State Machine

Audience: contributors and operators reviewing `credence_registry` emergency pause behavior.

The registry pause system has two layers:

- A stored live/paused bit at `DataKey::Paused`.
- Optional multisig governance for changing that bit through pause signers, a threshold, and proposal records.

The implementation lives in `contracts/credence_registry/src/pausable.rs`; the public entrypoints are exposed by `CredenceRegistry` in `contracts/credence_registry/src/lib.rs`.

## States

| State | Storage | Mutating registry calls | Pause-management calls | Read calls |
| --- | --- | --- | --- | --- |
| Live | `DataKey::Paused = false` | Allowed after their normal auth and validation checks | Allowed | Allowed |
| Pause proposed | `DataKey::Paused = false`, `PauseProposal(id) = "pause"` | Allowed until the proposal is executed | Signer approval and execution allowed | Allowed |
| Paused | `DataKey::Paused = true` | Blocked by `require_not_paused` with `ContractError::ContractPaused` | Allowed so operators can recover | Allowed |
| Unpause proposed | `DataKey::Paused = true`, `PauseProposal(id) = "unpause"` | Still blocked until execution | Signer approval and execution allowed | Allowed |

## Direct Admin Mode

When `DataKey::PauseThreshold` is `0`, `pause(caller)` and `unpause(caller)` require admin auth and change `DataKey::Paused` immediately. This is the initialization default in `initialize`, alongside `PauseSignerCount = 0` and `PauseProposalCounter = 0`.

Use this mode for deployments where emergency response speed is more important than distributed approval.

## Multisig Mode

When `DataKey::PauseThreshold` is greater than `0`, `pause(caller)` and `unpause(caller)` require `caller` to be an enabled pause signer.

The transition is two-step:

1. `pause` or `unpause` stores `PauseProposal(id)` as the action symbol and records the proposer's first approval.
2. Additional signers call `approve_pause_proposal(signer, id)` until `PauseApprovalCount(id) >= PauseThreshold`.
3. `execute_pause_proposal(id)` reads the action symbol and calls the internal `do_pause` or `do_unpause` transition.

Unknown proposal action symbols are rejected. Existing tests cover this by replacing a stored action with `"invalid"` and expecting `try_execute_pause_proposal` to fail.

## What The Pause Gate Blocks

Registry write paths call `pausable::require_not_paused(&e)` before continuing. In the current registry, that protects write entrypoints such as:

- `register`
- `self_register_bond`
- `deactivate`

Read-only entrypoints remain available while paused so dashboards and operators can inspect state during an incident. `get_pause_state()` returns the current pause bit, signer count, and threshold without exposing internal proposal identifiers.

## Review Checklist

- New registry write entrypoints must call `pausable::require_not_paused(&e)` before they mutate storage.
- Pause-management entrypoints must remain callable while paused; otherwise the contract can get stuck in emergency mode.
- Tests should cover one live happy path and one paused failure path for each newly gated write operation.
- Multisig tests should assert that the proposal alone does not flip `DataKey::Paused`; execution after enough approvals is the transition point.
- Docs and dashboards should use `get_pause_state()` for monitoring instead of reading raw storage keys.