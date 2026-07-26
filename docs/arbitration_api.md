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
| `resolved` | `bool` | Whether the dispute has been finalized. |
| `outcome` | `u32` | The winning result (0 = tie/unresolved). |

---

## ⚙️ Initialization & Administration

### `initialize(e: Env, admin: Address)`

Sets the master administrator who can manage the arbitrator pool. Can only be called once.

### `register_arbitrator(e: Env, arbitrator: Address, weight: i128)`

Adds or updates an arbitrator's voting power.

* **Weight**: Higher weight gives the arbitrator's vote more influence in the final tally.
* **Auth**: Requires Admin signature.

### `unregister_arbitrator(e: Env, arbitrator: Address)`

Removes an arbitrator from the pool, revoking their ability to vote on new or active disputes.

* **Auth**: Requires Admin signature.

---

## 🏛 Dispute Workflow

### `create_dispute(e: Env, creator: Address, description: String, duration: u64) -> u64`

Opens a new case for arbitration.

* **Duration**: Number of seconds the voting window remains open.
* **Returns**: The `id` of the newly created dispute.

### `vote(e: Env, voter: Address, dispute_id: u64, outcome: u32)`

Casts a weighted vote for a specific outcome.

* **Weighted Tally**: The total score for an outcome is the sum of the `weight` of all arbitrators who voted for it.
* **Constraints**:
* Voter must be a registered arbitrator.
* One vote per arbitrator per dispute.
* Must occur between `voting_start` and `voting_end`.



### `resolve_dispute(e: Env, dispute_id: u64) -> u32`

Calculates the winner and closes the case.

* **Resolution Logic**: The outcome with the highest total weight wins.
* **Ties**: If two or more outcomes have the same maximum weight, the outcome is set to `0` (Unresolved).
* **Prerequisite**: Must be called after the `voting_end` timestamp.

---

## 🔍 Read-Only View Functions

| Function | Returns | Description |
| --- | --- | --- |
| `get_dispute` | `Dispute` | Returns the full details and current status of a dispute. |
| `get_tally` | `i128` | Returns the current accumulated weight for a specific outcome. |
| `get_arbitrator_weight` | `Result<u32, Error>` | Returns the voting weight of an arbitrator. Returns `Error::NotArbitrator` if unregistered. |
| `has_voted` | `bool` | Returns `true` if the arbitrator has already voted on the specified dispute, `false` otherwise. |
| `get_arbitrators_page` | `(Vec<Address>, Option<u32>)` | Returns a page of registered arbitrator addresses starting at `cursor` up to `limit`. |

### 📖 Pagination Semantics (`get_arbitrators_page`)

- **Cursor-based**: Supply a 0-based index `cursor` to begin fetching results from.
- **Deterministic Ordering**: The order of returned arbitrator addresses matches their registration order.
- **Compaction**: When an arbitrator is removed, the registry is compacted without leaving gaps, and subsequent registrations append to the end.
- **Clamping**: The `limit` parameter is clamped to a hard cap of `200` to prevent gas limit exhaustion on-chain. If `limit` is set to `0`, a default limit of `50` is applied.
- **Returned Value**:
  - The first element is a `Vec<Address>` representing the page of arbitrators.
  - The second element is `Some(next_cursor)` containing the index of the next item to fetch if more results remain, or `None` if the pagination is complete.

---

## 📋 Summary of Error States

| Panic / Error Variant | Cause |
| --- | --- |
| `AlreadyInitialized` | Attempted to call `initialize` more than once. |
| `WeightNotPositive` | Attempted to register an arbitrator with a weight $\le 0$. |
| `VotingInactive` | Attempted to vote before the start or after the end time, or dispute not in Voting state. |
| `AlreadyVoted` | Attempted to cast multiple votes on the same dispute. |
| `VotingNotEnded` | Attempted to call `resolve_dispute` before the deadline. |
| `NotArbitrator` | The specified address is not a registered arbitrator (returned by `get_arbitrator_weight` or `vote`). |
| `DisputeNotFound` | The specified dispute ID does not exist in storage. |
| `InvalidTransition` | Attempted an invalid dispute status transition. |
| `NotAuthorized` | Caller is not authorized (e.g. non-creator/non-admin trying to cancel). |

