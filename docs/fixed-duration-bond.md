# Fixed Duration Bond Contract

A Soroban smart contract that allows users to lock USDC for a **fixed, predetermined time period**. After the lock expires, the owner may withdraw their full principal. Early withdrawal before expiry is permitted but incurs a configurable penalty fee.

---

## Overview

| Property             | Detail                                                            |
| -------------------- | ----------------------------------------------------------------- |
| **Package**          | `fixed_duration_bond`                                             |
| **Language**         | Rust / Soroban SDK 22.0                                           |
| **Token standard**   | SAC-compatible (USDC/any SAC token)                               |
| **Bond storage**     | Persistent — one active bond per owner address                    |
| **Security pattern** | Checks-Effects-Interactions (state written before token transfer) |

---

## Contract Functions

### Admin

| Function             | Parameters                               | Description                                                                                |
| -------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------ |
| `initialize`         | `admin: Address, token: Address`         | One-time setup. Stores admin and token. Panics if called again.                            |
| `set_fee_config`     | `admin, treasury: Address, fee_bps: u32` | Set optional bond-creation fee (basis points). 0 = disabled.                               |
| `set_penalty_config` | `admin, base_penalty_bps: u32`           | Set default early-exit penalty for bonds created after this call. 0 = early exit disabled. |
| `collect_fees`       | `admin, recipient: Address` → `i128`     | Transfer all accrued creation fees to `recipient`. Panics if no fees.                      |

### Bond Lifecycle

| Function         | Parameters                                                       | Description                                                          |
| ---------------- | ---------------------------------------------------------------- | -------------------------------------------------------------------- |
| `create_bond`    | `owner: Address, amount: i128, duration_secs: u64` → `FixedBond` | Lock `amount` USDC for `duration_secs`. One active bond per address. |
| `withdraw`       | `owner: Address` → `FixedBond`                                   | Withdraw full principal after lock period. Deactivates bond.         |
| `withdraw_early` | `owner: Address` → `FixedBond`                                   | Withdraw before lock period with penalty deducted.                   |

### Queries

| Function             | Parameters       | Returns     | Description                                     |
| -------------------- | ---------------- | ----------- | ----------------------------------------------- |
| `get_bond`           | `owner: Address` | `FixedBond` | Returns bond state for `owner`. Panics if none. |
| `is_matured`         | `owner: Address` | `bool`      | True if lock period has elapsed.                |
| `get_time_remaining` | `owner: Address` | `u64`       | Seconds until maturity; 0 if already matured.   |

---

## Data Structures

### `FixedBond`

```rust
pub struct FixedBond {
    pub owner: Address,
    pub amount: i128,        // net bonded amount (after creation fee)
    pub bond_start: u64,     // ledger timestamp at creation
    pub bond_duration: u64,  // lock period in seconds
    pub bond_expiry: u64,    // bond_start + bond_duration (pre-computed)
    pub penalty_bps: u32,    // early-exit penalty in bps (0 = disabled)
    pub active: bool,        // false once withdrawn
}
```

---

## Security Properties

1. **Exact lock enforcement** — `withdraw` panics with `"lock period has not elapsed yet"` if called before `bond_expiry`.
2. **No early exit without penalty** — `withdraw_early` panics if `penalty_bps == 0` for the bond.
3. **Overflow-safe expiry** — `bond_start.checked_add(duration)` panics on overflow.
4. **One-bond-per-owner** — `create_bond` panics if an active bond already exists.
5. **Auth required** — `owner.require_auth()` on all mutating owner calls; `caller.require_auth()` + admin equality check on all admin calls.
6. **CEI pattern** — Bond state (`active = false`) is written to storage _before_ any token transfer.
7. **Positive amounts only** — `amount <= 0` panics.
8. **Non-zero duration** — `duration_secs == 0` panics.

---

## Events

| Event name        | Data                             |
| ----------------- | -------------------------------- |
| `bond_created`    | `(net_amount, expiry_timestamp)` |
| `bond_withdrawn`  | `net_amount`                     |
| `bond_early_exit` | `(net_amount, penalty)`          |
| `fees_collected`  | `(admin, recipient, amount)`     |

---

## Example Usage (Rust test environment)

```rust
// Setup
client.initialize(&admin, &token);
client.set_penalty_config(&admin, &1_000); // 10% early-exit penalty

// Create a 30-day bond of 100 USDC (100_000_000 in 6-decimal units)
let bond = client.create_bond(&owner, &100_000_000, &(30 * 86_400));
assert_eq!(bond.bond_expiry, bond.bond_start + 30 * 86_400);
assert!(bond.active);

// Check how long is left
let remaining = client.get_time_remaining(&owner);

// After 30 days — normal withdrawal, full amount returned
// (advance ledger time past expiry)
client.withdraw(&owner);

// ---- Alternative: early exit with penalty ----
let bond = client.create_bond(&owner, &100_000_000, &(30 * 86_400));
client.withdraw_early(&owner);
// owner receives 90 USDC, 10 USDC goes to configured treasury
```

---

## Test Coverage

35 tests across 9 groups:

| Group                       | Tests |
| --------------------------- | ----- |
| Initialization              | 2     |
| Bond creation — happy path  | 4     |
| Bond creation — error paths | 5     |
| Maturity checks             | 5     |
| Normal withdrawal           | 5     |
| Early withdrawal            | 6     |
| Fee config / collection     | 4     |
| Re-bond after withdrawal    | 1     |
| Penalty config + queries    | 3     |
