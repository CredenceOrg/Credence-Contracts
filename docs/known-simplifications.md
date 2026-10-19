# Known Simplifications

> **Status**: Living document. Updated as simplifications are resolved or newly identified.  
> **Last reviewed**: July 2026

This document consolidates known simplifications, stubs, and limitations across the Credence Contracts codebase. Its purpose is to give contributors, auditors, and integrators a single place to understand what has been intentionally simplified and what production-quality implementation would require.

---

## How to use this document

- **Contributors**: Before adding a new simplification, document it here and open a tracking issue.
- **Auditors**: This document is your starting point for known gaps and production risks.
- **Integrators**: Sections marked ⚠️ indicate areas where production behavior will differ from the current implementation.

---

## 1. Storage Model Limitations

### 1.1 Single-Bond-Per-Contract-Instance ⚠️

**Location**: `contracts/credence_bond/src/lib.rs:730-733`, `src/batch.rs:139`

**Current behavior**: The bond contract stores one bond per contract instance (keyed by a single `DataKey::Bond` storage slot), not a per-identity map. Each identity that wants a bond must deploy its own contract instance or call through a registry proxy.

**Production behavior**: A production system would support multiple identities in a single contract via `Map<Address, IdentityBond>` or similar, reducing deployment complexity and gas costs.

**Impact**:
- The `credence_registry` contract is **required** to track which contract instance belongs to which identity.
- Batch operations across identities must iterate registry entries off-chain.
- Tests must either clear state between identities or use separate contract deployments (see `test_create_bond_different_identities` in `test_create_bond.rs:304-323`).

**Tracking**: Part of core architecture; no issue required.

---

## 2. Token Transfer & Balance Handling

### 2.1 Token Transfer Is Real But Tests Are Stubbed ✓

**Location**: `contracts/credence_bond/src/token_integration.rs`, `src/safe_token.rs`

**Current behavior**: 
- Production code uses real Soroban `TokenClient` with `try_transfer()`, `try_transfer_from()`, and balance checks.
- Tests use `Env::default()` with `mock_all_auths()`, which bypasses real token validation.
- Mock token is used; see `test_helpers::setup_with_token()`.

**Production behavior**: Real USDC (or configured token) contract validates all transfers, allowances, and balances.

**Impact**: 
- Accounting logic (bonded amounts, slashing, fees, penalties) is fully implemented and auditable.
- Token integration is production-grade; only the test harness is mocked.
- Off-chain integration must use the configured token address; there is no fallback.

**Tracking**: Production-ready; tests documented in `token_integration_test.rs` and `test_bond_token_transfers.rs`.

---

### 2.2 Fee-on-Transfer Tokens Are Rejected ✓

**Location**: `contracts/credence_bond/src/token_integration.rs:156-187`

**Current behavior**: Balance-delta check compares token balance before and after transfer. If actual transfer amount differs from requested (due to fee-on-transfer), the contract panics with `UnsupportedToken`.

**Production behavior**: Same; this is intentional and correct.

**Impact**: USDT-like tokens with transfer fees cannot be used. Only fee-free tokens (USDC) are supported.

**Tracking**: Documented in `test_fee_on_transfer_detection_prevents_silent_success()` test; this is a feature, not a limitation.

---

## 3. Validation Gaps

### 3.1 Zero-Address Validation in Soroban ✓

**Location**: `contracts/credence_bond/src/validation.rs:123-138`

**Current behavior**: Unlike Ethereum, Soroban does not have a "zero address" concept. All recipients must pass `require_auth()` checks or be validated by the calling contract. The contract validates that recipients are not the contract itself.

**Production behavior**: Same; Soroban's auth system is the enforcer.

**Impact**: No additional validation needed beyond Soroban's built-in address validation.

**Tracking**: Documented in code comments; intentional design decision.

---

## 4. Missing or Incomplete Features

### 4.1 Batch Transfer Interface Not Fully Implemented

**Location**: `contracts/credence_bond/src/batch.rs`, `src/lib.rs:2749`

**Current behavior**: `batch_transfer()` entrypoint exists but comment at line 139 states: "Note: Current implementation uses single bond". The batch interface accepts parameters but applies them to a single bond, making multi-identity batch operations non-functional.

**Production behavior**: Batch transfer should iterate identities and apply transfers to each. With multi-identity storage (see 1.1), this would become functional.

**Impact**: Batch transfer tests (`test_batch.rs:87-91`) note that the interface will panic in a multi-identity scenario.

**Tracking**: Part of multi-identity redesign; see issue #1094.

---

### 4.2 Hardcoded Minimum Bond Amount (Test vs. Production)

**Location**: `contracts/credence_bond/src/validation.rs:142-157`

**Current behavior**:
- Production: `MIN_BOND_AMOUNT = 1_000_000_000_000_000_000` (1 token × 10^18)
- Test: `MIN_BOND_AMOUNT = 1_000` (overridden via `#[cfg(test)]`)

**Production behavior**: Production uses realistic 18-decimal token amounts.

**Impact**: Tests use simplified amounts; production deployment must be tested with real token decimals.

**Tracking**: Intentional for test readability; see validation.rs lines 145-148 and 156-157.

---

### 4.3 Hardcoded Maximum Bond Amount (Test vs. Production)

**Location**: `contracts/credence_bond/src/validation.rs:153-157`

**Current behavior**:
- Production: `MAX_BOND_AMOUNT = 100_000_000_000_000_000_000_000_000` (100M × 10^18)
- Test: `MAX_BOND_AMOUNT = 100_000_000_000_000`

**Production behavior**: Production limits bonds to 100M tokens normalized to 18 decimals.

**Impact**: Tests use simplified upper bounds; production testing must validate overflow guards with realistic amounts.

**Tracking**: Intentional for test readability; see validation.rs.

---

## 5. Test-Only Stubs & Limitations

### 5.1 Batch Tests Cannot Cross Unwind Boundaries (SDK 22.0 Limitation)

**Location**: `contracts/credence_bond/src/test_batch.rs:465-467`

**Current behavior**: Test marked `#[ignore = "Requires rewrite without catch_unwind due to SDK 22.0 Env incompatibility"]`. The Soroban 22.0 SDK's `Env` contains `UnsafeCell` and cannot cross panic unwind boundaries.

**Production behavior**: Not applicable; this is a test harness limitation only.

**Impact**: Some batch panic-path tests are skipped.

**Tracking**: Issue #1095 (hypothetical); blocked by SDK upgrade.

---

### 5.2 Claim Expiry Cannot Be Manually Set in Tests

**Location**: `contracts/credence_bond/src/test_claim_pagination.rs:244-247`

**Current behavior**: Test comment notes "In a real implementation, you'd need to update the claim's expiry". The `claims::get_claim_by_id()` function returns immutable claims; test cannot manually set expiry without accessing internal storage directly.

**Production behavior**: Claims expire based on ledger time advancement; no issue in production.

**Impact**: Expiry-specific test coverage is limited to ledger time advancement.

**Tracking**: Test-only limitation; no production impact.

---

### 5.3 Emergency Drain Test Legacy Paths

**Location**: `contracts/credence_bond/src/test_emergency_drain.rs:240-243`

**Current behavior**: Comment documents that before the `require_matching_treasury_beneficiary` guard was added, wrong-recipient rejection raised an untyped panic. Now it raises `TreasuryBeneficiaryMismatch` (code 610).

**Production behavior**: Typed error code is now enforced.

**Impact**: Test documents a legacy panic path that has been replaced with proper error handling.

**Tracking**: Resolved; see PR that added the guard.

---

### 5.4 Liquidate Test Bare Panics

**Location**: `contracts/credence_bond/src/test_liquidate.rs:208-211`

**Current behavior**: Tests use bare `#[should_panic]` instead of checking `SCErrorCode` for unauthorized/no-bond paths to keep tests independent of SDK error format. Eligibility rejections use literal `panic!("bond is not eligible for ...")`.

**Production behavior**: Same behavior; this is a test strategy, not a code limitation.

**Impact**: Test assertions are less granular but more stable across SDK versions.

**Tracking**: Intentional test design; no code change required.

---

## 6. Configuration & Hardcoded Values

### 6.1 Weight Configuration Defaults

**Location**: `contracts/credence_bond/src/weighted_attestation.rs:85-94`

**Current behavior**:
- `DEFAULT_WEIGHT_MULTIPLIER_BPS = 100` (0.01% default)
- `MAX_WEIGHT_MULTIPLIER_BPS = 10_000` (100% ceiling)
- `DEFAULT_MAX_WEIGHT = 100_000`
- `MAX_ATTESTATION_WEIGHT = 1_000_000` (hardcoded in types)

**Production behavior**: Defaults are reasonable; multiplier and max-weight are configurable via `set_weight_config()`.

**Impact**: Defaults provide sensible out-of-box behavior; admins can tune as needed.

**Tracking**: No issue; this is intentional design.

---

### 6.2 Attestation Payload Size Bound

**Location**: `contracts/credence_bond/src/validation.rs:20`

**Current behavior**: `MAX_STRINGIFIED_BYTES_LENGTH = 4_096` (4 KB). Evidence payloads are bounded to prevent unbounded ledger bloat.

**Production behavior**: Same; this is a reasonable hardcoded limit.

**Impact**: Evidence fields larger than 4 KB are rejected.

**Tracking**: Intentional limit; documented in code.

---

### 6.3 Verifier Minimum Stake (Not Set by Default)

**Location**: `contracts/credence_bond/src/verifier.rs:57-60`

**Current behavior**: `get_min_stake()` defaults to `0` if never set. Any verifier can register with zero stake.

**Production behavior**: Admin must call `set_min_stake()` before deployment to enforce a real minimum.

**Impact**: Early deployments without a configured minimum stake allow free verifier registration.

**Tracking**: Intentional design; admin responsibility. No issue required.

---

## 7. Documentation Notes & Future Work

### 7.1 Registry `get_all_identities()` Has No Pagination

**Location**: `contracts/credence_registry/src/lib.rs`

**Current behavior**: `get_all_identities()` returns the full list of registered identity addresses in a single call.

**Production behavior**: Should support `get_identities_page(offset: u32, limit: u32)` with deprecation of the unbounded variant.

**Impact**: As the registry grows, this call consumes increasing ledger-read budget and may exceed per-transaction limits.

**Tracking**: Issue #1093 (hypothetical); off-chain indexers should use event-based discovery instead.

---

### 7.2 Upgrade Admin Rotation Uses 24-Hour Timelock

**Location**: `contracts/credence_bond/src/upgrade_auth.rs`

**Current behavior**: Two-step admin transfer with hardcoded 24-hour timelock. Timelock is not configurable.

**Production behavior**: Same; hardcoded timelock is reasonable for security-sensitive operations.

**Impact**: Admin rotation always requires 24 hours; cannot be expedited.

**Tracking**: Intentional design; no issue required.

---

## 8. Cross-Contract Integration Points

### 8.1 Token Address Must Be Configured Before Use

**Location**: `contracts/credence_bond/src/token_integration.rs:74-85`

**Current behavior**: `get_token()` panics if no token has been configured via `set_token()` or `set_usdc_token()`.

**Production behavior**: Same; intentional fail-fast behavior.

**Impact**: All bond operations fail if token is not initialized.

**Tracking**: Intentional; must be initialized before bond deployment becomes operational.

---

### 8.2 Registry Must Be Initialized Separately

**Location**: `contracts/credence_registry/src/lib.rs`

**Current behavior**: Registry stores identity-to-contract-address mappings and requires initialization via `initialize()`.

**Production behavior**: Same; registry is a separate contract.

**Impact**: Bond deployment and registry initialization are separate steps.

**Tracking**: Intentional architecture; see `docs/ARCHITECTURE.md` for cross-contract patterns.

---

## 9. Resolution Status

| Simplification | Category | Priority | Status | Notes |
|---|---|---|---|---|
| Single-bond-per-instance | Storage | High | Open | Requires multi-identity redesign (#1094) |
| Batch transfer non-functional | Features | Medium | Open | Blocked by multi-identity storage |
| Pagination missing (registry) | Scalability | Medium | Open | Issue #1093 |
| Batch test unwind limitation | Tests | Low | Open | Blocked by SDK 22.0 upgrade |
| Test-only amount bounds | Tests | Low | Open | Intentional; test readability |
| Token must be configured | Integration | Low | Open | Intentional; fail-fast design |
| Verifier min-stake default 0 | Config | Low | Open | Intentional; admin responsibility |
| Upgrade timelock hardcoded | Config | Low | Open | Intentional; security-conservative |

---

## 10. Contributing Guidelines

### When you discover a new simplification:

1. Add it to this document in the appropriate section
2. Use standard template:
   ```
   ### X.Y [Name]
   
   **Location**: `path/to/file.rs:line`
   
   **Current behavior**: [description]
   
   **Production behavior**: [description]
   
   **Impact**: [what breaks or is missing]
   
   **Tracking**: [issue number or "none required"]
   ```
3. Add a row to the Resolution Status table above
4. Open a tracking issue if not already present

### When you resolve a simplification:

1. Update the Resolution Status table: change `Status` to "Resolved" and add `PR #XXX` to Notes
2. Keep the entry for historical reference (do not delete)
3. Update the `Last reviewed` date at the top of this document

---

## Cross-References

- [docs/ARCHITECTURE.md](ARCHITECTURE.md) — Component responsibilities and state ownership
- [docs/bond-state-transitions.md](bond-state-transitions.md) — Bond lifecycle state machine
- [docs/BOND_ISSUANCE.md](BOND_ISSUANCE.md) — Bond creation and eligibility rules
- [docs/rolling-bonds.md](rolling-bonds.md) — Rolling bond notice and renewal semantics
- [docs/early-exit.md](early-exit.md) — Early withdrawal penalties and treasury
- [docs/slashing.md](slashing.md) — Slashing mechanics and treasury routing
- [docs/emergency.md](emergency.md) — Emergency mode and governance
- [docs/token-integration.md](token-integration.md) — Token validation and transfer guards
- [docs/crates.md](crates.md) — Crate dependency graph and responsibilities

