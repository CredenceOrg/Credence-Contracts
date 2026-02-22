# Early Exit Penalty

Penalty charged when users withdraw before the lock-up period ends. Penalty is configurable and transferred to the protocol treasury.

## Configuration

- **treasury**: Address that receives penalty amounts.
- **early_exit_penalty_bps**: Rate in basis points (e.g. 500 = 5%). Must be ≤ 10000.

Set via `set_early_exit_config(admin, treasury, penalty_bps)`. Admin-only.

## Penalty Formula

`penalty = (amount * penalty_bps / 10000) * (remaining_time / total_duration)`

- **remaining_time**: Time left until lock-up end.
- **total_duration**: Bond duration at creation.

So penalty is proportional to how much of the lock period remains.

## Functions

### withdraw_early(amount)

Withdraws `amount` before lock-up end. Applies penalty; penalty is attributed to treasury (in a full implementation, token transfer would send `amount - penalty` to user and `penalty` to treasury). Emits `early_exit_penalty` event with (identity, withdraw_amount, penalty_amount, treasury).

### withdraw(amount)

Use after lock-up or after notice period for rolling bonds. No penalty.

## Events

- **early_exit_penalty**: (identity, withdraw_amount, penalty_amount, treasury)

## Security

- Penalty capped by amount and rate; no overflow in calculation.
- Config can only be set by admin.
- Withdrawing after lock-up must use `withdraw`, not `withdraw_early`.
