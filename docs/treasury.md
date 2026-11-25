# Treasury Contract

Central contract for managing protocol fees and slashed funds with multi-signature withdrawal support.

## Overview

- **Receive and store protocol fees** — Admin or authorized depositors credit fees (e.g. early exit penalties) with a source tag.
- **Slashed fund tracking** — Slashed amounts are credited with source `SlashedFunds` for reporting and distribution.
- **Multi-sig withdrawals** — Withdrawals require a proposal plus a configurable number of signer approvals before execution.
- **Fund source tracking** — Balances are tracked by source (`ProtocolFee`, `SlashedFunds`) for accounting.
- **Flash loans** — The configured token can be lent for a single receiver callback, provided the principal plus fee is repaid atomically.

## Initialization

- `initialize(admin, token)` — Sets the admin and the accounting token. Must be called once. Admin can add/remove depositors and signers and set the approval threshold.

## Deposits

- **receive_fee(from, amount, source)**
  Credits the treasury. Caller must be the admin or an authorized depositor (e.g. bond contract).
  `source` is either `ProtocolFee` or `SlashedFunds`.
  Emits `treasury_deposit`.

- **add_depositor(depositor)** — Admin only. Allows the address to call `receive_fee`.
- **remove_depositor(depositor)** — Admin only.

## Flash loans

- **set_flash_loan_fee(fee_bps)** — Admin-authenticated (the stored admin address must sign). Sets the loan fee in basis points; `10,000` bps is 100%. Values above `10,000` revert with `InvalidFeeBps (#240)`. Defaults to `0` at initialization.
  Emits `flash_loan_fee_updated` with `(old_fee_bps, fee_bps)`.

- **get_flash_loan_fee()** — Current fee in basis points.

- **flash_loan(initiator, receiver, amount, data)**
  Transfers `amount` of the configured token from the treasury to `receiver`, then calls `receiver.on_flash_loan(initiator, token, amount, fee, data)`. The `initiator` must authorize the call. The receiver must implement the `FlashLoanReceiver` interface, return `FLASH_LOAN_SUCCESS`, and repay `amount + fee` to the treasury. The loan is rejected while paused; a negative `amount` reverts with `AmountMustBePositive`.

  The treasury snapshots its token balance before the transfer and requires the post-callback balance to be at least `balance_before + fee`. A wrong success symbol reverts with `InvalidFlashLoanCallback (#606)`, a short repayment with `FlashLoanRepaymentFailed (#607)`, and a callback re-entry with `ReentrancyDetected (#207)`. Any panic in the receiver, insufficient repayment, or failed check reverts the whole invocation, including the initial transfer, so funds can never be borrowed without repayment.

  Only the retained fee is booked as `ProtocolFee` revenue (plus cumulative totals); overpayment remains excess token balance.
  Emits `flash_loan` with `(receiver, amount, fee)` under the `initiator` topic.

## Multi-sig withdrawals

- **add_signer(signer)** — Admin only. Adds a signer.
- **remove_signer(signer)** — Admin only. Threshold is reduced if it exceeded the new signer count.
- **set_threshold(threshold)** — Admin only. Threshold must be ≤ number of signers.

- **propose_withdrawal(proposer, recipient, amount)**
  Creates a withdrawal proposal. Only a signer can propose. Amount must be positive and ≤ treasury balance.
  Sets `expires_at = proposed_at + ttl` (default TTL is 7 days).
  Emits `treasury_withdrawal_proposed`.

- **approve_withdrawal(approver, proposal_id)**
  Adds the signer’s approval. Double approval by the same signer is a no-op.
  Rejects with `ProposalExpired` + emits `treasury_proposal_expired` if `now >= expires_at`.
  Emits `treasury_withdrawal_approved`.

- **execute_withdrawal(proposal_id, min_amount_out)**
  Callable by anyone once approval count ≥ threshold. Deducts from treasury and marks the proposal executed.
  Rejects with `ProposalExpired` + emits `treasury_proposal_expired` if `now >= expires_at`.

  **Withdrawal Guardrails:**
  - **Liquidity Floor**: Ensures remaining balance after withdrawal is ≥ `min_liquidity`. Reverts with "liquidity guard: withdrawal would breach minimum liquidity floor" if violated.
  - **Slippage Protection**: Requires proposal amount ≥ `min_amount_out`. Reverts with "slippage: received amount below minimum" if violated. Pass `0` to skip this check.

  Emits `treasury_withdrawal_executed` with `(recipient, min_amount_out, actual_amount, treasury_debit)`.

## Queries

- **get_balance()** — Total treasury balance.
- **get_balance_by_source(source)** — Balance attributed to `ProtocolFee` or `SlashedFunds` (cumulative received from that source).
- **get_flash_loan_fee()** — Current flash loan fee in basis points.
- **get_token()** — Current accounting/lending token address.
- **get_min_liquidity()** — Current minimum liquidity floor that must remain after withdrawals.
- **set_min_liquidity(admin, min_liquidity)** — Admin only. Sets the minimum balance that must remain in the treasury after any withdrawal.
- **set_proposal_ttl(admin, ttl)** — Admin only. Sets the proposal time-to-live in ledger seconds (default 7 days; pass `0` for no expiry).
- **get_proposal_ttl()** — Current proposal TTL in ledger seconds.
- **get_admin()** — Admin address.
- **is_depositor(address)** — Whether the address can call `receive_fee`.
- **is_signer(address)** — Whether the address can propose and approve withdrawals.
- **get_threshold()** — Required number of approvals to execute.
- **get_proposal(proposal_id)** — Proposal details (recipient, amount, proposer, proposed_at, expires_at, executed).
- **get_approval_count(proposal_id)** — Current number of approvals.
- **has_approved(proposal_id, signer)** — Whether the signer has approved the proposal.

## Events

- **treasury_initialized** — (admin)
- **treasury_deposit** — (from, amount, source)
- **flash_loan_fee_updated** — (old_fee_bps, fee_bps)
- **flash_loan** — (receiver, amount, fee), topic initiator
- **depositor_added** / **depositor_removed** — (depositor)
- **signer_added** / **signer_removed** — (signer)
- **threshold_updated** — (threshold)
- **treasury_withdrawal_proposed** — (proposal_id, recipient, amount, proposer)
- **treasury_withdrawal_approved** — (proposal_id, approver)
- **treasury_withdrawal_executed** — (proposal_id, recipient, min_amount_out, actual_amount, treasury_debit)
- **treasury_proposal_expired** — (proposal_id)
- **proposal_ttl_updated** — (ttl)

## Per-source accounting identity

Every `execute_withdrawal` call splits the deducted amount proportionally across
`ProtocolFee` and `SlashedFunds` using integer division (`source * withdrawal / total`),
with the remainder assigned to `SlashedFunds`:

```
protocol_deduction = ProtocolFee_balance * actual_amount / TotalBalance   (floor division)
slashed_deduction  = actual_amount - protocol_deduction
```

A successful `flash_loan` adds only the retained fee to `TotalBalance` and
`BalanceBySource(ProtocolFee)`, and to both cumulative counters:

```
flash_loan_fee = amount * fee_bps / 10_000   (floor division)
TotalBalance += flash_loan_fee
BalanceBySource(ProtocolFee) += flash_loan_fee
CumulativeReceived += flash_loan_fee
CumulativeReceivedBySource(ProtocolFee) += flash_loan_fee
```

The following invariant **must hold after every withdrawal**:

```
BalanceBySource(ProtocolFee) + BalanceBySource(SlashedFunds) == TotalBalance
```

Both per-source balances are always non-negative.

**Rounding behaviour**: floor division means `protocol_deduction` may be slightly
less than the ideal proportional share. `slashed_deduction = actual_amount - protocol_deduction`
absorbs the rounding so the total deduction is always **exactly** `actual_amount` — never
more, never less. Because of this, the identity `protocol + slashed == total` holds without
drift after every withdrawal. When `actual_amount == TotalBalance` (full drain), the shortcut
`protocol_deduction = ProtocolFee_balance` is used, guaranteeing both balances reach zero
simultaneously.

These invariants are verified by property tests in `test_proportional_deduction_invariants.rs`
across random deposit/withdrawal sequences (50 proptest cases with up to 20 actions each).

## Security

- Only admin or authorized depositors can credit the treasury.
- Withdrawals require a proposal and at least `threshold` signer approvals.
- Threshold cannot exceed signer count; removing signers auto-caps threshold.
- Amounts use checked arithmetic to avoid overflow/underflow.
- Proposal execution is idempotent (executed flag prevents double spend).
- Proposals expire after the configured TTL (default 7 days); approvals and execution are rejected once expired.
- Flash loans require initiator authorization, a withdrawal lock, a `FLASH_LOAN_SUCCESS` callback, and a treasury-side repayment check; any failure reverts atomically.
- **Liquidity Floor Guardrail**: `min_liquidity` setting ensures treasury maintains minimum solvency after withdrawals.
- **Slippage Protection**: `min_amount_out` parameter protects withdrawal executors from unfavorable settlement conditions.

## Emergency rescue

- **rescue_native(admin, to, amount)**
  Admin-only. Transfers *excess* tokens — those held by the contract beyond the internally accounted `TotalBalance` — to `to`.
  This allows recovery of accidentally sent or airdropped tokens without touching user/protocol funds.

  **Excess-only bound**:
  ```
  excess = token_client.balance(contract) - TotalBalance
  ```
  `amount` must satisfy `0 < amount ≤ excess`. Any attempt to rescue accounted funds reverts with `InsufficientTreasuryBalance (#602)`.
  Emits `native_rescued` with `(to, amount, admin)` only after a successful transfer.
