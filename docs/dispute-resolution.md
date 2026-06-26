# Arbitration: Dispute Resolution

This document describes the dispute resolution lifecycle and quorum configuration for the `credence_arbitration` contract.

## Dispute Lifecycle

```
Open → Voting → Resolving → Resolved
  ↘        ↘
  Cancelled  Cancelled
```

Valid transitions enforced by the status machine:

| From | To | Trigger |
|---|---|---|
| Open | Voting | `create_dispute` (implicit) |
| Open | Cancelled | `cancel_dispute` (creator or admin) |
| Voting | Resolving | `resolve_dispute` (after voting ends) |
| Voting | Cancelled | `cancel_dispute` (creator or admin) |
| Resolving | Resolved | `resolve_dispute` (after tally) |

## Quorum Gate

The admin may set two quorum parameters via `set_quorum`:

- **`min_total_weight`** (`i128`) — minimum sum of vote weights required
- **`min_voters`** (`u32`) — minimum number of distinct voters required

Both default to `0`, preserving legacy behaviour (no quorum gate).

### Resolution flow with quorum

1. Voting period ends
2. Quorum check (before the Resolving transition):
   - Sum all vote weights across all outcomes
   - Count distinct voters from `VoterCounter`
   - If either threshold is unmet → emit `quorum_not_met` event, return `QuorumNotMet`
   - Dispute **stays in Voting**; caller may retry after more votes are cast
3. Transition to Resolving
4. Tally votes → determine winner
5. Transition to Resolved

### Error

`ArbitrationError::QuorumNotMet` (13) — returned when quorum thresholds are not satisfied.

### Events

| Event | Topics | Data | Trigger |
|---|---|---|---|
| `quorum_set` | `("quorum_set",)` | `(min_total_weight, min_voters)` | `set_quorum` |
| `quorum_not_met` | `("quorum_not_met", dispute_id)` | `(total_weight, min_total_weight, voter_count, min_voters)` | `resolve_dispute` when quorum not met |

## Admin Functions

- `set_quorum(admin, min_total_weight, min_voters)` — requires admin auth
- `get_quorum()` — returns `(min_total_weight, min_voters)`

## Edge Cases

- **Weight quorum met, voter quorum not met** → `QuorumNotMet`
- **Voter quorum met, weight quorum not met** → `QuorumNotMet`
- **Both met** → resolution proceeds
- **Default (0, 0)** → legacy behaviour, no quorum gate
- **Single voter under `min_voters`** → `QuorumNotMet`

## Tests

Quorum tests are in:
- `contracts/arbitration/src/test.rs` — basic config + single-voter edge case
- `contracts/arbitration/src/test_lifecycle.rs` — lifecycle integration tests for all quorum branches
