# Tier Transition Fuzzing Documentation

This document describes the property-based fuzz testing suite for the Credence Bond tier system, implemented to address Issue #368. The fuzz tests verify that all tier transitions, rolling renewals, withdrawals, and slashing sequences preserve the protocol's core security invariants.

---

## 1. Invariant Matrix

The following matrix documents the invariants enforced by the property tests:

| Invariant / Guard | Description | Formal / Code Condition | Checked In |
|---|---|---|---|
| **I2 — Slashed <= Bonded** | A bond can never be slashed for more than its deposited principal. | `slashed_amount <= bonded_amount` | `test_invariants.rs` / `proptest_tier.rs` |
| **No Negative Net Bond** | Available collateral (net bond) must always be non-negative. | `available_amount >= 0` | `proptest_tier.rs` |
| **I4 — Bonded >= 0** | Total bonded amount can never be negative. | `bonded_amount >= 0` | `test_invariants.rs` / `proptest_tier.rs` |
| **I5 — Slashed >= 0** | Cumulative slashed amount can never be negative. | `slashed_amount >= 0` | `test_invariants.rs` / `proptest_tier.rs` |
| **Tier Pure Function of Net Bond** | A bond's identity tier is a pure function of its net available collateral, not total bonded amount. | `tier == get_tier_for_amount(available_amount)` | `proptest_tier.rs` |
| **Tier Monotonicity** | Operations that reduce a bond's net value (Slash, Withdrawal, Settle) must never increase its tier rank. | `tier_after_op <= tier_before_op` | `proptest_tier.rs` |

---

## 2. Validation & Shrinking Experiment

To prove that the property test harness is load-bearing and capable of catching violations, a validation experiment was conducted by injecting a bug.

### The Injected Bug
In `contracts/credence_bond/src/tiered_bond.rs`, a bug was introduced to map a zero available amount to the **Platinum** tier:
```rust
pub fn get_tier_for_amount(e: &Env, amount: i128) -> BondTier {
    // ...
    if amount == 0 {
        return BondTier::Platinum;
    }
    // ...
}
```

### Shrinking Result
Running the proptest fuzzing suite successfully caught the violation and shrank it to a minimal failing trace of **exactly two actions**:

```text
cc 3d43fbad1114c66e476d5b0c492086c74999674e16f294199abbd81be9a818f8 # shrinks to ref actions = [Deposit { amount: 1000, duration: 86400, is_rolling: false, notice_period_duration: 0 }, Settle]
```

### Explaining the Trace
1. **Deposit**: A new bond is created with `amount = 1000`. Since `1000 < TIER_BRONZE_MAX`, the expected tier is `Bronze` (rank 0).
2. **Settle**: The bond is settled (fully withdrawn), bringing the available net bond to `0`. Under the injected bug, this causes the tier to jump to `Platinum` (rank 3).
3. **Violation**: The fuzzer catches that `rank_after (3) > rank_before (0)` for a withdrawal/settle action, which violates the monotonicity invariant (`tier_after_slash <= tier_before_slash`). It successfully shrinks the failing trace to these 2 simple actions.

---

## 3. Security Notes

1. **Reputation Security**: Basing identity tiers on net available bond rather than total bonded amount ensures that highly-slashed or empty bonds cannot leverage the privileges of a higher tier. Reputation decays dynamically upon slashing and withdrawal.
2. **Dust Protection**: The slashing logic implements a dust-floor clamp to prevent bad debt and residual dust balances from polluting available balance calculations.
3. **Alternating Cycle Resilience**: The custom `Arbitrary` strategy is designed to fuzz up to 256 actions per case, generating complex interleavings of deposits, slashes, top-ups, notice periods, and settlements. Monotonicity checks prevent alternating slash/top-up sequences from generating tier elevation exploits.
