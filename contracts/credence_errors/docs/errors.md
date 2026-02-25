# Error Handling — Credence Contracts

## Overview

All Credence smart contracts share a single error type: `ContractError`, defined
in the `credence_errors` crate. Every public entry-point returns
`Result<T, ContractError>` so callers always receive a typed, categorised,
wire-stable error code instead of an opaque transaction failure.

---

## Error Code Layout

| Range    | Category       | Contracts affected                          |
|----------|----------------|---------------------------------------------|
| 1-99     | Initialization | bond, registry, delegation, treasury        |
| 100-199  | Authorization  | bond, registry, delegation, treasury        |
| 200-299  | Bond           | credence_bond                               |
| 300-399  | Attestation    | credence_bond                               |
| 400-499  | Registry       | credence_registry                           |
| 500-599  | Delegation     | credence_delegation                         |
| 600-699  | Treasury       | credence_treasury                           |
| 700-799  | Arithmetic     | bond, treasury                              |

> **Stability guarantee** — codes must never be renumbered after deployment.
> Append new variants at the end of their category block only.

---

## Full Error Reference

### Initialization (1-99)

| Code | Variant | Replaces panic | Description |
|------|---------|----------------|-------------|
| 1 | `NotInitialized` | `"not initialized"` | Contract has not been initialized |
| 2 | `AlreadyInitialized` | `"already initialized"` | Contract has already been initialized |

### Authorization (100-199)

| Code | Variant | Replaces panic | Description |
|------|---------|----------------|-------------|
| 100 | `NotAdmin` | `"not admin"` | Caller is not the admin |
| 101 | `NotBondOwner` | `"not bond owner"` | Caller is not the bond owner |
| 102 | `UnauthorizedAttester` | `"unauthorized attester"` | Caller is not an authorized attester |
| 103 | `NotOriginalAttester` | `"only original attester can revoke"` | Only the original attester can revoke |
| 104 | `NotSigner` | `"only signer can propose/approve"` | Caller is not a registered multi-sig signer |
| 105 | `UnauthorizedDepositor` | `"only admin or authorized depositor"` | Caller is neither admin nor depositor |

### Bond (200-299)

| Code | Variant | Replaces panic | Description |
|------|---------|----------------|-------------|
| 200 | `BondNotFound` | `"no bond"` | No bond found for the given key |
| 201 | `BondNotActive` | `"bond not active"` | Bond is not in an active state |
| 202 | `InsufficientBalance` | `"insufficient balance for withdrawal"` | Caller balance is insufficient |
| 203 | `SlashExceedsBond` | `"slashed amount exceeds bonded amount"` | Slash amount exceeds the bond |
| 204 | `LockupNotExpired` | `"use withdraw for post lock-up"` | Lock-up period has not expired |
| 205 | `NotRollingBond` | `"not a rolling bond"` | Bond is not configured as rolling |
| 206 | `WithdrawalAlreadyRequested` | `"withdrawal already requested"` | Withdrawal already pending |
| 207 | `ReentrancyDetected` | `"reentrancy detected"` | Reentrancy guard triggered |
| 208 | `InvalidNonce` | `"invalid nonce: replay or out-of-order"` | Nonce is replayed or out of order |
| 209 | `NegativeStake` | `"attester stake cannot be negative"` | Stake would go negative |
| 210 | `EarlyExitConfigNotSet` | `"early exit config not set"` | Early-exit config missing |
| 211 | `InvalidPenaltyBps` | `"penalty_bps must be <= 10000"` | Penalty bps out of range |

### Attestation (300-399)

| Code | Variant | Replaces panic | Description |
|------|---------|----------------|-------------|
| 300 | `DuplicateAttestation` | `"duplicate attestation"` | Attestation already exists from this attester |
| 301 | `AttestationNotFound` | `"attestation not found"` | No attestation found |
| 302 | `AttestationAlreadyRevoked` | `"attestation already revoked"` | Attestation already revoked |
| 303 | `InvalidAttestationWeight` | `"attestation weight must be positive"` | Weight must be > 0 |
| 304 | `AttestationWeightExceedsMax` | `"attestation weight exceeds maximum"` | Weight above configured max |

### Registry (400-499)

| Code | Variant | Replaces panic | Description |
|------|---------|----------------|-------------|
| 400 | `IdentityAlreadyRegistered` | `"identity already registered"` | Identity exists in registry |
| 401 | `BondContractAlreadyRegistered` | `"bond contract already registered"` | Bond contract already in registry |
| 402 | `IdentityNotRegistered` | `"identity not registered"` | Identity missing from registry |
| 403 | `BondContractNotRegistered` | `"bond contract not registered"` | Bond contract missing from registry |
| 404 | `AlreadyDeactivated` | `"already deactivated"` | Record already deactivated |
| 405 | `AlreadyActive` | `"already active"` | Record already active |

### Delegation (500-599)

| Code | Variant | Replaces panic | Description |
|------|---------|----------------|-------------|
| 500 | `ExpiryInPast` | `"expiry must be in the future"` | Delegation expiry is in the past |
| 501 | `DelegationNotFound` | `"delegation not found"` | No delegation record found |
| 502 | `AlreadyRevoked` | `"already revoked"` | Delegation already revoked |

### Treasury (600-699)

| Code | Variant | Replaces panic | Description |
|------|---------|----------------|-------------|
| 600 | `AmountMustBePositive` | `"amount must be positive"` | Amount must be > 0 |
| 601 | `ThresholdExceedsSigners` | `"threshold cannot exceed signer count"` | Threshold > signer count |
| 602 | `InsufficientTreasuryBalance` | `"insufficient treasury balance"` | Balance too low for withdrawal |
| 603 | `ProposalNotFound` | `"proposal not found"` | Withdrawal proposal not found |
| 604 | `ProposalAlreadyExecuted` | `"proposal already executed"` | Proposal already executed |
| 605 | `InsufficientApprovals` | `"insufficient approvals to execute"` | Not enough approvals yet |

### Arithmetic (700-799)

| Code | Variant | Replaces | Description |
|------|---------|----------|-------------|
| 700 | `Overflow` | `.expect("overflow")` | Integer overflow in checked arithmetic |
| 701 | `Underflow` | `.expect("underflow")` | Integer underflow in checked arithmetic |

---

## Usage in Contracts

### Before (panic-based)
```rust
e.storage()
    .instance()
    .get(&DataKey::Bond(owner.clone()))
    .unwrap_or_else(|| panic!("no bond"))
```

### After (typed errors)
```rust
use credence_errors::ContractError;

e.storage()
    .instance()
    .get(&DataKey::Bond(owner.clone()))
    .ok_or(ContractError::BondNotFound)?
```

---

## Workspace Integration

Add to each contract's `Cargo.toml`:
```toml
[dependencies]
soroban-sdk = { version = "22.0" }
credence_errors = { path = "../../contracts/credence_errors" }
```

---

## Testing
```sh
cargo test -p credence_errors
```

Target: >= 95% line coverage on `src/lib.rs`.

The test suite covers:
- Wire code stability for all 42 variants
- Category mapping for every variant
- Unique non-empty descriptions
- Copy and Eq semantics
- Result integration tests mirroring every real contract call site
