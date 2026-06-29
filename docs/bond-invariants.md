# Bond Invariants Catalogue

The Credence bond contract must preserve a small set of **invariants** — properties
that must hold *after every state-changing operation*, regardless of the path taken.

Historically these checks were scattered across individual tests as ad-hoc
`assert!` statements. They are now consolidated into a single reusable library,
[`contracts/credence_bond/src/test_invariants.rs`](../contracts/credence_bond/src/test_invariants.rs),
which is gated behind `#[cfg(test)]` and can be called by any unit, integration,
fuzz, or mutation test via:

```rust
use crate::test_invariants::assert_all_invariants;

client.create_bond(&identity, &amount, &duration, &false, &0);
assert_all_invariants(&env, &contract_id);
```

## The seven invariants

| ID  | Invariant | Formal condition | Owner module | Helper |
|-----|-----------|------------------|--------------|--------|
| I1  | Attestation weight sum is non-negative | `Σ weight(att) ≥ 0` for each subject | `SubjectAttestationCount` / `weighted_attestation` | `assert_attestation_weight_sum_non_negative` |
| I2  | Slashed never exceeds bonded | `slashed_amount ≤ bonded_amount` | `IdentityBond` / `slashing` | `assert_slashed_within_bonded` |
| I3  | Withdrawal request requires rolling | `withdrawal_requested_at == 0 \|\| is_rolling` | `IdentityBond` / `rolling_bond` | `assert_withdrawal_request_requires_rolling` |
| I4  | Bonded amount is non-negative | `bonded_amount ≥ 0` | `IdentityBond` | `assert_bonded_non_negative` |
| I5  | Slashed amount is non-negative | `slashed_amount ≥ 0` | `IdentityBond` / `slashing` | `assert_slashed_non_negative` |
| I6  | Notice period is bounded | rolling ⇒ `notice_period_duration ≤ bond_duration` | `IdentityBond` / `rolling_bond` | `assert_notice_period_bounded` |
| I7  | Attestation count matches list | `SubjectAttestationCount == len(SubjectAttestations)` (when present) | `SubjectAttestationCount` | `assert_attestation_count_consistent` |

### I1 — Attestation weight sum is non-negative
The sum of every stored attestation `weight` for a subject must be `≥ 0`.
Attestation weights are `u32` on chain; this invariant guards the aggregation
logic against ever producing a negative signed total.
**Owner:** `Nonce`/attestation pipeline writers; `weighted_attestation` computes weights.

### I2 — Slashed never exceeds bonded
A bond can never be slashed for more than its principal. Enforced on chain in
`slashing::slash_bond` / `CredenceBond::slash_bond` with `ContractError::SlashExceedsBond`.
This is the canonical example used to validate the library (see below).
**Owner:** `IdentityBond`, `slashing`.

### I3 — Withdrawal request requires rolling
Only rolling bonds may carry a pending withdrawal request. A fixed-duration bond
must always have `withdrawal_requested_at == 0`. Enforced on chain in
`request_withdrawal` with `ContractError::NotRollingBond`.
**Owner:** `IdentityBond`, `rolling_bond`.

### I4 — Bonded amount is non-negative
No combination of top-ups, withdrawals, or slashes may drive the principal below
zero. Checked-arithmetic paths (`checked_sub`, `checked_add`) back this up on chain.
**Owner:** `IdentityBond`.

### I5 — Slashed amount is non-negative
Accumulated slashes can never be negative.
**Owner:** `IdentityBond`, `slashing`.

### I6 — Notice period is bounded
For a rolling bond with a configured notice period, the notice period must not
exceed the bond duration, otherwise a withdrawal could never become claimable.
Enforced at creation time via `ContractError::InvalidNoticePeriod`.
**Owner:** `IdentityBond`, `rolling_bond`.

### I7 — Attestation count matches the stored list
When `SubjectAttestationCount(subject)` is present it must equal the length of
`SubjectAttestations(subject)`. Catches divergence between the counter and the
canonical list.
**Owner:** `SubjectAttestationCount`.

## On-chain drift detection (issue #436)

Production bond writes also call `invariants::assert_self_consistent` after persisting
state. Failures emit `bond_drift_detected` and panic with `ContractError::InvariantViolation`
(code **218**). See [`bond-drift-detection.md`](bond-drift-detection.md) for the operations runbook.

## Usage

| Function | Scope | When to use |
|----------|-------|-------------|
| `assert_all_invariants(&env, &contract)` | All 7 (I1–I7), state read from storage | After **every** mutating call (default) |
| `assert_all_invariants_for_subject(&env, &contract, &subject)` | All 7, explicit attestation subject | When attester subject ≠ bond identity |
| `assert_bond_invariants(&bond)` | I2–I6 against an explicit `IdentityBond` | When the call already returned the updated bond |
| `assert_all_bond_invariants(&env, &contract)` | I2–I6 from storage | Bond-only contexts |
| Individual `assert_*` helpers | One invariant each | Targeted/negative tests |

## Validation (mutation experiment)

The library is proven to be **load-bearing**: disabling the I2 check makes a test
fail. I2 is guarded by a compile-time flag:

```bash
# Normal: all tests pass.
cargo test -p credence_bond

# Skip I2: the `slashed_over_bonded_is_detected` test no longer panics → FAILS.
RUSTFLAGS="--cfg skip_slash_invariant" cargo test -p credence_bond --lib slashed_over_bonded_is_detected
```

Observed result with the flag set:

```
test test_invariants_usage::slashed_over_bonded_is_detected - should panic ... FAILED
note: test did not panic as expected
```

This confirms that removing the invariant check is detected by the test suite.

## Test coverage

The library is exercised at **30+ call sites** across
[`test_invariants_usage.rs`](../contracts/credence_bond/src/test_invariants_usage.rs),
covering the required edge cases:

- **post-slash** — full-amount slash (I2 boundary), incremental slashes.
- **post-renew** — `renew_if_rolling` resets `withdrawal_requested_at`.
- **post-withdraw-request** — rolling-bond request, request-then-slash, request-then-renew (no-op).
- **attestations** — I1/I7 after multiple attestations.
