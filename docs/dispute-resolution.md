# Arbitration: Dispute Resolution

This document describes the dispute resolution lifecycle and quorum configuration for the `credence_arbitration` contract.

## Dispute Lifecycle

```
Open → Voting → Resolving → Resolved
  ↘        ↘         ↘
  Cancelled  Cancelled  Tied
```

Valid state transitions:

| From     | To       | Trigger           | Authorisation              |
| -------- | -------- | ----------------- | -------------------------- |
| Open     | Resolved | `resolve_dispute` | resolver only              |
| Resolved | Closed   | `close`           | resolver only              |
| Open     | Closed   | `close`           | resolver only (if allowed) |
| (any)    | (same)   | (no‑op)           | –                          |

## Closure Invariants

- **No double‑close**  
  Calling `close` on a dispute already in `Closed` state reverts with `AlreadyClosed`.

- **No unauthorised close**  
  Only the designated `resolver` (set at dispute creation) may call `close`. Any other account triggers `Unauthorized`.

- **Deterministic terminal state**  
  Once a dispute is `Closed`, **no further state changes are allowed**. All mutating functions (`resolve_dispute`, `close`, and any future extensions) will revert with `DisputeClosed` error, ensuring a permanent and unambiguous final state.

## Functions

| Function          | Parameters                            | Description                                     |
| ----------------- | ------------------------------------- | ----------------------------------------------- |
| `create_dispute`  | `resolver: Address` → `u64`           | Creates a new dispute with the given resolver.  |
| `get_dispute`     | `id: u64` → `Dispute`                 | Returns the dispute details.                    |
| `resolve_dispute` | `id: u64, outcome: String` → `Result` | Sets the outcome and transitions to `Resolved`. |
| `close`           | `id: u64` → `Result`                  | Finalises the dispute (transition to `Closed`). |

## Errors

| Error             | Code | Description                                |
| ----------------- | ---- | ------------------------------------------ |
| `DisputeNotFound` | 1    | The given dispute ID does not exist.       |
| `AlreadyClosed`   | 2    | Attempt to close a dispute already closed. |
| `Unauthorized`    | 3    | Caller is not the authorised resolver.     |
| `DisputeClosed`   | 4    | Attempt to modify a closed dispute.        |

## Events

| Event              | Topics             | Data              | Trigger           |
| ------------------ | ------------------ | ----------------- | ----------------- |
| `dispute_closed`   | `("closed", id)`   | `("by", Address)` | `close`           |
| `dispute_resolved` | `("resolved", id)` | `("by", Address)` | `resolve_dispute` |

## Authorisation

- The `resolver` address is immutable and set at creation.
- Only the resolver can call `resolve_dispute` and `close`.
- No other roles (e.g., admin) exist in this contract.

## Security & Determinism

- The `Closed` state is final – the contract guarantees that once a dispute is closed, its outcome cannot be altered, ensuring determinism for off‑chain consumers (e.g., slashing, payouts).
- All checks are performed upfront; no re‑entrancy or storage corruption risks.

## Tied vs. Resolved

When `resolve_dispute` is called after the voting period ends:

- **Clear Winner**: Highest-weight outcome is unique → transitions to `Resolved` with `outcome = &lt;winning_outcome&gt;`
- **Tie**: Two or more outcomes have equal highest weight → transitions to `Tied` with `outcome = 0`

The `Tied` state makes tie ambiguity explicit. Outcome 0 is reserved (rejected by `vote` as `InvalidOutcome`), so a dispute in the `Tied` state with `outcome = 0` cannot be confused with a valid ruling. Consumers (e.g., slashing/settlement logic) must handle `Tied` separately from `Resolved`.

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

| Event            | Topics                           | Data                                                        | Trigger                               |
| ---------------- | -------------------------------- | ----------------------------------------------------------- | ------------------------------------- |
| `quorum_set`     | `("quorum_set",)`                | `(min_total_weight, min_voters)`                            | `set_quorum`                          |
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

## Arbitration-backed resolution (`dispute_resolution`)

A `dispute_resolution` dispute can delegate its ruling to a `credence_arbitration`
contract instead of a single resolver address. The two contracts are connected via
a binding record, so a dispute is resolved from an on-chain weighted vote with no
off-chain glue.

### Binding

`create_arbitration_dispute(creator, arbitration_contract, arbitration_dispute_id)`
creates a dispute whose `resolver` is `arbitration_contract` and writes an
`ArbitrationBinding { arbitration_contract, arbitration_dispute_id }` at
`DataKey::ArbitrationBinding(id)`. The binding is the sole thing that makes
`resolve` require a verified ruling.

### Verified resolution

`resolve(caller, id)` still requires `caller == dispute.resolver`, so only the
bound arbitration contract can move the dispute to `Resolved`. When a binding
exists, `resolve` reads the arbitration dispute through the cross-contract client
(`interfaces::arbitration::ArbitrationClient`) and applies the ruling:

| Arbitration status | Outcome               | `dispute_resolution` result          |
| ------------------ | --------------------- | ------------------------------------ |
| `Resolved`         | non-zero (a winner)   | dispute → `Resolved`                 |
| `Resolved`         | `0` (inconsistent)    | `DisputeError::ArbitrationNotResolved` (4) |
| `Tied`             | `0`                   | `DisputeError::ArbitrationTied` (5)   |
| `NoQuorum`         | `0`                   | `DisputeError::ArbitrationNoQuorum` (6) |
| anything else      | –                     | `DisputeError::ArbitrationNotResolved` (4) |

A rejected ruling leaves the dispute `Open`, so the same call can be retried once
the arbitration dispute reaches a final ruling. A dispute with no binding keeps
the original behaviour exactly: its plain address resolver resolves it as before.

### Events

| Event       | Topics                       | Data                                                  | Trigger                        |
| ----------- | ---------------------------- | ----------------------------------------------------- | ------------------------------ |
| `arb_bound` | `("arb_bound", id)`          | `(arbitration_contract, arbitration_dispute_id)`      | `create_arbitration_dispute`   |
| `arb_res`   | `("arb_res", id)`            | `(arbitration_dispute_id, outcome)`                   | arbitration-backed `resolve`   |

### Errors

| Error                     | Code | Description                                              |
| ------------------------- | ---- | -------------------------------------------------------- |
| `ArbitrationNotResolved`  | 4    | Bound arbitration dispute has no final `Resolved` ruling. |
| `ArbitrationTied`         | 5    | Bound arbitration dispute ended in a tie.                 |
| `ArbitrationNoQuorum`     | 6    | Bound arbitration dispute ended without meeting quorum.   |
