# Rolling Bonds

Bonds that auto-renew at period end unless the user requests withdrawal with a notice period.

## Creation

Create with `create_bond(..., is_rolling: true, notice_period_duration: N)`. `notice_period_duration` is in seconds. The notice period must satisfy `notice_period_duration <= bond_duration` (invariant I6); a notice longer than the bond period would make withdrawal perpetually impossible.

## State Machine

```
Active ──request_withdrawal──► PendingNotice
  │                                │
  │                          notice elapsed?
  │                          ├─ no  → settle rejected ("notice period not elapsed")
  │                          └─ yes → withdraw() / withdraw_bond() allowed
  │
  └──renew_if_rolling (no request, period ended)──► Active (new period, bond_start = now)

PendingNotice ──renew_if_rolling──► PendingNotice (no-op; withdrawal_requested_at preserved)
```

## Withdrawal Request

- **`request_withdrawal(identity)`**: Marks that the user wants to withdraw. Sets `withdrawal_requested_at` to the current ledger timestamp. Emits `withdrawal_requested`. Panics if:
  - The bond is not rolling (`ContractError::NotRollingBond`)
  - A request is already pending (`ContractError::WithdrawalAlreadyRequested`)
- Withdrawal is **only allowed** after `now >= withdrawal_requested_at + notice_period_duration`. Calling `withdraw(amount)` or `withdraw_bond(identity)` before this threshold panics with `"notice period not elapsed"`. The arithmetic is overflow-safe (`checked_add`).
- Calling either withdrawal entrypoint without a prior `request_withdrawal` panics with `"withdrawal not requested"`.

### Same-ledger sequencing (anti-sandwich, issue #245)

A `request_withdrawal` and a subsequent `withdraw` issued in the **same ledger** (same timestamp) are always rejected: the notice period `N > 0` means `now < withdrawal_requested_at + N` is always true at the instant of the request. This cannot be bypassed.

## Renewal

- **`renew_if_rolling(identity)`**: If the bond is rolling, **no withdrawal has been requested** (`withdrawal_requested_at == 0`), and `now >= bond_start + bond_duration`, starts a new period: `bond_start = now`, `withdrawal_requested_at = 0`. Emits `bond_renewed`.
- Once `request_withdrawal` has been called (`withdrawal_requested_at != 0`), `renew_if_rolling` is a **no-op** — the pending notice is preserved and the bond will not auto-renew. Confirmed by `test_renew_during_open_request_is_noop` and `test_renew_during_open_request_emits_no_bond_renewed`.
- Can be called by anyone when the period has ended and no withdrawal is pending.
- If not rolling or period not ended, no-op (returns bond unchanged without mutating state).

### Post-renewal cycle

After a successful renewal (`bond_start` advances), a fresh `request_withdrawal` / `withdraw` cycle is fully supported. The new notice window is computed from the new `withdrawal_requested_at`, not from the original bond start.

## Security

The notice window is the slashing window: it gives the protocol time to detect and respond to misbehaviour before funds leave. The on-chain enforcement (`withdrawal_requested_at != 0` and `now >= withdrawal_requested_at + notice_period_duration`) cannot be bypassed — both `withdraw` and `withdraw_bond` perform the check with overflow-safe arithmetic (`checked_add`).

After a full slash `bonded_amount - slashed_amount == 0`, any `withdraw(amount > 0)` reverts even if the notice has elapsed.

## Events

| Event | Topics | Data |
|---|---|---|
| `withdrawal_requested` | identity | withdrawal_requested_at |
| `bond_renewed` | identity | bond_start, bond_duration |

## Test Coverage

State-machine transitions are covered by `test_rolling_notice.rs`:

| Test | Transition |
|---|---|
| `test_request_sets_withdrawal_requested_at` | Active → PendingNotice; timestamp stamped correctly |
| `test_settle_before_notice_panics` | PendingNotice + t < notice_end → rejected |
| `test_settle_exactly_at_notice_boundary_allowed` | PendingNotice + t == notice_end → allowed |
| `test_settle_after_notice_elapsed_allowed` | PendingNotice + t > notice_end → allowed |
| `test_withdraw_without_request_panics` | Active + no request → rejected |
| `test_renew_during_open_request_is_noop` | PendingNotice + renew → no-op (state) |
| `test_renew_during_open_request_emits_no_bond_renewed` | PendingNotice + renew → no event emitted |
| `test_renew_without_request_advances_period` | Active + renew → new period |
| `test_renew_emits_bond_renewed_event` | Active + renew → `bond_renewed` emitted |
| `test_request_emits_withdrawal_requested_event` | request → `withdrawal_requested` emitted |
| `test_same_ledger_request_and_settle_rejected` | Same-ledger anti-sandwich |
| `test_double_request_panics` | PendingNotice + request → rejected |
| `test_renew_exactly_at_period_end` | Renewal at exact period boundary |
| `test_request_and_settle_after_renewal` | Post-renewal full cycle |
| `test_request_then_full_slash_withdraw_fails` | Slash wipes available balance; withdraw rejected |

## Scoring

Rolling periods can be tracked via `bond_renewed` and `withdrawal_requested` for scoring and analytics.
