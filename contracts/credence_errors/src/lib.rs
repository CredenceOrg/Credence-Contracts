#![no_std]
#![deny(clippy::float_arithmetic)]
#![allow(
    deprecated,
    unused_imports,
    unused_variables,
    dead_code,
    unused_assignments,
    unused_mut,
    mismatched_lifetime_syntaxes,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::restriction
)]
// Must come AFTER `#![allow(clippy::restriction, ...)]` above: the
// `clippy::disallowed_macros` lint belongs to the `restriction` group, so
// a later allow would re-silence it. cargo build --release / WASM build
// is the only mode where this deny fires (tests stay free to
// use format!/write! for diagnostics).
#![cfg_attr(not(test), deny(clippy::disallowed_macros))]

use soroban_sdk::{contracterror, contracttype, panic_with_error, Address, Env};
/// Project-wide version constant.
pub const VERSION: &str = "0.1.0";

pub mod macros;

pub mod lease;
pub use lease::{lease_op, require_matching_lease_scope, require_no_expired_lease, Lease};

/// Panic with `AlreadyInitialized` if the contract has already been initialized.
///
/// # Usage
/// ```ignore
/// credence_errors::require_contract_uninitialized(&e, storage::get_admin(&e).is_some());
/// ```
pub fn require_contract_uninitialized(e: &Env, already_initialized: bool) {
    if already_initialized {
        e.panic_with_error(ContractError::AlreadyInitialized);
    }
}
/// Panic with  if .
/// Defence-in-depth guard that rejects treasury-flow payments to any
/// recipient other than the configured treasury.
pub fn require_matching_treasury_beneficiary(e: &Env, recipient: &Address, treasury: &Address) {
    if recipient != treasury {
        e.panic_with_error(ContractError::TreasuryBeneficiaryMismatch);
    }
}


/// Simple role enum for admin checks.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    User,
}

/// @title  ErrorCategory
/// @notice Groups errors by domain for monitoring, alerting, and dashboards.
/// @dev    Off-chain consumers should switch on this value first, then on the
///         specific `ContractError` code for fine-grained handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Contract setup and initialization errors (codes 1-99).
    Initialization,
    /// Caller identity and permission errors (codes 100-199).
    Authorization,
    /// Bond lifecycle errors (codes 200-299).
    Bond,
    /// Attestation errors (codes 300-399).
    Attestation,
    /// Registry identity/contract errors (codes 400-499).
    Registry,
    /// Delegation errors (codes 500-599).
    Delegation,
    /// Treasury proposal and balance errors (codes 600-699).
    Treasury,
    /// Safe-math errors (codes 700-799).
    Arithmetic,
}

/// @title  ContractError
/// @notice Canonical error enum shared by all Credence smart contracts.
/// @dev    Codes are wire-stable. Never renumber a variant after deployment.
///         Append new variants at the end of their category block only.
///         Use the ErrorExt trait to retrieve the category and description.
///
/// Error Code Layout:
///   1  -  99  : Initialization
///   100 - 199 : Authorization
///   200 - 299 : Bond
///   300 - 399 : Attestation
///   400 - 499 : Registry
///   500 - 599 : Delegation
///   600 - 699 : Treasury
///   700 - 799 : Arithmetic
// Keep conversions generated, but do not export this utility enum as contract
// spec metadata. The shared enum has more variants than Soroban's current
// exported error-enum case vector limit supports, and this crate is not a
// deployed contract interface.
#[contracterror(export = false)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ContractError {
    // --- Initialization (1-99) ---
    /// Contract has not been initialized yet.
    /// Replaces: panic!("not initialized")
    /// Contracts: bond, registry, delegation, treasury
    /// Wire-stable: do not renumber this error code.
    NotInitialized = 1,

    /// Contract has already been initialized and cannot be re-initialized.
    /// Replaces: panic!("already initialized")
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    AlreadyInitialized = 2,

    // --- Authorization (100-199) ---
    /// Caller is not the admin.
    /// Replaces: panic!("not admin")
    /// Contracts: bond, registry, delegation
    /// Wire-stable: do not renumber this error code.
    NotAdmin = 100,

    /// Caller is not the bond owner.
    /// Replaces: panic!("not bond owner")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    NotBondOwner = 101,

    /// Caller is not an authorized attester for this bond.
    /// Replaces: panic!("unauthorized attester")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    UnauthorizedAttester = 102,

    /// Caller is not the original attester who created the attestation.
    /// Replaces: panic!("only original attester can revoke")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    NotOriginalAttester = 103,

    /// Caller is not a registered multi-sig signer.
    /// Replaces: panic!("only signer can propose withdrawal")
    ///           panic!("only signer can approve")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    NotSigner = 104,

    /// Caller is neither the admin nor an authorized depositor.
    /// Replaces: panic!("only admin or authorized depositor can receive_fee")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    UnauthorizedDepositor = 105,

    /// Contract is currently paused and does not allow state mutations.
    /// Replaces: panic!("contract is paused")
    /// Contracts: bond, registry, treasury
    /// Wire-stable: do not renumber this error code.
    ContractPaused = 106,

    /// A storage migration is currently in progress; state mutations are
    /// rejected until it completes.
    /// Raised by `require_no_ongoing_migration`.
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    MigrationInProgress = 124,

    /// Borrows are currently frozen; new bond creation and top-ups are not allowed.
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    BorrowFrozen = 114,

    /// Actor did not hold the required role at the given ledger timestamp.
    ///
    /// Raised by `require_role_at_ledger` when the actor's `assigned_at`
    /// timestamp is later than the ledger timestamp under inspection, meaning
    /// the role was not yet granted at the time of the delegated action.
    /// Contracts: admin
    /// Wire-stable: do not renumber this error code.
    RoleNotHeldAtLedger = 116,

    /// Scheduled operation outside UTC business hours (Mon-Fri 09:00-17:00).
    /// Contracts: admin, timelock
    /// Wire-stable: do not renumber this error code.
    OutsideBusinessHours = 120,

    /// Pause proposal action value is invalid.
    /// Replaces: panic!("invalid pause action")
    /// Contracts: registry, treasury
    /// Wire-stable: do not renumber this error code.
    InvalidPauseAction = 107,

    /// Not enough approvals to execute the proposal.
    /// Replaces: panic!("insufficient signatures to execute"), panic!("insufficient approvals")
    /// Contracts: multisig, treasury
    /// Wire-stable: do not renumber this error code.
    InsufficientSignatures = 108,

    /// Signature deadline has expired (even with grace window).
    /// Replaces: panic!("signature expired")
    /// Contracts: bond
    SignatureExpired = 109,

    /// Idempotency key has already been used for this operation.
    /// Replaces: panic!("duplicate idempotency key")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    DuplicateIdempotencyKey = 117,
    /// The target admin is currently suspended (suspended_until > now).
    /// Contracts: admin
    /// Wire-stable: do not renumber this error code.
    AdminSuspended = 113,

    /// Input BytesN<32> argument is all-zero when a non-zero value is required.
    /// Replaces: panic!("zero bytes32")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    ZeroBytes32 = 109,

    /// Lease scope bitmask does not cover the requested operation.
    /// Raised by `require_matching_lease_scope` when `(lease.scope & op) != op`.
    /// Contracts: general-purpose (lease auth)
    /// Wire-stable: do not renumber this error code.
    LeaseScopeMismatch = 121,

    /// Lease `expires_at` has been reached or passed (hard cliff).
    /// Raised by `require_no_expired_lease` when `now >= lease.expires_at`.
    /// Contracts: general-purpose (lease auth)
    /// Wire-stable: do not renumber this error code.
    LeaseExpired = 122,

    /// Caller is not the required lease signer.
    /// Raised by `require_matching_lease_signer` when `lease.signer != caller`.
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    LeaseSignerMismatch = 126,

    // --- Bond (200-299) ---
    /// No bond exists for the given address or key.
    /// Replaces: panic!("no bond")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    BondNotFound = 200,

    /// Bond is not in the active state required for this operation.
    /// Replaces: panic!("bond not active")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    BondNotActive = 201,

    /// Caller balance is insufficient for the requested withdrawal.
    /// Replaces: panic!("insufficient balance for withdrawal")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InsufficientBalance = 202,

    /// The slash amount exceeds the bonded amount.
    /// Replaces: panic!("slashed amount exceeds bonded amount")
    ///           panic!("slash exceeds bond")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    SlashExceedsBond = 203,

    /// Snapshot generation mismatch between state and expectation.
    /// Triggered by: batch operations across epochs.
    /// Wire-stable: do not renumber this error code.
    SnapshotGenerationMismatch = 235,

    /// Storage cap for attestations or slash history reached.
    /// Replaces: panic!("storage cap reached")
    StorageCapReached = 224,

    /// Bond lock-up period has not yet expired.
    /// Replaces: panic!("use withdraw for post lock-up")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    LockupNotExpired = 204,

    /// Operation requires a rolling bond but this bond is not rolling.
    /// Replaces: panic!("not a rolling bond")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    NotRollingBond = 205,

    /// A withdrawal has already been requested for this bond.
    /// Replaces: panic!("withdrawal already requested")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    WithdrawalAlreadyRequested = 206,

    /// A cooldown withdrawal request is already pending for this bond.
    /// Replaces: panic!("cooldown request already pending")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    CooldownRequestAlreadyPending = 400,

    /// No cooldown withdrawal request exists for this bond.
    /// Replaces: panic!("no cooldown request")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    CooldownRequestNotFound = 401,

    /// The cooldown period has not yet elapsed.
    /// Replaces: panic!("cooldown period has not elapsed")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    CooldownPeriodNotElapsed = 402,

    /// Reentrancy was detected; the call is rejected.
    /// Replaces: panic!("reentrancy detected")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    ReentrancyDetected = 207,

    /// Signature or operation deadline has passed.
    /// Replaces: panic!("signature expired")
    /// Contracts: bond, delegation
    /// Wire-stable: do not renumber this error code.
    SignatureExpired = 222,

    /// Nonce is invalid - either replayed or out of order.
    /// Replaces: panic!("invalid nonce: replay or out-of-order")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidNonce = 208,

    /// Signature/operation deadline has passed (now > deadline + grace).
    /// Contracts: bond, delegation
    DeadlineExpired = 222,

    /// Attester stake would go negative, which is not permitted.
    /// Replaces: panic!("attester stake cannot be negative")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    NegativeStake = 209,

    /// Early-exit configuration has not been set for this bond.
    /// Replaces: panic!("early exit config not set")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    EarlyExitConfigNotSet = 210,

    /// Penalty basis-points value must be in the range 0-10000.
    /// Replaces: panic!("penalty_bps must be <= 10000")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidPenaltyBps = 211,

    /// Resulting leverage exceeds the configured maximum.
    /// Replaces: panic!("leverage exceeds maximum")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    LeverageExceeded = 212,

    /// Token transfer resulted in different amount than requested (fee-on-transfer tokens).
    /// Replaces: panic!("unsupported token: transfer amount mismatch")
    /// Contracts: bond, dispute_resolution, fixed_duration_bond
    /// Wire-stable: do not renumber this error code.
    UnsupportedToken = 213,

    /// Token decimals are outside the supported range used for normalization.
    /// Triggered by token ingress when a configured token reports unsupported decimals.
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    UnsupportedDecimals = 229,

    /// Bond amount must be strictly positive (> 0).
    /// Triggered by: create_bond called with amount <= 0
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidBondAmount = 214,

    /// Amount argument is explicitly set to zero, which is a bug.
    /// Distinguishes "not set" (None) from "explicitly zero" (Some(0)) via Option<i128>.
    /// Triggered by: require_no_leading_zero_amount with Some(0)
    /// Contracts: bond, treasury
    /// Wire-stable: do not renumber this error code.
    AmountExplicitlyZero = 215,

    /// Bond duration must be strictly positive (> 0).
    /// Triggered by: create_bond called with duration == 0
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidBondDuration = 216,

    /// Rolling-bond notice_period_duration must be > 0 and <= duration.
    /// Triggered by: create_bond called with is_rolling=true and notice_period_duration == 0
    ///               or notice_period_duration > duration
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidNoticePeriod = 217,

    /// Bond already exists for this identity.
    /// Triggered by: create_bond called for an identity that already has an active bond
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    BondAlreadyExists = 218,

    /// Hex/base64 stringified bytes input is malformed or exceeds the length bound.
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidStringifiedBytes = 230,

    /// Token address is not in the set of accepted tokens.
    /// Triggered by: initialize called with a token not in the accepted tokens set
    /// Contracts: bond
    UnauthorizedToken = 231,
    /// An idempotency key has already been used for this operation.
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    DuplicateIdempotencyKey = 232,
    /// Post-write invariant self-check detected bond or attestation accounting drift.
    /// Triggered by: `invariants::assert_self_consistent` after a bond-module write
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvariantViolation = 230,

    /// Slash treasury address has not been configured.
    /// Triggered by: `slash_bond` when `DataKey::SlashTreasury` is absent.
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    TreasuryNotConfigured = 223,

    /// Pagination cursor is out of range (cursor >= registry_slots).
    /// Triggered by: `scan_liquidation_candidates` when the supplied cursor
    /// equals or exceeds the current registry slot count. Accepting
    /// cursor == registry_slots would silently return a done=true result,
    /// allowing a malicious keeper to synthesize a completed-scan response
    /// without actually scanning any positions.
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    CursorOutOfRange = 226,

    /// Batch input exceeds the maximum allowed size constant.
    /// Prevents a single transaction from exhausting CPU/ledger budgets.
    /// Replaces: panic!("batch too large")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    BatchTooLarge = 227,

    /// Batch input is empty (len == 0) when at least one item is required.
    /// Replaces: panic!("empty batch")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    EmptyBatch = 228,

    // --- Attestation (300-399) ---
    /// An attestation already exists from this attester for this bond.
    /// Replaces: panic!("duplicate attestation")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    DuplicateAttestation = 300,

    /// No attestation was found for the given key.
    /// Replaces: panic!("attestation not found")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    AttestationNotFound = 301,

    /// Attestation has already been revoked.
    /// Replaces: panic!("attestation already revoked")
    /// Contracts: bond, delegation
    /// Wire-stable: do not renumber this error code.
    AttestationAlreadyRevoked = 302,

    /// Attestation weight must be a positive value.
    /// Replaces: panic!("attestation weight must be positive")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidAttestationWeight = 303,

    /// Attestation weight exceeds the configured maximum.
    /// Replaces: panic!("attestation weight exceeds maximum")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    AttestationWeightExceedsMax = 304,

    // --- Registry (400-499) ---
    /// Identity has already been registered in the registry.
    /// Replaces: panic!("identity already registered")
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    IdentityAlreadyRegistered = 400,

    /// Bond contract address has already been registered.
    /// Replaces: panic!("bond contract already registered")
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    BondContractAlreadyRegistered = 401,

    /// Identity is not registered in the registry.
    /// Replaces: panic!("identity not registered")
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    IdentityNotRegistered = 402,

    /// Bond contract is not registered in the registry.
    /// Replaces: panic!("bond contract not registered")
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    BondContractNotRegistered = 403,

    /// Identity or bond contract is already in the deactivated state.
    /// Replaces: panic!("already deactivated")
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    AlreadyDeactivated = 404,

    /// Identity or bond contract is already in the active state.
    /// Replaces: panic!("already active")
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    AlreadyActive = 405,

    /// Provided contract address is not a deployed contract.
    /// Replaces: panic!("invalid contract address")
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    InvalidContractAddress = 406,

    /// Contract code hash verification failed during trustless registration.
    /// The calling contract's WASM code hash does not match the expected bond code hash.
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    ContractCodeVerificationFailed = 407,

    /// Bond contract does not support required interface.
    /// Replaces: panic!("bond contract does not support required interface")
    /// Contracts: registry
    /// Wire-stable: do not renumber this error code.
    UnsupportedInterface = 408,
    // --- Delegation (500-599) ---
    /// Delegation expiry timestamp must be in the future.
    /// Replaces: panic!("expiry must be in the future")
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    ExpiryInPast = 500,

    /// No delegation record was found for the given key.
    /// Replaces: panic!("delegation not found")
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    DelegationNotFound = 501,

    /// Delegation has already been revoked.
    /// Replaces: panic!("already revoked")
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    AlreadyRevoked = 502,

    /// Delegation expiry timestamp exceeds the maximum allowed lifetime.
    /// Triggered by: expires_at > now + MAX_DELEGATION_DURATION
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    DelegationExpiryTooLong = 503,

    /// Delegation is not active (revoked or expired).
    /// Replaces: panic!("delegation inactive")
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    DelegationInactive = 511,
    // Note: DomainMismatch (225), OwnerMismatch (219), TargetMismatch (220),
    // ContractIdMismatch (221), and SignatureExpired (222) are shared Bond/Delegation
    // variants defined in the Bond section above.
    /// Unknown or unsupported signature scheme tag.
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    UnknownScheme = 504,

    /// Verifier already registered for the given scheme tag.
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    VerifierAlreadyRegistered = 505,

    /// No verifier registered for the given scheme tag.
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    VerifierNotRegistered = 506,

    /// Signature verification failed for the given scheme and payload.
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    VerificationFailed = 507,

    /// Post-expiry revocation attempted outside the configured grace window.
    /// Triggered when `revocation_grace_period > 0` and
    /// `now > expires_at + revocation_grace_period`.
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    RevocationGraceExpired = 508,

    /// Cleanup attempted on a delegation that is not expired yet.
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    DelegationNotExpired = 509,

    /// Signed operation payload is older than MAX_PAYLOAD_AGE_LEDGERS ledgers.
    ///
    /// An attacker who intercepts a signed `DelegatedActionPayload` and delays
    /// its submission can replay it arbitrarily far in the future as long as
    /// the nonce has not been consumed. `ledger_number` bounds the replay
    /// window to a short, forward-only interval so that a captured-but-unspent
    /// payload automatically expires on-chain.
    ///
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    PayloadTooOld = 510,

    /// Off-chain promise hash does not match on-chain execution.
    ///
    /// Raised by `require_kept_promise` when the hash of the off-chain signed
    /// payload (the "promise") does not match the hash of the actual on-chain
    /// execution parameters. This detects cases where a relayer or attacker
    /// submits a payload that differs from what the signer authorized.
    ///
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    PromiseNotKept = 512,

    /// Governance epoch reference in the proposal ID is stale.
    /// Contracts: delegation
    /// Wire-stable: do not renumber this error code.
    StaleEpoch = 513,

    /// Admin pause proposal ID was derived in a stale epoch.
    /// Contracts: admin
    /// Wire-stable: do not renumber this error code.
    StaleAdminEpoch = 514,

    /// Signer pause proposal ID was derived in a stale epoch.
    /// Contracts: multisig
    /// Wire-stable: do not renumber this error code.
    StaleSignerEpoch = 515,

    // --- Shared Bond/Delegation payload mismatch errors (218-221) ---
    // Wire-stable: codes documented in the note above; kept distinct from the
    // delegation scheme/verifier errors (504-507).
    DomainMismatch = 231,
    OwnerMismatch = 219,
    TargetMismatch = 220,
    ContractIdMismatch = 221,

    // --- Admin Transfer (115-119) ---
    /// No pending admin transfer exists.
    NoPendingAdmin = 114,

    /// Proposed admin is the zero/identity address.
    InvalidAdminAddress = 110,

    /// Proposed admin is the same as the current admin.
    AdminUnchanged = 111,

    /// Timelock delay has not yet elapsed.
    TimelockNotReady = 112,

    /// Emergency drain is not permitted: contract must be paused and timelock window must have elapsed.
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    EmergencyDrainNotPermitted = 115,

    /// Supplied timestamp or ledger number is ahead of the current ledger.
    ///
    /// Raised by `verify_no_future_ledger` when the caller-supplied
    /// timestamp exceeds the on-chain ledger timestamp, indicating the
    /// value could not have been produced by the network.
    ///
    /// Contracts: general-purpose
    /// Wire-stable: do not renumber this error code.
    TimestampInFuture = 118,

    /// Requested max-pause-signers value is zero or exceeds the hard cap.
    /// Contracts: multisig
    /// Wire-stable: do not renumber this error code.
    InvalidMaxPauseSigners = 119,

    /// Registering another pause signer would exceed the configured cap.
    /// Contracts: multisig
    /// Wire-stable: do not renumber this error code.
    MaxPauseSignersExceeded = 125,



    /// Cross-contract caller does not match the configured partner address.
    /// Contracts: general-purpose
    /// Wire-stable: do not renumber this error code.
    CrossContractCallerMismatch = 123,
    RoleNotHeldAtLedger = 116,

    // --- Treasury (600-699) ---
    /// Amount argument must be strictly positive (> 0).
    /// Replaces: panic!("amount must be positive")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    AmountMustBePositive = 600,

    /// Approval threshold cannot exceed the current number of signers.
    /// Replaces: panic!("threshold cannot exceed signer count")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    ThresholdExceedsSigners = 601,

    /// Treasury balance is insufficient for the requested withdrawal.
    /// Replaces: panic!("insufficient treasury balance")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    InsufficientTreasuryBalance = 602,

    /// Withdrawal proposal was not found for the given id.
    /// Replaces: panic!("proposal not found")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    ProposalNotFound = 603,

    /// Withdrawal proposal has already been executed.
    /// Replaces: panic!("proposal already executed")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    ProposalAlreadyExecuted = 604,

    /// Proposal does not yet have enough approvals to execute.
    /// Replaces: panic!("insufficient approvals to execute")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    InsufficientApprovals = 605,

    /// Flashloan callback returned an invalid magic value.
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    InvalidFlashLoanCallback = 606,

    /// Flashloan principal plus fee was not fully repaid.
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    FlashLoanRepaymentFailed = 607,

    /// Withdrawal proposal has expired and can no longer be approved or executed.
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    ProposalExpired = 608,

    /// Settled withdrawal amount fell below the caller's `min_amount_out`
    /// slippage bound. Distinct from `InsufficientTreasuryBalance`: the treasury
    /// had funds, but the realized amount tripped the caller's slippage guard.
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    SlippageExceeded = 609,

    /// Payment beneficiary does not match the expected treasury address.
    /// Defence-in-depth guard that rejects treasury-flow payments to any
    /// recipient other than the configured treasury. Without this check an
    /// attacker who can influence the `recipient` argument of a treasury-bound
    /// transfer (e.g. via a misconfigured proposal, a confused-deputy
    /// cross-contract call, or a bug that overwrites the stored treasury) can
    /// redirect protocol funds to an attacker-controlled address.
    /// Contracts: bond, treasury
    /// Wire-stable: do not renumber this error code.
    TreasuryBeneficiaryMismatch = 610,

    /// Settlement destination is not a registered corridor.
    /// Raised by `settle` when the caller-supplied destination address has
    /// not been added via `register_corridor`. Corridors are an explicit
    /// admin-managed allowlist, so settlement cannot be routed to an
    /// arbitrary, unvetted destination.
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    CorridorNotRegistered = 611,

    // --- Arithmetic (700-799) ---
    /// Integer overflow detected during a checked arithmetic operation.
    /// Replaces: .expect("... overflow")
    /// Contracts: bond, treasury
    /// Wire-stable: do not renumber this error code.
    Overflow = 700,

    /// Integer underflow detected during a checked arithmetic operation.
    /// Replaces: .expect("... underflow")
    /// Contracts: treasury
    /// Wire-stable: do not renumber this error code.
    Underflow = 701,

    /// Division (or remainder) by a zero denominator was attempted.
    /// Replaces: panic!("...") in the safe-math div/ceil_div helpers when `b == 0`.
    /// Contracts: math, bond
    /// Wire-stable: do not renumber this error code.
    DivisionByZero = 702,

    /// Percentage splits do not sum to exactly 10 000 bps.
    /// Contracts: math
    /// Wire-stable: do not renumber this error code.
    InvalidPercentSplit = 703,
}

/// @title  ErrorExt
/// @notice Provides category(), description(), and is_recoverable() on every
///         ContractError variant.
/// @dev    Use this for structured logging, monitoring, and off-chain display.
///
/// `is_recoverable()` classifies an error as recoverable when the
/// caller can fix their input or wait for state to change and retry
/// the same kind of operation successfully (e.g. `AlreadyInitialized`,
/// `LockupNotExpired`, `InsufficientSignatures`). It returns `false`
/// for **fatal** errors that indicate either a code-level fault
/// (`Overflow`, `Underflow`, `InvariantViolation`), a security halt
/// (`ReentrancyDetected`), a cryptographic failure
/// (`VerificationFailed`), or a payload binding mismatch
/// (`DomainMismatch`, `OwnerMismatch`, `TargetMismatch`,
/// `ContractIdMismatch`). Off-chain clients (indexers, admin CLI,
/// alerting) should use this signal to decide between
/// "retry/ignore" vs "alert/halt".
///
/// `is_recoverable()` is metadata only: it does not panic, does not
/// allocate, and does not touch storage. It does not change any
/// wire codes, categories, or description strings.
///
/// New `ContractError` variants must be added with an explicit
/// classification - the matching `impl` is exhaustive and the test
/// suite forces a decision for every variant (see `test_is_recoverable_exhaustive`).
pub trait ErrorExt {
    /// @return The ErrorCategory bucket this error belongs to.
    fn category(&self) -> ErrorCategory;

    /// @return A static string description safe for logging or display.
    fn description(&self) -> &'static str;

    /// @return `true` if a caller can fix their input or wait for state to
    ///         change and retry the same operation successfully;
    ///         `false` if the error indicates a code-level fault, security
    ///         halt, or payload-binding mismatch where blind retry will not
    ///         help.
    fn is_recoverable(&self) -> bool;
}

impl ErrorExt for ContractError {
    fn category(&self) -> ErrorCategory {
        match self {
            ContractError::NotInitialized | ContractError::AlreadyInitialized => {
                ErrorCategory::Initialization
            }
            ContractError::NotAdmin
            | ContractError::NotBondOwner
            | ContractError::UnauthorizedAttester
            | ContractError::NotOriginalAttester
            | ContractError::NotSigner
            | ContractError::UnauthorizedDepositor
            | ContractError::ContractPaused
            | ContractError::MigrationInProgress
            | ContractError::BorrowFrozen
            | ContractError::InvalidPauseAction
            | ContractError::InsufficientSignatures
            | ContractError::AdminSuspended
            | ContractError::RoleNotHeldAtLedger
            | ContractError::ZeroBytes32
            | ContractError::TimestampInFuture
            | ContractError::InvalidMaxPauseSigners
            | ContractError::MaxPauseSignersExceeded
            | ContractError::LeaseScopeMismatch
            | ContractError::LeaseExpired
            | ContractError::OutsideBusinessHours
            | ContractError::StaleAdminEpoch
            | ContractError::StaleSignerEpoch
            | ContractError::OutsideBusinessHours
            | ContractError::CrossContractCallerMismatch
            | ContractError::LeaseSignerMismatch => ErrorCategory::Authorization,
            | ContractError::DuplicateIdempotencyKey
            | ContractError::DeadlineExpired => ErrorCategory::Authorization,

            ContractError::BondNotFound
            | ContractError::BondNotActive
            | ContractError::InsufficientBalance
            | ContractError::SlashExceedsBond
            | ContractError::LockupNotExpired
            | ContractError::NotRollingBond
            | ContractError::WithdrawalAlreadyRequested
            | ContractError::CooldownRequestAlreadyPending
            | ContractError::CooldownRequestNotFound
            | ContractError::CooldownPeriodNotElapsed
            | ContractError::ReentrancyDetected
            | ContractError::InvalidNonce
            | ContractError::SignatureExpired
            | ContractError::DeadlineExpired
            | ContractError::NegativeStake
            | ContractError::EarlyExitConfigNotSet
            | ContractError::InvalidPenaltyBps
            | ContractError::LeverageExceeded
            | ContractError::UnsupportedToken
            | ContractError::UnsupportedDecimals
            | ContractError::InvalidBondAmount
            | ContractError::InvalidBondDuration
            | ContractError::InvalidNoticePeriod
            | ContractError::BondAlreadyExists
            | ContractError::UnauthorizedToken
            | ContractError::DuplicateIdempotencyKey
            | ContractError::InvariantViolation
            | ContractError::InvalidCurrency
            | ContractError::StorageCapReached
            | ContractError::TreasuryNotConfigured
            | ContractError::CursorOutOfRange
            | ContractError::BatchTooLarge
            | ContractError::EmptyBatch
            | ContractError::InvariantViolation
            | ContractError::UnauthorizedToken
            | ContractError::UnsupportedDecimals => ErrorCategory::Bond,

            ContractError::DuplicateAttestation
            | ContractError::AttestationNotFound
            | ContractError::AttestationAlreadyRevoked
            | ContractError::InvalidAttestationWeight
            | ContractError::AttestationWeightExceedsMax => ErrorCategory::Attestation,

            ContractError::IdentityAlreadyRegistered
            | ContractError::BondContractAlreadyRegistered
            | ContractError::IdentityNotRegistered
            | ContractError::BondContractNotRegistered
            | ContractError::AlreadyDeactivated
            | ContractError::AlreadyActive
            | ContractError::InvalidContractAddress
            | ContractError::ContractCodeVerificationFailed
            | ContractError::UnsupportedInterface => ErrorCategory::Registry,

            ContractError::ExpiryInPast
            | ContractError::DelegationNotFound
            | ContractError::AlreadyRevoked
            | ContractError::DelegationExpiryTooLong
            | ContractError::UnknownScheme
            | ContractError::VerifierAlreadyRegistered
            | ContractError::VerifierNotRegistered
            | ContractError::VerificationFailed
            | ContractError::RevocationGraceExpired
            | ContractError::DelegationNotExpired
            | ContractError::DelegationInactive
            | ContractError::PayloadTooOld
            | ContractError::PromiseNotKept
            | ContractError::StaleEpoch => ErrorCategory::Delegation,

            ContractError::AmountMustBePositive
            | ContractError::ThresholdExceedsSigners
            | ContractError::InsufficientTreasuryBalance
            | ContractError::ProposalNotFound
            | ContractError::ProposalAlreadyExecuted
            | ContractError::InsufficientApprovals
            | ContractError::InvalidFlashLoanCallback
            | ContractError::FlashLoanRepaymentFailed
            | ContractError::ProposalExpired
            | ContractError::SlippageExceeded
            | ContractError::TreasuryBeneficiaryMismatch
            | ContractError::CorridorNotRegistered => ErrorCategory::Treasury,

            ContractError::Overflow
            | ContractError::Underflow
            | ContractError::DivisionByZero
            | ContractError::InvalidPercentSplit => ErrorCategory::Arithmetic,
            ContractError::NoPendingAdmin
            | ContractError::InvalidAdminAddress
            | ContractError::AdminUnchanged
            | ContractError::TimelockNotReady
            | ContractError::EmergencyDrainNotPermitted
            | ContractError::DomainMismatch
            | ContractError::OwnerMismatch
            | ContractError::TargetMismatch
            | ContractError::ContractIdMismatch => ErrorCategory::Authorization,
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ContractError::NotInitialized => "Contract has not been initialized",
            ContractError::AlreadyInitialized => "Contract has already been initialized",
            ContractError::NotAdmin => "Caller is not the admin",
            ContractError::NotBondOwner => "Caller is not the bond owner",
            ContractError::UnauthorizedAttester => "Caller is not an authorized attester",
            ContractError::NotOriginalAttester => "Only the original attester can revoke",
            ContractError::NotSigner => "Caller is not a registered multi-sig signer",
            ContractError::UnauthorizedDepositor => {
                "Caller is neither admin nor an authorized depositor"
            }
            ContractError::ContractPaused => "Contract is paused",
            ContractError::MigrationInProgress => "Migration in progress",
            ContractError::LeaseSignerMismatch => "Lease signer must match calling actor",
            ContractError::OutsideBusinessHours => {
                "Scheduled operation falls outside UTC business hours (Mon-Fri 09:00-17:00)"
            }
            ContractError::BorrowFrozen => "Borrows are frozen: new bond creation and top-ups are not allowed",
            ContractError::InvalidPauseAction => "Pause proposal action is invalid",
            ContractError::InsufficientSignatures => "Not enough approvals to execute proposal",
            ContractError::AdminSuspended => "Admin is currently suspended",
            ContractError::DuplicateIdempotencyKey => "Idempotency key has already been used for this operation",
            ContractError::DeadlineExpired => "Signature/operation deadline has passed",
            ContractError::RoleNotHeldAtLedger => {
                "Actor did not hold the required role at the specified ledger timestamp"
            }
            ContractError::LeaseScopeMismatch => {
                "Lease scope does not cover the requested operation"
            }
            ContractError::LeaseExpired => "Lease has expired and can no longer authorise operations",
            ContractError::OutsideBusinessHours => "Operation attempted outside permitted business hours",
            ContractError::BondNotFound => "No bond exists for the supplied identity",
            ContractError::BondNotActive => "Bond is not in an active state",
            ContractError::InsufficientBalance => "Insufficient balance for withdrawal",
            ContractError::SlashExceedsBond => "Slash amount exceeds the bonded amount",
            ContractError::LockupNotExpired => "Lock-up period has not yet expired",
            ContractError::NotRollingBond => "Bond is not configured as a rolling bond",
            ContractError::WithdrawalAlreadyRequested => {
                "A withdrawal has already been requested for this bond"
            }
            ContractError::CooldownRequestAlreadyPending => {
                "A cooldown withdrawal request is already pending"
            }
            ContractError::CooldownRequestNotFound => {
                "No cooldown withdrawal request exists"
            }
            ContractError::CooldownPeriodNotElapsed => {
                "Cooldown period has not yet elapsed"
            }
            ContractError::ReentrancyDetected => "Reentrancy detected; call rejected",
            ContractError::InvalidNonce => "Nonce is replayed or out of order",
            ContractError::SignatureExpired => "Signature/operation deadline has passed",
            ContractError::NegativeStake => "Attester stake cannot be negative",
            ContractError::EarlyExitConfigNotSet => {
                "Early-exit configuration has not been set for this bond"
            }
            ContractError::InvalidPenaltyBps => "Penalty bps must be in range 0-10000",
            ContractError::LeverageExceeded => "Resulting leverage exceeds the configured maximum",
            ContractError::UnsupportedToken => "Token transfer resulted in different amount than requested (fee-on-transfer tokens not supported)",
            ContractError::UnsupportedDecimals => "Token decimals are outside the supported normalization range",
            ContractError::InvalidBondAmount => "Bond amount must be strictly positive (> 0)",
            ContractError::AmountExplicitlyZero => {
                "Amount argument is explicitly set to zero, which is a bug (use Option to distinguish not-set from zero)"
            }
            ContractError::InvalidBondDuration => "Bond duration must be strictly positive (> 0)",
            ContractError::InvalidNoticePeriod => "Rolling-bond notice_period_duration must be > 0 and <= duration",
            ContractError::BondAlreadyExists => "Bond already exists for this identity",
            ContractError::UnauthorizedToken => "Token address is not in the set of accepted tokens",
            ContractError::InvalidCurrency => "Empty or whitespace-only currency symbol",
            ContractError::InvalidStringifiedBytes => {
                "Hex/base64 stringified bytes input is malformed or too long"
            }
            ContractError::SnapshotGenerationMismatch => {
                "Snapshot generation does not match the current state"
            }
            ContractError::StorageCapReached => "Storage cap for attestations or slash history reached",
            ContractError::TreasuryNotConfigured => "Slash treasury address has not been configured",
            ContractError::CursorOutOfRange => "Pagination cursor is out of range (cursor >= registry_slots)",
            ContractError::BatchTooLarge => "Batch input exceeds the maximum allowed size",
            ContractError::EmptyBatch => "Batch input must contain at least one item",
            ContractError::DuplicateIdempotencyKey => "Idempotency key has already been used for this operation",
            ContractError::InvariantViolation => {
                "Bond storage drift detected; bonded/slashed or attestation counters inconsistent"
            }
            ContractError::DuplicateAttestation => "Attestation already exists from this attester",
            ContractError::AttestationNotFound => "No attestation found for the given key",
            ContractError::AttestationAlreadyRevoked => "Attestation has already been revoked",
            ContractError::InvalidAttestationWeight => "Attestation weight must be positive",
            ContractError::AttestationWeightExceedsMax => {
                "Attestation weight exceeds the configured maximum"
            }
            ContractError::IdentityAlreadyRegistered => {
                "Identity has already been registered in the registry"
            }
            ContractError::BondContractAlreadyRegistered => {
                "Bond contract address has already been registered"
            }
            ContractError::IdentityNotRegistered => "Identity is not registered in the registry",
            ContractError::BondContractNotRegistered => {
                "Bond contract is not registered in the registry"
            }
            ContractError::AlreadyDeactivated => "Record is already in the deactivated state",
            ContractError::AlreadyActive => "Record is already in the active state",
            ContractError::InvalidContractAddress => {
                "Provided contract address is not a deployed contract"
            }
            ContractError::ContractCodeVerificationFailed => {
                "Contract code hash verification failed during trustless registration"
            }
            ContractError::UnsupportedInterface => "Bond contract does not support required interface",
            ContractError::ExpiryInPast => "Delegation expiry must be in the future",
            ContractError::DelegationNotFound => "No delegation found for the given key",
            ContractError::AlreadyRevoked => "Delegation has already been revoked",
            ContractError::DelegationExpiryTooLong => {
                "Delegation expiry exceeds the maximum allowed lifetime"
            }
            ContractError::UnknownScheme => "Unknown or unsupported signature scheme tag",
            ContractError::VerifierAlreadyRegistered => {
                "Verifier already registered for the given scheme tag"
            }
            ContractError::VerifierNotRegistered => {
                "No verifier registered for the given scheme tag"
            }
            ContractError::VerificationFailed => {
                "Signature verification failed for the given scheme and payload"
            }
            ContractError::RevocationGraceExpired => {
                "Post-expiry revocation attempted outside the configured grace window"
            }
            ContractError::DelegationNotExpired => {
                "Cleanup attempted on a delegation that is not expired yet"
            }
            ContractError::DelegationInactive => {
                "Delegation is not active (revoked or expired)"
            }
            ContractError::PayloadTooOld => {
                "Signed payload ledger_number is older than MAX_PAYLOAD_AGE_LEDGERS ledgers"
            }
            ContractError::PromiseNotKept => {
                "off-chain promise hash does not match on-chain execution"
            }
            ContractError::StaleEpoch => "governance epoch reference is stale",
            ContractError::AmountMustBePositive => "Amount must be strictly positive",
            ContractError::ThresholdExceedsSigners => {
                "Threshold cannot exceed the current signer count"
            }
            ContractError::InsufficientTreasuryBalance => {
                "Treasury balance is insufficient for withdrawal"
            }
            ContractError::ProposalNotFound => "Withdrawal proposal not found",
            ContractError::ProposalAlreadyExecuted => {
                "Withdrawal proposal has already been executed"
            }
            ContractError::InsufficientApprovals => {
                "Proposal does not have enough approvals to execute"
            }
            ContractError::InvalidFlashLoanCallback => {
                "Flashloan callback returned an invalid magic value"
            }
            ContractError::FlashLoanRepaymentFailed => {
                "Flashloan principal plus fee was not fully repaid"
            }
            ContractError::ProposalExpired => "Withdrawal proposal has expired",
            ContractError::SlippageExceeded => {
                "Settled withdrawal amount fell below the caller's minimum (slippage)"
            }
            ContractError::TreasuryBeneficiaryMismatch => {
                "Payment beneficiary does not match the expected treasury address"
            }
            ContractError::CorridorNotRegistered => {
                "Settlement destination is not a registered corridor"
            }
            ContractError::Overflow => "Integer overflow in checked arithmetic",
            ContractError::NoPendingAdmin => "No pending admin transfer exists",
            ContractError::DomainMismatch => "Payload domain tag does not match expected",
            ContractError::OwnerMismatch => "Payload owner does not match expected caller",
            ContractError::TargetMismatch => "Payload target does not match expected action",
            ContractError::ContractIdMismatch => "Payload contract_id does not match current contract",
            ContractError::InvalidAdminAddress => "Proposed admin is the zero or identity address",
            ContractError::AdminUnchanged => "Proposed admin is the same as the current admin",
            ContractError::TimelockNotReady => "Timelock delay has not yet elapsed",
            ContractError::ZeroBytes32 => "Input BytesN<32> argument is all-zero",
            ContractError::TimestampInFuture => {
                "Supplied timestamp or ledger number is ahead of the current ledger"
            }
            ContractError::CrossContractCallerMismatch => {
                "Cross-contract caller does not match the configured partner address"
            }
            ContractError::OutsideBusinessHours => {
                "Scheduled operation is outside permitted UTC business hours (Mon-Fri 09:00-17:00)"
            }
            ContractError::InvalidMaxPauseSigners => {
                "Max-pause-signers value must be greater than zero and within the hard cap"
            }
            ContractError::MaxPauseSignersExceeded => {
                "Registering another pause signer would exceed the configured cap"
            }
            ContractError::StaleAdminEpoch => {
                "Admin pause proposal carries a stale epoch reference"
            }
            ContractError::StaleSignerEpoch => {
                "Signer pause proposal carries a stale epoch reference"
            }
            ContractError::EmergencyDrainNotPermitted => "Emergency drain requires contract to be paused and timelock window to have elapsed",
            ContractError::Underflow => "Integer underflow in checked arithmetic",
            ContractError::DivisionByZero => "Division by a zero denominator",
            ContractError::BondNotFound => "No bond exists for the given address",
            ContractError::BatchTooLarge => "Batch input exceeds the maximum allowed size",
            ContractError::EmptyBatch => "Batch input is empty",
            ContractError::AmountMustBePositive => "Amount must be positive",
            ContractError::InvalidFlashLoanCallback => "Invalid flash loan callback",
            ContractError::FlashLoanRepaymentFailed => "Flash loan repayment failed",
            ContractError::DeadlineExpired => "Signature/operation deadline has passed",
            _ => "Unknown error",
        }
    }

    fn is_recoverable(&self) -> bool {
        // Classification rule (informs every arm below):
        //   RECOVERABLE — caller can fix their own input or wait for state
        //                 they observe to change, then retry the same
        //                 kind of operation successfully without code/
        //                 deployment changes.
        //   FATAL       — retrying the same caller input is guaranteed
        //                 to fail, and the fix is not in caller's hands:
        //                 code-level impossibility, security halt,
        //                 cryptographic failure, or system capacity
        //                 reached. Indexers/admins should be alerted;
        //                 clients should NOT retry.
        // Per-arm rationale is the trailing `// ...` comment so reviewers
        // can audit each decision next to its arm. The `///` trait rustdoc
        // captures the rule globally.
        match self {
            // --- Initialization: caller fixes setup state. ---
            ContractError::NotInitialized | ContractError::AlreadyInitialized => true,

            // --- Authorization (100-199) + Admin Transfer (109-117):
            //     switch to the correct signer/role, or wait/correct
            //     payload/state. Caller-fixable in every case. ---
            ContractError::NotAdmin
            | ContractError::NotBondOwner
            | ContractError::UnauthorizedAttester
            | ContractError::NotOriginalAttester
            | ContractError::NotSigner
            | ContractError::UnauthorizedDepositor
            | ContractError::ContractPaused         // wait for unpause
            | ContractError::MigrationInProgress    // caller waits for migration to complete, then retries
            | ContractError::BorrowFrozen           // wait for unfreeze
            | ContractError::OutsideBusinessHours   // retry within business hours
            | ContractError::InvalidPauseAction     // correct action byte
            | ContractError::InsufficientSignatures // gather more approvals
            | ContractError::AdminSuspended         // wait for suspension
            | ContractError::NoPendingAdmin         // call begin_admin_transfer first
            | ContractError::InvalidAdminAddress
            | ContractError::AdminUnchanged
            | ContractError::TimelockNotReady
            | ContractError::EmergencyDrainNotPermitted
            | ContractError::RoleNotHeldAtLedger
            | ContractError::ZeroBytes32
            | ContractError::TimestampInFuture
            | ContractError::LeaseScopeMismatch
            | ContractError::LeaseExpired
            | ContractError::OutsideBusinessHours   // retry within business hours
            | ContractError::MigrationInProgress    // retry after migration completes
            | ContractError::LeaseSignerMismatch    // fix lease signer
            => true,


            // Admin can supply a valid value / remove a signer or raise the
            // cap, then retry.
            ContractError::InvalidMaxPauseSigners => true,
            ContractError::MaxPauseSignersExceeded => true,

            // Retry after the business-hours window opens.
            // Stale epoch proposals cannot be fixed by retry — re-propose in the
            // current bucket.
            ContractError::StaleAdminEpoch | ContractError::StaleSignerEpoch => false,

            // Cross-contract caller mismatch is a security halt; do not retry.
            ContractError::CrossContractCallerMismatch => false,

            // --- Bond (200-299): most errors are caller-fixable. ---
            ContractError::BondNotFound                 // create_bond first
            | ContractError::BondNotActive
            | ContractError::InsufficientBalance        // top up
            | ContractError::SlashExceedsBond           // reduce slash amount
            | ContractError::LockupNotExpired           // wait for the lock-up
            | ContractError::NotRollingBond
            | ContractError::WithdrawalAlreadyRequested // wait for the existing request
            | ContractError::CooldownRequestAlreadyPending
            | ContractError::CooldownRequestNotFound
            | ContractError::CooldownPeriodNotElapsed
            | ContractError::InvalidNonce               // bump nonce
            | ContractError::SignatureExpired           // re-sign with later deadline
            | ContractError::NegativeStake              // reduce the stake
            | ContractError::EarlyExitConfigNotSet      // configure early exit first
            | ContractError::InvalidPenaltyBps          // use 0..=10000
            | ContractError::LeverageExceeded           // reduce operation size
            | ContractError::UnsupportedToken           // use a safe token (e.g. SAC)
            | ContractError::UnsupportedDecimals
            | ContractError::InvalidBondAmount
            | ContractError::AmountExplicitlyZero  // supply a non-zero amount
            | ContractError::InvalidBondDuration
            | ContractError::InvalidNoticePeriod
            | ContractError::BondAlreadyExists
            | ContractError::UnauthorizedToken
            | ContractError::InvalidCurrency
            | ContractError::InvalidStringifiedBytes
            | ContractError::DuplicateIdempotencyKey    // use a different idempotency key
            | ContractError::BatchTooLarge         // reduce batch size
            | ContractError::EmptyBatch            // supply at least one item
            => true,

            // FATAL Bond: caller cannot directly fix any of these.
            ContractError::StorageCapReached => false,    // system capacity; only operator prune fixes it
            ContractError::TreasuryNotConfigured => true, // admin can configure treasury then retry
            ContractError::CursorOutOfRange => true,      // caller can supply a valid cursor in range
            ContractError::ReentrancyDetected => false,   // SECURITY HALT: investigate, do not retry
            ContractError::InvariantViolation => false,   // post-write drift detection
            ContractError::UnauthorizedToken => false,
            ContractError::UnsupportedDecimals => false,

            // FATAL Bond/Delegation payload binding mismatches (218/219/220/221).
            // Same payload will fail again; clients must not blindly retry.
            ContractError::DomainMismatch
            | ContractError::OwnerMismatch
            | ContractError::TargetMismatch
            | ContractError::ContractIdMismatch => false,

            // --- Attestation (300-399): all caller-fixable. ---
            ContractError::DuplicateAttestation
            | ContractError::AttestationNotFound
            | ContractError::AttestationAlreadyRevoked
            | ContractError::InvalidAttestationWeight
            | ContractError::AttestationWeightExceedsMax => true,

            // --- Registry (400-499): all caller-fixable. ---
            ContractError::IdentityAlreadyRegistered
            | ContractError::BondContractAlreadyRegistered
            | ContractError::IdentityNotRegistered
            | ContractError::BondContractNotRegistered
            | ContractError::AlreadyDeactivated
            | ContractError::AlreadyActive
            | ContractError::InvalidContractAddress
            | ContractError::ContractCodeVerificationFailed
            | ContractError::UnsupportedInterface => true,

            // --- Delegation (500-599): mostly caller-fixable ---
            ContractError::ExpiryInPast                // supply a future expiry
            | ContractError::DelegationNotFound        // create the delegation first
            | ContractError::AlreadyRevoked            // idempotent
            | ContractError::DelegationExpiryTooLong   // shorten to MAX_DURATION
            | ContractError::VerifierAlreadyRegistered // idempotent
            | ContractError::VerifierNotRegistered
            | ContractError::DelegationNotExpired
            | ContractError::PayloadTooOld => true,    // re-sign with current ledger number

            // FATAL Delegation: caller cannot fix these.
            ContractError::UnknownScheme => false,         // scheme tag not supported by this build
            ContractError::VerificationFailed => false,    // crypto failure; same input will fail
            ContractError::RevocationGraceExpired => false,           // grace window is admin-controlled; expiry is terminal for the caller
            ContractError::DelegationInactive => false,
            ContractError::PromiseNotKept => false,               // off-chain promise hash does not match on-chain execution
            ContractError::StaleEpoch => false,


            // --- Treasury (600-699): mostly caller-fixable ---
            ContractError::AmountMustBePositive            // supply amount > 0
            | ContractError::ThresholdExceedsSigners        // lower threshold to <= signer count
            | ContractError::InsufficientTreasuryBalance    // top up
            | ContractError::ProposalNotFound               // supply a valid proposal id
            | ContractError::ProposalAlreadyExecuted        // idempotent
            | ContractError::InsufficientApprovals          // collect more approvals
            | ContractError::ProposalExpired                // create a new proposal
            | ContractError::SlippageExceeded               // retry with a looser min_amount_out
            | ContractError::TreasuryBeneficiaryMismatch    // call with the correct treasury address
            | ContractError::CorridorNotRegistered => true, // admin registers the corridor, then retry




            ContractError::InvalidPercentSplit => true, // caller can provide valid splits

            // --- Arithmetic (700-799): code-level impossibility. ---
            ContractError::Overflow | ContractError::Underflow => false,
            ContractError::UnsupportedInterface => false,
            ContractError::DivisionByZero => false,
            ContractError::DeadlineExpired => true,  // re-sign with later deadline
            ContractError::InvalidFlashLoanCallback => false,
            ContractError::FlashLoanRepaymentFailed => false,
        }
    }
}

/// Constructor guard. Reject calls that arrive after the contract was already initialized.
///
/// Pass `already_initialized = <expression that returns true when the contract is
/// already initialized>` (e.g., `storage::get_admin(&e).is_some()`).
/// Panics with `ContractError::AlreadyInitialized` when `true`.
///
/// Implemented as a free function rather than a `#[macro_export]` macro because
/// constructor entrypoints return `()`, not `Result<_, _>`. Peer helpers
/// (`require_no_leading_zero_amount!`, `require_positive_amount!`,
/// `verify_no_future_ledger!`, `require_non_zero_bytes32!`) early-return `Err`,
/// which would not compile in a `()`-returning constructor; this helper must
/// `panic_with_error!` instead.
#[macro_export]
macro_rules! require_within_ttl {
    ($env:expr, $expires_at:expr) => {
        if $env.ledger().timestamp() >= $expires_at {
            return Err($crate::ContractError::SignatureExpired);
        }
    };
}

pub fn is_expired(e: &Env, expires_at: u64) -> bool {
    expires_at != 0 && e.ledger().timestamp() >= expires_at
}

pub fn require_within_ttl_panic(e: &Env, expires_at: u64, err: ContractError) {
    if is_expired(e, expires_at) {
        panic_with_error!(e, err);
    }
}

pub fn require_within_ttl_result(e: &Env, expires_at: u64) -> Result<(), ContractError> {
    if is_expired(e, expires_at) {
        return Err(ContractError::SignatureExpired);
    }
    Ok(())
}

#[cfg(test)]
mod test_errors;

/// Rejects a call when the contract has already been initialized.
///
/// Pass `true` if the contract storage already contains an admin/key,
/// meaning `initialize` has already been called.
///
/// Panics with `ContractError::AlreadyInitialized` when `is_initialized` is `true`.
#[macro_export]
macro_rules! require_contract_uninitialized {
    ($env:expr, $is_initialized:expr) => {
        if $is_initialized {
            return Err($crate::ContractError::AlreadyInitialized);
        }
    };
}

/// Wraps `env.current_contract_address()` with a mock hook for tests.
#[macro_export]
macro_rules! contract_address {
    ($env:expr) => {
        $env.current_contract_address()
    };
}

/// Requires that an `Option<i128>` amount is not explicitly set to zero.
/// This distinguishes "not set" (None) from "explicitly zero" (Some(0)),
/// where the latter indicates a bug in the caller.
/// Returns `ContractError::AmountExplicitlyZero` if the amount is `Some(0)`.
#[macro_export]
macro_rules! require_no_leading_zero_amount {
    ($env:expr, $amount:expr) => {
        if let Some(0) = $amount {
            return Err($crate::ContractError::AmountExplicitlyZero);
        }
    };
}

/// Requires that an i128 amount is strictly positive (> 0), panicking
/// with `ContractError::AmountMustBePositive` if not.
#[macro_export]
macro_rules! require_positive_amount {
    ($env:expr, $amount:expr) => {
        if $amount <= 0 {
            panic_with_error!($env, $crate::ContractError::AmountMustBePositive);
        }
    };
}

/// Rejects a caller-supplied ledger sequence number or timestamp that is
/// strictly ahead of the current on-chain value.
///
/// # Threat being mitigated
///
/// Without this check, an attacker can submit a payload with a far-future
/// `ledger_number` or `timestamp` field. When the staleness check uses
/// `current.saturating_sub(signed_at)`, a future value yields 0 (due to
/// saturation), making the payload appear perpetually fresh and bypassing the
/// entire staleness window.
///
/// # Arguments
///
/// * `e` - The Soroban environment
/// * `ledger_number` - The claimed ledger sequence number or timestamp from the payload
///
/// # Panics
///
/// Panics with `ContractError::TimestampInFuture` when the supplied value is
/// greater than the current ledger sequence or timestamp.
pub fn verify_no_future_ledger(e: &Env, ledger_number: impl Into<u64>) {
    let ledger_number = ledger_number.into();
    let current_sequence = u64::from(e.ledger().sequence());
    if ledger_number > current_sequence {
        e.panic_with_error(ContractError::TimestampInFuture);
    }
}

/// Rejects a caller-supplied ledger sequence number that is strictly ahead of
/// the current on-chain ledger sequence.
///
/// Returns `ContractError::TimestampInFuture` when `$seq` exceeds
/// `env.ledger().sequence()`, preventing the contract from accepting
/// values that could only originate from the future.
///
/// # Examples
///
/// ```ignore
/// verify_no_future_ledger_sequence!(&env, payload.ledger_number);
/// ```
#[macro_export]
macro_rules! verify_no_future_ledger_sequence {
    ($env:expr, $seq:expr) => {
        if $seq > $env.ledger().sequence() {
            panic_with_error!($env, $crate::ContractError::TimestampInFuture);
        }
    };
}

#[macro_export]
macro_rules! require_non_zero_bytes32 {
    ($env:expr, $val:expr) => {
        if $val == &soroban_sdk::BytesN::<32>::from_array($env, &[0u8; 32]) {
            return Err($crate::ContractError::ZeroBytes32);
        }
    };
}

/// Requires that a cross-contract caller address matches the expected partner address.
///
/// This is a defence-in-depth guard that prevents attackers from redirecting
/// cross-contract calls to malicious contracts (e.g. a fake verifier, a rogue
/// callback, or a spoofed registry). It should be called before or after
/// `e.invoke_contract()` to ensure the counterparty is the pre-configured
/// partner.
///
/// # Arguments
/// * `e` - The Soroban environment
/// * `caller` - The address of the contract making/receiving the cross-contract call
/// * `expected` - The pre-configured expected partner address
///
/// # Panics
/// Panics with `ContractError::CrossContractCallerMismatch` (code 119) if
/// `caller != expected`.
///
/// # Example
/// ```ignore
/// // Before making a cross-contract call to a verifier:
/// let verifier_addr = e.storage().instance().get(&DataKey::Verifier(scheme)).unwrap();
/// credence_errors::require_matching_contract_id(&e, &verifier_addr, &expected_verifier);
/// e.invoke_contract(&verifier_addr, &Symbol::new(&e, "verify"), args);
///
/// // Or when receiving a cross-contract call:
/// let caller = e.current_contract_address(); // or get from auth context
/// credence_errors::require_matching_contract_id(&e, &caller, &expected_partner);
/// ```
pub fn require_matching_contract_id(e: &Env, caller: &Address, expected: &Address) {
    if caller != expected {
        panic_with_error!(e, ContractError::CrossContractCallerMismatch);
    }
}

/// Validates that the provided lease signer address matches the calling actor.
///
/// Defence-in-depth guard for lease-gated operations: without it, an actor
/// who obtains a valid lease object could invoke lease-gated operations as a
/// different actor, bypassing the intended delegation boundary.
///
/// # Panics
/// Panics with `ContractError::LeaseSignerMismatch` (code 126) if
/// `lease != actor`.
#[inline]
pub fn require_matching_lease_signer(e: &Env, lease: &Address, actor: &Address) {
    if lease != actor {
        panic_with_error!(e, ContractError::LeaseSignerMismatch);
    }
}

/// Validates that the provided timestamp (seconds since UNIX epoch) falls
/// within UTC business hours (Monday-Friday, 09:00:00 to 16:59:59).
///
/// # Panics
/// Panics with `ContractError::OutsideBusinessHours` (code 124) if it does
/// not.
#[inline]
pub fn require_within_business_hours(e: &Env, t: u64) {
    let days_since_epoch = t / 86_400;
    // 1970-01-01 was a Thursday.
    // 0 = Thu, 1 = Fri, 2 = Sat, 3 = Sun, 4 = Mon, 5 = Tue, 6 = Wed
    let weekday_shifted = days_since_epoch % 7;
    let is_weekend = weekday_shifted == 2 || weekday_shifted == 3;

    let time_of_day = t % 86_400;
    // 09:00:00 = 32_400
    // 17:00:00 = 61_200
    let is_business_time = time_of_day >= 32_400 && time_of_day < 61_200;

    if is_weekend || !is_business_time {
        panic_with_error!(e, ContractError::OutsideBusinessHours);
    }
}
