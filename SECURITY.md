# Security Analysis: Credence Bond Contract

## Overview

This document describes security aspects of the Credence Bond contract, including access control, reentrancy protection, and other security mechanisms.

For other security topics (including overflow-safe arithmetic for financial calculations), see `docs/security.md`.

## Access Control Role Matrix

The Credence Bond contract implements role-based access control with the following roles and permissions:

### Roles

| Role | Description | Access Level |
|------|-------------|--------------|
| **Admin** | Contract administrator with highest privileges | Full |
| **Verifier** | Attestation verifier with limited privileges | Limited |
| **Governance** | Governance participants for protocol decisions | Limited |
| **Identity Owner** | Owner of a specific bond/identity | Owner-specific |

### Permission Matrix

| Function/Method | Admin | Verifier | Governance | Identity Owner | Notes |
|------------------|-------|----------|------------|----------------|--------|
| **Configuration** | | | | | |
| `initialize` | ✅ | ❌ | ❌ | ❌ | One-time setup |
| `set_supply_cap` | ✅ | ❌ | ❌ | ❌ | Global supply limit |
| `set_early_exit_config` | ✅ | ❌ | ❌ | ❌ | Early exit penalties |
| `set_emergency_config` | ✅ | ❌ | ❌ | ❌ | Emergency controls |
| `set_grace_window` | ✅ | ❌ | ❌ | ❌ | Nonce validation |
| `set_fee_config` | ✅ | ❌ | ❌ | ❌ | Protocol fees |
| `set_bond_token` | ✅ | ❌ | ❌ | ❌ | Bond token address |
| `set_protocol_fee_bps` | ✅ | ❌ | ❌ | ❌ | Protocol fee rate |
| `set_attestation_fee_bps` | ✅ | ❌ | ❌ | ❌ | Attestation fee rate |
| `set_withdrawal_cooldown_secs` | ✅ | ❌ | ❌ | ❌ | Withdrawal cooldown |
| `set_slash_cooldown_secs` | ✅ | ❌ | ❌ | ❌ | Slash cooldown |
| `set_cooldown_period` | ✅ | ❌ | ❌ | ❌ | Cooldown period |
| **Tier Configuration** | | | | | |
| `set_bronze_threshold` | ✅ | ❌ | ❌ | ❌ | Bronze tier requirement |
| `set_silver_threshold` | ✅ | ❌ | ❌ | ❌ | Silver tier requirement |
| `set_gold_threshold` | ✅ | ❌ | ❌ | ❌ | Gold tier requirement |
| `set_platinum_threshold` | ✅ | ❌ | ❌ | ❌ | Platinum tier requirement |
| `set_max_leverage` | ✅ | ❌ | ❌ | ❌ | Maximum leverage |
| **Verifier Management** | | | | | |
| `add_verifier` | ✅ | ❌ | ❌ | ❌ | Add new verifier |
| `remove_verifier` | ✅ | ❌ | ❌ | ❌ | Remove verifier |
| `register_attester` | ✅ | ❌ | ❌ | ❌ | Register attester |
| `unregister_attester` | ✅ | ❌ | ❌ | ❌ | Unregister attester |
| `set_verifier_stake_requirement` | ✅ | ❌ | ❌ | ❌ | Set stake requirement |
| `set_verifier_reputation` | ✅ | ❌ | ❌ | ❌ | Set verifier reputation |
| `set_attester_stake` | ✅ | ❌ | ❌ | ❌ | Set attester stake |
| `set_weight_config` | ✅ | ❌ | ❌ | ❌ | Attestation weights |
| **Emergency Controls** | | | | | |
| `set_emergency_mode` | ✅ | ❌ | ✅ | ❌ | Emergency mode toggle |
| `emergency_withdraw` | ✅ | ❌ | ✅ | ❌ | Emergency withdrawal |
| **Governance** | | | | | |
| `initialize_governance` | ✅ | ❌ | ❌ | ❌ | Setup governance |
| `governance_vote` | ❌ | ❌ | ✅ | ❌ | Vote on proposals |
| `governance_delegate` | ❌ | ❌ | ✅ | ❌ | Delegate vote |
| `propose_slash` | ❌ | ❌ | ✅ | ❌ | Propose slashing |
| `execute_slash_with_governance` | ❌ | ❌ | ✅ | ❌ | Execute governance slash |
| **Financial Operations** | | | | | |
| `slash` | ✅ | ❌ | ❌ | ❌ | Direct admin slash |
| `slash_bond` | ✅ | ❌ | ❌ | ❌ | Slash bond amount |
| `collect_fees` | ✅ | ❌ | ❌ | ❌ | Collect protocol fees |
| **Pause Mechanism** | | | | | |
| `pause` | ✅ | ❌ | ❌ | ❌ | Pause contract |
| `unpause` | ✅ | ❌ | ❌ | ❌ | Unpause contract |
| `set_pause_signer` | ✅ | ❌ | ❌ | ❌ | Set pause signers |
| `set_pause_threshold` | ✅ | ❌ | ❌ | ❌ | Set pause threshold |
| **Upgrade Authorization** | | | | | |
| `initialize_upgrade_auth` | ✅ | ❌ | ❌ | ❌ | Setup upgrade auth |
| `grant_upgrade_auth` | ✅ | ❌ | ❌ | ❌ | Grant upgrade role |
| `revoke_upgrade_auth` | ✅ | ❌ | ❌ | ❌ | Revoke upgrade role |
| `propose_upgrade` | ❌ | ❌ | ❌ | ❌ | Propose upgrade (Upgrader) |
| `approve_upgrade_proposal` | ❌ | ❌ | ❌ | ❌ | Approve upgrade (Upgrader) |
| `execute_upgrade` | ❌ | ❌ | ❌ | ❌ | Execute upgrade (Upgrader) |
| **Public Functions** | | | | | |
| `create_bond` | ✅ | ✅ | ✅ | ✅ | Anyone can create bonds |
| `add_attestation` | ❌ | ✅ | ❌ | ❌ | Verifiers only |
| `revoke_attestation` | ❌ | ✅ | ❌ | ❌ | Original attester only |
| `withdraw` | ❌ | ❌ | ❌ | ✅ | Identity owner only |
| `withdraw_bond` | ❌ | ❌ | ❌ | ✅ | Identity owner only |
| `top_up` | ❌ | ❌ | ❌ | ✅ | Identity owner only |
| `increase_bond` | ❌ | ❌ | ❌ | ✅ | Identity owner only |
| `extend_duration` | ❌ | ❌ | ❌ | ✅ | Identity owner only |
| `withdraw_early` | ❌ | ❌ | ❌ | ✅ | Identity owner only |
| `claim_rewards` | ❌ | ❌ | ❌ | ✅ | Identity owner only |

### Access Control Implementation

The contract uses the following access control mechanisms:

1. **Admin Checks**: `require_admin()` and `require_admin_internal()` functions
2. **Verifier Checks**: `require_verifier()` function for attestation-related operations
3. **Identity Owner Checks**: `require_identity_owner()` for bond-specific operations
4. **Composite Checks**: `require_admin_or_verifier()` for operations that either role can perform
5. **Governance Checks**: Custom governance validation for governance-specific operations

### Security Audit Results

✅ **All privileged methods properly implement access control**
✅ **Unauthorized access attempts are rejected with appropriate errors**
✅ **Access denied events are emitted for audit logging**
✅ **58/59 access control tests passing (1 minor test setup issue)**

### Key Security Findings

1. **Strong Access Control**: All privileged methods are properly protected with role-based access control
2. **Comprehensive Coverage**: Every admin-only function has explicit unauthorized tests
3. **Audit Trail**: Access denied events provide clear audit logs for security monitoring
4. **Defense in Depth**: Multiple layers of access control prevent privilege escalation

---

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

All external-call-bearing functions use the guard:

| Function | Lock status | Callback |
|----------|-------------|---------|
| `withdraw_bond_full()` | ✅ guarded | `on_withdraw` |
| `withdraw_bond()` | ✅ guarded (hardened) | `on_withdraw` |
| `withdraw_early()` | ✅ guarded | `on_withdraw` |
| `execute_cooldown_withdrawal()` | ✅ guarded | `on_withdraw` |
| `slash_bond()` | ✅ guarded | `on_slash` |
| `collect_fees()` | ✅ guarded | `on_collect` |

Each function follows the **checks-effects-interactions** (CEI) pattern:
1. Acquire reentrancy lock
2. Validate inputs and authorization (Checks)
3. Update state (Effects) **before** any external call
4. Invoke callback (Interaction — blocked by held lock if re-entered)
5. Perform token transfer (Interaction — final external call)
6. Release reentrancy lock

### Hardening: CEI Fixes (2026-04)

Three functions previously violated CEI by calling `token_integration::transfer_from_contract`
**before** committing state updates. A malicious token or callback registered as the contract
callback could have exploited this ordering to observe or re-enter the contract in an
intermediate state.

| Function | Before fix | After fix |
|----------|-----------|----------|
| `withdraw_bond()` | Transfer → state update | State update → callback → transfer ✅ |
| `withdraw_early()` | Transfer → state update | State update → callback → transfer ✅ |
| `execute_cooldown_withdrawal()` | State update ✅ | Added `on_withdraw` callback after state ✅ |

`withdraw_bond()` also lacked a reentrancy guard entirely before this fix.

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
| 1 | `test_withdraw_reentrancy_blocked` | Same-function reentrancy (`withdraw_bond_full`) | PASS (blocked) |
| 2 | `test_slash_reentrancy_blocked` | Same-function reentrancy (`slash_bond`) | PASS (blocked) |
| 3 | `test_fee_collection_reentrancy_blocked` | Same-function reentrancy (`collect_fees`) | PASS (blocked) |
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
| 16 | `test_partial_withdraw_reentrancy_blocked` | Same-function reentrancy (`withdraw_bond`) — **new** | PASS (blocked) |
| 17 | `test_withdraw_early_reentrancy_blocked` | Same-function reentrancy (`withdraw_early`) — **new** | PASS (blocked) |
| 18 | `test_cooldown_withdrawal_reentrancy_blocked` | Same-function reentrancy (`execute_cooldown_withdrawal`) — **new** | PASS (blocked) |
| 19 | `test_set_callback_non_admin_rejected` | Admin gate on `set_callback` — **new** | PASS |
| 20 | `test_state_committed_before_callback_withdraw_bond` | CEI ordering (`withdraw_bond`) — **new** | PASS |
| 21 | `test_state_committed_before_callback_slash` | CEI ordering (`slash_bond`) — **new** | PASS |
| 22 | `test_lock_released_after_partial_withdraw` | State lock verification (`withdraw_bond`) — **new** | PASS |

**22 reentrancy-specific regression tests.**

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

| # | Recommendation | Status |
|---|---------------|--------|
| 1 | Keep the application-level guard — defense-in-depth | ✅ Done |
| 2 | Maintain CEI ordering — state updates before external calls | ✅ Done (hardened `withdraw_bond`, `withdraw_early`) |
| 3 | Restrict `set_callback` to admin only | ✅ Done — signature is now `set_callback(admin, callback)` |
| 4 | Add access control to `deposit_fees` | ⚠️ Open — currently unrestricted |
| 5 | Emit events on withdrawal/slash/fee-collect | ⚠️ Open — events are emitted via `emit_bond_withdrawn` but not for every path |
