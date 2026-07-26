# Credence Treasury — Multi-Source Withdrawal Accounting Model

> **Scope:** `contracts/credence_treasury/src/treasury.rs`  
> **SDK:** Soroban SDK 22

This document is the canonical reference for the treasury's accounting model. It describes how inflows are tracked, how the on-chain balance is split across fund sources, and exactly how a withdrawal is proportionally deducted from each source. All formulas are derived directly from the code — any discrepancy between this document and the code is flagged as a note.

---

## 1. Fund Sources (`FundSource`)

The treasury tracks two independent lanes of capital:

| Variant | Code value | Meaning |
|---------|-----------|---------|
| `ProtocolFee` | `0` | Protocol service fees, early-exit penalties |
| `SlashedFunds` | `1` | Bond-slashing proceeds |

Both lanes are deposited via `receive_fee(from, amount, source: FundSource)` ([`treasury.rs:249`](../src/treasury.rs#L249)).

---

## 2. Balance Accounting

Three storage keys track live (spendable) balances:

| Key | Type | Invariant |
|-----|------|-----------|
| `DataKey::TotalBalance` | `i128` | Sum of all source balances |
| `DataKey::BalanceBySource(ProtocolFee)` | `i128` | Protocol-fee lane balance |
| `DataKey::BalanceBySource(SlashedFunds)` | `i128` | Slashed-funds lane balance |

**Invariant maintained by `receive_fee` and `execute_withdrawal`:**

```
TotalBalance == BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds)
```

Neither `BalanceBySource` can go negative; the `proportional_deduction` helper (see §4) guarantees rounding never over-subtracts.

---

## 3. Cumulative Inflow Tracking (`CumulativeAmount`)

### Why `i128` alone is insufficient

`i128::MAX` ≈ 1.7 × 10^38. For a high-throughput protocol that accumulates fees in small-denomination stroops, this ceiling can be reached over the contract's lifetime. Resetting the counter would lose history; overflowing would panic.

### The rollover-safe struct

```rust
// treasury.rs:47
pub struct CumulativeAmount {
    pub rollovers: u64,   // number of times the remainder crossed i128::MAX
    pub remainder: i128,  // current partial accumulation in [0, CUMULATIVE_SEGMENT)
}
```

`CUMULATIVE_SEGMENT = (i128::MAX as u128) + 1 = 2^127` ([`treasury.rs:13`](../src/treasury.rs#L13)).

### Reconstruction formula

```
total = rollovers * CUMULATIVE_SEGMENT + remainder
      = rollovers * 2^127              + remainder
```

This is implemented on-chain by `cumulative_to_u256` using `soroban_sdk::U256` so that reconstruction is always identical across all callers.

### `add_to_cumulative` ([`treasury.rs:111`](../src/treasury.rs#L111))

```rust
let sum = current.remainder (as u128) + amount (as u128);
if sum >= CUMULATIVE_SEGMENT {
    rollovers += 1;
    remainder = sum - CUMULATIVE_SEGMENT;
} else {
    remainder = sum;
}
```

**Invariant:** `remainder` is always in `[0, CUMULATIVE_SEGMENT)`. The cumulative value is **monotonically non-decreasing** — it never decreases even after withdrawals (it tracks *received*, not *available*).

### Four cumulative keys

| Key | Tracks |
|-----|--------|
| `DataKey::CumulativeReceived` | Total across both sources |
| `DataKey::CumulativeReceivedBySource(ProtocolFee)` | Protocol-fee lane only |
| `DataKey::CumulativeReceivedBySource(SlashedFunds)` | Slashed-funds lane only |

**Invariant:**
```
CumulativeReceived == CumulativeReceivedBySource(ProtocolFee) + CumulativeReceivedBySource(SlashedFunds)
```
(when reconstructed as U256 via `cumulative_to_u256`).

---

## 4. Proportional Deduction (`proportional_deduction`)

[`treasury.rs:138`](../src/treasury.rs#L138)

When a withdrawal is executed, the amount is split across sources proportional to each source's share of the total available balance:

```
deduction_for_source = floor( source_balance * withdrawal_amount / total_balance )
```

Implemented with `ethnum::U256` intermediates to avoid 128-bit overflow during the multiplication:

```rust
let deduction = (source_balance_u256 * withdrawal_amount_u256) / total_u256;
```

**Special cases:**

| Condition | Result |
|-----------|--------|
| `source_balance == 0` | Returns `0` (lane is empty) |
| `withdrawal_amount == 0` | Returns `0` |
| `withdrawal_amount == total_balance` | Returns `source_balance` exactly (avoids rounding error on full drain) |

### Slashed-funds deduction

The slashed-funds deduction is derived as the remainder to ensure the two deductions always sum to `actual_amount` with no rounding gap:

```rust
// treasury.rs:681
let protocol_deduction = proportional_deduction(&e, protocol_balance, actual_amount, total);
let slashed_deduction  = actual_amount - protocol_deduction;
```

This means any rounding difference always falls on the slashed-funds lane.

---

## 5. Worked Multi-Source Example

Assume:

- `BalanceBySource(ProtocolFee)` = 700
- `BalanceBySource(SlashedFunds)` = 300
- `TotalBalance` = 1000
- Withdrawal amount = 400

**Calculations:**

```
protocol_deduction = floor(700 * 400 / 1000) = floor(280_000 / 1000) = 280
slashed_deduction  = 400 - 280 = 120
```

**Post-withdrawal balances:**

| Field | Before | After |
|-------|--------|-------|
| `TotalBalance` | 1000 | 600 |
| `BalanceBySource(ProtocolFee)` | 700 | 420 |
| `BalanceBySource(SlashedFunds)` | 300 | 180 |

The split ratio is preserved: 420/600 = 70% protocol, 180/600 = 30% slashed.

---

## 6. `execute_withdrawal` Flow ([`treasury.rs:599`](../src/treasury.rs#L599))

1. **Proposal validity:** not expired, not already executed.
2. **Approval threshold:** `ApprovalCount(proposal_id) >= Threshold`.
3. **Balance check:** `TotalBalance >= proposal.amount`.
4. **MinLiquidity floor:** `TotalBalance - proposal.amount >= MinLiquidity` (see §7).
5. **Token transfer:** actual on-chain transfer; the settled `actual_amount` may differ from `proposal.amount` if the token has transfer fees (deflation).
6. **Slippage guard:** `actual_amount >= min_amount_out` — the caller sets this; revert if token misbehaves.
7. **Proportional deduction:** split `actual_amount` across sources (§4).
8. **Persist updated balances and mark proposal executed.**

---

## 7. `MinLiquidity` Interaction

`DataKey::MinLiquidity` is an admin-configurable floor. The check in `execute_withdrawal`:

```rust
let remaining = total - proposal.amount;
if remaining < min_liquidity {
    panic_with_error!(&e, ContractError::InsufficientTreasuryBalance);
}
```

This means even if the total balance is sufficient for the withdrawal amount, the withdrawal is rejected if it would leave the treasury with fewer funds than `min_liquidity`. This protects against draining the treasury to zero in one proposal.

**Note:** `MinLiquidity` is checked against `proposal.amount`, not `actual_amount`. If the token's actual transfer settles for less (slippage), the remaining balance after settlement will be *higher* than the floor check predicted — this is safe (conservative).

---

## 8. Function-to-Invariant Map

| Function | Invariants it maintains |
|----------|------------------------|
| `receive_fee` | `TotalBalance` grows; per-source balance grows; `CumulativeReceived*` is updated monotonically |
| `execute_withdrawal` | `TotalBalance` shrinks by `actual_amount`; per-source balances shrink by their proportional deduction; `TotalBalance >= MinLiquidity` after withdrawal |
| `proportional_deduction` | Per-source deductions sum exactly to `actual_amount`; no source goes negative |
| `add_to_cumulative` | `remainder` in `[0, CUMULATIVE_SEGMENT)`; `rollovers` monotonically non-decreasing |
| `cumulative_to_u256` | Canonical reconstruction — identical result for any caller |

---

## 9. Discrepancies / Notes

- **`MinLiquidity` vs slippage:** The pre-transfer balance check uses `proposal.amount`, but the actual deducted amount is `actual_amount` (post-transfer). If a deflation token causes `actual_amount < proposal.amount`, the post-withdrawal balance is higher than expected and the MinLiquidity floor is not violated. This is intentionally conservative.
- **Cumulative is not decremented on withdrawal.** It only grows. Auditors should not use `CumulativeReceived` as a proxy for current balance; use `TotalBalance` for that.
- **Rounding:** `proportional_deduction` truncates (floor division). Any fractional stroop stays in the slashed-funds lane due to the remainder calculation. Over many withdrawals this is a tiny systematic bias toward the protocol-fee lane retaining slightly more than its exact pro-rata share.
