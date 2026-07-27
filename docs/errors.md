# Error Handling — Credence Contracts

## Overview

All Credence smart contracts share a single error type: `ContractError`, defined in the
`credence_errors` crate (`contracts/credence_errors`). Every public entry-point returns
`Result<T, ContractError>` so callers always receive a typed, categorised, wire-stable
error code instead of an opaque transaction failure.

---

## Error Code Layout

| Range   | Category       | Primary Contracts                                 |
|---------|----------------|---------------------------------------------------|
| 1–99    | Initialization | all (bond, registry, delegation, treasury, etc.)  |
| 100–199 | Authorization  | all (bond, registry, delegation, treasury, etc.)  |
| 200–299 | Bond           | credence\_bond                                    |
| 300–399 | Attestation    | credence\_bond, credence\_delegation              |
| 400–499 | Registry       | credence\_registry                                |
| 500–599 | Delegation     | credence\_delegation                              |
| 600–699 | Treasury       | credence\_treasury                                |
| 700–799 | Arithmetic     | bond, treasury, and others                        |

> **Stability Guarantee** — Error codes are wire-stable and must **never** be renumbered
> after deployment. Append new variants at the end of their category block only.
>
> See [`docs/error-codes-wire.md`](error-codes-wire.md) for the official bump procedure
> and the wire-format stability test in
> `contracts/credence_errors/tests/error_codes_wire.rs`.

---

## Canonical Error Reference

### Initialization (1–99)

| Code | Variant | Recoverable | Description |
|------|---------|:-----------:|-------------|
| 1 | `NotInitialized` | ✓ | Contract has not been initialized yet |
| 2 | `AlreadyInitialized` | ✓ | Contract has already been initialized (re-init rejected) |

### Authorization (100–199)

| Code | Variant | Recoverable | Description |
|------|---------|:-----------:|-------------|
| 100 | `NotAdmin` | ✓ | Caller is not the contract admin |
| 101 | `NotBondOwner` | ✓ | Caller is not the bond owner |
| 102 | `UnauthorizedAttester` | ✓ | Caller is not an authorized attester |
| 103 | `NotOriginalAttester` | ✓ | Only the original attester can revoke |
| 104 | `NotSigner` | ✓ | Caller is not a registered multi-sig signer |
| 105 | `UnauthorizedDepositor` | ✓ | Caller is neither admin nor authorized depositor |
| 106 | `ContractPaused` | ✓ | Contract is paused; state-mutating operations disallowed |
| 107 | `InvalidPauseAction` | ✓ | Pause action value is invalid |
| 108 | `InsufficientSignatures` | ✓ | Not enough approvals to execute proposal |
| 109 | `ZeroBytes32` | ✓ | Input `BytesN<32>` argument is all-zero |
| 110 | `InvalidAdminAddress` | ✓ | Proposed admin is the zero/identity address |
| 111 | `AdminUnchanged` | ✓ | Proposed admin is the same as the current admin |
| 112 | `TimelockNotReady` | ✓ | Timelock delay has not yet elapsed |
| 113 | `AdminSuspended` | ✓ | Target admin is currently suspended |
| 114 | `BorrowFrozen` | ✓ | New bond creation and top-ups are frozen |
| 115 | `NoPendingAdmin` | ✓ | No pending admin transfer exists |
| 116 | `RoleNotHeldAtLedger` | ✓ | Actor did not hold the required role at the specified ledger |
| 117 | `EmergencyDrainNotPermitted` | ✓ | Emergency drain requires contract paused and timelock elapsed |
| 118 | `TimestampInFuture` | ✓ | Caller-supplied timestamp is ahead of the current ledger |
| 119 | `InvalidMaxPauseSigners` | ✓ | Max-pause-signers value is zero or exceeds the hard cap |
| 120 | `OutsideBusinessHours` | ✓ | Operation not permitted outside UTC business hours (Mon–Fri 09:00–17:00) |
| 121 | `LeaseScopeMismatch` | ✓ | Lease scope bitmask does not cover the requested operation |
| 122 | `LeaseExpired` | ✓ | Lease `expires_at` has been reached or passed |
| 123 | `CrossContractCallerMismatch` | ✗ | Cross-contract caller does not match the configured partner |
| 124 | `MigrationInProgress` | ✓ | State migration in progress; retry after it completes |
| 125 | `MaxPauseSignersExceeded` | ✓ | Adding a pause signer would exceed the configured cap |

### Bond (200–299)

| Code | Variant | Recoverable | Description |
|------|---------|:-----------:|-------------|
| 200 | `BondNotFound` | ✓ | No bond exists for the given address/key |
| 201 | `BondNotActive` | ✓ | Bond is not in an active state |
| 202 | `InsufficientBalance` | ✓ | Caller balance is insufficient for withdrawal |
| 203 | `SlashExceedsBond` | ✓ | Slash amount exceeds the total bonded amount |
| 204 | `LockupNotExpired` | ✓ | Lock-up period has not yet expired |
| 205 | `NotRollingBond` | ✓ | Operation requires a rolling bond; this bond is not rolling |
| 206 | `WithdrawalAlreadyRequested` | ✓ | A withdrawal has already been requested for this bond |
| 207 | `ReentrancyDetected` | ✗ | Reentrancy guard triggered — security halt |
| 208 | `InvalidNonce` | ✓ | Nonce is replayed or out of order |
| 209 | `NegativeStake` | ✓ | Attester stake would go negative |
| 210 | `EarlyExitConfigNotSet` | ✓ | Early-exit configuration has not been set for this bond |
| 211 | `InvalidPenaltyBps` | ✓ | Penalty basis-points out of range 0–10 000 |
| 212 | `LeverageExceeded` | ✓ | Resulting leverage exceeds the configured maximum |
| 213 | `UnsupportedToken` | ✓ | Token transfer returned a different amount (fee-on-transfer tokens unsupported) |
| 214 | `InvalidBondAmount` | ✓ | Bond amount must be strictly positive |
| 215 | `AmountExplicitlyZero` | ✓ | Amount explicitly set to zero — use `Option` to distinguish from not-set |
| 216 | `InvalidBondDuration` | ✓ | Bond duration must be strictly positive |
| 217 | `InvalidNoticePeriod` | ✓ | Rolling-bond notice period must be > 0 and ≤ duration |
| 218 | `BondAlreadyExists` | ✓ | Bond already exists for this identity |
| 219 | `OwnerMismatch` | ✗ | Payload owner does not match expected caller |
| 220 | `TargetMismatch` | ✗ | Payload target does not match expected action |
| 221 | `ContractIdMismatch` | ✗ | Payload contract\_id does not match current contract |
| 222 | `SignatureExpired` | ✓ | Signature/operation deadline has passed |
| 223 | `TreasuryNotConfigured` | ✓ | Slash treasury address has not been configured |
| 224 | `StorageCapReached` | ✗ | Storage cap for attestations or slash history reached |
| 225 | `DomainMismatch` | ✗ | Payload domain tag does not match expected |
| 226 | `CursorOutOfRange` | ✓ | Pagination cursor is out of range (cursor ≥ registry slots) |
| 227 | `BatchTooLarge` | ✓ | Batch input exceeds the maximum allowed size |
| 228 | `EmptyBatch` | ✓ | Batch input is empty; at least one item required |
| 229 | `UnsupportedDecimals` | ✓ | Token decimals are outside the supported normalization range |
| 230 | `InvalidStringifiedBytes` | ✓ | Hex/base64 stringified bytes input is malformed or too long |
| 231 | `UnauthorizedToken` | ✓ | Token address is not in the accepted-tokens set |
| 232 | `DuplicateIdempotencyKey` | ✓ | Idempotency key has already been used for this operation |
| 233 | `InvariantViolation` | ✗ | Post-write self-check detected bond/attestation accounting drift |
| 234 | `InvalidCurrency` | ✓ | Empty or whitespace-only currency symbol |
| 235 | `SnapshotGenerationMismatch` | ✗ | Snapshot generation does not match the current epoch |

### Attestation (300–399)

| Code | Variant | Recoverable | Description |
|------|---------|:-----------:|-------------|
| 300 | `DuplicateAttestation` | ✓ | Attestation from this attester already exists |
| 301 | `AttestationNotFound` | ✓ | No attestation found for the given key |
| 302 | `AttestationAlreadyRevoked` | ✓ | Attestation has already been revoked |
| 303 | `InvalidAttestationWeight` | ✓ | Attestation weight must be positive |
| 304 | `AttestationWeightExceedsMax` | ✓ | Attestation weight exceeds the configured maximum |

### Registry (400–499)

| Code | Variant | Recoverable | Description |
|------|---------|:-----------:|-------------|
| 400 | `IdentityAlreadyRegistered` | ✓ | Identity has already been registered |
| 401 | `BondContractAlreadyRegistered` | ✓ | Bond contract address has already been registered |
| 402 | `IdentityNotRegistered` | ✓ | Identity is not registered |
| 403 | `BondContractNotRegistered` | ✓ | Bond contract is not registered |
| 404 | `AlreadyDeactivated` | ✓ | Record is already in the deactivated state |
| 405 | `AlreadyActive` | ✓ | Record is already in the active state |
| 406 | `InvalidContractAddress` | ✓ | Provided address is not a deployed contract |
| 407 | `ContractCodeVerificationFailed` | ✓ | WASM code hash verification failed during trustless registration |
| 408 | `UnsupportedInterface` | ✓ | Bond contract does not support the required interface |

### Delegation (500–599)

| Code | Variant | Recoverable | Description |
|------|---------|:-----------:|-------------|
| 500 | `ExpiryInPast` | ✓ | Delegation expiry timestamp must be in the future |
| 501 | `DelegationNotFound` | ✓ | No delegation record found for the given key |
| 502 | `AlreadyRevoked` | ✓ | Delegation has already been revoked |
| 503 | `DelegationExpiryTooLong` | ✓ | Delegation expiry exceeds maximum allowed lifetime |
| 504 | `UnknownScheme` | ✗ | Unknown or unsupported signature scheme tag |
| 505 | `VerifierAlreadyRegistered` | ✓ | Verifier already registered for this scheme tag |
| 506 | `VerifierNotRegistered` | ✓ | No verifier registered for this scheme tag |
| 507 | `VerificationFailed` | ✗ | Signature verification failed — cryptographic failure |
| 508 | `RevocationGraceExpired` | ✗ | Post-expiry revocation attempted outside the grace window |
| 509 | `DelegationNotExpired` | ✓ | Cleanup attempted on a delegation that is not expired yet |
| 510 | `PayloadTooOld` | ✓ | Signed payload ledger number is older than `MAX_PAYLOAD_AGE_LEDGERS` |
| 511 | `DelegationInactive` | ✗ | Delegation is not active (revoked or expired) |
| 512 | `PromiseNotKept` | ✗ | Off-chain promise hash does not match on-chain execution |
| 513 | `StaleEpoch` | ✗ | Governance epoch reference in proposal ID is stale |
| 514 | `StaleAdminEpoch` | ✗ | Admin pause proposal carries a stale epoch reference |
| 515 | `StaleSignerEpoch` | ✗ | Signer pause proposal carries a stale epoch reference |

### Treasury (600–699)

| Code | Variant | Recoverable | Description |
|------|---------|:-----------:|-------------|
| 600 | `AmountMustBePositive` | ✓ | Amount must be strictly positive |
| 601 | `ThresholdExceedsSigners` | ✓ | Threshold cannot exceed the current signer count |
| 602 | `InsufficientTreasuryBalance` | ✓ | Treasury balance is insufficient for withdrawal |
| 603 | `ProposalNotFound` | ✓ | Withdrawal proposal not found |
| 604 | `ProposalAlreadyExecuted` | ✓ | Proposal has already been executed |
| 605 | `InsufficientApprovals` | ✓ | Not enough approvals to execute proposal |
| 606 | `InvalidFlashLoanCallback` | ✗ | Flashloan callback returned an invalid magic value |
| 607 | `FlashLoanRepaymentFailed` | ✗ | Flashloan principal plus fee was not fully repaid |
| 608 | `ProposalExpired` | ✓ | Withdrawal proposal has expired; create a new proposal |
| 609 | `SlippageExceeded` | ✓ | Settled amount fell below the caller's `min_amount_out` guard |
| 610 | `TreasuryBeneficiaryMismatch` | ✓ | Payment beneficiary does not match the configured treasury address |
| 611 | `CorridorNotRegistered` | ✓ | Settlement destination is not a registered corridor |

### Arithmetic (700–799)

| Code | Variant | Recoverable | Description |
|------|---------|:-----------:|-------------|
| 700 | `Overflow` | ✗ | Integer overflow in checked arithmetic |
| 701 | `Underflow` | ✗ | Integer underflow in checked arithmetic |
| 702 | `DivisionByZero` | ✗ | Division by a zero denominator |
| 703 | `InvalidPercentSplit` | ✓ | Percentage splits do not sum to exactly 10 000 basis points |

---

## Recoverability

The `ErrorExt::is_recoverable()` method classifies each error as:

- **Recoverable (`true`)** — the caller can fix their input or wait for state to change and
  retry. Examples: `NotAdmin` (switch signer), `LockupNotExpired` (wait), `InvalidNonce`
  (bump nonce).
- **Fatal (`false`)** — the same input will always fail. The fix is not in the caller's
  hands: code-level impossibility, security halt, cryptographic failure, or capacity limit.
  Examples: `Overflow`, `ReentrancyDetected`, `VerificationFailed`, `DomainMismatch`.

Off-chain clients, indexers and the admin CLI should use this signal to decide between
"retry/ignore" vs "alert/halt".

---

## Workspace Integration

### Adding `credence_errors` to `Cargo.toml`

```toml
[dependencies]
credence_errors = { path = "../../contracts/credence_errors" }
soroban-sdk = { version = "22.0" }

[dev-dependencies]
soroban-sdk = { version = "22.0", features = ["testutils"] }
```

### Importing in contract source

```rust
use credence_errors::{ContractError, ErrorExt};
use soroban_sdk::panic_with_error;

// Fallible entry-points:
pub fn some_fn(e: &Env, ...) -> Result<(), ContractError> {
    if bad_state {
        return Err(ContractError::BondNotFound);
    }
    Ok(())
}

// Panicking guards (e.g., constructor):
pub fn initialize(e: &Env, ...) {
    credence_errors::require_contract_uninitialized(e, storage::get_admin(e).is_some());
}
```

---

## Common Entry-Point Error Matrix

| Entry-point | Possible `ContractError` codes |
|-------------|-------------------------------|
| `create_bond()` | 2, 106, 114, 218, 214, 216, 217, 231, 234, 700 |
| `top_up()` | 2, 106, 200, 202, 700 |
| `withdraw()` | 106, 200, 201, 202, 204 |
| `withdraw_early()` | 106, 200, 201, 202, 210 |
| `request_withdrawal()` | 106, 200, 205, 206 |
| `slash()` | 100, 200, 203, 700 |
| `attest()` | 100, 102, 200, 300, 303, 304 |
| `revoke_attestation()` | 103, 301, 302 |
| `register_identity()` | 100, 106, 400 |
| `delegate()` | 500, 503 |
| `revoke_delegation()` | 501, 502, 508 |
| `execute_delegated_action()` | 507, 510, 511, 512, 513 |

This table is a convenience reference; the authoritative source is `contracts/credence_errors/src/lib.rs`.

---

## Best Practices

1. **Use `?` operator** for fallible entry-points returning `Result<T, ContractError>`.
2. **Use `panic_with_error!`** for initialization and configuration guards.
3. **Never panic with ad-hoc strings** — always map to a canonical error code.
4. **Test both happy path and failure** — at least one test per error code path.
5. **Preserve error codes** — never renumber or remove variants after deployment.
6. **Consult `is_recoverable()`** in off-chain monitoring to distinguish alert vs retry.

---

## FAQ

**Q: How do tests assert a specific error code?**  
Use `assert_eq!` on the `Result::Err` arm, or `#[should_panic(expected = "Error(Contract, #NNN)")]`:

```rust
#[test]
#[should_panic(expected = "Error(Contract, #200)")]
fn bond_not_found_panics() {
    let env = Env::default();
    // ... call contract method that triggers BondNotFound
}
```

**Q: Can I add new codes?**  
Yes — append only, within the correct category range. Run `cargo test -p credence_errors` afterwards to confirm `ALL_VARIANTS_COUNT` and the exhaustive match arms are updated.

**Q: What if I need a new category?**  
Allocate a new range (e.g. 800–899) and document it here, in `error-codes-wire.md`, and in `contracts/credence_errors/tests/discriminant_uniqueness.rs::RANGES`.
