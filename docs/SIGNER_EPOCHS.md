# Signer Epochs — Motivation and Bumping Cadence

## Audience: Contributor

This document explains why pause proposals on the **multisig contract**
(`credence_multisig`) are scoped to ledger epoch buckets, when those buckets
advance ("bump"), and how `require_matching_signer_epoch` rejects approvals and
executions that reference a stale bucket. It is written for contributors
working on the multisig pause path or reviewing epoch-related security changes.

For the same model on the delegation contract's admin pause path, see
[ADMIN_EPOCHS.md](ADMIN_EPOCHS.md); the shared hash-derivation formula lives in
[proposal-id-derivation.md](proposal-id-derivation.md).

---

## Motivation

Signer epochs solve two problems at once on the multisig pause path.

### 1. Stale authority

A pause proposal that stays approvable forever is a standing hazard:

1. A pause signer proposes `Pause` during an incident and collects some
   approvals.
2. The signing round stalls; the incident is resolved another way.
3. Many ledgers later, another signer approves the **old** `proposal_id` and
   anyone calls `execute_pause_proposal` — the contract pauses for a reason
   that no longer exists.

With signer epochs, a proposal is only actionable while the ledger is still in
the same epoch bucket that produced its ID. Once the bucket rolls, approvals
and executions of the old ID fail with `ContractError::StaleSignerEpoch`
(code **515**).

### 2. Concurrent-submission convergence

Proposal IDs are derived by hashing, not by a counter, so two signers who
independently call `pause` in the same epoch derive the **same** `proposal_id`
and their approvals accumulate on one shared proposal. A counter-based scheme
would give each submission its own ID and split the vote (see
[proposal-id-derivation.md](proposal-id-derivation.md) for the full
post-mortem of that failure mode).

The epoch bucket is the boundary of both guarantees: inside one bucket,
same-action submissions converge and votes accumulate; across buckets, IDs
diverge and stale proposals die.

---

## Epoch Definition and Bumping Cadence

```
epoch = ledger_sequence / PROPOSAL_EPOCH_SIZE
```

`PROPOSAL_EPOCH_SIZE` is **100** ledger sequences per bucket, defined in
`contracts/credence_multisig/src/pausable.rs`:

```rust
/// Number of ledger sequences per signer pause-proposal epoch bucket.
pub const PROPOSAL_EPOCH_SIZE: u32 = 100;
```

The epoch index bumps **automatically** every time the network crosses a
multiple of `PROPOSAL_EPOCH_SIZE` ledgers — there is no "bump epoch"
entrypoint, no admin action, and no storage write. At Stellar's ≈5-second
ledger close time, one bucket is roughly **8 minutes** of wall-clock time.

That cadence is a deliberate tradeoff:

| `PROPOSAL_EPOCH_SIZE` | Convergence window (≈5 s/ledger) | Re-proposal delay after abandon |
| --- | --- | --- |
| 10 | ~50 seconds | ~50 seconds |
| **100 (current)** | **~8 minutes** | **~8 minutes** |
| 1000 | ~1.4 hours | ~1.4 hours |

The value must be comfortably larger than a realistic multisig signing
round-trip (so signers rarely straddle a boundary mid-round), yet small enough
that an abandoned pause proposal cannot sit actionable for hours. Changing it
is a consensus-visible change to proposal identity: proposals in flight at
upgrade time land in a different bucket arithmetic and become stale.

---

## The Guard

Signer pause proposal IDs embed the epoch by construction:

```
epoch    = ledger_sequence / PROPOSAL_EPOCH_SIZE
preimage = action_u32_be ++ epoch_u32_be   (8 bytes)
id       = first 8 bytes of SHA-256(preimage) as big-endian u64
```

where `action` is `1` (Pause) or `2` (Unpause). Because the ID is a pure
function of `(action, epoch)`, checking freshness is just re-deriving the ID
for the **current** ledger and comparing
(`contracts/credence_multisig/src/pausable.rs`):

```rust
fn require_matching_signer_epoch(e: &Env, action: PauseAction, ep: u64) {
    let expected_id = derive_proposal_id(e, action);
    if ep != expected_id {
        panic_with_error!(e, ContractError::StaleSignerEpoch);
    }
}
```

### Where it is enforced

| Entrypoint | Epoch check | Notes |
| --- | --- | --- |
| `pause` / `unpause` (threshold > 0) | Implicit | Derives a fresh ID from the current epoch, so it is always in-bucket; returns `Some(proposal_id)` |
| `pause` / `unpause` (threshold == 0) | None | Direct admin path — no proposal exists, returns `None` |
| `approve_pause_proposal` | **Explicit** | Loads the stored action for `proposal_id`, then `require_matching_signer_epoch` before recording the approval |
| `execute_pause_proposal` | **Explicit** | Same check before counting approvals against the threshold and mutating pause state |

`StaleSignerEpoch` is classified **non-retryable** in `credence_errors`:
retrying the same call can never succeed because the bucket has already
rolled. The correct recovery is to call `pause`/`unpause` again, which starts
a fresh proposal in the current epoch. Votes never carry across the boundary.

---

## Worked Example

With `PROPOSAL_EPOCH_SIZE = 100`:

| Ledger sequence | Epoch | What happens |
| --- | --- | --- |
| 50 | 0 | Signer A calls `pause` → proposal `id₀ = derive(Pause, epoch=0)` |
| 75 | 0 | Signer B calls `approve_pause_proposal(id₀)` → **passes** (same bucket) |
| 99 | 0 | Last sequence of epoch 0 — `id₀` still approvable / executable |
| 100 | 1 | Bucket bumps: `derive(Pause, epoch=1) ≠ id₀` |
| 100 | 1 | `approve_pause_proposal(id₀)` → **`StaleSignerEpoch` (515)** |
| 100 | 1 | Signer A calls `pause` again → **new** proposal `id₁`, fresh vote count |
| 1050 | 10 | Any call referencing `id₀` is rejected the same way |

---

## Guard Matrix and Tests

| Case | Ledger relative to proposal | Expected |
| --- | --- | --- |
| Same epoch | Still inside the proposal's bucket | Approval / execution succeeds (auth + threshold permitting) |
| Off-by-one | First sequence of the next bucket | `StaleSignerEpoch` |
| Ancient | Many buckets later | `StaleSignerEpoch` |

All three cells — plus the stale-**execution** case, which asserts the
contract stays unpaused — are locked by the regression suite in
`contracts/credence_multisig/src/test_signer_epoch_guard.rs` (issue #838).
The off-by-one test is the sharpest edge: it proposes at sequence
`PROPOSAL_EPOCH_SIZE - 1` and approves at `PROPOSAL_EPOCH_SIZE`, one ledger
later, proving the boundary is exact rather than approximate.

```rust
let epoch_boundary = u32::from(PROPOSAL_EPOCH_SIZE);
env.ledger().with_mut(|l| {
    l.sequence_number = epoch_boundary - 1;
});

let id = client.pause(&s1).unwrap();

env.ledger().with_mut(|l| {
    l.sequence_number = epoch_boundary;
});

let res = client.try_approve_pause_proposal(&s2, &id);
assert!(res.is_err(), "off-by-one epoch approval must fail");
let err = res.unwrap_err().unwrap();
assert_eq!(err, soroban_sdk::Error::from_contract_error(515)); // StaleSignerEpoch
```

Run the suite with:

```
cargo test -p credence_multisig signer_epoch
```

---

## Cross-References

- [ADMIN_EPOCHS.md](ADMIN_EPOCHS.md) — the same epoch model on the delegation
  contract's admin pause path (`StaleAdminEpoch`)
- [proposal-id-derivation.md](proposal-id-derivation.md) — hash-derivation
  formula, counter-scheme post-mortem, `PROPOSAL_EPOCH_SIZE` tuning
- [multisig.md](multisig.md) — multisig contract overview and API
- [pause-signer-invariant.md](pause-signer-invariant.md) — pause signer set and
  threshold guarantees
- [governance.md](governance.md) — higher-level pause and emergency flows
- [error-codes-wire.md](error-codes-wire.md) — surfacing error 515 to off-chain
  clients
- [TIME_UNITS.md](TIME_UNITS.md) — epochs here are **ledger-sequence buckets**,
  not Unix time

---

## Version History

| Version | Date | Notes |
| --- | --- | --- |
| 1.0 | 2026-07-27 | Initial signer-epoch motivation and bumping cadence |
