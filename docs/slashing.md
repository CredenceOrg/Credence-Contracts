# Slashing

Admin-only reduction of bond value (e.g. for misbehaviour). Slashed amount is tracked and reduces withdrawable balance.

## Authorization

- **slash(admin, amount)**: Only the contract admin can slash. Rejects with "not admin" if caller is not the stored admin.

## Behaviour

- Increases `slashed_amount` by `amount`, capped at current `bonded_amount` (over-slash prevention).
- Withdrawable balance = `bonded_amount - slashed_amount`.
- Emits **slashed** event: (identity, amount, new_slashed_amount).

## Test scenarios (test_slashing.rs)

- Successful slash execution
- Unauthorized slash rejection (non-admin panics)
- Over-slash prevention (slash capped at bonded amount)
- Slash history (cumulative slashed_amount)
- Slash events emitted
- Withdrawal after slash respects available balance
- Withdraw more than available after slash fails
- Multiple slashes capped at bonded total
