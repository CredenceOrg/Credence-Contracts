# Arbitration Voting System

The `CredenceArbitration` contract provides a weighted voting mechanism for dispute resolution, allowing authorized arbitrators to decide on outcomes.

## Overview

Disputes are created with a specific duration. During this time, registered arbitrators can cast weighted votes for different outcomes. Once the voting period ends, the dispute can be resolved, and the outcome with the highest total weight is declared the winner.

## Dispute Status Machine

Disputes follow a canonical status machine with enforced transitions:

```
Open ──────> Voting ──────> Resolving ──────> Resolved
  │            │              │                  │
  └────────────┴──────────────┤──────────> Cancelled
                              ↓                  │
                  ┌───────────┴───────────┐      │
                  Tied                 NoQuorum  │
                    └───────────┬───────────┴────┘
                                ↓
                             Archived ──────> Voting
```

### Valid Transitions

- `Open → Voting` — Voting period begins immediately when a dispute is created.
- `Voting → Resolving` — The voting period ends and `resolve_dispute` begins tallying.
- `Voting → Cancelled` — The dispute is cancelled by the creator (only before any vote is cast and before `voting_end`) or by the admin (at any time before resolution).
- `Resolving → Resolved` — The tally produces a clear winner.
- `Resolving → Tied` — The tally produces an equal highest-weight outcome.
- `Resolving → NoQuorum` — Voting ended without meeting the configured quorum.
- `Open → Cancelled` — The dispute is cancelled before voting begins.
- `Resolved → Archived` — The admin archives a resolved dispute.
- `Tied → Archived` — The admin archives a tied dispute.
- `NoQuorum → Archived` — The admin archives a dispute that missed quorum.
- `Cancelled → Archived` — The admin archives a cancelled dispute.
- `Archived → Voting` — The admin reopens an archived dispute for new voting.

All other transitions are rejected with `ArbitrationError::InvalidTransition`.

### Lifecycle Events

The contract emits lifecycle events for the major milestones in the dispute lifecycle:

- `dispute_created` when a dispute is opened.
- `status_transition` for every accepted transition, including the initial `Open → Voting` step.
- `dispute_cancelled` when a dispute is cancelled.
- `dispute_resolved` when a dispute reaches a terminal resolved state.
- `dispute_tied` when tallying ends in a tie.
- `quorum_not_met` when resolution completes without meeting the configured quorum; data is `(total_weight, min_total_weight, voter_count, min_voters)`.
- `dispute_archived` when an admin archives a finalized dispute.
- `dispute_reopened` when an admin reopens an archived dispute.

These events are intended to make the dispute lifecycle auditable and easy to consume off-chain.

## Types

### DisputeStatus

| Status    | Value | Description                                       |
| --------- | ----: | ------------------------------------------------- |
| Open      |     0 | Initial state (immediately transitions to Voting) |
| Voting    |     1 | Arbitrators can cast votes                        |
| Resolving |     2 | Tallying votes (transient state)                  |
| Resolved  |     3 | Final outcome determined with a clear winner      |
| Cancelled |     4 | Dispute cancelled by creator or admin             |
| Tied      |     5 | Votes resulted in a tie (equal highest weights)   |
| Archived  |     6 | Finalized dispute archived by an admin            |
| NoQuorum  |     7 | Voting ended without meeting quorum; outcome is 0 |

### Dispute

| Field               | Type             | Description                                    |
| ------------------- | ---------------- | ---------------------------------------------- |
| id                  | `u64`            | Unique identifier for the dispute              |
| creator             | `Address`        | Address that created the dispute               |
| description         | `String`         | Brief description of the dispute               |
| voting_start        | `u64`            | Timestamp when voting begins                   |
| voting_end          | `u64`            | Timestamp when voting ends                     |
| status              | `DisputeStatus`  | Current status in the lifecycle                |
| outcome             | `u32`            | Winning outcome (0 when Tied or NoQuorum)      |
| cancellation_reason | `Option<String>` | Optional cancellation reason (max 256 bytes)   |
| cancelled_by_role   | `Option<Symbol>` | Role that cancelled (`"creator"` or `"admin"`) |

### ArbitrationError

Generated directly from `contracts/arbitration/src/status.rs`. Note that `ReasonTooLong = 14` is declared before `QuorumNotMet = 13` in source; the table below is ordered by numeric discriminant.

| Code | Variant              | Description                                                                                 |
| ---: | -------------------- | ------------------------------------------------------------------------------------------- |
|    1 | `InvalidTransition`  | The requested dispute status transition is not allowed.                                     |
|    2 | `AlreadyInitialized` | The contract has already been initialized.                                                  |
|    3 | `NotInitialized`     | The contract has not been initialized.                                                      |
|    4 | `NotAdmin`           | The caller is not the admin.                                                                |
|    5 | `NotArbitrator`      | The caller or queried address is not a registered arbitrator.                               |
|    6 | `AlreadyVoted`       | The voter has already cast a vote on this dispute.                                          |
|    7 | `VotingInactive`     | The dispute is not in `Voting`, or the ledger time is outside `[voting_start, voting_end]`. |
|    8 | `VotingNotEnded`     | The voting window has not yet ended.                                                        |
|    9 | `DisputeNotFound`    | The dispute ID does not exist.                                                              |
|   10 | `InvalidOutcome`     | The supplied outcome is invalid (`0` is reserved for `Tied` or `NoQuorum`).                 |
|   11 | `WeightNotPositive`  | The supplied arbitrator weight must be greater than zero.                                   |
|   12 | `NotAuthorized`      | The caller is not authorized for this action.                                               |
|   13 | `QuorumNotMet`       | Quorum was not met (currently unused; unmet quorum transitions to `NoQuorum`).              |
|   14 | `ReasonTooLong`      | The supplied cancellation reason exceeds 256 bytes.                                         |
|   15 | `PromiseNotKept`     | The actual outcome does not match the promised outcome.                                     |
|   16 | `CursorOutOfRange`   | Pagination cursor is out of range (`cursor >= count`).                                      |
|   17 | `DisputeActive`      | Dispute is still active (`Open`, `Voting`, or `Resolving`) and blocks this operation.       |
|   18 | `OngoingDispute`     | A creator already has an unresolved dispute tracked as active.                              |

## Contract Functions

### Initialization & Administration

- `initialize(admin: Address) -> Result<(), ArbitrationError>` — Sets the contract administrator and initializes pause state. Can only be called once. Errors: `AlreadyInitialized`.
- `register_arbitrator(arbitrator: Address, weight: i128) -> Result<(), ArbitrationError>` — Registers or updates an arbitrator with a positive voting weight. Requires admin auth. Errors: `NotInitialized`, `WeightNotPositive` (panics if paused).
- `unregister_arbitrator(arbitrator: Address) -> Result<(), ArbitrationError>` — Removes an arbitrator's voting rights and compacts the registry. Requires admin auth. Errors: `NotInitialized` (panics if paused).
- `set_quorum(admin: Address, min_total_weight: i128, min_voters: u32) -> Result<(), ArbitrationError>` — Configures minimum total weight and distinct voter count for resolution. Requires admin auth. Errors: `NotInitialized`, `NotAdmin` (panics if paused).
- `transfer_admin(new_admin: Address)` — Atomically transfers admin control to `new_admin`. Requires current admin auth. Panics with `NotInitialized` if uninitialized.
- `get_admin() -> Address` *(Governable)* — Returns the current admin address. Panics with `NotInitialized` if uninitialized.
- `set_admin(new_admin: Address)` *(Governable)* — Delegates to `transfer_admin`.

### Dispute Lifecycle

- `create_dispute(creator: Address, description: String, duration: u64) -> Result<u64, ArbitrationError>` — Creates a new dispute (`Open → Voting`). Requires creator auth. Errors: `OngoingDispute`, `InvalidTransition` (panics if paused or on overflow).
- `cancel_dispute(caller: Address, dispute_id: u64, reason: Option<String>) -> Result<(), ArbitrationError>` — Cancels a dispute from `Open` or `Voting`. Creator may cancel only while no votes have been cast and `now <= voting_end`; admin may cancel at any time before resolution. Errors: `ReasonTooLong`, `DisputeNotFound`, `NotInitialized`, `NotAuthorized`, `InvalidTransition` (panics if paused).
- `vote(voter: Address, dispute_id: u64, outcome: u32) -> Result<(), ArbitrationError>` — Casts a weighted vote for `outcome > 0`. Requires voter auth; voter must be a registered arbitrator and not the dispute creator. Errors: `InvalidOutcome`, `NotArbitrator`, `DisputeNotFound`, `VotingInactive`, `AlreadyVoted` (panics if paused or on overflow).
- `resolve_dispute(dispute_id: u64) -> Result<u32, ArbitrationError>` — Resolves the dispute after `voting_end`. Transitions `Voting → Resolving → Resolved`, `Tied`, or `NoQuorum` (returning `0` on `Tied`/`NoQuorum`). Errors: `DisputeNotFound`, `InvalidTransition`, `VotingNotEnded` (panics if paused or on overflow).
- `archive_dispute(admin: Address, dispute_id: u64) -> Result<(), ArbitrationError>` — Archives a `Resolved`, `Tied`, `NoQuorum`, or `Cancelled` dispute. Requires admin auth. Errors: `NotInitialized`, `NotAdmin`, `DisputeNotFound`, `InvalidTransition` (panics if paused).
- `reopen_dispute(admin: Address, dispute_id: u64, duration: u64) -> Result<(), ArbitrationError>` — Reopens an `Archived` dispute into `Voting` with a fresh voting window, clearing prior votes and restoring the creator's active-dispute guard. Requires admin auth. Errors: `NotInitialized`, `NotAdmin`, `DisputeNotFound`, `InvalidTransition` (panics if paused or on overflow).

### Pause & Multisig Pause Controls

- `pause(caller: Address) -> Option<u64>` — Pauses immediately when `threshold == 0` (admin auth, returns `None`), or proposes a pause when `threshold > 0` (signer auth, returns `Some(proposal_id)`).
- `unpause(caller: Address) -> Option<u64>` — Unpauses immediately when `threshold == 0` (admin auth, returns `None`), or proposes an unpause when `threshold > 0` (signer auth, returns `Some(proposal_id)`).
- `is_paused() -> bool` — Returns whether the contract is currently paused.
- `set_pause_signer(admin: Address, signer: Address, enabled: bool)` — Adds or removes an authorized pause signer. Requires admin auth.
- `set_pause_threshold(admin: Address, threshold: u32)` — Sets the multisig pause approval threshold. Requires admin auth.
- `approve_pause_proposal(signer: Address, proposal_id: u64)` — Records a pause signer's approval for a proposal.
- `execute_pause_proposal(proposal_id: u64)` — Executes a pause/unpause proposal once approvals meet `threshold`.

### Read-Only Views

- `get_quorum() -> (i128, u32)` — Returns `(min_total_weight, min_voters)`.
- `get_dispute(dispute_id: u64) -> Result<Dispute, ArbitrationError>` — Retrieves dispute details. Errors: `DisputeNotFound`.
- `get_tally(dispute_id: u64, outcome: u32) -> i128` — Returns the current total weight for an outcome.
- `get_arbitrator_weight(arbitrator: Address) -> Result<u32, ArbitrationError>` — Returns a registered arbitrator's weight. Errors: `NotArbitrator`.
- `has_voted(dispute_id: u64, voter: Address) -> bool` — Returns whether `voter` has voted on `dispute_id`.
- `get_arbitrators_page(cursor: u32, limit: u32) -> Result<(Vec<Address>, Option<u32>), ArbitrationError>` — Returns a paginated slice of registered arbitrators. Errors: `CursorOutOfRange`.
- `get_disputes_by_creator(creator: Address, cursor: u32, limit: u32) -> Result<(Vec<u64>, Option<u32>), ArbitrationError>` — Returns a paginated slice of dispute IDs created by `creator`. Errors: `CursorOutOfRange`.

For full parameter details, pagination semantics, and error behavior, see [arbitration_api.md](arbitration_api.md).

## Events

- `arbitrator_registered` — Emitted when an arbitrator is registered or updated
- `arbitrator_unregistered` — Emitted when an arbitrator is removed
- `dispute_created` — Emitted when a new dispute is opened
- `status_transition` — Emitted on every status change `(from, to)`
- `vote_cast` — Emitted when an arbitrator casts a vote
- `dispute_cancelled` — Emitted when a dispute is cancelled
- `dispute_resolved` — Emitted when a dispute is resolved with a clear winner
- `dispute_tied` — Emitted when a dispute resolution results in a tie
- `quorum_not_met` — Emitted when resolution ends without meeting configured quorum
- `quorum_set` — Emitted when quorum parameters are updated
- `dispute_archived` — Emitted when an admin archives a finalized dispute
- `dispute_reopened` — Emitted when an admin reopens an archived dispute
- `admin_transferred` — Emitted when admin control is transferred
- `paused` / `unpaused` / `pause_proposed` / `pause_approved` / `pause_signer_set` / `pause_threshold_set` — Emitted by pause controls

## Security

- Admin-only functions for arbitrator management, quorum configuration, archiving, and reopening
- Authorization required for creating disputes, cancelling disputes, and casting votes
- Double-voting prevention
- Time-bound voting periods
- Overflow protection for weight tallies and counters
- Canonical status machine prevents invalid state transitions
- Result-based error handling for all state-changing operations
- Creator cancellation is bounded by vote count and voting deadline to prevent discarding a losing tally
- Conflict-of-interest check prevents a creator from voting on their own dispute

## Testing

The contract includes comprehensive test coverage:

- Basic arbitration flow (creation, voting, resolution)
- Tie scenarios
- Double-voting prevention
- Unauthorized voter rejection
- Creator cancellation restrictions (before votes, after votes, after deadline)
- Admin cancellation override after votes
- Creator cannot vote on own dispute
- All valid status transitions
- All invalid status transitions (regression tests)
- Edge cases (zero/negative weights, outcome validation, etc.)

Run tests:

```bash
cargo test -p credence_arbitration
```
