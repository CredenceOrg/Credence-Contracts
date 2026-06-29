# Pause Proposal State View

`get_pause_proposal_state` is a **read-only** entrypoint on the delegation
contract that returns the full state of a pause/unpause proposal in one call,
for operator monitoring and alerting. It performs no `require_auth` and mutates
nothing, so it is safe to expose publicly.

```rust
fn get_pause_proposal_state(
    e: Env,
    proposal_id: u64,
    signers: Vec<Address>,
) -> PauseProposalView
```

## Why it exists

`pausable.rs` spreads a proposal's state across four separate storage entries.
Before this view, an operator had to issue four reads (and know how to derive
"executed") to understand one in-flight proposal. This entrypoint **aggregates
those four entries** into a single typed struct:

| Storage entry (`DataKey`)      | Contributes |
|--------------------------------|-------------|
| `PauseProposalCounter`         | distinguishes an allocated id from one never issued → `executed` |
| `PauseProposal(id)`            | `action` |
| `PauseApprovalCount(id)`       | `approvals` |
| `PauseApproval(id, signer)`    | `approved_by` (per supplied signer) |

## `PauseProposalView` fields

| Field         | Type           | Meaning |
|---------------|----------------|---------|
| `proposal_id` | `u64`          | Echoes the queried id. |
| `action`      | `u32`          | `1` = Pause, `2` = Unpause, `0` = no live payload (never allocated, or already executed/cleared). |
| `approvals`   | `u32`          | Distinct signer approvals recorded (`PauseApprovalCount`). Global — not affected by the `signers` argument. |
| `approved_by` | `Vec<Address>` | The subset of the supplied `signers` whose approval flag is set. |
| `executed`    | `bool`         | `true` when the id is below the counter (was allocated) but has no live payload (was executed/cleared). |

## The `signers` argument

Soroban instance storage is a key/value map with **no key enumeration**, and the
contract keeps no list of approvers — only per-`(proposal, signer)` flags. The
view therefore cannot discover approvers on its own; the caller passes the
candidate addresses it wants resolved into `approved_by`. Operators already track
their pause-signer set off-chain, so this is a natural input.

- `action`, `approvals`, and `executed` are **independent** of `signers`.
- Passing an empty vector returns an empty `approved_by` with all other fields
  still populated.
- Passing a subset scopes `approved_by` to that subset (see the
  `approved_by_is_scoped_to_supplied_signers` test); `approvals` still reports
  the global count.

## State-transition semantics

| Stage | `action` | `approvals` | `approved_by` | `executed` |
|-------|----------|-------------|---------------|------------|
| Never allocated (`id >= counter`) | `0` | `0` | `[]` | `false` |
| Proposed                          | `1`/`2` | `1` | `[proposer]` | `false` |
| Approved (quorum)                 | `1`/`2` | `n` | approvers | `false` |
| Executed                          | `0` | `0` | see note | `true` |

> **Note on post-execution `approved_by`:** `execute_pause_proposal` removes the
> `PauseProposal` and `PauseApprovalCount` entries but not the per-signer
> `PauseApproval` flags. The view reports raw storage faithfully, so a query
> after execution may still list approvers even though `approvals` is `0`. Treat
> `executed = true` as authoritative: the proposal is closed.

## Guarantees

- **Read-only / no auth.** No `require_auth`, no writes. Verified by the
  `view_requires_no_auth` test, which runs the call under `set_auths(&[])`.
- **Consistency.** The view equals the underlying per-key reads at every stage,
  verified by `view_matches_per_key_reads` and `view_tracks_state_transitions`.
