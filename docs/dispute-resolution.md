# Dispute Resolution

This document describes the dispute lifecycle and emergency pause controls for the `dispute_resolution` contract.

## Dispute Lifecycle

```
Open → Resolved → Closed
  └─────────────→ Closed
```

Valid state transitions:

| From     | To       | Trigger   | Authorisation |
| -------- | -------- | --------- | ------------- |
| Open     | Resolved | `resolve` | resolver only |
| Open     | Closed   | `close`   | resolver only |
| Resolved | Closed   | `close`   | resolver only |

## Closure Invariants

- **No double‑close**  
  Calling `close` on a dispute already in `Closed` state reverts with `AlreadyClosed`.

- **No unauthorised close**  
  Only the designated `resolver` (set at dispute creation) may call `close`. Any other account triggers `Unauthorized`.

- **Deterministic terminal state**  
  Once a dispute is `Closed`, no further lifecycle transitions are allowed. `resolve` and `close` return `AlreadyClosed` for invalid transitions.

## Functions

| Function | Parameters | Description |
| -------- | ---------- | ----------- |
| `initialize` | `admin: Address` | Initializes the pause administrator and state; callable once. |
| `create_dispute` | `creator, resolver: Address` → `u64` | Creates an open dispute; the creator authorizes the call. |
| `get_dispute` | `id: u64` → `Result<Dispute, DisputeError>` | Returns the dispute details. |
| `resolve` | `caller: Address, id: u64` → `Result<(), DisputeError>` | Marks an open dispute as resolved. |
| `close` | `caller: Address, id: u64` → `Result<(), DisputeError>` | Closes an open or resolved dispute. |
| `pause` / `unpause` | `caller: Address` → `Option<u64>` | Pauses immediately as admin at threshold zero, or returns a proposal ID. |
| `is_paused` | — → `bool` | Returns whether lifecycle writes are paused. |
| `set_pause_signer` | `admin, signer: Address, enabled: bool` | Adds or removes an authorized pause signer. |
| `set_pause_threshold` | `admin, threshold: u32` | Sets the signer approval threshold. |
| `approve_pause_proposal` | `signer, proposal_id: u64` | Adds a signer approval to a pause proposal. |
| `execute_pause_proposal` | `proposal_id: u64` | Executes a proposal once its approval threshold is met. |

## Errors

| Error | Code | Description |
| ----- | ---- | ----------- |
| `DisputeNotFound` | 1 | The given dispute ID does not exist. |
| `AlreadyClosed` | 2 | Invalid transition from a closed or resolved state. |
| `Unauthorized` | 3 | Caller is not the authorized resolver. |
| `Paused` | 4 | State-changing dispute operation rejected while paused. |

Pause administration errors use the shared `credence_errors::ContractError`
catalogue, including `AlreadyInitialized`, `NotInitialized`, `NotAdmin`,
`NotSigner`, `ThresholdExceedsSigners`, `ProposalNotFound`, and
`InsufficientApprovals`.

## Events

| Event | Topics | Data | Trigger |
| ----- | ------ | ---- | ------- |
| `created` | `("created", id)` | `(creator, resolver)` | `create_dispute` |
| `resolved` | `("resolved", id)` | `(creator, caller)` | `resolve` |
| `closed` | `("closed", id)` | `(creator, caller)` | `close` |
| `pause_proposed` | `("pause_proposed", id)` | action code | `pause` / `unpause` |
| `pause_approved` | `("pause_approved", id)` | signer | `approve_pause_proposal` |
| `paused` / `unpaused` | corresponding event topic | proposal ID and pause reason, where applicable | pause execution |
| `pause_signer_set` / `pause_threshold_set` | corresponding event topic | updated configuration | admin configuration |

## Authorisation

- The `resolver` address is immutable and set at creation.
- Only the resolver can call `resolve` and `close`.
- `initialize(admin)` sets the administrator for pause controls.
- The administrator manages pause signers and the approval threshold.
- When the threshold is zero, only the admin can pause or unpause directly.
- When the threshold is non-zero, registered signers propose and approve pause or unpause.

## Emergency Pause

The pause guard rejects `create_dispute`, `resolve`, and `close` with
`DisputeError::Paused`. Read operations, including `get_dispute` and
`is_paused`, remain available. Pause proposals can still be approved and
executed while paused so the contract can be unpaused.

## Security & Determinism

- `Closed` is terminal; paused write attempts do not change dispute records.
- `get_dispute` and `is_paused` remain available during an emergency pause.

## Tests

Pause behavior is covered in `contracts/dispute_resolution/tests/test_pausable.rs`.
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
