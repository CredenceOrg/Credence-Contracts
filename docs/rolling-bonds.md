# Rolling Bonds

Bonds that auto-renew at period end unless the user requests withdrawal with a notice period.

## Creation

Create with `create_bond(..., is_rolling: true, notice_period_duration: N)`. `notice_period_duration` is in seconds.

## Withdrawal Request

- **request_withdrawal()**: Marks that the user wants to withdraw. Sets `withdrawal_requested_at` to current time. Emits `withdrawal_requested`.
- Withdrawal is allowed only after `withdrawal_requested_at + notice_period_duration` has passed. Use **withdraw(amount)** then.

## Renewal

- **renew_if_rolling()**: If the bond is rolling and the current time is past `bond_start + bond_duration`, starts a new period: `bond_start = now`, `withdrawal_requested_at = 0`. Emits `bond_renewed`.
- Can be called by anyone when the period has ended.
- If not rolling or period not ended, no-op.

## Events

- **withdrawal_requested**: (identity, withdrawal_requested_at)
- **bond_renewed**: (identity, bond_start, bond_duration)

## Scoring

Rolling periods can be tracked via `bond_renewed` and `withdrawal_requested` for scoring and analytics.
