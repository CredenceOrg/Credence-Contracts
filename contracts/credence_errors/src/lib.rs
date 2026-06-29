#![no_std]
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

use soroban_sdk::contracterror;
/// Project-wide version constant.
pub const VERSION: &str = "0.1.0";

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

    /// The target admin is currently suspended (suspended_until > now).
    /// Used by suspend_admin when `until_ts` is not strictly in the future,
    /// and by callers that detect a suspended admin attempting a privileged
    /// action.
    /// Contracts: admin
    /// Wire-stable: do not renumber this error code.
    AdminSuspended = 113,

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

    /// Reentrancy was detected; the call is rejected.
    /// Replaces: panic!("reentrancy detected")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    ReentrancyDetected = 207,

    /// Nonce is invalid - either replayed or out of order.
    /// Replaces: panic!("invalid nonce: replay or out-of-order")
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidNonce = 208,

    /// Signature/operation deadline has passed (now > deadline + grace).
    /// Contracts: bond, delegation
    SignatureExpired = 222,

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

    /// Bond amount must be strictly positive (> 0).
    /// Triggered by: create_bond called with amount <= 0
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidBondAmount = 214,

    /// Bond duration must be strictly positive (> 0).
    /// Triggered by: create_bond called with duration == 0
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidBondDuration = 215,

    /// Rolling-bond notice_period_duration must be > 0 and <= duration.
    /// Triggered by: create_bond called with is_rolling=true and notice_period_duration == 0
    ///               or notice_period_duration > duration
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvalidNoticePeriod = 216,

    /// Bond already exists for this identity.
    /// Triggered by: create_bond called for an identity that already has an active bond
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    BondAlreadyExists = 217,

    /// Post-write invariant self-check detected bond or attestation accounting drift.
    /// Triggered by: `invariants::assert_self_consistent` after a bond-module write
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    InvariantViolation = 218,

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
    // Note: DomainMismatch (218), OwnerMismatch (219), TargetMismatch (220),
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

    // --- Shared Bond/Delegation payload mismatch errors (218-221) ---
    // Wire-stable: codes documented in the note above; kept distinct from the
    // delegation scheme/verifier errors (504-507).
    DomainMismatch = 225,
    OwnerMismatch = 219,
    TargetMismatch = 220,
    ContractIdMismatch = 221,

    // --- Admin Transfer (109-112) ---
    /// No pending admin transfer exists.
    NoPendingAdmin = 109,

    /// Proposed admin is the zero/identity address.
    InvalidAdminAddress = 110,

    /// Proposed admin is the same as the current admin.
    AdminUnchanged = 111,

    /// Timelock delay has not yet elapsed.
    TimelockNotReady = 112,

    /// Emergency drain is not permitted: contract must be paused and timelock window must have elapsed.
    /// Contracts: bond
    /// Wire-stable: do not renumber this error code.
    EmergencyDrainNotPermitted = 113,

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
            | ContractError::InvalidPauseAction
            | ContractError::InsufficientSignatures
            | ContractError::AdminSuspended => ErrorCategory::Authorization,

            ContractError::BondNotFound
            | ContractError::BondNotActive
            | ContractError::InsufficientBalance
            | ContractError::SlashExceedsBond
            | ContractError::LockupNotExpired
            | ContractError::NotRollingBond
            | ContractError::WithdrawalAlreadyRequested
            | ContractError::ReentrancyDetected
            | ContractError::InvalidNonce
            | ContractError::SignatureExpired
            | ContractError::NegativeStake
            | ContractError::EarlyExitConfigNotSet
            | ContractError::InvalidPenaltyBps
            | ContractError::LeverageExceeded
            | ContractError::UnsupportedToken
            | ContractError::InvalidBondAmount
            | ContractError::InvalidBondDuration
            | ContractError::InvalidNoticePeriod
            | ContractError::BondAlreadyExists
            | ContractError::StorageCapReached
            | ContractError::TreasuryNotConfigured
            | ContractError::CursorOutOfRange
            | ContractError::BatchTooLarge
            | ContractError::EmptyBatch
            | ContractError::InvariantViolation => ErrorCategory::Bond,

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
            | ContractError::ContractCodeVerificationFailed => ErrorCategory::Registry,

            ContractError::ExpiryInPast
            | ContractError::DelegationNotFound
            | ContractError::AlreadyRevoked
            | ContractError::DelegationExpiryTooLong
            | ContractError::UnknownScheme
            | ContractError::VerifierAlreadyRegistered
            | ContractError::VerifierNotRegistered
            | ContractError::VerificationFailed
            | ContractError::RevocationGraceExpired
            | ContractError::DelegationNotExpired => ErrorCategory::Delegation,

            ContractError::AmountMustBePositive
            | ContractError::ThresholdExceedsSigners
            | ContractError::InsufficientTreasuryBalance
            | ContractError::ProposalNotFound
            | ContractError::ProposalAlreadyExecuted
            | ContractError::InsufficientApprovals
            | ContractError::InvalidFlashLoanCallback
            | ContractError::FlashLoanRepaymentFailed
            | ContractError::ProposalExpired => ErrorCategory::Treasury,

            ContractError::Overflow | ContractError::Underflow => ErrorCategory::Arithmetic,
            ContractError::NoPendingAdmin
            | ContractError::InvalidAdminAddress
            | ContractError::AdminUnchanged
            | ContractError::TimelockNotReady
            | ContractError::EmergencyDrainNotPermitted => ErrorCategory::Authorization,
            ContractError::DomainMismatch
            | ContractError::OwnerMismatch
            | ContractError::TargetMismatch
            | ContractError::ContractIdMismatch => ErrorCategory::Delegation,
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
            ContractError::InvalidPauseAction => "Pause proposal action is invalid",
            ContractError::InsufficientSignatures => "Not enough approvals to execute proposal",
            ContractError::AdminSuspended => "Admin is currently suspended",
            ContractError::BondNotFound => "No bond found for the given key",
            ContractError::BondNotActive => "Bond is not in an active state",
            ContractError::InsufficientBalance => "Insufficient balance for withdrawal",
            ContractError::SlashExceedsBond => "Slash amount exceeds the bonded amount",
            ContractError::LockupNotExpired => "Lock-up period has not yet expired",
            ContractError::NotRollingBond => "Bond is not configured as a rolling bond",
            ContractError::WithdrawalAlreadyRequested => {
                "A withdrawal has already been requested for this bond"
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
            ContractError::InvalidBondAmount => "Bond amount must be strictly positive (> 0)",
            ContractError::InvalidBondDuration => "Bond duration must be strictly positive (> 0)",
            ContractError::InvalidNoticePeriod => "Rolling-bond notice_period_duration must be > 0 and <= duration",
            ContractError::BondAlreadyExists => "Bond already exists for this identity",
            ContractError::StorageCapReached => "Storage cap for attestations or slash history reached",
            ContractError::TreasuryNotConfigured => "Slash treasury address has not been configured",
            ContractError::CursorOutOfRange => "Pagination cursor is out of range (cursor >= registry_slots)",
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
            ContractError::AmountMustBePositive => "Amount must be strictly positive (> 0)",
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
            ContractError::Overflow => "Integer overflow in checked arithmetic",
            ContractError::NoPendingAdmin => "No pending admin transfer exists",
            ContractError::DomainMismatch => "Payload domain tag does not match expected",
            ContractError::OwnerMismatch => "Payload owner does not match expected caller",
            ContractError::TargetMismatch => "Payload target does not match expected action",
            ContractError::ContractIdMismatch => "Payload contract_id does not match current contract",
            ContractError::InvalidAdminAddress => "Proposed admin is the zero or identity address",
            ContractError::AdminUnchanged => "Proposed admin is the same as the current admin",
            ContractError::TimelockNotReady => "Timelock delay has not yet elapsed",
            ContractError::EmergencyDrainNotPermitted => "Emergency drain requires contract to be paused and timelock window to have elapsed",
            ContractError::Underflow => "Integer underflow in checked arithmetic",
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

            // --- Authorization (100-199) + Admin Transfer (109-112):
            //     switch to the correct signer/role, or wait/correct
            //     payload/state. Caller-fixable in every case. ---
            ContractError::NotAdmin
            | ContractError::NotBondOwner
            | ContractError::UnauthorizedAttester
            | ContractError::NotOriginalAttester
            | ContractError::NotSigner
            | ContractError::UnauthorizedDepositor
            | ContractError::ContractPaused         // wait for unpause
            | ContractError::InvalidPauseAction     // correct action byte
            | ContractError::InsufficientSignatures // gather more approvals
            | ContractError::AdminSuspended         // wait for suspension
            | ContractError::NoPendingAdmin         // call begin_admin_transfer first
            | ContractError::InvalidAdminAddress
            | ContractError::AdminUnchanged
            | ContractError::TimelockNotReady => true,

            // --- Bond (200-299): most errors are caller-fixable. ---
            ContractError::BondNotFound                 // create_bond first
            | ContractError::BondNotActive
            | ContractError::InsufficientBalance        // top up
            | ContractError::SlashExceedsBond           // reduce slash amount
            | ContractError::LockupNotExpired           // wait for the lock-up
            | ContractError::NotRollingBond
            | ContractError::WithdrawalAlreadyRequested // wait for the existing request
            | ContractError::InvalidNonce               // bump nonce
            | ContractError::SignatureExpired           // re-sign with later deadline
            | ContractError::NegativeStake              // reduce the stake
            | ContractError::EarlyExitConfigNotSet      // configure early exit first
            | ContractError::InvalidPenaltyBps          // use 0..=10000
            | ContractError::LeverageExceeded           // reduce operation size
            | ContractError::UnsupportedToken           // use a safe token (e.g. SAC)
            | ContractError::InvalidBondAmount
            | ContractError::InvalidBondDuration
            | ContractError::InvalidNoticePeriod
            | ContractError::BondAlreadyExists
            | ContractError::BatchTooLarge         // reduce batch size
            | ContractError::EmptyBatch            // supply at least one item
            => true,

            // FATAL Bond: caller cannot directly fix any of these.
            ContractError::StorageCapReached => false,    // system capacity; only operator prune fixes it
            ContractError::TreasuryNotConfigured => true, // admin can configure treasury then retry
            ContractError::CursorOutOfRange => true,      // caller can supply a valid cursor in range
            ContractError::ReentrancyDetected => false,   // SECURITY HALT: investigate, do not retry
            ContractError::InvariantViolation => false,   // post-write drift detection

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
            | ContractError::ContractCodeVerificationFailed => true,

            // --- Delegation (500-599): mostly caller-fixable ---
            ContractError::ExpiryInPast                // supply a future expiry
            | ContractError::DelegationNotFound        // create the delegation first
            | ContractError::AlreadyRevoked            // idempotent
            | ContractError::DelegationExpiryTooLong   // shorten to MAX_DURATION
            | ContractError::VerifierAlreadyRegistered // idempotent
            | ContractError::VerifierNotRegistered
            | ContractError::DelegationNotExpired => true,

            // FATAL Delegation: caller cannot fix these.
            ContractError::UnknownScheme => false,         // scheme tag not supported by this build
            ContractError::VerificationFailed => false,    // crypto failure; same input will fail
            ContractError::RevocationGraceExpired => false,           // grace window is admin-controlled; expiry is terminal for the caller

            // --- Treasury (600-699): mostly caller-fixable ---
            ContractError::AmountMustBePositive            // supply amount > 0
            | ContractError::ThresholdExceedsSigners        // lower threshold to <= signer count
            | ContractError::InsufficientTreasuryBalance    // top up
            | ContractError::ProposalNotFound               // supply a valid proposal id
            | ContractError::ProposalAlreadyExecuted        // idempotent
            | ContractError::InsufficientApprovals          // collect more approvals
            | ContractError::ProposalExpired => true,       // create a new proposal

            // FATAL Treasury flashloan failures: callback contract misbehaved.
            ContractError::InvalidFlashLoanCallback => false, // bad magic value
            ContractError::FlashLoanRepaymentFailed => false, // principal+fee mismatch

            // --- Arithmetic (700-799): code-level impossibility. ---
            ContractError::Overflow | ContractError::Underflow => false,
        }
    }
}

#[cfg(test)]
mod test_errors;
