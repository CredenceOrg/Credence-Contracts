# Proposal ID Derivation — Pause System

## Why Counter-Based IDs Fail Under Concurrent Submission

Before this migration, pause proposal IDs were assigned by a monotonic
counter (`PauseProposalCounter`) that was incremented on every
`propose_action` call. Each operator submission received a unique,
strictly-increasing ID.

**Problem:** When two operators submitted the *same* logical proposal
(identical action and target contract) at nearly the same time, the
counter produced two distinct IDs. Each ID accumulated its own approval
set, so neither proposal could reach the signing threshold unless
*all* operators approved both. The system effectively required
serialised submissions — a race-condition that could prevent a timely
pause even when the required number of operators had voted.

---

## Derivation Formula

```
epoch    = ledger_sequence / PROPOSAL_EPOCH_SIZE
preimage = action_u32_big_endian ++ epoch_u32_big_endian   (8 bytes)
hash     = SHA-256(preimage)                                (32 bytes)
id       = first 8 bytes of hash interpreted as big-endian u64
```

where `action` is `1` for Pause and `2` for Unpause.

The derivation is implemented in `derive_proposal_id()` in
`contracts/credence_delegation/src/pausable.rs`. It is a **pure**
function — identical inputs always produce identical output, and it
writes nothing to storage.

---

## Epoch Bucket Size Constant

Defined as `PROPOSAL_EPOCH_SIZE = 100` in
`contracts/credence_delegation/src/pausable.rs`.

```rust
/// Number of ledger sequences that form one epoch bucket for proposal-ID
/// derivation.
///
/// **Tradeoff:** a wider bucket (larger N) gives operators a longer window in
/// which concurrent submissions of the same action converge to one ID, but also
/// means a new `(action, target_ledger)` pair reuses the same ID within the
/// epoch — even after the previous proposal is executed.  A narrow
/// bucket (smaller N) reduces the convergence window and increases the chance
/// that two operators straddle an epoch boundary.
///
/// 100 ledgers ≈ 8 minutes on Stellar (≈ 5-second close times), which is
/// comfortably wider than any realistic multisig signing round-trip.
pub const PROPOSAL_EPOCH_SIZE: u32 = 100;
```

### How to Tune

| `PROPOSAL_EPOCH_SIZE` | Convergence Window (≈5s/ledger) | Re-proposal Delay |
|------------------------|--------------------------------|-------------------|
| 10                     | ~50 seconds                    | ~50 seconds       |
| 100 (default)          | ~8 minutes                     | none (same epoch) |
| 1000                   | ~1.4 hours                     | ~1.4 hours        |
| 7200                   | ~10 hours                      | ~10 hours         |

Choose a value that is safely larger than the expected wall-clock time
for a multisig signing round-trip, but small enough that a stale or
abandoned proposal can be re-proposed without an unreasonable wait.
Re-proposal within the same epoch is supported (see below).

---

## Idempotency Guarantee

When `propose_action` is called, it:

1. Derives `proposal_id = derive_proposal_id(e, action)`.
2. Checks whether a proposal with that ID already exists in storage.
3. **If absent:** writes the proposal record, resets the approval count
   to 0, and clears the per-proposal approver list.
4. **If present:** skips the write — the existing record is preserved
   unchanged.
5. Records the caller's approval on the (existing or newly created)
   proposal.

**Result:** any number of operators submitting the same action in the
same epoch converge on one shared proposal. Votes accumulate correctly
regardless of submission order or count.

Because IDs are reused within an epoch, a fresh proposal under an ID
with no live `PauseProposal` payload always starts from a clean
approval set — earlier approvals for that ID neither block nor count
towards the new round.

### Limits

- **Different epochs:** the same action submitted in epoch *k* and
  epoch *k+1* produces two different IDs and two independent proposals.
  This is intentional — it allows re-proposing a stale action once a
  new epoch begins, and it bounds how long stale approval state can
  linger for an ID.
- **Different actions:** Pause and Unpause produce different IDs even
  within the same epoch because the action value is part of the hash
  preimage.

---

## Backward Compatibility Shim

Clients that persisted counter-based proposal IDs before the migration
can still resolve them via `get_proposal_by_legacy_id()`.

```rust
pub fn get_proposal_by_legacy_id(
    e: &Env,
    legacy_id: u64,
) -> Result<u32, ContractError>
```

- Returns `Ok(action)` if a proposal exists under the legacy ID.
- Returns `Err(ContractError::ProposalNotFound)` (code 603) if no
  proposal exists — it **never panics**.
- The public contract entrypoint `get_proposal_by_legacy_id` unwraps
  the Result and panics with `ProposalNotFound` on error, preserving
  the same error code for off-chain clients.

### Migration Note

New code should call `get_pause_proposal_state` with a hash-derived
ID instead. The legacy function is retained solely for clients that
stored counter-based IDs before the migration and need to look up
those proposals post-migration.

---

## Storage Layout After Migration

| Key | Payload | Notes |
|-----|---------|-------|
| `PauseProposalCounter` | `u64` | **Retained** for backward-compat `executed` detection in `get_pause_proposal_state`, but never incremented. |
| `PauseProposal(id)` | `u32` | Action value (`1` = Pause, `2` = Unpause). |
| `PauseApprovalCount(id)` | `u32` | Cached vote tally; revalidated at execution. |
| `PauseApproval(id, Address)` | `bool` | Per-signer approval flag. |
| `PauseApprovers(id)` | `Vec<Address>` | Bounded list of approvers for the current round (capped by `get_max_pause_signers`); reset on new payload, cleared on execution. |

When a proposal is executed, `PauseProposal(id)`, `PauseApprovalCount(id)`,
`PauseApprovers(id)`, and every `PauseApproval(id, Address)` flag for that
proposal are removed. Because IDs are reused within an epoch, leftover
approval flags must not survive execution — otherwise a re-proposal in the
same epoch would see stale approvals.

### Execution-Time Revalidation

`execute_pause_proposal` does not trust the cached `PauseApprovalCount(id)`.
It recounts approvals against the *current* pause-signer set
(`PauseSigner(addr)`), so approvals from signers removed after they voted
no longer count. When the cached and effective counts differ, the contract
emits `pause_approvals_revalidated(proposal_id, cached, effective)`.
The epoch guard (`StaleSignerEpoch`, `#515`) is unaffected by this
revalidation.
