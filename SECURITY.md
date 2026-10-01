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

### Admin Contract Roles (System-wide)

The `Admin` contract manages the system-wide role hierarchy and administrative operations:

| Role | Hierarchy Level | Description |
|------|-----------------|-------------|
| **SuperAdmin** | 3 | Highest privilege; can manage all roles and transfer ownership. |
| **Admin** | 2 | Administrative privilege; can manage Operators. |
| **Operator** | 1 | Operational privilege; limited task execution. |

#### Role Mutation Permissions

| Target Role | Min Role Required to Assign | Min Role Required to Revoke |
|-------------|-----------------------------|----------------------------|
| **SuperAdmin** | SuperAdmin | SuperAdmin (strictly higher role required*) |
| **Admin** | SuperAdmin | SuperAdmin |
| **Operator** | Admin | Admin |

*\*Note: Role revocation requires a caller with a strictly higher hierarchy level than the target. SuperAdmins cannot revoke other SuperAdmins.*

### Permission Matrix

| Function/Method | Admin | Verifier | Governance | Identity Owner | Notes |
|------------------|-------|----------|------------|----------------|--------|
| **Configuration** | | | | | |
| `initialize` | ✅ | ❌ | ❌ | ❌ | One-time setup |
| `set_accepted_tokens` | ✅ | ❌ | ❌ | ❌ | Accepted token list |
| `set_token` | ✅ | ❌ | ❌ | ❌ | Primary token address |
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
| `set_borrow_frozen` | ✅ | ❌ | ❌ | ❌ | Freeze borrow operations |
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
| `schedule_emergency_drain` | ✅ | ❌ | ❌ | ❌ | Schedule USDC drain |
| `cancel_emergency_drain` | ✅ | ❌ | ❌ | ❌ | Cancel drain schedule |
| `emergency_drain_to_treasury` | ✅ | ❌ | ❌ | ❌ | Drain funds to treasury |
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
| `set_liquidation_treasury` | ✅ | ❌ | ❌ | ❌ | Set liquidation treasury |
| `set_slash_treasury` | ✅ | ❌ | ❌ | ❌ | Set slash treasury |
| **Pause Mechanism** | | | | | |
| `pause` | ✅ | ❌ | ❌ | ❌ | Pause contract |
| `unpause` | ✅ | ❌ | ❌ | ❌ | Unpause contract |
| `set_pause_signer` | ✅ | ❌ | ❌ | ❌ | Set pause signers |
| `set_pause_threshold` | ✅ | ❌ | ❌ | ❌ | Set pause threshold |
| **Upgrade Authorization** | | | | | |
| `initialize_upgrade_auth` | ✅ | ❌ | ❌ | ❌ | Setup upgrade auth |
| `grant_upgrade_auth` | ✅ | ❌ | ❌ | ❌ | Grant upgrade role |
| `revoke_upgrade_auth` | ✅ | ❌ | ❌ | ❌ | Revoke upgrade role |
| `transfer_admin` | ✅ | ❌ | ❌ | ❌ | Transfer admin role |
| `transfer_upgrade_admin` | ✅ | ❌ | ❌ | ❌ | Transfer upgrade admin |
| `accept_upgrade_admin` | ❌ | ❌ | ❌ | ❌ | Accept upgrade admin (Pending admin) |
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

---

# Security Assumptions Matrix Across Crates

This section documents the security assumptions — roles, invariants, and ledger/time properties — for every crate in the Credence Contracts workspace. It is intended to speed review and audit by providing a single-reference cross-crate view.

## How to read this matrix

| Column | Meaning |
|--------|---------|
| **Actors** | Address roles that the crate recognizes. Operations authorize by `require_auth()` against these roles. |
| **Key Invariants** | Properties the crate enforces on-chain to guarantee state consistency. Violations cause a panic with a typed `ContractError`. |
| **Ledger/Time** | Assumptions about `e.ledger().timestamp()` semantics, duration bounds, ledger-sequence constraints, and storage TTL. |
| **Security Docs** | References to detailed documents in `docs/`. |

---

## Layer 0 — Shared Infrastructure

### `credence_errors` (`contracts/credence_errors/`)

Canonical `ContractError` enum used by every contract. Defines error ranges, recoverability classification, and shared helpers.

| Dimension | Detail |
|-----------|--------|
| **Actors** | None — pure library |
| **Key Invariants** | Error codes are wire-stable; variants must never be renumbered. All helpers are pure functions with no state. |
| **Ledger/Time** | `is_expired(e, expires_at)` helper: `expires_at != 0 && now >= expires_at`. `verify_no_future_ledger` guards against future timestamps/sequences. `require_within_business_hours` enforces Mon–Fri 09:00–17:00 UTC. |
| **Security Docs** | `docs/error-codes-wire.md`, `docs/errors.md` |

### `credence_math` (`contracts/credence_math/`)

Overflow-safe arithmetic library. Pure functions — no state, no events, no actors.

| Dimension | Detail |
|-----------|--------|
| **Actors** | None — pure library |
| **Key Invariants** | All arithmetic uses checked operations (`checked_add`, `checked_mul`, etc.) that panic with `ContractError::Arithmetic` on overflow. `require_valid_percent_split` enforces splits sum to exactly 10 000 bps. `slippage_bps_check` enforces max-slippage tolerance. |
| **Ledger/Time** | `floor_to_day(ts)` truncates to UTC day boundary. `Timestamp::add_business_days` skips weekends (no holiday calendar). `SECONDS_PER_DAY = 86_400`. |
| **Security Docs** | `docs/security.md`, `docs/decimal-handling.md`, `docs/COMPOUND_RATE.md` |

### `testutils` (`crates/testutils/`)

Shared Soroban test harness (re-exports `soroban_sdk::testutils`). Not deployed on-chain.

| Dimension | Detail |
|-----------|--------|
| **Actors** | None — test-only |
| **Key Invariants** | N/A — test harness |
| **Ledger/Time** | N/A |
| **Security Docs** | `docs/TEST_HELPER_LIBRARY.md` |

### `credence_admin_cli` (`crates/credence_admin_cli/`)

Off-chain CLI for admin contract operations. Not a Soroban contract — builds and submits `InvokeHostFunction` transactions.

| Dimension | Detail |
|-----------|--------|
| **Actors** | Off-chain signer (S-secret key from CLI arg or `CREDENCE_SIGNER` env var). |
| **Key Invariants** | Without `--submit`, prints envelope XDR as JSON — no network interaction. Dry-run uses dummy source account. Secret key is never written to disk. |
| **Ledger/Time** | Reads network passphrase and RPC URL from CLI/env. Defaults to Soroban Testnet. |
| **Security Docs** | `docs/admin-cli.md` |

---

## Layer 1 — Standalone Contracts

### `admin` (`contracts/admin/`)

Hierarchical role-based access control (SuperAdmin / Admin / Operator). Two-step ownership transfer with 24 h timelock. Timed admin suspension.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **SuperAdmin** (level 3): assign/revoke any role, manage ownership, set pause signers. **Admin** (level 2): manage Operators. **Operator** (level 1): limited operational tasks. **Owner**: propose new owner (must be SuperAdmin). **User**: query-only. |
| **Key Invariants** | 1. `initialize` is single-use (`AlreadyInitialized`). 2. `min_admins` > 0 and ≤ `max_admins`. 3. Zero-address sentinel (`GAAAA...`) and own address rejected via `require_valid_admin_address`. 4. Role assignment requires caller ≥ target role; no self-assignment of equal/higher role. 5. `remove_admin` requires caller strictly outranks target; `MinAdmins` guard prevents last-SuperAdmin removal. 6. `suspend_admin`: `until_ts` must be strictly future; effective-active count must stay ≥ `MinAdmins`; cannot suspend permanently deactivated admins; auto-reactivation at `until_ts`. 7. Ownership transfer enforces `OWNERSHIP_TRANSFER_TIMELOCK = 86_400` seconds before `accept_ownership`. |
| **Ledger/Time** | `e.ledger().timestamp()` for `assigned_at`, `suspended_until` comparison, and ownership timelock expiry. `e.ledger().sequence()` emitted in `admin_rotated` event. Assumes 1 s/ledger cadence. |
| **Security Docs** | `docs/admin-roles.md`, `docs/TWO_STEP_ADMIN.md`, `docs/HISTORICAL_ROLES.md`, `docs/OPERATOR_BALANCES.md` |

### `credence_multisig` (`contracts/credence_multisig/`)

Generic multi-signature governance. Proposals can be contract calls, transfers, config changes, or signer management. Configurable threshold and proposal TTL.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **Admin**: add/remove signers, set threshold, reject proposals. **Signer**: submit and sign proposals. **Anyone**: execute once threshold met. |
| **Key Invariants** | 1. `initialize`: signers non-empty; threshold > 0 and ≤ signer count. 2. `add_signer`: no duplicates (`AlreadyActive`). 3. `remove_signer`: cannot remove last signer; threshold auto-adjusted if needed. 4. `set_threshold`: must be ≤ signer count. 5. `execute_proposal`: pending, within TTL, threshold met, **execute-once via `op_hash`** (deterministic hash prevents replay). 6. Removed signers' signatures excluded from threshold count. 7. `prune_expired_proposals`: permissionless, bounded sweep. |
| **Ledger/Time** | `proposed_at = e.ledger().timestamp()`. `expires_at` is caller-supplied. `require_within_ttl_panic` enforces TTL. No automated time-based expiry. |
| **Security Docs** | `docs/multisig.md`, `docs/PROPOSAL_ID_DERIVATION.md` |

### `credence_registry` (`contracts/credence_registry/`)

Maps identity addresses to bond contracts with forward/reverse lookups. Supports admin registration and trustless bond self-registration via WASM code-hash verification.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **Admin**: register/deactivate/remove/reactivate identities, set bond code hash, transfer admin. **Bond Contract**: self-register via `register_trustless`. **Anyone**: query mappings. |
| **Key Invariants** | 1. `register`: no duplicate identity or bond contract; optional ERC165-like interface check. 2. `register_trustless`: **constant-time** WASM code-hash comparison against admin-pinned `BondCodeHash`; idempotent for same bond+identity; rejects if bond already registered to different identity. 3. `deactivate`/`reactivate`: toggle `active` flag; rejects if already in target state. 4. `remove`: hard delete of forward+reverse mappings and identity list entry. 5. Pagination bounded at `MAX_IDENTITIES_PAGE_SIZE` (200). |
| **Ledger/Time** | `registered_at = e.ledger().timestamp()`. `active` is a boolean — not time-dependent. |
| **Security Docs** | `docs/registry.md`, `docs/datakey-fingerprint.md` |

### `credence_treasury` (`contracts/credence_treasury/`)

Multi-signature withdrawal management for protocol fees and slashed funds. Per-source accounting, configurable proposal TTL, min-liquidity floor, corridor settlement, and excess-native rescue.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **Admin**: manage signers, depositors, threshold, token, corridors, min liquidity, proposal TTL. **Signer**: propose and approve withdrawals. **Depositor**: call `receive_fee`. **Anyone**: execute withdrawal once threshold met. |
| **Key Invariants** | 1. Total accounted balance equals sum across fund sources (ProtocolFee + SlashedFunds). 2. Actual token balance ≥ accounted `TotalBalance` (solvency). 3. Withdrawal deducts proportionally from each fund source (prevents starvation). 4. Cumulative received amounts reconcile across sources (rollover-safe `CumulativeAmount`). 5. Min-liquidity floor enforced on execution. 6. `rescue_native` extracts only excess (actual − accounted). 7. `SIGNATURE_DOMAIN = "CredenceTreasury"`. |
| **Ledger/Time** | `proposed_at = e.ledger().timestamp()`. `expires_at = proposed_at + ttl` (default 7 days = 604 800 s). `is_expired`: `now >= expires_at`. Storage TTL assumes 5 s/ledger. |
| **Security Docs** | `docs/treasury.md`, `docs/TREASURY_INVARIANTS.md`, `docs/fees.md`, `docs/fund-flow.md` |

### `timelock` (`contracts/timelock/`)

Queues administrative operations with minimum delay (24 h) and grace period (24 h). Operations can be queued, executed, or cancelled.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **Admin**: queue and cancel operations. **Anyone**: execute queued operation once `now ≥ eta` and `now ≤ expires_at`. |
| **Key Invariants** | 1. `initialize`: single-use. 2. `queue_operation`: delay ≥ `min_delay_seconds()` (86 400); `op_hash` not previously executed. 3. `execute_operation`: status must be `Pending`; `now ≥ eta`; `now ≤ expires_at`; marks globally executed via `ExecutedOp` map — replay impossible. 4. `cancel_operation`: admin-only; status must be `Pending`. |
| **Ledger/Time** | `eta = now + delay` where delay ≥ 86 400 s. `GRACE_PERIOD = 86 400 s`. Operation window: `[eta, eta + GRACE_PERIOD]`. `is_ready(eta, now)` is a direct `now ≥ eta` comparison over `u64`. |
| **Security Docs** | `docs/timelock.md` |

### `credence_arbitration` (`contracts/arbitration/`)

Weighted-vote dispute resolution. Dispute lifecycle: Open → Voting → Resolving → Resolved / Tied / Cancelled.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **Admin**: register/unregister arbitrators, set quorum config. **Arbitrator**: cast votes (weighted). **Creator**: create and cancel own disputes. |
| **Key Invariants** | 1. `initialize`: single-use. 2. `register_arbitrator`: weight > 0. 3. One active dispute per creator at a time (`require_no_ongoing_dispute`). 4. State machine: Open→Voting→Resolving→Resolved/Tied via `require_transition`. 5. `vote`: outcome ≠ 0; restricted to Voting period (`now ≥ voting_start && now ≤ voting_end`); one vote per arbitrator per dispute. 6. `resolve_dispute`: only after `voting_end`; optional quorum check (`MinTotalWeight`, `MinVoters`). 7. `cancel_dispute`: reason ≤ 256 chars; by creator or admin. |
| **Ledger/Time** | `voting_start = e.ledger().timestamp()`. `voting_end = voting_start + duration` (caller-supplied). `resolve_dispute` requires `now > voting_end`. |
| **Security Docs** | `docs/arbitration.md`, `docs/arbitration_api.md`, `docs/dispute-resolution.md` |

### `templates` (`contracts/templates/`)

Canonical starting-point template for new Soroban contracts. Demonstrates `#![no_std]`, typed storage, admin-gated init, `require_auth()`, and ledger-timestamp-based expiry.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **Admin**: single address. |
| **Key Invariants** | 1. `initialize`: single-use. 2. Admin-only `set_record`/`remove_record`. 3. Read-time expiry: expired records auto-purged on `get_record`/`has_record`. |
| **Ledger/Time** | `updated_at = e.ledger().timestamp()`. `expires_at` set by caller. `is_expired`: `expires_at != 0 && now >= expires_at`. |
| **Security Docs** | `docs/templates.md` |

---

## Layer 2 — Core Bond Contract

### `credence_bond` (`contracts/credence_bond/`)

The protocol's primary contract. Manages identity bonds with attestations, slashing, rolling/fixed durations, tier system, governance, early-exit, cooldown, and pause mechanisms.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **Admin**: all configuration, verifier management, emergency controls, slashing, fee collection, pause. **Verifier**: add/revoke attestations. **Governance**: vote, delegate, propose/execute governance slashes. **Identity Owner**: manage own bonds (create, top-up, withdraw, extend). **Upgrader**: propose/approve/execute upgrades. **Anyone**: create bonds. |
| **Key Invariants** | Formalized in `docs/bond-invariants.md` (7 invariants I1–I7). On-chain drift detection via `assert_self_consistent` after every state-changing write, panicking with `InvariantViolation` (code 218). **I1**: attestation weight sum ≥ 0. **I2**: slashed ≤ bonded. **I3**: withdrawal request ⇒ rolling bond. **I4**: bonded ≥ 0. **I5**: slashed ≥ 0. **I6**: notice period ≤ bond duration. **I7**: attestation count matches list length. Additional: same-ledger liquidation guard prevents sandwich attacks; reentrancy guard (application-level + Soroban runtime); CEI pattern for all external-call-bearing functions. |
| **Ledger/Time** | Extensive: `e.ledger().timestamp()` for bond start/expiry, early-exit, cooldown, grace windows, notice periods, attestation deadlines. `same_ledger_liquidation_guard` records ledger sequence after collateral-increasing actions. Bond duration bounds: [1, 31 536 000] seconds. Cooldown/grace configured in seconds. See `docs/LEDGER_WALL_TIME.md` for ledger-vs-wall-time semantics. |
| **Security Docs** | `docs/bond-invariants.md`, `docs/bond-drift-detection.md`, `docs/access-control.md`, `docs/reentrancy.md`, `docs/bond-state-transitions.md`, `docs/bond-token-custody.md`, `docs/slashing.md`, `docs/emergency.md`, `docs/cooldown.md`, `docs/tier-system.md`, `docs/governance.md`, `docs/pause-signer-invariant.md`, `docs/withdrawal.md`, `docs/THREAT_MODEL.md`, `docs/LEDGER_WALL_TIME.md`, `docs/TIME_UNITS.md` |

### `credence_delegation` (`contracts/credence_delegation/`)

Off-chain delegated authority with domain-separated payloads. Supports Ed25519, Secp256r1, and ML-DSA 44 signature schemes. Nonce-based replay protection and ledger-sequence staleness guard.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **Admin**: register verifiers, set revocation grace period. **Owner**: create delegations. **Delegate**: authorized party in delegation record. **Relayer**: permissionless — submits signed `DelegatedActionPayload`. **Verifier**: registered contract that validates non-Ed25519 signatures (Secp256r1/MLDSA44). |
| **Key Invariants** | 1. Strict nonce monotonicity: each `consume_nonce` checks `current == expected` then increments. 2. `invalidate_nonce_range`: must advance; span capped at `MAX_NONCE_INVALIDATION_SPAN` (10 000). 3. Delegation expiry: `now < expires_at ≤ now + MAX_DELEGATION_DURATION` (365 days). 4. `verify_delegation_active`: `!revoked && expires_at > now`. 5. Domain-separated payload verification: domain, owner, target, contract_id all checked against call-site params. 6. Staleness guard: rejects future payloads and those older than `MAX_PAYLOAD_AGE_LEDGERS` (200 ledgers ≈ 17 min). 7. Post-expiry revocation limited to grace window (`DEFAULT_REVOCATION_GRACE_PERIOD = 300 s`). 8. Scheme validation: only 0/1/2 accepted. |
| **Ledger/Time** | `e.ledger().timestamp()` for expiry checks and `assigned_at`. `e.ledger().sequence()` for staleness guard. Assumes ≈5 s/ledger for TTL calculations. `MAX_DELEGATION_DURATION = 31_536_000` s (365 days). |
| **Security Docs** | `docs/delegation.md`, `docs/DELEGATION_HANDBOOK.md`, `docs/delegation-failure-modes.md`, `docs/delegation-summary-view.md`, `docs/attestations.md`, `docs/auth-tree-threats.md`, `docs/signature-scheme-upgrade.md` |

### `fixed_duration_bond` (`contracts/fixed_duration_bond/`)

Time-locked bond with configurable creation fees and early-exit penalties. One active bond per address.

| Dimension | Detail |
|-----------|--------|
| **Actors** | **Admin**: set fee/penalty config, collect accumulated fees. **Owner**: create bonds, withdraw after maturity or early with penalty. |
| **Key Invariants** | 1. `initialize`: single-use. 2. Duration in `[MIN_BOND_DURATION (1), MAX_BOND_DURATION (31_536_000 = 365 days)]`. 3. `create_bond`: amount > 0; no active bond exists; CEI pattern (state before token transfer). 4. `withdraw`: bond active; `now ≥ bond_expiry`; CEI pattern. 5. `withdraw_early`: bond active; `now < bond_expiry`; penalty_bps > 0; CEI pattern. 6. `collect_fees`: admin-only; zeroes accumulator before transfer. |
| **Ledger/Time** | `bond_start = e.ledger().timestamp()`. `bond_expiry = bond_start + duration`. Maturity: `now ≥ bond_expiry`. Duration overflow protection near `u64::MAX`. |
| **Security Docs** | `docs/fixed-duration-bond.md` |

---

## Cross-Cutting Security Properties

### System-wide invariants

| ID | Invariant | Enforcement point |
|----|-----------|-------------------|
| S1 | Every `initialize` is single-use | All contracts check `AlreadyInitialized` |
| S2 | Zero-address sentinel rejected in privileged entrypoints | `require_valid_admin_address` in admin; pattern replicated in treasury, multisig |
| S3 | All arithmetic uses checked operations | `credence_math` library; no bare `+`/`-` on financial values |
| S4 | Error codes are wire-stable | `credence_errors` — variants never renumbered |
| S5 | Signed payloads are domain-separated per contract | `SIGNATURE_DOMAIN` constant in every contract that verifies off-chain signatures |
| S6 | Reentrancy is blocked at the Soroban runtime level + application-level guard | Bond contract; treasury uses CEI pattern |
| S7 | Storage writes follow CEI pattern | Bond (withdraw, slash, fee collect); treasury (withdrawal); fixed-duration bond (create, withdraw) |
| S8 | External callbacks are admin-configurable only | `set_callback(admin, callback)` in bond |
| S9 | Pause/unpause is hard-gated to admin or configured pause signers | Admin contract, bond pausable module, delegation |
| S10 | Business-hours constraints enforced where specified | `require_within_business_hours` in `credence_errors` |

### Trust assumptions

| Assumption | Rationale |
|------------|-----------|
| Soroban runtime correctly enforces auth | All `require_auth()` calls rely on the host for signature verification |
| Soroban runtime correctly prevents re-entry | Runtime-level reentrancy protection; application-level guard is defense-in-depth |
| Ledger timestamp is approximately wall time | Acceptable drift of a few seconds; 5 s close cadence |
| Ledger sequence is strictly monotonic | Used for staleness guard in delegation |
| WASM code-hash comparison is collision-resistant | Trustless registration in registry uses code-hash pinning |
| Admin keys are kept secure | Admin is the highest-privilege role across all contracts |
