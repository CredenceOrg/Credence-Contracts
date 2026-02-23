# Security Analysis: Reentrancy Protection

## Overview

This document describes the reentrancy attack vectors relevant to the Credence Bond contract, the protection mechanisms in place, and the test results verifying their effectiveness.

## Reentrancy in Soroban vs EVM

Unlike EVM-based contracts (Solidity), Soroban smart contracts on Stellar benefit from **runtime-level reentrancy protection**. The Soroban VM prevents a contract from being re-entered while it is already executing — any cross-contract call that attempts to invoke the originating contract will fail with:

```
HostError: Error(Context, InvalidAction)
"Contract re-entry is not allowed"
```

This is a fundamental architectural advantage over EVM, where reentrancy must be handled entirely at the application level.

## Defense-in-Depth: Application-Level Guards

Despite Soroban's built-in protection, the Credence Bond contract implements an **application-level reentrancy guard** as a defense-in-depth measure. This protects against:

- Future changes to the Soroban runtime behavior
- Logical reentrancy through indirect call chains
- State consistency during external interactions

### Guard Implementation

The guard uses a boolean `locked` flag in instance storage:

| Function | Description |
|---|---|
| `acquire_lock()` | Sets `locked = true`; panics with `"reentrancy detected"` if already locked |
| `release_lock()` | Sets `locked = false` |
| `check_lock()` | Returns current lock state |

### Protected Functions

All three external-call-bearing functions use the guard:

1. **`withdraw_bond()`** — Withdraws bonded amount to identity
2. **`slash_bond()`** — Admin slashes a portion of a bond
3. **`collect_fees()`** — Admin collects accumulated protocol fees

Each function follows the **checks-effects-interactions** pattern:
1. Acquire reentrancy lock
2. Validate inputs and authorization
3. Update state (effects) **before** any external call
4. Perform external call (invoke callback)
5. Release reentrancy lock

## Attack Vectors Tested

### 1. Same-Function Reentrancy
An attacker contract registered as a callback attempts to re-enter the same function during execution:
- `withdraw_bond` → `on_withdraw` callback → `withdraw_bond` (re-entry)
- `slash_bond` → `on_slash` callback → `slash_bond` (re-entry)
- `collect_fees` → `on_collect` callback → `collect_fees` (re-entry)

**Result**: All blocked by Soroban runtime (`HostError: Error(Context, InvalidAction)`).

### 2. Cross-Function Reentrancy
An attacker contract attempts to call a *different* guarded function during a callback:
- `withdraw_bond` → `on_withdraw` callback → `slash_bond` (cross-function re-entry)

**Result**: Blocked by Soroban runtime. The application-level guard would also catch this since all guarded functions share the same lock.

### 3. State Consistency After Operations
Verified that the reentrancy lock is:
- Not held before any operation
- Released after successful `withdraw_bond`
- Released after successful `slash_bond`
- Released after successful `collect_fees`

### 4. Sequential Operation Safety
Multiple guarded operations called in sequence (slash → collect fees → withdraw) all succeed, confirming the lock is properly released between calls.

## Test Summary

| # | Test | Type | Result |
|---|------|------|--------|
| 1 | `test_withdraw_reentrancy_blocked` | Same-function reentrancy | PASS (blocked) |
| 2 | `test_slash_reentrancy_blocked` | Same-function reentrancy | PASS (blocked) |
| 3 | `test_fee_collection_reentrancy_blocked` | Same-function reentrancy | PASS (blocked) |
| 4 | `test_lock_not_held_initially` | State lock verification | PASS |
| 5 | `test_lock_released_after_withdraw` | State lock verification | PASS |
| 6 | `test_lock_released_after_slash` | State lock verification | PASS |
| 7 | `test_lock_released_after_fee_collection` | State lock verification | PASS |
| 8 | `test_normal_withdraw_succeeds` | Happy path | PASS |
| 9 | `test_normal_slash_succeeds` | Happy path | PASS |
| 10 | `test_normal_fee_collection_succeeds` | Happy path | PASS |
| 11 | `test_sequential_operations_succeed` | Sequential safety | PASS |
| 12 | `test_slash_exceeds_bond_rejected` | Input validation | PASS |
| 13 | `test_withdraw_non_owner_rejected` | Authorization | PASS |
| 14 | `test_double_withdraw_rejected` | State transition | PASS |
| 15 | `test_cross_function_reentrancy_blocked` | Cross-function reentrancy | PASS |

**All 15 reentrancy-specific tests + 1 existing test = 16 tests passing.**

## Malicious Contract Mocks

Five attacker/mock contracts were created for testing:

| Mock | Behavior |
|------|----------|
| `WithdrawAttacker` | Re-enters `withdraw_bond` from `on_withdraw` callback |
| `SlashAttacker` | Re-enters `slash_bond` from `on_slash` callback |
| `FeeAttacker` | Re-enters `collect_fees` from `on_collect` callback |
| `CrossAttacker` | Calls `slash_bond` from `on_withdraw` callback (cross-function) |
| `BenignCallback` | No-op callbacks for happy-path testing with external calls |

## Key Finding

**Soroban provides runtime-level reentrancy protection.** The VM itself prevents contract re-entry, making reentrancy attacks fundamentally impossible in the current Soroban execution model. The application-level guard (`acquire_lock`/`release_lock`) serves as defense-in-depth and ensures the contract remains safe even if the runtime behavior changes in future versions.

## Recommendations

1. **Keep the application-level guard** — defense-in-depth is a security best practice
2. **Maintain checks-effects-interactions ordering** — state updates before external calls
3. **Restrict `set_callback`** — in production, only admin should be able to set callback addresses
4. **Add access control to `deposit_fees`** — currently unrestricted
5. **Consider event emission** — emit events on withdrawal, slashing, and fee collection for auditability
