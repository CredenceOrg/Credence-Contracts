This documentation provides a comprehensive reference for the `CredenceArbitration` contract, which handles decentralized dispute resolution through weighted voting by registered arbitrators.

---

# ⚖️ CredenceArbitration API Reference

The **CredenceArbitration** contract enables a multi-party arbitration system. Administrators can register trusted arbitrators with specific voting weights, and users can submit disputes that are resolved based on the majority weight of arbitrator votes.

## 🏗 Data Structures

### `Dispute`

Represents the state and parameters of an arbitration case.

| Field | Type | Description |
| :--- | :--- | :--- |
| `id` | `u64` | Unique identifier for the dispute. |
| `creator` | `Address` | The account that opened the dispute. |
| `description` | `String` | Textual details of the conflict. |
| `voting_start` | `u64` | Ledger timestamp when voting opens. |
| `voting_end` | `u64` | Ledger timestamp when voting closes. |
| `status` | `DisputeStatus` | Lifecycle state, including `NoQuorum` when configured quorum is missed. |
| `outcome` | `u32` | The winning result (`0` for `Tied` or `NoQuorum`, `> 0` for `Resolved`). |
| `cancellation_reason` | `Option<String>` | Optional reason recorded when a dispute is cancelled (max 256 bytes). |
| `cancelled_by_role` | `Option<Symbol>` | Role that cancelled the dispute (`"creator"` or `"admin"`), if cancelled. |

### `DisputeStatus`

| Status | Value | Meaning |
| :--- | ---: | :--- |
| `Open` | 0 | Initial state (immediately transitions to `Voting`). |
| `Voting` | 1 | Voting is active. |
| `Resolving` | 2 | Votes are being tallied (transient state). |
| `Resolved` | 3 | A winning outcome was determined. |
| `Cancelled` | 4 | Cancelled by the creator or admin. |
| `Tied` | 5 | No unique highest-weight outcome (`outcome == 0`). |
| `Archived` | 6 | Archived by an admin; may be reopened via `reopen_dispute`. |
| `NoQuorum` | 7 | Voting ended without reaching configured quorum (`outcome == 0`). |

---

## ⚙️ Initialization & Administration

### `initialize(e: Env, admin: Address) -> Result<(), ArbitrationError>`

Sets the master administrator who can manage the arbitrator pool and initializes pause state (`paused = false`, `pause_signer_count = 0`, `pause_threshold = 0`, `pause_proposal_counter = 0`). Can only be called once.

* **Errors**: `AlreadyInitialized`.

### `register_arbitrator(e: Env, arbitrator: Address, weight: i128) -> Result<(), ArbitrationError>`

Adds or updates an arbitrator's voting power.

* **Weight**: Higher weight gives the arbitrator's vote more influence in the final tally; must be `> 0`.
* **Auth**: Requires Admin signature.
* **Errors**: `NotInitialized`, `WeightNotPositive`. Panics with `ContractError::ContractPaused` if paused.

### `unregister_arbitrator(e: Env, arbitrator: Address) -> Result<(), ArbitrationError>`

Removes an arbitrator from the pool, revoking their ability to vote on new or active disputes, and compacts the arbitrator registry.

* **Auth**: Requires Admin signature.
* **Errors**: `NotInitialized`. Panics with `ContractError::ContractPaused` if paused.

### `set_quorum(e: Env, admin: Address, min_total_weight: i128, min_voters: u32) -> Result<(), ArbitrationError>`

Configures quorum requirements for `resolve_dispute`. Default `(0, 0)` disables quorum enforcement.

* **Auth**: Requires Admin signature.
* **Errors**: `NotInitialized`, `NotAdmin`. Panics with `ContractError::ContractPaused` if paused.

### `transfer_admin(e: Env, new_admin: Address)`

Atomically transfers administrative control to `new_admin` and emits `admin_transferred`.

* **Auth**: Requires current Admin signature.
* **Errors**: Panics with `ArbitrationError::NotInitialized` if the contract is not initialized.

### `get_admin(e: Env) -> Address` *(Governable)*

Returns the current admin address.

* **Errors**: Panics with `ArbitrationError::NotInitialized` if the contract is not initialized.

### `set_admin(e: Env, new_admin: Address)` *(Governable)*

Delegates to `transfer_admin(e, new_admin)`.

* **Auth**: Requires current Admin signature.
* **Errors**: Panics with `ArbitrationError::NotInitialized` if the contract is not initialized.

---

## 🏛 Dispute Workflow

### `create_dispute(e: Env, creator: Address, description: String, duration: u64) -> Result<u64, ArbitrationError>`

Opens a new case for arbitration. Transitions `Open → Voting` immediately.

* **Duration**: Number of seconds the voting window remains open.
* **Auth**: Requires `creator` signature.
* **Returns**: The `id` of the newly created dispute.
* **Errors**: `OngoingDispute`, `InvalidTransition`. Panics with `ContractError::ContractPaused` if paused or `ContractError::Overflow` on counter/timestamp overflow.

### `cancel_dispute(e: Env, caller: Address, dispute_id: u64, reason: Option<String>) -> Result<(), ArbitrationError>`

Cancels a dispute from `Open` or `Voting`.

* **Auth**: Requires `caller` signature (must be the dispute creator or the contract admin).
* **Constraints**: `reason`, if supplied, must be at most 256 bytes. The creator may only cancel while no votes have been cast and `now <= voting_end`; the admin may cancel at any time before resolution.
* **Errors**: `ReasonTooLong`, `DisputeNotFound`, `NotInitialized`, `NotAuthorized`, `InvalidTransition`. Panics with `ContractError::ContractPaused` if paused.

### `vote(e: Env, voter: Address, dispute_id: u64, outcome: u32) -> Result<(), ArbitrationError>`

Casts a weighted vote for a specific outcome.

* **Weighted Tally**: The total score for an outcome is the sum of the `weight` of all arbitrators who voted for it.
* **Auth**: Requires `voter` signature.
* **Constraints**:
  * `outcome` must be `> 0` (`0` is reserved for `Tied` / `NoQuorum`).
  * `voter` must be a registered arbitrator and cannot be the dispute creator.
  * One vote per arbitrator per dispute.
  * Dispute must be in `Voting` status and `voting_start <= now <= voting_end`.
* **Errors**: `InvalidOutcome`, `NotArbitrator`, `DisputeNotFound`, `VotingInactive`, `AlreadyVoted`. Panics with `ContractError::ContractPaused` if paused or `ContractError::Overflow` on tally/counter overflow.

### `resolve_dispute(e: Env, dispute_id: u64) -> Result<u32, ArbitrationError>`

Calculates the winner and closes the case.

* **Resolution Logic**: The outcome with the highest total weight wins.
* **Ties**: If two or more outcomes have the same maximum weight (or no votes were cast), status transitions to `Tied` and `outcome` is set to `0`.
* **Prerequisite**: Must be called after the `voting_end` timestamp (`now > voting_end`).
* **Quorum miss**: When configured minimum total weight or voter count is not met, the contract transitions through `Resolving` to terminal `NoQuorum`, stores outcome `0`, preserves vote tallies, releases the creator's active-dispute guard, publishes `status_transition` and `quorum_not_met`, and returns `0` successfully. The event data is `(total_weight, min_total_weight, voter_count, min_voters)`. This is a successful call so its events are persisted on-ledger.
* **Errors**: `DisputeNotFound`, `InvalidTransition`, `VotingNotEnded`. Panics with `ContractError::ContractPaused` if paused or `ContractError::Overflow` on weight sum overflow.

### `archive_dispute(e: Env, admin: Address, dispute_id: u64) -> Result<(), ArbitrationError>`

Archives a finalized dispute (`Resolved`, `Tied`, `NoQuorum`, or `Cancelled` → `Archived`).

* **Auth**: Requires Admin signature.
* **Errors**: `NotInitialized`, `NotAdmin`, `DisputeNotFound`, `InvalidTransition`. Panics with `ContractError::ContractPaused` if paused.

### `reopen_dispute(e: Env, admin: Address, dispute_id: u64, duration: u64) -> Result<(), ArbitrationError>`

Reopens an `Archived` dispute, transitioning `Archived → Voting` with a fresh voting window of `duration` seconds, resetting outcome/cancellation fields, clearing prior vote records, and re-establishing the creator's active-dispute guard.

* **Auth**: Requires Admin signature.
* **Errors**: `NotInitialized`, `NotAdmin`, `DisputeNotFound`, `InvalidTransition`. Panics with `ContractError::ContractPaused` if paused or `ContractError::Overflow` on timestamp overflow.

---

## ⏸ Pause & Multisig Pause Controls

| Function | Signature | Description | Errors / Panics |
| :--- | :--- | :--- | :--- |
| `pause` | `pause(e: Env, caller: Address) -> Option<u64>` | Immediately pauses the contract when `threshold == 0` (admin auth, returns `None`), or creates a pause proposal when `threshold > 0` (signer auth, returns `Some(proposal_id)`). | `ContractError::ContractPaused`, `ContractError::NotSigner`, `ContractError::Overflow`, or admin auth error. |
| `unpause` | `unpause(e: Env, caller: Address) -> Option<u64>` | Immediately unpauses the contract when `threshold == 0` (admin auth, returns `None`), or creates an unpause proposal when `threshold > 0` (signer auth, returns `Some(proposal_id)`). | `ContractError::InvalidPauseAction`, `ContractError::NotSigner`, `ContractError::Overflow`, or admin auth error. |
| `is_paused` | `is_paused(e: Env) -> bool` | Returns `true` if the contract is currently paused, `false` otherwise. | None. |
| `set_pause_signer` | `set_pause_signer(e: Env, admin: Address, signer: Address, enabled: bool)` | Adds or removes an authorized pause multisig signer; clamps threshold if signer count drops below it. | Admin auth / initialization error. |
| `set_pause_threshold` | `set_pause_threshold(e: Env, admin: Address, threshold: u32)` | Sets the multisig approval threshold (`0` = single-admin pause). | `ContractError::ThresholdExceedsSigners`, or admin auth / initialization error. |
| `approve_pause_proposal` | `approve_pause_proposal(e: Env, signer: Address, proposal_id: u64)` | Records a pause signer's approval for an existing proposal. | `ContractError::NotSigner`, `ContractError::ProposalNotFound`, `ContractError::Overflow`. |
| `execute_pause_proposal` | `execute_pause_proposal(e: Env, proposal_id: u64)` | Executes a pause/unpause proposal once approvals reach `threshold` and removes the proposal. | `ContractError::ProposalNotFound`, `ContractError::InsufficientApprovals`, `ContractError::ContractPaused`, `ContractError::InvalidPauseAction`. |

---

## 🔍 Read-Only View Functions

| Function | Signature | Returns | Errors | Description |
| :--- | :--- | :--- | :--- | :--- |
| `get_admin` | `get_admin(e: Env)` | `Address` | Panics with `NotInitialized` | Returns the current contract admin. |
| `get_quorum` | `get_quorum(e: Env)` | `(i128, u32)` | None | Returns `(min_total_weight, min_voters)`. |
| `get_dispute` | `get_dispute(e: Env, dispute_id: u64)` | `Result<Dispute, ArbitrationError>` | `DisputeNotFound` | Returns the full details and current status of a dispute. |
| `get_tally` | `get_tally(e: Env, dispute_id: u64, outcome: u32)` | `i128` | None | Returns the current accumulated weight for a specific outcome. |
| `get_arbitrator_weight` | `get_arbitrator_weight(e: Env, arbitrator: Address)` | `Result<u32, ArbitrationError>` | `NotArbitrator` | Returns the voting weight of an arbitrator. |
| `has_voted` | `has_voted(e: Env, dispute_id: u64, voter: Address)` | `bool` | None | Returns `true` if the arbitrator has already voted on the specified dispute. |
| `get_arbitrators_page` | `get_arbitrators_page(e: Env, cursor: u32, limit: u32)` | `Result<(Vec<Address>, Option<u32>), ArbitrationError>` | `CursorOutOfRange` | Returns a page of registered arbitrator addresses starting at `cursor` up to `limit`. |
| `get_disputes_by_creator` | `get_disputes_by_creator(e: Env, creator: Address, cursor: u32, limit: u32)` | `Result<(Vec<u64>, Option<u32>), ArbitrationError>` | `CursorOutOfRange` | Returns dispute IDs created by an address, in creation order, starting at `cursor`. |
| `is_paused` | `is_paused(e: Env)` | `bool` | None | Returns whether the contract is paused. |

### 📖 Creator Dispute Pagination (`get_disputes_by_creator`)

- The cursor is a 0-based index in the creator's dispute history.
- A `limit` of `0` uses the default page size of `50`; larger limits are capped at `200`.
- `CursorOutOfRange` is returned when `cursor` is greater than or equal to the creator's dispute count, including an empty history.
- IDs are stored in persistent per-creator index slots and appended on dispute creation. The count and each slot use a 180-day TTL, extended when written or read; inactive index entries may expire under Soroban storage TTL.

### 📖 Pagination Semantics (`get_arbitrators_page`)

- **Cursor-based**: Supply a 0-based index `cursor` to begin fetching results from.
- **Deterministic Ordering**: The order of returned arbitrator addresses matches their registration order.
- **Compaction**: When an arbitrator is removed, the registry is compacted without leaving gaps, and subsequent registrations append to the end.
- **Clamping**: The `limit` parameter is clamped to a hard cap of `200` to prevent gas limit exhaustion on-chain. If `limit` is set to `0`, a default limit of `50` is applied.
- **Returned Value**:
  - The first element is a `Vec<Address>` representing the page of arbitrators.
  - The second element is `Some(next_cursor)` containing the index of the next item to fetch if more results remain, or `None` if the pagination is complete.

---

## 📋 Complete Error Code Reference (`ArbitrationError`)

Generated directly from `contracts/arbitration/src/status.rs`. Note that `ReasonTooLong = 14` is declared before `QuorumNotMet = 13` in source; the table below is ordered by numeric discriminant.

| Code | Variant | Description |
| ---: | :--- | :--- |
| `1` | `InvalidTransition` | The requested dispute status transition is not allowed. |
| `2` | `AlreadyInitialized` | The contract has already been initialized. |
| `3` | `NotInitialized` | The contract has not been initialized. |
| `4` | `NotAdmin` | The caller is not the admin. |
| `5` | `NotArbitrator` | The caller or queried address is not a registered arbitrator. |
| `6` | `AlreadyVoted` | The voter has already cast a vote on this dispute. |
| `7` | `VotingInactive` | The dispute is not in `Voting`, or the current ledger time is outside the voting window (before `voting_start` or after `voting_end`). |
| `8` | `VotingNotEnded` | The voting window has not yet ended. |
| `9` | `DisputeNotFound` | The dispute ID does not exist. |
| `10` | `InvalidOutcome` | The supplied outcome is invalid (outcome `0` is reserved for tied or no-quorum results). |
| `11` | `WeightNotPositive` | The supplied arbitrator weight must be greater than zero. |
| `12` | `NotAuthorized` | The caller is not authorized for this action. |
| `13` | `QuorumNotMet` | Quorum was not met. Currently unused: an unmet quorum finalizes the dispute as `DisputeStatus::NoQuorum` instead of returning this error. |
| `14` | `ReasonTooLong` | The supplied reason exceeds the maximum length (256 bytes). |
| `15` | `PromiseNotKept` | The actual outcome does not match the promised outcome. |
| `16` | `CursorOutOfRange` | Pagination cursor is out of range (`cursor >= count`). |
| `17` | `DisputeActive` | Dispute is still active (`Open`, `Voting`, or `Resolving`), blocking operations that require an inactive or resolved dispute. |
| `18` | `OngoingDispute` | A creator already has an unresolved dispute tracked as active. |
