# Operator Epochs — Motivation and Bumping Cadence

## Audience: Operator

This document explains why operator-facing pause proposals on the delegation contract are scoped to **ledger epoch buckets**, when those buckets advance, and how a matching-epoch guard rejects stale approvals and executions. It is written for operators (e.g., multisig signers) who initiate or approve pause operations.

---

## Motivation

Pause proposals that remain approvable forever create a stale-authority hazard:

1. A pause signer proposes `pause()` during an operational incident and collects some approvals.
2. The incident is resolved out-of-band and the signing round stalls; the reason for the pause disappears.
3. Many ledgers later another signer approves the **old** `proposal_id`, and someone executes it, unintentionally halting the protocol.

Without an epoch bound, that abandoned proposal is still live. Operator epochs close the gap: a proposal is only actionable while the ledger is still in the **same epoch bucket** that produced its ID. Approvals or executions that carry a stale epoch reference are rejected with a `StaleEpoch` error.

---

## Epoch Definition and Bumping Cadence

```
epoch = ledger_sequence / PROPOSAL_EPOCH_SIZE
```

`PROPOSAL_EPOCH_SIZE` defaults to **100** ledger sequences per bucket. The epoch index advances automatically as the network produces ledgers — there is no separate "bump epoch" transaction that operators need to submit.

| Constant | Default | Wall-clock window (≈5 s / ledger) | Effect when the bucket rolls |
| --- | --- | --- | --- |
| `PROPOSAL_EPOCH_SIZE` | `100` | ≈ 8 minutes | Same-action proposals still converge inside the window; abandoned proposals stop being approvable / executable in the next bucket |

### What this means for Operators

- **Time to converge**: You have approximately 8 minutes from the first `pause()` or `unpause()` submission to collect the required signatures and execute the proposal.
- **Failed convergence**: If you miss the window (the ledger crosses a multiple of 100), the old `proposal_id` becomes permanently stale. You must submit a fresh `pause()` transaction to generate a new proposal ID in the new epoch, and gather approvals for that new ID.

---

## Concrete Example

Assume the current ledger sequence is `50` (Epoch 0), and the operator threshold is 2.

### Successful Same-Epoch Execution

1. **Signer A** calls `pause()` at ledger 50.
   - The contract derives a `proposal_id` (e.g., `12345`) bound to Epoch 0.
2. **Signer B** calls `approve_pause_proposal(&signer_b, &12345)` at ledger 75.
   - The ledger is still in Epoch 0. The approval succeeds.
3. **Anyone** calls `execute_pause_proposal(&12345)` at ledger 80.
   - The ledger is still in Epoch 0. The protocol pauses.

### Stale Epoch Rejection (Off-by-One)

1. **Signer A** calls `pause()` at ledger 95.
   - The contract derives a `proposal_id` (e.g., `67890`) bound to Epoch 0.
2. **Signer B** calls `approve_pause_proposal(&signer_b, &67890)` at ledger 102.
   - The ledger is now 102, which means it crossed into **Epoch 1** (`102 / 100 = 1`).
   - The approval **fails** with `StaleEpoch`.
3. **Resolution**: Signer A (or any signer) must call `pause()` again in Epoch 1 to get a new ID, and signers must approve that new ID.

---

## Cross-References

- [DEDUP_POLICY.md](DEDUP_POLICY.md) — why same-epoch submissions must converge
- [ADMIN_EPOCHS.md](ADMIN_EPOCHS.md) — equivalent defence-in-depth model for admin-level operations
- [TIME_UNITS.md](TIME_UNITS.md) — ledger time vs Unix timestamps (epochs here are **sequence buckets**, not Unix time)
