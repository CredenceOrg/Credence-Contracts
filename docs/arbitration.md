# Arbitration Voting System

The `CredenceArbitration` contract provides a weighted voting mechanism for dispute resolution, allowing authorized arbitrators to decide on outcomes.

## Overview

Disputes are created with a specific duration. During this time, registered arbitrators can cast weighted votes for different outcomes. Once the voting period ends, the dispute can be resolved, and the outcome with the highest total weight is declared the winner.

## Dispute Status Machine

Disputes follow a canonical status machine with enforced transitions:

```
Open ──────> Voting ──────> Resolving ──────> Resolved
  │            │
  └────────────┴──────────> Cancelled
```

### Valid Transitions

- `Open → Voting` — Voting period begins (implicit at creation)
- `Voting → Resolving` — Voting period ends, `resolve_dispute` called
- `Voting → Cancelled` — Dispute cancelled by creator or admin
- `Resolving → Resolved` — Outcome tallied and stored
- `Open → Cancelled` — Cancelled before voting starts

All other transitions are rejected with `ArbitrationError::InvalidTransition`.

## Types

### DisputeStatus

| Status     | Value | Description                                      |
|------------|-------|--------------------------------------------------|
| Open       | 0     | Initial state (immediately transitions to Voting)|
| Voting     | 1     | Arbitrators can cast votes                       |
| Resolving  | 2     | Tallying votes (transient state)                 |
| Resolved   | 3     | Final outcome determined                         |
| Cancelled  | 4     | Dispute cancelled by creator or admin            |

### Dispute

| Field         | Type           | Description                                      |
|---------------|----------------|--------------------------------------------------|
| id            | u64            | Unique identifier for the dispute                |
| creator       | Address        | Address that created the dispute                 |
| description   | String         | Brief description of the dispute                 |
| voting_start  | u64            | Timestamp when voting begins                     |
| voting_end    | u64            | Timestamp when voting ends                       |
| status        | DisputeStatus  | Current status in the lifecycle                  |
| outcome       | u32            | The winning outcome (0 if unresolved or tie)     |

### ArbitrationError

| Error                | Code | Description                                      |
|----------------------|------|--------------------------------------------------|
| InvalidTransition    | 1    | Attempted an invalid status transition           |
| AlreadyInitialized   | 2    | Contract already initialized                     |
| NotInitialized       | 3    | Contract not initialized                         |
| NotAdmin             | 4    | Caller is not the admin                          |
| NotArbitrator        | 5    | Voter is not a registered arbitrator             |
| AlreadyVoted         | 6    | Arbitrator already voted on this dispute         |
| VotingInactive       | 7    | Voting period is not active                      |
| VotingNotEnded       | 8    | Voting period has not ended yet                  |
| DisputeNotFound      | 9    | Dispute ID does not exist                        |
| InvalidOutcome       | 10   | Outcome must be > 0                              |
| WeightNotPositive    | 11   | Arbitrator weight must be positive               |
| NotAuthorized        | 12   | Caller not authorized for this action            |

## Contract Functions

### `initialize(admin: Address) -> Result<(), ArbitrationError>`
Sets the contract administrator. Can only be called once.

### `register_arbitrator(arbitrator: Address, weight: i128) -> Result<(), ArbitrationError>`
Registers or updates an arbitrator with a specific voting weight. Requires admin authorization. Weight must be positive.

### `unregister_arbitrator(arbitrator: Address) -> Result<(), ArbitrationError>`
Removes an arbitrator's voting rights. Requires admin authorization.

### `create_dispute(creator: Address, description: String, duration: u64) -> Result<u64, ArbitrationError>`
Creates a new dispute. Requires creator authorization. Returns the dispute ID. Status starts as `Voting`.

### `cancel_dispute(caller: Address, dispute_id: u64) -> Result<(), ArbitrationError>`
Cancels a dispute. Only the creator or admin may cancel. Valid from `Open` or `Voting` status.

### `vote(voter: Address, dispute_id: u64, outcome: u32) -> Result<(), ArbitrationError>`
Casts a weighted vote for an outcome. Requires voter authorization. Voter must be a registered arbitrator. Dispute must be in `Voting` status.

### `resolve_dispute(dispute_id: u64) -> Result<u32, ArbitrationError>`
Resolves the dispute after the voting period has ended. Transitions `Voting → Resolving → Resolved`. Calculates the winning outcome based on total weight. Handles ties by returning 0.

### `get_dispute(dispute_id: u64) -> Result<Dispute, ArbitrationError>`
Retrieves the details of a specific dispute.

### `get_tally(dispute_id: u64, outcome: u32) -> i128`
Returns the current total weight for a specific outcome.

## Events

- `arbitrator_registered` — Emitted when an arbitrator is registered or updated
- `arbitrator_unregistered` — Emitted when an arbitrator is removed
- `dispute_created` — Emitted when a new dispute is opened
- `status_transition` — Emitted on every status change (from, to)
- `vote_cast` — Emitted when an arbitrator casts a vote
- `dispute_cancelled` — Emitted when a dispute is cancelled
- `dispute_resolved` — Emitted when a dispute is resolved

## Security

- Admin-only functions for arbitrator management
- Authorization required for creating disputes and casting votes
- Double-voting prevention
- Time-bound voting periods
- Overflow protection for weight tallies and counters
- Canonical status machine prevents invalid state transitions
- Result-based error handling for all state-changing operations

## Testing

The contract includes comprehensive test coverage:

- Basic arbitration flow (creation, voting, resolution)
- Tie scenarios
- Double-voting prevention
- Unauthorized voter rejection
- All valid status transitions
- All invalid status transitions (regression tests)
- Edge cases (zero/negative weights, outcome validation, etc.)

Run tests:
```bash
cargo test -p credence_arbitration
```
