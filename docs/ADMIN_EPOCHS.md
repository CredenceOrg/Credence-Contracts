# Admin Epochs — Motivation and Bumping Cadence

## Audience: Contributor

This document explains why admin-facing pause proposals are scoped to **ledger epoch buckets**, when those buckets advance, and how a matching-epoch guard rejects stale approvals and executions. It is written for contributors working on admin pause logic or reviewing epoch-related security changes.

---

## Motivation

Pause proposals that remain approvable forever create a stale-authority hazard:

1. A pause signer proposes `Pause` during an operational incident and collects some approvals.
2. The signing round stalls; the reason for the pause disappears.
3. Many ledgers later another signer approves the **old** `proposal_id`, and someone executes it.

Without an epoch bound, that abandoned proposal is still live. Admin epochs close the gap: a proposal is only actionable while the ledger is still in the **same epoch bucket** that produced its ID. Approvals or executions that carry a stale epoch reference are rejected with a typed error (`StaleAdminEpoch` once the admin-epoch guard is wired).

This is the same defence-in-depth model used for operator pause proposals on `credence_delegation` (see [proposal-id-derivation.md](proposal-id-derivation.md)). Admin epochs are the admin-contract / admin-ops view of that model: motivation, cadence, and the `require_matching_admin_epoch(ep)` check that callers must satisfy.

---

## Epoch Definition and Bumping Cadence

```
epoch = ledger_sequence / PROPOSAL_EPOCH_SIZE
```

`PROPOSAL_EPOCH_SIZE` defaults to **100** ledger sequences per bucket (defined for the hash-derived pause path in `contracts/credence_delegation/src/pausable.rs`). The epoch index advances automatically as the network produces ledgers — there is no separate admin “bump epoch” entrypoint.

| Constant | Default | Wall-clock window (≈5 s / ledger) | Effect when the bucket rolls |
| --- | --- | --- | --- |
| `PROPOSAL_EPOCH_SIZE` | `100` | ≈ 8 minutes | Same-action proposals still converge inside the window; abandoned proposals stop being approvable / executable in the next bucket |

### How to tune

| `PROPOSAL_EPOCH_SIZE` | Convergence window | Re-proposal delay after abandon |
| --- | --- | --- |
| 10 | ~50 seconds | ~50 seconds |
| **100 (default)** | **~8 minutes** | **~8 minutes** |
| 1000 | ~1.4 hours | ~1.4 hours |

Choose a value larger than a realistic multi-sig signing round-trip, but small enough that a stale pause cannot sit actionable for hours.

---

## Proposal ID Embeds the Epoch

Admin pause proposal IDs (on the epoch-derived path) are hashed from the action and the current epoch so the bucket is part of the identifier:

```
preimage = action_u32_be ++ epoch_u32_be   (8 bytes)
hash     = SHA-256(preimage)
id       = first 8 bytes of hash as big-endian u64
```

where `action` is `1` (Pause) or `2` (Unpause). The matching guard is:

```rust
fn require_matching_admin_epoch(e: &Env, action: PauseAction, ep: u64) {
    let expected_id = derive_proposal_id(e, action);
    if ep != expected_id {
        panic_with_error!(e, ContractError::StaleAdminEpoch);
    }
}
```

`approve_pause_proposal` / `execute_pause_proposal` load the stored action for `proposal_id`, then call `require_matching_admin_epoch(e, action, proposal_id)` before recording an approval or mutating pause state. If `ep` was derived under an older bucket, the call fails.

Concrete derivation details and the shared formula live in [proposal-id-derivation.md](proposal-id-derivation.md).

---

## Worked Example

Assume `PROPOSAL_EPOCH_SIZE = 100`.

| Ledger sequence | Epoch | What happens |
| --- | --- | --- |
| 50 | 0 | Signer proposes Pause → `id₀ = derive(Pause, epoch=0)` |
| 75 | 0 | Second signer approves `id₀` → **passes** (same epoch) |
| 99 | 0 | Still epoch 0 — approvals of `id₀` still valid |
| 100 | 1 | Off-by-one boundary — `derive(Pause, epoch=1)` ≠ `id₀` |
| 100 | 1 | `approve_pause_proposal(id₀)` → **stale admin epoch** |
| 1050 | 10 | Ancient `id₀` rejected the same way |

Re-proposing Pause in epoch 1 yields a **new** ID; votes do not carry across the boundary.

---

## Guard Matrix (same / off-by-one / ancient)

| Case | Ledger relative to proposal | Expected |
| --- | --- | --- |
| Same epoch | Still inside the proposal's bucket | Approval / execute succeeds (auth + threshold permitting) |
| Off-by-one | First sequence of the next bucket | Stale admin-epoch rejection |
| Ancient | Many buckets later | Stale admin-epoch rejection |

These three cells are the regression surface covered by the admin-epoch guard tests (same / off-by-one / ancient).

---

## Cross-References

- [proposal-id-derivation.md](proposal-id-derivation.md) — hash derivation formula and `PROPOSAL_EPOCH_SIZE` tradeoffs
- [DEDUP_POLICY.md](DEDUP_POLICY.md) — why same-epoch submissions must converge
- [admin-roles.md](admin-roles.md) — who may configure pause signers / thresholds
- [governance.md](governance.md) — higher-level pause and emergency flows
- [TIME_UNITS.md](TIME_UNITS.md) — ledger time vs Unix timestamps (epochs here are **sequence buckets**, not Unix time)

---

## Version History

| Version | Date | Notes |
| --- | --- | --- |
| 1.0 | 2026-07-26 | Initial admin-epoch motivation and bumping cadence |
