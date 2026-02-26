# Arbitration Voting System

The `CredenceArbitration` contract provides a weighted voting mechanism for dispute resolution, allowing authorized arbitrators to decide on outcomes.

## Overview

Disputes are created with a specific duration. During this time, registered arbitrators can cast weighted votes for different outcomes. Once the voting period ends, the dispute can be resolved, and the outcome with the highest total weight is declared the winner.

## Types

### Dispute

| Field         | Type     | Description                                      |
|---------------|----------|--------------------------------------------------|
| id            | u64      | Unique identifier for the dispute                |
| creator       | Address  | Address that created the dispute                 |
| description   | String   | Brief description of the dispute                 |
| voting_start  | u64      | Timestamp when voting begins                     |
| voting_end    | u64      | Timestamp when voting ends                       |
| resolved      | bool     | Whether the dispute has been resolved            |
| outcome       | u32      | The winning outcome (0 if unresolved or tie)     |

## Contract Functions

### `initialize(admin: Address)`
Sets the contract administrator. Can only be called once.

### `register_arbitrator(arbitrator: Address, weight: i128)`
Registers or updates an arbitrator with a specific voting weight. Requires admin authorization.

### `unregister_arbitrator(arbitrator: Address)`
Removes an arbitrator's voting rights. Requires admin authorization.

### `create_dispute(creator: Address, description: String, duration: u64) -> u64`
Creates a new dispute. Requires creator authorization. Returns the dispute ID.

### `vote(voter: Address, dispute_id: u64, outcome: u32)`
Casts a weighted vote for an outcome. Requires voter authorization. Voter must be a registered arbitrator.

### `resolve_dispute(dispute_id: u64) -> u32`
Resolves the dispute after the voting period has ended. Calculates the winning outcome based on total weight. Handles ties by returning 0.

### `get_dispute(dispute_id: u64) -> Dispute`
Retrieves the details of a specific dispute.

### `get_tally(dispute_id: u64, outcome: u32) -> i128`
Returns the current total weight for a specific outcome.

## Events

- `arbitrator_registered`: Emitted when an arbitrator is registered or updated.
- `arbitrator_unregistered`: Emitted when an arbitrator is removed.
- `dispute_created`: Emitted when a new dispute is opened.
- `vote_cast`: Emitted when an arbitrator casts a vote.
- `dispute_resolved`: Emitted when a dispute is resolved.

## Security

- Admin-only functions for arbitrator management.
- Authorization required for creating disputes and casting votes.
- Double-voting prevention.
- Time-bound voting periods.
- Overflow protection for weight tallies and counters.
