# Treasury Invariants

This document details the core invariants that the Credence Treasury contract is designed to preserve. These invariants govern the flow of funds (deposits, withdrawals, rescues) and the multisig proposal lifecycle.

## Invariant 1: Total Balance Consistency
The total accounted balance in the treasury must always equal the sum of the accounted balances across all fund sources.
```rust
TotalBalance == BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds)
```
- **Deposits**: `receive_fee` increases both `TotalBalance` and the corresponding `BalanceBySource` by the deposited amount.
- **Withdrawals**: `execute_withdrawal` decrements the total balance and allocates the deduction proportionally across both fund sources.

## Invariant 2: Solvency and Custody Safety
The actual token balance held by the treasury contract address must never be less than the accounted `TotalBalance` in instance storage.
```rust
token_client.balance(current_contract_address) >= TotalBalance
```
- The difference `actual_balance - TotalBalance` represents excess tokens.
- Excess tokens may accumulate via direct transfers to the contract that bypass the `receive_fee` entrypoint.
- Only these excess tokens can be extracted via the `rescue_native` admin function. Accounted user and protocol funds can never be rescued.

## Invariant 3: Proportional Deduction allocation
Withdrawal execution must deduct from each fund source proportionally to its current share of the total accounted balance to prevent starvation of any single source.
```rust
ProtocolFeeDeduction = (BalanceBySource(ProtocolFee) * withdrawal_amount) / TotalBalance
SlashedFundsDeduction = withdrawal_amount - ProtocolFeeDeduction
```
- Deduction calculations utilize U256 precision to prevent intermediate overflows.
- The remainder of integer division is fully accounted for by deducting it from the slashed funds source.

## Invariant 4: Cumulative Invariant Tracking
Lifetime cumulative received amounts across all sources must remain fully reconciled.
```rust
CumulativeReceived == CumulativeReceivedBySource(ProtocolFee) + CumulativeReceivedBySource(SlashedFunds)
```
- Cumulative tracking uses a rollover-safe `CumulativeAmount` struct split into `rollovers` and `remainder` fields.
- Reconciliations are performed on the reconstructed U256 values:
  `U256(CumulativeReceived) == U256(CumulativeReceivedBySource(ProtocolFee)) + U256(CumulativeReceivedBySource(SlashedFunds))`

## Invariant 5: Minimum Liquidity Floor
A withdrawal must never reduce the total accounted balance below the configured minimum liquidity floor.
```rust
TotalBalance - withdrawal_amount >= MinLiquidity
```
- Enforced at the time of execution in `execute_withdrawal`.
- A proposal that was valid when proposed will revert on execution if other withdrawals have since reduced the available balance below the required floor.

## Invariant 6: Pause Signer Count Integrity
The cached `SignerCount` must accurately reflect the number of active addresses in the signer map.
- Duplicate registrations must be rejected or treated as no-ops.
- Signer removals must decrement the `SignerCount` cache.
- The withdrawal threshold is automatically capped to `SignerCount` when signers are removed to prevent a deadlocked contract state.
