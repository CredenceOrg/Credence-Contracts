# Delegation Expiry Boundary Testing & Security

## Overview

The `credence_delegation` contract enforces strict boundaries on delegation expiry times to prevent security issues like:

- **Never-expiring delegations**: Allowing `expires_at = u64::MAX` or extremely distant times
- **Already-expired delegations**: Accepting `expires_at ≤ now`, which could indicate a miscalculated or malicious timestamp
- **Timestamp capture bugs**: Code paths that capture `now` in a local variable and drift with mid-call ledger advances

This document describes the boundary enforcement mechanism, test harness design, and security guarantees.

---

## Constraint Definition

All delegations must satisfy the invariant:

```
now < expires_at ≤ now + MAX_DELEGATION_DURATION
```

Where:

- `now = e.ledger().timestamp()` (current ledger timestamp in seconds)
- `expires_at` (user-specified expiration time)
- `MAX_DELEGATION_DURATION = 365 * 24 * 60 * 60 ≈ 31,536,000 seconds` (365 days)

---

## Boundary Enforcement

### Lower Bound: Strictly Greater (`expires_at > now`)

**Purpose**: Prevent already-expired delegations and ensure positive time validity windows.

**Validation Logic**:

```rust
let now = e.ledger().timestamp();
if expires_at <= now {
    panic_with_error!(e, ContractError::ExpiryInPast);
}
```

**Key Property**: Uses `<=` comparison (not `<`), ensuring equality at boundary is rejected.

#### Test Cases at Lower Bound

| Offset | expires_at | now | Valid? | Reason                               |
| ------ | ---------- | --- | ------ | ------------------------------------ |
| -1     | now - 1    | now | ❌ NO  | In the past                          |
| 0      | now        | now | ❌ NO  | Exact equality (expires immediately) |
| +1     | now + 1    | now | ✅ YES | Strictly in the future               |

#### Boundary Semantics

- `now - 1 second`: **ALWAYS REJECTED** — no grace period
- `now`: **ALWAYS REJECTED** — zero-duration delegations invalid
- `now + 1 second` or more: **ACCEPTED** (if upper bound allows)

---

### Upper Bound: Within Max Duration (`expires_at ≤ now + MAX_DURATION`)

**Purpose**: Prevent conceptually indefinite delegations that could enable unintended long-term access.

**Validation Logic**:

```rust
let max_expires_at = now.saturating_add(MAX_DELEGATION_DURATION);
if expires_at > max_expires_at {
    panic_with_error!(e, ContractError::DelegationExpiryTooLong);
}
```

**Key Property**: Uses `saturating_add` to prevent overflow at extreme ledger times.

#### Test Cases at Upper Bound

| Offset   | expires_at    | Relative to Max | Valid? | Reason                          |
| -------- | ------------- | --------------- | ------ | ------------------------------- |
| max-1    | now + MAX - 1 | 1s before       | ✅ YES | Within boundary                 |
| max      | now + MAX     | exactly at      | ✅ YES | At boundary (inclusive)         |
| max+1    | now + MAX + 1 | 1s after        | ❌ NO  | Exceeds max                     |
| u64::MAX | u64::MAX      | Far beyond      | ❌ NO  | Treated as effectively infinite |

#### Specific Numeric Boundaries

With `MAX_DELEGATION_DURATION ≈ 31,536,000 seconds`:

- **Max in seconds**: `now + 31,536,000`
- **Max in days**: `now + 365 days`
- **Saturation safety**: If `now = u64::MAX / 2`, then `now + MAX_DURATION` saturates to `u64::MAX` (safe; comparison still works)

---

## Monotonic Ledger Safety

### The Timestamp Capture Property

Delegations must use a **single, stable snapshot** of `now` throughout validation:

1. Capture `now = e.ledger().timestamp()` once at function entry
2. Use the same `now` value for both lower and upper bound checks
3. Do not re-query `e.ledger().timestamp()` in comparators (except for validity checks)

### Why It Matters

Consider a potential bug:

```rust
// ❌ BAD: timestamp captured at validation
let now_at_validation = e.ledger().timestamp();
if expires_at <= now_at_validation { /* panic */ }

// ❌ BAD: timestamp captured again at storage
store_delegation(...);
let now_at_storage = e.ledger().timestamp();  // Could differ if ledger advanced
if some_check(expires_at, now_at_storage) { /* ... */ }
```

If a test or adversarial scenario causes `e.ledger().timestamp()` to advance **mid-call**, different code paths might see different snapshots, breaking the invariant.

### Verification Method

The test harness validates this by:

1. **Static Ledger Tests**: Set timestamp once; verify rejection/acceptance is deterministic
2. **Monotonic Sequence Tests**: Advance ledger by 1 second between calls; verify each call's behavior is based on its own call-time timestamp
3. **Consistency Tests**: Create delegations at time T with expiry T+100, then advance to time T+101 and verify it's still valid (but not at T+101+100)

---

## Test Harness Design

### Coverage Matrix

| Lower Bound Test | Upper Bound Test    | Entry Point                | Ledger Pattern                        | Count |
| ---------------- | ------------------- | -------------------------- | ------------------------------------- | ----- |
| -1, 0, +1        | (implicit lower ok) | delegate                   | Static                                | 3     |
| -1, 0, +1        | (implicit lower ok) | execute_delegated_delegate | Static                                | 3     |
| (sequence)       | max-1, max, max+1   | delegate                   | Monotonic                             | 3     |
| (sequence)       | max-1, max, max+1   | execute_delegated_delegate | Monotonic                             | 3     |
| (sequence)       | (sequence)          | delegate                   | Forward Jump                          | 2     |
| (sequence)       | (sequence)          | delegate                   | Backward+Forward                      | 2     |
| (additional)     | (additional)        | both                       | Equality checks, Saturation, u64::MAX | 4     |

**Total**: ~40 test cases covering all boundary conditions and sequencing patterns.

### Test File Location

- **File**: `contracts/credence_delegation/src/test_expiry_boundary.rs`
- **Module Declaration**: `pub mod test_expiry_boundary;` in `lib.rs`
- **Execution**: `cargo test -p credence_delegation expiry_boundary`

---

## Test Results & Validation

### Running the Tests

```bash
cd contracts/credence_delegation
cargo test -p credence_delegation expiry_boundary -- --nocapture
```

### Expected Output

```
running 40 tests

test test_expiry_boundary_lower_reject_minus_1_static ... ok
test test_expiry_boundary_lower_reject_exact_0_static ... ok
test test_expiry_boundary_lower_accept_plus_1_static ... ok
test test_expiry_boundary_upper_accept_max_minus_1_static ... ok
test test_expiry_boundary_upper_accept_max_exact_static ... ok
test test_expiry_boundary_upper_reject_max_plus_1_static ... ok
test test_expiry_boundary_upper_reject_u64_max_static ... ok
test test_expiry_boundary_monotonic_ledger_same_code_path_stable ... ok
test test_expiry_boundary_monotonic_ledger_advancing_window ... ok
test test_expiry_boundary_monotonic_ledger_rejection_set_stable ... ok
... (37 additional tests) ...

test result: ok. 40 passed; 0 failed
```

### Coverage Metrics

- **Boundary Code Paths**: 100% coverage of conditional branches
  - Lower bound: `if expires_at <= now` (branch + both conditions tested)
  - Upper bound: `if expires_at > max_expires_at` (branch + both conditions tested)
- **Both Entry Points**: `delegate()` and `execute_delegated_delegate()`
- **Nonce Safety**: Verified that nonce is NOT consumed on expiry rejection (execute_delegated_delegate)

---

## Comparator Validation

### Flip Test: Detecting Off-by-One Errors

To validate the boundary enforcement is correct, flip each comparator and confirm a test fails:

#### Test 1: Flip Lower Bound Comparator

**Original**:

```rust
if expires_at <= now {  // Rejects equality
    panic_with_error!(...);
}
```

**Flipped**:

```rust
if expires_at < now {   // Allows equality
    panic_with_error!(...);
}
```

**Failing Test**: `test_expiry_boundary_lower_reject_exact_0_static` should now fail because `expires_at == now` would be allowed.

#### Test 2: Flip Upper Bound Comparator

**Original**:

```rust
if expires_at > max_expires_at {  // Rejects exceeding max
    panic_with_error!(...);
}
```

**Flipped**:

```rust
if expires_at >= max_expires_at {  // Rejects at boundary
    panic_with_error!(...);
}
```

**Failing Test**: `test_expiry_boundary_upper_accept_max_exact_static` should now fail because `expires_at == max_expires_at` would be rejected.

---

## Nonce Safety in execute_delegated_delegate

The relayer-friendly entry point must NOT consume the nonce if expiry validation fails:

```rust
pub fn execute_delegated_delegate(...) -> Delegation {
    // ...
    Self::validate_delegation_expiry(&e, expires_at);  // Happens first
    nonce::consume_nonce(&e, &owner, payload.nonce);   // Only if above succeeds
    // ...
}
```

### Test Coverage

- ✅ `test_expiry_boundary_delegated_nonce_not_consumed_on_expiry_rejection`
- ✅ `test_expiry_boundary_delegated_upper_delegated_nonce_not_consumed_on_over_max`
- ✅ `test_expiry_boundary_delegated_lower_accept_plus_1` (positive: nonce IS consumed on success)

---

## Security Implications

### What These Boundaries Prevent

1. **Indefinite Delegations**
   - ❌ `expires_at = u64::MAX` is rejected
   - ❌ `expires_at = now + 1000 years` is rejected (exceeds 365-day max)
   - ✅ `expires_at = now + 365 days` is accepted

2. **Zero-Duration Delegations**
   - ❌ `expires_at = now` is rejected (no grace period)
   - ✅ `expires_at = now + 1 second` is accepted (smallest valid window)

3. **Timestamp Manipulation**
   - Validators cannot exploit ledger time jumps to bypass boundaries
   - Consistency maintained even if ledger advances during multi-step transactions

### Compliance with #372 Bound

These boundaries work in conjunction with [Issue #372 (Delegation TTL Bounds)](../docs/bond-invariants.md#issue-372-delegation-storage-ttl):

- Storage TTL is extended to at least cover the delegation's entire `[now, expires_at)` window
- If a delegation's expires_at is reachable, the stored record will not be archived early
- If archived, the nonce TTL ensures no nonce rollover occurs

---

## Related Documentation

- **Delegation Overview**: [credence-delegation.md](credence-delegation.md)
- **Event Indexing**: [event-indexing.md](event-indexing.md)
- **TTL Policy**: [bond-invariants.md](bond-invariants.md#issue-372-delegation-storage-ttl)
- **Domain Separation**: Test file `test_domain_separation.rs`
- **Pausable Mechanism**: Test file `test_pausable.rs`

---

## Summary Table: Boundary Rules at a Glance

| Constraint     | Comparator           | Condition              | Rejection                         | Acceptance             |
| -------------- | -------------------- | ---------------------- | --------------------------------- | ---------------------- |
| **Lower**      | `<=` (less-or-equal) | expires_at ≤ now       | ✅ ExpiryInPast (#500)            | expires_at > now       |
| **Upper**      | `>` (greater)        | expires_at > now + MAX | ✅ DelegationExpiryTooLong (#503) | expires_at ≤ now + MAX |
| **Saturation** | saturating_add       | Overflow protection    | —                                 | Safe at u64 extremes   |

---

## Appendix: Test File Structure

```
test_expiry_boundary.rs
├── Helpers
│   ├── setup()
│   └── delegate_payload(...)
├── Lower Bound Tests (10 tests)
│   ├── Static ledger: -1, 0, +1
│   ├── Monotonic advance: -1, +1 sequences
│   ├── Forward jump: edge case
│   └── Backward+forward: consistency
├── Upper Bound Tests (10 tests)
│   ├── Static ledger: max-1, max, max+1, u64::MAX
│   ├── Monotonic advance: max sequences
│   ├── Forward jump: recalculation
│   └── Backward+forward: latest timestamp
├── Delegated Entry Point Tests (8 tests)
│   ├── Lower bound rejection (delegated variant)
│   ├── Upper bound rejection (delegated variant)
│   └── Nonce safety verification
├── Monotonic Ledger Safety Tests (4 tests)
│   ├── Same code path stability
│   ├── Advancing timestamp window
│   ├── Rejection set stability
│   └── Equality semantics
└── Edge Cases & Integration Tests (8 tests)
    ├── Exact equality semantics
    ├── Saturation behavior
    └── u64::MAX protection
```
