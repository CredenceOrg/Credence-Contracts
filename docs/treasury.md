# Treasury Contract

Central contract for managing protocol fees and slashed funds with multi-signature withdrawal support.

## Overview

- **Receive and store protocol fees** — Admin or authorized depositors credit fees (e.g. early exit penalties) with a source tag.
- **Slashed fund tracking** — Slashed amounts are credited with source `SlashedFunds` for reporting and distribution.
- **Multi-sig withdrawals** — Withdrawals require a proposal plus a configurable number of signer approvals before execution.
- **Fund source tracking** — Balances are tracked by source (`ProtocolFee`, `SlashedFunds`) for accounting.

## Initialization

- `initialize(admin)` — Sets the admin. Must be called once. Admin can add/remove depositors and signers and set the approval threshold.

## Deposits

- **receive_fee(from, amount, source)**  
  Credits the treasury. Caller must be the admin or an authorized depositor (e.g. bond contract).  
  `source` is either `ProtocolFee` or `SlashedFunds`.  
  Emits `treasury_deposit`.

- **add_depositor(depositor)** — Admin only. Allows the address to call `receive_fee`.
- **remove_depositor(depositor)** — Admin only.

## Multi-sig withdrawals

- **add_signer(signer)** — Admin only. Adds a signer.
- **remove_signer(signer)** — Admin only. Threshold is reduced if it exceeded the new signer count.
- **set_threshold(threshold)** — Admin only. Threshold must be ≤ number of signers.

- **propose_withdrawal(proposer, recipient, amount)**  
  Creates a withdrawal proposal. Only a signer can propose. Amount must be positive and ≤ treasury balance.  
  Emits `treasury_withdrawal_proposed`.

- **approve_withdrawal(approver, proposal_id)**  
  Adds the signer’s approval. Double approval by the same signer is a no-op.  
  Emits `treasury_withdrawal_approved`.

- **execute_withdrawal(proposal_id)**  
  Callable by anyone once approval count ≥ threshold. Deducts from treasury and marks the proposal executed.  
  Emits `treasury_withdrawal_executed`.

## Queries

- **get_balance()** — Total treasury balance.
- **get_balance_by_source(source)** — Balance attributed to `ProtocolFee` or `SlashedFunds` (cumulative received from that source).
- **get_admin()** — Admin address.
- **is_depositor(address)** — Whether the address can call `receive_fee`.
- **is_signer(address)** — Whether the address can propose and approve withdrawals.
- **get_threshold()** — Required number of approvals to execute.
- **get_proposal(proposal_id)** — Proposal details (recipient, amount, proposer, executed).
- **get_approval_count(proposal_id)** — Current number of approvals.
- **has_approved(proposal_id, signer)** — Whether the signer has approved the proposal.

## Events

- **treasury_initialized** — (admin)
- **treasury_deposit** — (from, amount, source)
- **depositor_added** / **depositor_removed** — (depositor)
- **signer_added** / **signer_removed** — (signer)
- **threshold_updated** — (threshold)
- **treasury_withdrawal_proposed** — (proposal_id, recipient, amount, proposer)
- **treasury_withdrawal_approved** — (proposal_id, approver)
- **treasury_withdrawal_executed** — (proposal_id, recipient, amount)

## Security

- Only admin or authorized depositors can credit the treasury.
- Withdrawals require a proposal and at least `threshold` signer approvals.
- Threshold cannot exceed signer count; removing signers auto-caps threshold.
- Amounts use checked arithmetic to avoid overflow/underflow.
- Proposal execution is idempotent (executed flag prevents double spend).
