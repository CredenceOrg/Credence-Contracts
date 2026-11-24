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
| --------- | ----- | ------------------------------------------------- |
| Open      | 0     | Initial state (immediately transitions to Voting) |
| Voting    | 1     | Arbitrators can cast votes                        |
| Resolving | 2     | Tallying votes (transient state)                  |
| Resolved  | 3     | Final outcome determined with a clear winner      |
| Cancelled | 4     | Dispute cancelled by creator or admin             |
| Tied      | 5     | Votes resulted in a tie (equal highest weights)   |
| Archived  | 6     | Finalized dispute archived by an admin             |
| NoQuorum  | 7     | Voting ended without meeting quorum; outcome is 0   |
| Archived  | 6     | Dispute archived by admin (can be reopened)       |
| NoQuorum  | 7     | Voting ended but quorum was not met               |
| Appealed  | 8     | Second voting round in progress (non-terminal)    |
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

| Error              | Code | Description                              |
| ------------------ | ---- | ---------------------------------------- |
| InvalidTransition  | 1    | Attempted an invalid status transition   |
| AlreadyInitialized | 2    | Contract already initialized             |
| NotInitialized     | 3    | Contract not initialized                 |
| NotAdmin           | 4    | Caller is not the admin                  |
| NotArbitrator      | 5    | Voter is not a registered arbitrator     |
| AlreadyVoted       | 6    | Arbitrator already voted on this dispute |
| VotingInactive     | 7    | Voting period is not active              |
| VotingNotEnded     | 8    | Voting period has not ended yet          |
| DisputeNotFound    | 9    | Dispute ID does not exist                |
| InvalidOutcome     | 10   | Outcome must be > 0                      |
| WeightNotPositive  | 11   | Arbitrator weight must be positive       |
| NotAuthorized      | 12   | Caller not authorized for this action    |
| QuorumNotMet       | 13   | Quorum requirements were not met         |
| ReasonTooLong      | 14   | Cancellation reason exceeds the limit    |
| PromiseNotKept     | 15   | The promise outcome did not match reality |
| CursorOutOfRange   | 16   | Pagination cursor is out of range         |
| DisputeActive      | 17   | Dispute is still active and blocks this action |
| OngoingDispute     | 18   | Creator already has an unresolved dispute |
| CancelNotAllowed   | 19   | Creator cannot cancel after votes or after voting_end |
| ConflictOfInterest | 20   | Voter is the creator of the dispute       |
| Error                    | Code | Description                                      |
| ------------------------ | ---- | ------------------------------------------------ |
| InvalidTransition        | 1    | Attempted an invalid status transition           |
| AlreadyInitialized       | 2    | Contract already initialized                     |
| NotInitialized           | 3    | Contract not initialized                         |
| NotAdmin                 | 4    | Caller is not the admin                          |
| NotArbitrator            | 5    | Voter is not a registered arbitrator             |
| AlreadyVoted             | 6    | Arbitrator already voted on this dispute         |
| VotingInactive           | 7    | Voting period is not active                      |
| VotingNotEnded           | 8    | Voting period has not ended yet                  |
| DisputeNotFound          | 9    | Dispute ID does not exist                        |
| InvalidOutcome           | 10   | Outcome must be > 0                              |
| WeightNotPositive        | 11   | Arbitrator weight must be positive               |
| NotAuthorized            | 12   | Caller not authorized for this action            |
| QuorumNotMet             | 13   | Round-one quorum requirements were not met       |
| ReasonTooLong            | 14   | Cancellation reason exceeds the limit            |
| PromiseNotKept           | 15   | The promise outcome did not match reality        |
| CursorOutOfRange         | 16   | Pagination cursor is out of range                |
| DisputeActive            | 17   | Dispute is still active and blocks this action   |
| OngoingDispute           | 18   | Creator already has an unresolved dispute        |
| AppealWindowClosed       | 19   | Appeal window has passed                         |
| AlreadyAppealed          | 20   | Dispute has already been appealed once           |
| NotAppealable            | 21   | Dispute status is not appealable                 |
| NotAParty                | 22   | Caller is not the dispute creator                |
| ArbitratorVotedInRoundOne | 23  | Arbitrator voted in round one; excluded from round two |
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
#### Unregistration policy

Unregistration is **always permitted**, including while the arbitrator has votes outstanding on unresolved disputes. The policy is snapshot-based:

- **Cast votes keep their weight.** `vote` records the arbitrator's weight into the dispute's tally (`DisputeVotes`) at vote time. After unregistration those recorded weights are unchanged and continue to count toward quorum (`min_total_weight`) and toward the winning outcome in `resolve_dispute`.
- **No new votes.** An unregistered arbitrator calling `vote` receives `NotArbitrator`, regardless of any votes already cast.
- **Views reflect registration only.** `get_arbitrator_weight` returns `NotArbitrator` after unregistration and the address disappears from `get_arbitrators_page`; this does not affect tallies or quorum math, which read the per-dispute snapshot.
- **Re-registration restores voting rights.** If the arbitrator is registered again they may vote on disputes that are still in `Voting` status and where they have not already voted (`AlreadyVoted` still applies per dispute).
- **Reopen consistency.** `reopen_dispute` clears the `VoterCasted` marker for every address recorded in the dispute's voter list (`DisputeVoters`), not just currently registered arbitrators, so an arbitrator unregistered and later re-registered can vote again on a reopened dispute.

### `create_dispute(creator: Address, description: String, duration: u64) -> Result<u64, ArbitrationError>`

- `get_quorum() -> (i128, u32)` — Returns `(min_total_weight, min_voters)`.
- `get_dispute(dispute_id: u64) -> Result<Dispute, ArbitrationError>` — Retrieves dispute details. Errors: `DisputeNotFound`.
- `get_tally(dispute_id: u64, outcome: u32) -> i128` — Returns the current total weight for an outcome.
- `get_arbitrator_weight(arbitrator: Address) -> Result<u32, ArbitrationError>` — Returns a registered arbitrator's weight. Errors: `NotArbitrator`.
- `has_voted(dispute_id: u64, voter: Address) -> bool` — Returns whether `voter` has voted on `dispute_id`.
- `get_arbitrators_page(cursor: u32, limit: u32) -> Result<(Vec<Address>, Option<u32>), ArbitrationError>` — Returns a paginated slice of registered arbitrators. Errors: `CursorOutOfRange`.
- `get_disputes_by_creator(creator: Address, cursor: u32, limit: u32) -> Result<(Vec<u64>, Option<u32>), ArbitrationError>` — Returns a paginated slice of dispute IDs created by `creator`. Errors: `CursorOutOfRange`.

When a dispute is created, the contract records a frozen per-dispute snapshot of all currently registered arbitrators and their weights under `DataKey::DisputeWeightSnapshot(dispute_id)` in persistent storage. Subsequent calls to `register_arbitrator` or `unregister_arbitrator` never alter the effective voting weights, tallies, or quorum calculations of already-open disputes.

**Gas and Storage Considerations**:
- `create_dispute` iterates over `ArbitratorRegistry` once to build the snapshot (`O(N)` reads where `N` is the number of registered arbitrators) and writes a single `Map<Address, i128>` entry to persistent storage.
- Storing snapshots in persistent storage (rather than instance storage) keeps instance storage bounded across disputes, and `archive_dispute` removes the snapshot entry once a dispute is archived.
- **Legacy Disputes (Migration Safety)**: Disputes created before weight snapshotting was introduced have no `DisputeWeightSnapshot` entry; `vote` detects the missing snapshot and falls back to the live `Arbitrator(voter)` registry entry so in-flight legacy disputes remain resolvable.

### `cancel_dispute(caller: Address, dispute_id: u64) -> Result<(), ArbitrationError>`

Cancels a dispute. Only the creator or admin may cancel. Valid from `Open` or `Voting` status. The creator may cancel a `Voting` dispute only while no votes have been cast (`VoterCounter(dispute_id) == 0`) and `now <= voting_end`; otherwise the call returns `CancelNotAllowed`. The admin may cancel a `Voting` dispute at any time before resolution, and the `dispute_cancelled` event reports `cancelled_by_role` as `"admin"`.

### `vote(voter: Address, dispute_id: u64, outcome: u32) -> Result<(), ArbitrationError>`

Casts a weighted vote for an outcome. Requires voter authorization. Voter must be a registered arbitrator. Dispute must be in `Voting` status. A dispute's creator cannot vote on their own dispute; such a call returns `ConflictOfInterest`.

### `resolve_dispute(dispute_id: u64) -> Result<u32, ArbitrationError>`

Resolves the dispute after the voting period has ended. Transitions `Voting → Resolving → Resolved` (or `Tied` if outcomes are tied). If configured quorum is not met, transitions `Voting → Resolving → NoQuorum`, keeps votes and tallies for audit, clears the creator's active-dispute guard, emits `quorum_not_met`, and returns 0. A NoQuorum dispute is terminal and can be archived and reopened by an admin.

### `get_dispute(dispute_id: u64) -> Result<Dispute, ArbitrationError>`

Retrieves the details of a specific dispute.

### `get_tally(dispute_id: u64, outcome: u32) -> i128`

Returns the current total weight for a specific outcome.
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

## Appeals

Parties may challenge a first-round outcome by filing an appeal. An appeal opens a fresh voting window governed by stricter quorum rules.

### Who may appeal

Only the dispute **creator** (the address that called `create_dispute`) may appeal. Third parties receive `NotAParty`.

### Appealable statuses

A dispute is appealable when it is in `Resolved`, `Tied`, or `NoQuorum` status. Any other status returns `NotAppealable`.

### Appeal window

The appeal window is set by `set_appeal_config` (default **86 400 s / 24 h**). A party may appeal while:

```
ledger_timestamp <= resolved_at + appeal_window_secs
```

The boundary is **inclusive**: an appeal submitted at exactly `resolved_at + window` is accepted; one submitted at `resolved_at + window + 1` returns `AppealWindowClosed`.

`resolved_at` is recorded when `resolve_dispute`, `no_quorum_resolve`, or round-two resolution writes the terminal status.

### Maximum appeals

Each dispute may be appealed **at most once**. A second attempt returns `AlreadyAppealed`. A second-round result (`Resolved`, `Tied`, or `NoQuorum` from the `Appealed` state) is **final**.

### Round-two quorum

Round-two quorum is configured via `set_appeal_config`:

| Parameter | Storage key | Description |
|---|---|---|
| `appeal_window_secs` | `AppealWindow` | Seconds after resolution the appeal is open |
| `appeal_min_total_weight` | `AppealMinTotalWeight` | Minimum cumulative vote weight for round two |
| `appeal_min_voters` | `AppealMinVoters` | Minimum distinct voter count for round two |

Validation at set time: `appeal_min_total_weight` must be strictly greater than the current `MinTotalWeight`, and `appeal_min_voters` must be strictly greater than the current `MinVoters`. This guarantees round two is always harder to satisfy than round one.

Default values (before `set_appeal_config` is called): `appeal_window_secs = 86 400`, `appeal_min_total_weight = 1`, `appeal_min_voters = 1`.

If round-two quorum is not met, `resolve_dispute` stores `NoQuorum` as the final outcome (it does **not** return `QuorumNotMet` for a round-two call; that error is reserved for round one).

### Round-one voter exclusion

When `appeal_dispute` is called, every arbitrator who cast a vote in round one is recorded under `RoundOneVoter(dispute_id, address)`. Any such arbitrator who attempts to vote in round two receives `ArbitratorVotedInRoundOne`. Arbitrators who did not vote in round one may participate freely.

### Status transition table (complete)

| From | To | Trigger |
|------|----|---------|
| Open | Voting | `create_dispute` |
| Open | Cancelled | `cancel_dispute` |
| Voting | Resolving | `resolve_dispute` (round one, quorum met) |
| Voting | Cancelled | `cancel_dispute` |
| Voting | NoQuorum | `no_quorum_resolve` (voting ended, quorum not met) |
| Resolving | Resolved | tally: clear winner |
| Resolving | Tied | tally: tie or no votes |
| Resolved | Archived | `archive_dispute` |
| Resolved | Appealed | `appeal_dispute` (within window, first appeal) |
| Tied | Archived | `archive_dispute` |
| Tied | Appealed | `appeal_dispute` (within window, first appeal) |
| Cancelled | Archived | `archive_dispute` |
| NoQuorum | Archived | `archive_dispute` |
| NoQuorum | Appealed | `appeal_dispute` (within window, first appeal) |
| Archived | Voting | `reopen_dispute` (admin) |
| Appealed | Resolved | round-two tally: clear winner |
| Appealed | Tied | round-two tally: tie or no votes |
| Appealed | NoQuorum | round-two `resolve_dispute`: quorum not met (final) |

All other transitions return `InvalidTransition`.

`Appealed` is **non-terminal**: `is_dispute_active` returns `true` for it, blocking new disputes by the same creator until round two concludes. `NoQuorum` is terminal like `Tied`.

### Events

| Event | Topics | Data | When |
|-------|--------|------|------|
| `dispute_appealed` | `(dispute_id,)` | `(appellant, round: u32, timestamp: u64)` | `appeal_dispute` succeeds |
| `status_transition` | `(dispute_id,)` | `(from: u32, to: u32)` | every accepted transition, including appeal transitions |

### New storage keys

| Key | Type | Description |
|-----|------|-------------|
| `AppealWindow` | `u64` | Appeal window in seconds (default 86 400) |
| `AppealMinTotalWeight` | `i128` | Round-two minimum total vote weight (default 1) |
| `AppealMinVoters` | `u32` | Round-two minimum voter count (default 1) |
| `ResolvedAt(dispute_id)` | `u64` | Ledger timestamp when the first-round outcome was stored |
| `AppealCount(dispute_id)` | `u32` | Number of times this dispute has been appealed (max 1) |
| `RoundOneVoter(dispute_id, address)` | `bool` | Marks an arbitrator as having voted in round one |

### New and updated DisputeStatus discriminants

| Status | Discriminant | Notes |
|--------|-------------|-------|
| Open | 0 | unchanged |
| Voting | 1 | unchanged |
| Resolving | 2 | unchanged |
| Resolved | 3 | unchanged |
| Cancelled | 4 | unchanged |
| Tied | 5 | unchanged |
| Archived | 6 | unchanged |
| NoQuorum | **7** | new — voting ended, quorum not met |
| Appealed | **8** | new — second voting round in progress |

### New error variants (append-only)

| Error | Discriminant | Meaning |
|-------|-------------|---------|
| `AppealWindowClosed` | **19** | Ledger time is past `resolved_at + appeal_window_secs` |
| `AlreadyAppealed` | **20** | This dispute has already been appealed once |
| `NotAppealable` | **21** | Dispute status is not `Resolved`, `Tied`, or `NoQuorum` |
| `NotAParty` | **22** | Caller is not the dispute creator |
| `ArbitratorVotedInRoundOne` | **23** | Arbitrator is excluded from round two |

### New contract functions

#### `set_appeal_config(admin, appeal_window_secs, appeal_min_total_weight, appeal_min_voters) -> Result<(), ArbitrationError>`

Admin-only. Sets the appeal window and round-two quorum. `appeal_window_secs` must be > 0. Both quorum values must be strictly greater than the corresponding round-one values.

#### `get_appeal_config() -> (u64, i128, u32)`

Returns `(appeal_window_secs, appeal_min_total_weight, appeal_min_voters)`.

#### `no_quorum_resolve(dispute_id) -> Result<(), ArbitrationError>`

Transitions a `Voting` dispute to `NoQuorum` when the voting period has ended and quorum was not met. Callable by anyone. Records `resolved_at` for the appeal window.

#### `appeal_dispute(appellant, dispute_id, duration) -> Result<(), ArbitrationError>`

Appeals a `Resolved`, `Tied`, or `NoQuorum` dispute within the appeal window. Only the creator may call this. Resets vote state, records round-one voters, transitions to `Appealed`, and starts a new voting window of `duration` seconds. Emits `dispute_appealed`.

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
- Appeal limited to one per dispute; round-one voters excluded from round two
- Appeal window enforced by ledger timestamp comparison

## Testing

The contract includes comprehensive test coverage:

- Basic arbitration flow (creation, voting, resolution)
- Tie scenarios
- Double-voting prevention
- Unauthorized voter rejection
- Creator cancellation restrictions (before votes, after votes, after deadline)
- Admin cancellation override after votes
- Creator cannot vote on own dispute
- Unregistration with outstanding votes: snapshot weight still counts toward tally and quorum, unregistered arbitrator cannot vote again, re-registration restores voting rights
- All valid status transitions
- All invalid status transitions (regression tests)
- Edge cases (zero/negative weights, outcome validation, etc.)

Run tests:

```bash
cargo test -p credence_arbitration
```

## Dispute resolution integration

Arbitration rulings back the `dispute_resolution` contract directly: a dispute
there can name an arbitration contract as its resolver, and is then resolved from
the arbitration ruling rather than a single address.

- `dispute_resolution::create_arbitration_dispute(creator, arbitration_contract, arbitration_dispute_id)`
  binds a dispute to an arbitration dispute id.
- When resolving, `dispute_resolution` reads the ruling through the shared client
  in `crates/interfaces/src/arbitration.rs` (mirroring `Dispute`/`DisputeStatus`,
  so no contract-to-contract build edge is created) and calls `get_dispute`.
- Only a final `Resolved` ruling with a non-zero outcome is accepted; `Tied`,
  `NoQuorum`, cancelled/archived, and any non-final status are rejected with typed
  errors.

See [docs/dispute-resolution.md](dispute-resolution.md) for the full
mapping and the events emitted.
