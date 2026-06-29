# Early Exit Penalty

Penalty charged when users withdraw before the lock-up period ends.
Penalty is configurable and attributed to the protocol treasury.

## Overview

The early-exit penalty system ensures that users who exit their bond before the
lock-up period ends pay a proportional penalty to the treasury. This maintains
protocol economics and prevents users from bypassing lock-up commitments.

**Critical security:** `withdraw()` enforces lock-up expiry and panics with
`"lock-up not expired; use withdraw_early"` if called before lock-up ends. Users
attempting early exit must use `withdraw_early()`, which applies the penalty.

## Configuration

| Field         | Description                                                                                   |
| ------------- | --------------------------------------------------------------------------------------------- |
| `treasury`    | Address that receives penalty amounts.                                                        |
| `penalty_bps` | Rate in basis points. Must be in `[0, 10_000]` (0% - 100%). Values above 10 000 are rejected. |

Set via `set_early_exit_config(admin, treasury, penalty_bps)`. Admin-only.

Every successful call emits `"early_exit_config_set"` with:

```rust
(treasury: Address, penalty_bps: u32)
```

## Penalty Formula

```text
penalty = (amount * penalty_bps / 10_000) * (remaining_time / total_duration)
```

- `remaining_time`: time left until lock-up end (`end - now`).
- `total_duration`: bond duration at creation.

Configured withdrawals cap `penalty_bps` at 10 000 and only run while
`remaining_time <= total_duration`, so the user's net withdrawal
`amount - penalty` is non-negative. The raw `calculate_penalty` helper keeps the
existing floor-division math unchanged.

## Validation Rules

| Check                                                     | Error                                               |
| --------------------------------------------------------- | --------------------------------------------------- |
| `penalty_bps > 10_000`                                    | Panics with `"penalty_bps must be <= 10000"`        |
| Config not set when `withdraw_early` is called            | `ContractError::EarlyExitConfigNotSet` (210)        |
| Penalty plus user payout does not sum to gross withdrawal | `ContractError::InvariantViolation` (218)           |
| Arithmetic underflow/overflow                             | `ContractError::Underflow` (701) / `Overflow` (700) |

## Example

- Bond: 1000 USDC, 365 days duration
- Penalty rate: 10% (1000 bps)
- Withdraw 500 USDC after 182 days
- Remaining time: 183 days
- Penalty: `(500 * 1000 / 10000) * (183 / 365)` ~= 25 USDC
- User receives: 475 USDC
- Treasury receives: 25 USDC

## Functions

### `set_early_exit_config(admin, treasury, penalty_bps)`

Stores the early-exit configuration. Rejects `penalty_bps > 10_000`.
Emits `"early_exit_config_set"`.

### `withdraw_early(amount)`

Withdraws `amount` before lock-up end. **Reverts** with
`ContractError::EarlyExitConfigNotSet` if the treasury/penalty configuration is
not set, so penalty revenue is never silently dropped. Computes the penalty,
transfers `penalty` to the configured treasury as protocol-fee funds and
`amount - penalty` to the user, then emits `"early_exit_penalty"` with
`(identity, amount, penalty, treasury)`.

### `withdraw(amount)`

Use after lock-up or after the rolling-bond notice period. No penalty.

## Mutual Exclusivity

The two withdrawal functions have non-overlapping valid time windows:

| Time               | `withdraw()`                                            | `withdraw_early()`                             |
| ------------------ | ------------------------------------------------------- | ---------------------------------------------- |
| Before lock-up end | Panics with `"lock-up not expired; use withdraw_early"` | Succeeds with penalty if configured            |
| At lock-up end     | Succeeds, no penalty                                    | Reverts with `ContractError::LockupNotExpired` |
| After lock-up end  | Succeeds, no penalty                                    | Reverts with `ContractError::LockupNotExpired` |

This design ensures:

1. Early exits always require configured treasury routing.
2. Post-lock-up withdrawals never pay a penalty.
3. There is no way to bypass the penalty system by calling `withdraw()` early.

## Events

| Event                     | Payload                                                 |
| ------------------------- | ------------------------------------------------------- |
| `"early_exit_config_set"` | `(treasury, penalty_bps)`                               |
| `"early_exit_penalty"`    | `(identity, withdraw_amount, penalty_amount, treasury)` |
| `"bond_fund_transfer"`    | `(treasury, penalty_amount, FundSource::ProtocolFee)`   |

## Security

- Penalty rate is capped by configuration.
- Config can only be set by admin.
- Early exits require a configured treasury before any penalty is charged.
- The penalty amount is transferred to the configured treasury.
- Penalty plus user payout must sum exactly to the gross withdrawal amount.
- External token transfers run through the bond reentrancy guard.
- Withdrawing after lock-up must use `withdraw`, not `withdraw_early`.
- Withdrawing before lock-up must use `withdraw_early`, not `withdraw`.

## Attack Prevention

### Penalty Bypass Attack (Prevented)

Attack scenario:

1. Attacker creates a bond with a 365-day lock-up.
2. On day 364, attacker calls `withdraw()` to avoid penalty.
3. Attacker receives the full amount without paying penalty to treasury.

Prevention:

`withdraw()` computes `end = bond_start + bond_duration` and requires
`now >= end`. If called before lock-up expiry, it panics with
`"lock-up not expired; use withdraw_early"`, forcing the caller to use
`withdraw_early()`.

```rust
let end = bond
    .bond_start
    .checked_add(bond.bond_duration)
    .expect("bond end timestamp overflow");
if now < end {
    panic!("lock-up not expired; use withdraw_early");
}
```
