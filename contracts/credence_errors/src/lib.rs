#![no_std]

use soroban_sdk::contracterror;

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
#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ContractError {
    // --- Initialization (1-99) ---
    /// Contract has not been initialized yet.
    /// Replaces: panic!("not initialized")
    /// Contracts: bond, registry, delegation, treasury
    NotInitialized = 1,

    /// Contract has already been initialized and cannot be re-initialized.
    /// Replaces: panic!("already initialized")
    /// Contracts: registry
    AlreadyInitialized = 2,

    // --- Authorization (100-199) ---
    /// Caller is not the admin.
    /// Replaces: panic!("not admin")
    /// Contracts: bond, registry, delegation
    NotAdmin = 100,

    /// Caller is not the bond owner.
    /// Replaces: panic!("not bond owner")
    /// Contracts: bond
    NotBondOwner = 101,

    /// Caller is not an authorized attester for this bond.
    /// Replaces: panic!("unauthorized attester")
    /// Contracts: bond
    UnauthorizedAttester = 102,

    /// Caller is not the original attester who created the attestation.
    /// Replaces: panic!("only original attester can revoke")
    /// Contracts: bond
    NotOriginalAttester = 103,

    /// Caller is not a registered multi-sig signer.
    /// Replaces: panic!("only signer can propose withdrawal")
    ///           panic!("only signer can approve")
    /// Contracts: treasury
    NotSigner = 104,

    /// Caller is neither the admin nor an authorized depositor.
    /// Replaces: panic!("only admin or authorized depositor can receive_fee")
    /// Contracts: treasury
    UnauthorizedDepositor = 105,

    // --- Bond (200-299) ---
    /// No bond exists for the given address or key.
    /// Replaces: panic!("no bond")
    /// Contracts: bond
    BondNotFound = 200,

    /// Bond is not in the active state required for this operation.
    /// Replaces: panic!("bond not active")
    /// Contracts: bond
    BondNotActive = 201,

    /// Caller balance is insufficient for the requested withdrawal.
    /// Replaces: panic!("insufficient balance for withdrawal")
    /// Contracts: bond
    InsufficientBalance = 202,

    /// The slash amount exceeds the bonded amount.
    /// Replaces: panic!("slashed amount exceeds bonded amount")
    ///           panic!("slash exceeds bond")
    /// Contracts: bond
    SlashExceedsBond = 203,

    /// Bond lock-up period has not yet expired.
    /// Replaces: panic!("use withdraw for post lock-up")
    /// Contracts: bond
    LockupNotExpired = 204,

    /// Operation requires a rolling bond but this bond is not rolling.
    /// Replaces: panic!("not a rolling bond")
    /// Contracts: bond
    NotRollingBond = 205,

    /// A withdrawal has already been requested for this bond.
    /// Replaces: panic!("withdrawal already requested")
    /// Contracts: bond
    WithdrawalAlreadyRequested = 206,

    /// Reentrancy was detected; the call is rejected.
    /// Replaces: panic!("reentrancy detected")
    /// Contracts: bond
    ReentrancyDetected = 207,

    /// Nonce is invalid - either replayed or out of order.
    /// Replaces: panic!("invalid nonce: replay or out-of-order")
    /// Contracts: bond
    InvalidNonce = 208,

    /// Attester stake would go negative, which is not permitted.
    /// Replaces: panic!("attester stake cannot be negative")
    /// Contracts: bond
    NegativeStake = 209,

    /// Early-exit configuration has not been set for this bond.
    /// Replaces: panic!("early exit config not set")
    /// Contracts: bond
    EarlyExitConfigNotSet = 210,

    /// Penalty basis-points value must be in the range 0-10000.
    /// Replaces: panic!("penalty_bps must be <= 10000")
    /// Contracts: bond
    InvalidPenaltyBps = 211,

    // --- Attestation (300-399) ---
    /// An attestation already exists from this attester for this bond.
    /// Replaces: panic!("duplicate attestation")
    /// Contracts: bond
    DuplicateAttestation = 300,

    /// No attestation was found for the given key.
    /// Replaces: panic!("attestation not found")
    /// Contracts: bond
    AttestationNotFound = 301,

    /// Attestation has already been revoked.
    /// Replaces: panic!("attestation already revoked")
    /// Contracts: bond, delegation
    AttestationAlreadyRevoked = 302,

    /// Attestation weight must be a positive value.
    /// Replaces: panic!("attestation weight must be positive")
    /// Contracts: bond
    InvalidAttestationWeight = 303,

    /// Attestation weight exceeds the configured maximum.
    /// Replaces: panic!("attestation weight exceeds maximum")
    /// Contracts: bond
    AttestationWeightExceedsMax = 304,

    // --- Registry (400-499) ---
    /// Identity has already been registered in the registry.
    /// Replaces: panic!("identity already registered")
    /// Contracts: registry
    IdentityAlreadyRegistered = 400,

    /// Bond contract address has already been registered.
    /// Replaces: panic!("bond contract already registered")
    /// Contracts: registry
    BondContractAlreadyRegistered = 401,

    /// Identity is not registered in the registry.
    /// Replaces: panic!("identity not registered")
    /// Contracts: registry
    IdentityNotRegistered = 402,

    /// Bond contract is not registered in the registry.
    /// Replaces: panic!("bond contract not registered")
    /// Contracts: registry
    BondContractNotRegistered = 403,

    /// Identity or bond contract is already in the deactivated state.
    /// Replaces: panic!("already deactivated")
    /// Contracts: registry
    AlreadyDeactivated = 404,

    /// Identity or bond contract is already in the active state.
    /// Replaces: panic!("already active")
    /// Contracts: registry
    AlreadyActive = 405,

    // --- Delegation (500-599) ---
    /// Delegation expiry timestamp must be in the future.
    /// Replaces: panic!("expiry must be in the future")
    /// Contracts: delegation
    ExpiryInPast = 500,

    /// No delegation record was found for the given key.
    /// Replaces: panic!("delegation not found")
    /// Contracts: delegation
    DelegationNotFound = 501,

    /// Delegation has already been revoked.
    /// Replaces: panic!("already revoked")
    /// Contracts: delegation
    AlreadyRevoked = 502,

    // --- Treasury (600-699) ---
    /// Amount argument must be strictly positive (> 0).
    /// Replaces: panic!("amount must be positive")
    /// Contracts: treasury
    AmountMustBePositive = 600,

    /// Approval threshold cannot exceed the current number of signers.
    /// Replaces: panic!("threshold cannot exceed signer count")
    /// Contracts: treasury
    ThresholdExceedsSigners = 601,

    /// Treasury balance is insufficient for the requested withdrawal.
    /// Replaces: panic!("insufficient treasury balance")
    /// Contracts: treasury
    InsufficientTreasuryBalance = 602,

    /// Withdrawal proposal was not found for the given id.
    /// Replaces: panic!("proposal not found")
    /// Contracts: treasury
    ProposalNotFound = 603,

    /// Withdrawal proposal has already been executed.
    /// Replaces: panic!("proposal already executed")
    /// Contracts: treasury
    ProposalAlreadyExecuted = 604,

    /// Proposal does not yet have enough approvals to execute.
    /// Replaces: panic!("insufficient approvals to execute")
    /// Contracts: treasury
    InsufficientApprovals = 605,

    // --- Arithmetic (700-799) ---
    /// Integer overflow detected during a checked arithmetic operation.
    /// Replaces: .expect("... overflow")
    /// Contracts: bond, treasury
    Overflow = 700,

    /// Integer underflow detected during a checked arithmetic operation.
    /// Replaces: .expect("... underflow")
    /// Contracts: treasury
    Underflow = 701,
}

/// @title  ErrorExt
/// @notice Provides category() and description() on every ContractError variant.
/// @dev    Use this for structured logging, monitoring, and off-chain display.
pub trait ErrorExt {
    /// @return The ErrorCategory bucket this error belongs to.
    fn category(&self) -> ErrorCategory;

    /// @return A static string description safe for logging or display.
    fn description(&self) -> &'static str;
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
            | ContractError::UnauthorizedDepositor => ErrorCategory::Authorization,

            ContractError::BondNotFound
            | ContractError::BondNotActive
            | ContractError::InsufficientBalance
            | ContractError::SlashExceedsBond
            | ContractError::LockupNotExpired
            | ContractError::NotRollingBond
            | ContractError::WithdrawalAlreadyRequested
            | ContractError::ReentrancyDetected
            | ContractError::InvalidNonce
            | ContractError::NegativeStake
            | ContractError::EarlyExitConfigNotSet
            | ContractError::InvalidPenaltyBps => ErrorCategory::Bond,

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
            | ContractError::AlreadyActive => ErrorCategory::Registry,

            ContractError::ExpiryInPast
            | ContractError::DelegationNotFound
            | ContractError::AlreadyRevoked => ErrorCategory::Delegation,

            ContractError::AmountMustBePositive
            | ContractError::ThresholdExceedsSigners
            | ContractError::InsufficientTreasuryBalance
            | ContractError::ProposalNotFound
            | ContractError::ProposalAlreadyExecuted
            | ContractError::InsufficientApprovals => ErrorCategory::Treasury,

            ContractError::Overflow | ContractError::Underflow => ErrorCategory::Arithmetic,
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
            ContractError::NegativeStake => "Attester stake cannot be negative",
            ContractError::EarlyExitConfigNotSet => {
                "Early-exit configuration has not been set for this bond"
            }
            ContractError::InvalidPenaltyBps => "Penalty bps must be in range 0-10000",
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
            ContractError::ExpiryInPast => "Delegation expiry must be in the future",
            ContractError::DelegationNotFound => "No delegation found for the given key",
            ContractError::AlreadyRevoked => "Delegation has already been revoked",
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
            ContractError::Overflow => "Integer overflow in checked arithmetic",
            ContractError::Underflow => "Integer underflow in checked arithmetic",
        }
    }
}

#[cfg(test)]
mod test_errors;
