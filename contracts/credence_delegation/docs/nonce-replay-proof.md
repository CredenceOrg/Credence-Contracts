# Nonce Replay-Window Proof

## Overview

This document proves that `nonce::invalidate_nonce_range` in the Credence
delegation contract permanently invalidates all payloads whose nonce falls
inside the specified half-open range, and demonstrates the threat model
assumptions under which the proof holds.

---

## Background: The Nonce System

The delegation contract maintains a **per-identity, strictly monotone nonce**
stored in Soroban persistent storage under `DataKey::Nonce(identity)`.

Every state-changing call — both the direct-auth path (`delegate`,
`revoke_delegation`, `revoke_attestation`) and the relayed path
(`execute_delegated_delegate`, `execute_delegated_revoke`,
`execute_delegated_revoke_attest`) — must supply the current nonce value.
`consume_nonce` accepts the nonce only when it exactly equals the stored
value, then unconditionally increments the stored value by 1.

```
consume_nonce(identity, expected):
    current ← storage[Nonce(identity)]   // default 0 if not set
    require current == expected           // else panic InvalidNonce
    storage[Nonce(identity)] ← current + 1
```

---

## The `invalidate_nonce_range` Operation

```rust
/// MAX_NONCE_INVALIDATION_SPAN = 10_000
pub fn invalidate_nonce_range(
    identity: &Address,
    new_nonce: u64,
    max_span: u64,        // MAX_NONCE_INVALIDATION_SPAN
) -> (current, new_nonce)
```

The operation:
1. Reads `current = storage[Nonce(identity)]`.
2. Rejects if `new_nonce <= current` (monotonicity guard).
3. Rejects if `new_nonce - current > max_span` (gas-bound guard).
4. Writes `storage[Nonce(identity)] = new_nonce`.

---

## Formal Proof

### Definitions

Let `S(t)` denote the stored nonce for `identity` at "time" `t` (i.e., after
ledger sequence `t`).

**Invariant I**: `S(t)` is strictly non-decreasing: for all `t' > t`,
`S(t') ≥ S(t)`.

*Proof of I*: `S(t)` is only written in two places:

- `consume_nonce`: sets `S ← S + 1`.
- `invalidate_nonce_range`: sets `S ← new_nonce` where `new_nonce > S`.

Both writes strictly increase `S`. No write decreases it. ∎

---

### Main Theorem

**Theorem**: Let `t*` be the ledger sequence number at which a successful call
to `invalidate_nonce_range(identity, N)` completes (writing `S(t*) = N`).
Then for all `t > t*` and for any payload `P` with `P.nonce < N`, any call
that invokes `consume_nonce(identity, P.nonce)` will panic with `InvalidNonce`.

**Proof**:

By definition, `P.nonce < N`.

By Invariant I and the fact that `S(t*) = N`:

```
∀ t > t*:  S(t) ≥ S(t*) = N > P.nonce
```

Therefore `S(t) ≠ P.nonce` for all future states.

`consume_nonce` requires `expected == S(t)`. Since `P.nonce ≠ S(t)`, the
check fails and the call panics with `InvalidNonce`. ∎

**Corollary (boundary)**: The last invalidated nonce is `N - 1`.  
Any payload with `P.nonce = N - 1` satisfies `P.nonce < N`, hence it is
permanently rejected. Conversely, the first spendable nonce after invalidation
is exactly `N`.

---

## The Half-Open Range `[current, new_nonce)`

The range of invalidated nonces is:

```
{ n ∈ ℕ₀ | current ≤ n < new_nonce }
```

This is a half-open interval in the mathematical sense. It includes:

- The nonce at `current` — the identity could have signed payloads at this
  value before the invalidation call.
- Every nonce up to and including `new_nonce - 1`.

After the call, all these nonces become permanently unspendable by the theorem
above.

---

## Span Cap: `MAX_NONCE_INVALIDATION_SPAN = 10_000`

A single `invalidate_nonce_range` call is limited to at most 10 000 nonces.
This bound exists to:

1. **Prevent gas exhaustion** — Soroban ledger resources are bounded; an
   unbounded skip could be used to DoS the contract in the same transaction.
2. **Predictable fee model** — Relayers can estimate the cost of an
   invalidation without reading contract state.
3. **Correctness** — The cap is checked *before* the write, so a rejected call
   leaves the stored nonce unchanged.

Invalidating more than 10 000 nonces requires multiple successive calls,
each bounded by the same cap. The security guarantee is identical: after `k`
calls that advance from `a` to `b = a + k × 10_000`, every nonce in `[a, b)`
is unspendable.

---

## Threat Model

### Assumptions

| Assumption | Justification |
|---|---|
| The Soroban ledger is honest (no storage tampering) | Soroban's Byzantine-fault-tolerant consensus |
| `consume_nonce` is the only path to consume a nonce | Verified by code review: all state-changing entry points call `consume_nonce` |
| Overflow of `u64` nonces is impossible in practice | At 10 000 nonces/second continuously for 58 million years |
| `identity.require_auth()` is enforced before `invalidate_nonce_range` | Soroban's auth engine rejects unauthorised callers |

### Attack Vectors Addressed

**Pre-signed payload replay after key compromise**:  
If an attacker obtains pre-signed payloads with nonces `[0, K)`, the identity
owner calls `invalidate_nonce_range(new_nonce = K)`.  By the theorem, all `K`
payloads are permanently invalidated.

**Batch payload invalidation**:  
An off-chain service may have queued many payloads.  The identity owner can
skip ahead by up to `MAX_NONCE_INVALIDATION_SPAN` per call, voiding the entire
pending queue in `⌈K / 10_000⌉` transactions.

**Cross-path replay (direct ↔ relayer)**:  
Both the direct (`delegate`, `revoke_*`) and relayed
(`execute_delegated_*`) paths consume from the same nonce namespace.
Invalidating the range blocks both interaction types uniformly: a pre-signed
delegated payload and a pre-signed direct payload with the same nonce value
are both rejected after invalidation.

**Re-init / nonce reset attack**:  
The contract panics if `initialize` is called a second time.  The storage
`Nonce` key is never re-zeroed; `get_nonce` returns 0 only if the key is
absent (i.e., no action has ever been taken for that identity).

### Out-of-Scope Threats

- **Key compromise after invalidation**: If the attacker obtains the private
  key *after* the invalidation but the identity has not yet used nonce `N`,
  the attacker could sign a fresh payload at nonce `N`.  The solution is to
  rotate the identity's key, not just invalidate nonces.
- **Side-channel attacks on the ledger nodes**: Out of scope for contract-level
  security.

---

## Test Coverage

The property `prop_all_invalidated_nonces_rejected` in
`tests/nonce_replay.rs` exercises this theorem directly with **10 000
random (current, span, offset) triples**, covering:

- Every possible offset within the invalidated range for randomly chosen range
  sizes (1 to 10 000).
- Non-zero starting nonces (0 to 999) to test mid-stream invalidations.
- The `prop_post_invalidation_nonce_accepted` corollary — the first
  post-invalidation nonce must succeed.

Additionally, `src/test_nonce_replay.rs` contains deterministic boundary tests:

| Test | Coverage |
|---|---|
| `nonce_replay_span_one_rejects_only_nonce` | Span = 1 (minimum) |
| `nonce_replay_max_span_succeeds_and_rejects_all_prior_nonces` | Span = 10 000 (maximum) |
| `nonce_replay_over_max_span_panics` | Span = 10 001 must fail |
| `nonce_replay_boundary_last_in_range_rejected` | `new_nonce - 1` is rejected |
| `nonce_replay_boundary_first_after_range_accepted` | `new_nonce` is accepted |
| `nonce_replay_double_invalidation_still_rejects_old_nonces` | Chained invalidations |
| `nonce_replay_10k_deterministic_sweep` | 10 000-iteration deterministic PRNG sweep |

---

## Wire Stability

`MAX_NONCE_INVALIDATION_SPAN = 10_000` is a deployment-time constant, not
stored on-chain. Changing it after deployment would affect only future calls;
all previously invalidated ranges remain permanently invalidated because the
stored nonce value is immutable once written.
