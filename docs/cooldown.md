# Cooldown Window

## Overview

The cooldown window mechanism enforces a configurable delay between a withdrawal
request and the actual withdrawal execution. This gives the protocol time to
detect and respond to malicious activity before funds leave the system.

## How It Works

1. **Admin configures the cooldown period** via `set_cooldown_period`. The value
   is stored in seconds and can be updated at any time.
2. **Bond holder requests a withdrawal** by calling
   `request_cooldown_withdrawal` with the desired amount. A `CooldownRequest`
   record is created containing the requester address, amount, and the ledger
   timestamp at which the request was made.
3. **After the cooldown elapses** (i.e. `current_time >= requested_at + period`),
   the holder calls `execute_cooldown_withdrawal` to finalize the withdrawal.
   The bond's `bonded_amount` is reduced and the request is removed from storage.
4. **At any point before execution**, the holder may call `cancel_cooldown` to
   remove the pending request without any withdrawal taking place.

## Storage Layout

| Key | Type | Description |
|-----|------|-------------|
| `cooldown_period` (Symbol) | `u64` | Seconds that must elapse between request and execution. |
| `CooldownReq(Address)` | `CooldownRequest` | Pending withdrawal request keyed by requester address. |

### CooldownRequest Fields

| Field | Type | Description |
|-------|------|-------------|
| `requester` | `Address` | Bond holder who initiated the request. |
| `amount` | `i128` | Amount to withdraw after cooldown. |
| `requested_at` | `u64` | Ledger timestamp when the request was made. |

## Contract Methods

### `set_cooldown_period(admin, period)`
Set or update the cooldown duration. Only the contract admin may call this.

### `get_cooldown_period() -> u64`
Returns the current cooldown period in seconds. Defaults to 0 (no delay).

### `request_cooldown_withdrawal(requester, amount) -> CooldownRequest`
Initiate a cooldown withdrawal. The caller must be the bond holder, amount must
be positive and not exceed the available balance (bonded minus slashed). Only one
pending request per address is allowed.

### `execute_cooldown_withdrawal(requester) -> IdentityBond`
Execute a previously requested withdrawal after the cooldown has elapsed.
Verifies available balance at execution time, deducts from the bond, and removes
the stored request. Panics if the period has not passed or if the balance is
insufficient (e.g. if slashing occurred during the cooldown window).

### `cancel_cooldown(requester)`
Cancel a pending cooldown request. Only the original requester may cancel.

### `get_cooldown_request(requester) -> CooldownRequest`
Read the pending cooldown request for an address. Panics if no request exists.

## Events

| Event | Data | When |
|-------|------|------|
| `cooldown_period_updated` | `(old_period, new_period)` | Admin changes the cooldown period. |
| `cooldown_requested` | `(requester, amount)` | A withdrawal request is created. |
| `cooldown_executed` | `(requester, amount)` | A withdrawal is executed after cooldown. |
| `cooldown_cancelled` | `(requester)` | A pending request is cancelled. |

## Security Considerations

- **Balance re-validation at execution**: The available balance is checked both
  at request time and at execution time. If slashing occurs during the cooldown
  window, the withdrawal will be rejected rather than creating an inconsistent
  state.
- **Single pending request**: Only one cooldown request per address is allowed.
  This prevents a holder from queuing multiple requests to bypass slashing.
- **Bond holder verification**: Only the identity that owns the bond can request
  or execute a cooldown withdrawal.
- **Overflow protection**: Timestamp arithmetic uses `saturating_add` to prevent
  overflow when `requested_at + period` would exceed `u64::MAX`.
- **Admin-only configuration**: The cooldown period can only be modified by the
  contract admin.
