# Error Handling — Credence Contracts

## Overview

All Credence smart contracts share a single error type: `ContractError`, defined in the `credence_errors` crate. Every public entry-point returns `Result<T, ContractError>` so callers always receive a typed, categorized, wire-stable error code instead of an opaque transaction failure.

---

## Error Code Layout

| Range    | Category       | Contracts                                      |
|----------|----------------|------------------------------------------------|
| 1-99     | Initialization | all (bond, registry, delegation, treasury, etc.) |
| 100-199  | Authorization  | all (bond, registry, delegation, treasury, etc.) |
| 200-299  | Bond           | credence_bond                                  |
| 300-399  | Attestation    | credence_bond, delegation                      |
| 400-499  | Registry       | credence_registry                              |
| 500-599  | Delegation     | credence_delegation                            |
| 600-699  | Treasury       | credence_treasury                              |
| 700-799  | Arithmetic     | bond, treasury, and others                     |

> **Stability Guarantee** — Error codes are wire-stable and must never be renumbered after deployment. New variants are appended at the end of their category block only.
>
> See `docs/error-codes-wire.md` for the official bump procedure and wire-format stability test.

---

## Canonical Error Reference

### Initialization (1-99)

| Code | Variant | Description |
|------|---------|-------------|
| 1 | `NotInitialized` | Contract has not been initialized |
| 2 | `AlreadyInitialized` | Contract has already been initialized (re-initialization attempted) |

### Authorization (100-199)

| Code | Variant | Description |
|------|---------|-------------|
| 100 | `NotAdmin` | Caller is not the contract admin |
| 101 | `NotBondOwner` | Caller is not the bond owner |
| 102 | `UnauthorizedAttester` | Caller is not an authorized attester |
| 103 | `NotOriginalAttester` | Only original attester can perform this action |
| 104 | `NotSigner` | Caller is not a registered multi-sig signer |
| 105 | `UnauthorizedDepositor` | Caller is neither admin nor authorized depositor |
| 106 | `ContractPaused` | Contract is paused; state-mutating operations disallowed |
| 107 | `InvalidPauseAction` | Pause action value is invalid |
| 108 | `InsufficientSignatures` | Not enough approvals/signatures to execute proposal (multisig) |

### Bond (200-299)

| Code | Variant | Description |
|------|---------|-------------|
| 200 | `BondNotFound` | No bond found for the given key |
| 201 | `BondNotActive` | Bond is not in an active state |
| 202 | `InsufficientBalance` | Caller balance is insufficient for withdrawal |
| 203 | `SlashExceedsBond` | Slash amount exceeds total bonded amount |
| 204 | `LockupNotExpired` | Lock-up period has not yet expired |
| 205 | `NotRollingBond` | Operation requires rolling bond but this is not rolling |
| 206 | `WithdrawalAlreadyRequested` | Withdrawal already requested for this bond |
| 207 | `ReentrancyDetected` | Reentrancy guard was triggered |
| 208 | `InvalidNonce` | Nonce is replayed or out of order |
| 209 | `NegativeStake` | Attester stake would go negative |
| 210 | `EarlyExitConfigNotSet` | Early-exit configuration has not been set |
| 211 | `InvalidPenaltyBps` | Penalty basis-points out of range (0-10000) |
| 212 | `LeverageExceeded` | Resulting leverage exceeds configured maximum |
| 213 | `UnsupportedToken` | Token transfer resulted in different amount (fee-on-transfer) |

#### Bond variants — common emitters

Below are common contract entry-points and the Bond-related `ContractError` variants they may emit. Use `try_*` client calls in tests to assert these variants.

- `get_identity_state()` — `ContractError::BondNotFound (200)`
- `withdraw()` / `withdraw_early()` — `ContractError::BondNotFound (200)`, `ContractError::InsufficientBalance (202)`, `ContractError::SlashExceedsBond (203)`, `ContractError::Underflow (701)`
- `withdraw_early()` — additionally: `ContractError::LockupNotExpired (204)`
- `request_withdrawal()` — `ContractError::BondNotFound (200)`, `ContractError::NotRollingBond (205)`, `ContractError::WithdrawalAlreadyRequested (206)`
- `renew_if_rolling()` — `ContractError::BondNotFound (200)`
- `slash()` / `slash_bond()` — `ContractError::BondNotFound (200)`, `ContractError::Overflow (700)`, `ContractError::SlashExceedsBond (203)`
- `top_up()` / `extend_duration()` — `ContractError::BondNotFound (200)`, `ContractError::Overflow (700)`
- `withdraw_bond()` — `ContractError::BondNotFound (200)`, `ContractError::NotBondOwner (101)`, `ContractError::BondNotActive (201)`, `ContractError::ReentrancyDetected (207)`

This mapping is a convenience reference for authors and testers; the authoritative source is `contracts/credence_errors/src/lib.rs`.

### Attestation (300-399)

| Code | Variant | Description |
|------|---------|-------------|
| 300 | `DuplicateAttestation` | Attestation from this attester already exists |
| 301 | `AttestationNotFound` | No attestation found for the given key |
| 302 | `AttestationAlreadyRevoked` | Attestation has already been revoked |
| 303 | `InvalidAttestationWeight` | Attestation weight must be positive |
| 304 | `AttestationWeightExceedsMax` | Attestation weight exceeds configured maximum |

### Registry (400-499)

| Code | Variant | Description |
|------|---------|-------------|
| 400 | `IdentityAlreadyRegistered` | Identity already exists in registry |
| 401 | `BondContractAlreadyRegistered` | Bond contract already registered |
| 402 | `IdentityNotRegistered` | Identity not registered in registry |
| 403 | `BondContractNotRegistered` | Bond contract not registered |
| 404 | `AlreadyDeactivated` | Record already in deactivated state |
| 405 | `AlreadyActive` | Record already in active state |
| 406 | `InvalidContractAddress` | Provided contract address is not deployed |

### Delegation (500-599)

| Code | Variant | Description |
|------|---------|-------------|
| 500 | `ExpiryInPast` | Delegation expiry timestamp is in the past |
| 501 | `DelegationNotFound` | No delegation record found |
| 502 | `AlreadyRevoked` | Delegation already revoked |
| 503 | `DelegationExpiryTooLong` | Delegation expiry exceeds maximum allowed lifetime |

### Treasury (600-699)

| Code | Variant | Description |
|------|---------|-------------|
| 600 | `AmountMustBePositive` | Amount must be > 0 |
| 601 | `ThresholdExceedsSigners` | Threshold cannot exceed signer count |
| 602 | `InsufficientTreasuryBalance` | Treasury balance insufficient for withdrawal |
| 603 | `ProposalNotFound` | Withdrawal proposal not found |
| 604 | `ProposalAlreadyExecuted` | Proposal already executed |
| 605 | `InsufficientApprovals` | Not enough approvals to execute proposal |
| 606 | `InvalidFlashLoanCallback` | Flashloan callback returned invalid magic value |
| 607 | `FlashLoanRepaymentFailed` | Flashloan principal plus fee was not fully repaid |

### Arithmetic (700-799)

| Code | Variant | Description |
|------|---------|-------------|
| 700 | `Overflow` | Integer overflow in checked arithmetic |
| 701 | `Underflow` | Integer underflow in checked arithmetic |

---

---

## Workspace Integration

### Adding credence_errors to Cargo.toml
```toml
[dependencies]
soroban-sdk = { version = "22.0", features = ["testutils"] }
credence_errors = { path = "../../contracts/credence_errors" }

[dev-dependencies]
soroban-sdk = { version = "22.0", features = ["testutils"] }
```

### Importing errors in contracts
```rust
use credence_errors::ContractError;
use soroban_sdk::panic_with_error;

// In fallible functions:
pub fn some_function(...) -> Result<..., ContractError> {
    ...
    Err(ContractError::SomeError)?
}

// In panicking functions (for initialization, config):
pub fn initialize(...) -> Result<..., ContractError> {
    if already_initialized {
        panic_with_error!(e, ContractError::AlreadyInitialized);
    }
    ...
    Ok(())
}
```

---

## Best Practices

1. **Use `?` operator** for fallible entry-points that return `Result<T, ContractError>`.
2. **Use `panic_with_error!`** for initialization and configuration that cannot fail gracefully.
3. **Never panic with ad-hoc strings** — always map to canonical error codes.
4. **Document error cases** in function docs, referencing the error code.
5. **Test both success and failure** — include at least one test per error code path.
6. **Preserve error codes** — never renumber or remove variants from the canonical taxonomy.

---

## FAQ

**Q: What if I need a custom error message?**  
A: Use the error code to identify the failure type. Off-chain indexers and clients decode the error code; custom messages can be reconstructed from the code if needed.

**Q: Can I add new error codes?**  
A: Yes, but only by appending to the end of the applicable category block. Never insert in the middle or renumber existing codes.

**Q: What about panics from dependencies?**  
A: Map library panics to canonical errors at contract boundaries. For example, overflow from checked_mul should return `Overflow` (code 700).

**Q: How do tests validate error codes?**  
A: Use `try_*` client methods with `matches!()` or `#[should_panic(expected = "Error(Contract, #NNN)")]` annotations.
