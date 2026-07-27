// Single source of truth for exhaustive `ContractError` variant coverage in tests.
//
// Included from `tests/discriminant_uniqueness.rs` and `src/test_errors.rs`.
// When adding a variant to `src/lib.rs`, add exactly one row here — do not
// maintain parallel lists or manually bumped counts elsewhere.

/// Every `ContractError` variant, one row per name, in numeric-code order
/// within each category block.
pub const ALL_VARIANTS: &[(&'static str, ContractError)] = &[
    // --- Initialization (1-99) ---
    ("NotInitialized", ContractError::NotInitialized),
    ("AlreadyInitialized", ContractError::AlreadyInitialized),
    // --- Authorization (100-199) ---
    ("NoPendingAdmin", ContractError::NoPendingAdmin),
    ("InvalidAdminAddress", ContractError::InvalidAdminAddress),
    ("AdminUnchanged", ContractError::AdminUnchanged),
    ("TimelockNotReady", ContractError::TimelockNotReady),
    ("AdminSuspended", ContractError::AdminSuspended),
    ("BorrowFrozen", ContractError::BorrowFrozen),
    (
        "EmergencyDrainNotPermitted",
        ContractError::EmergencyDrainNotPermitted,
    ),
    ("RoleNotHeldAtLedger", ContractError::RoleNotHeldAtLedger),
    ("TimestampInFuture", ContractError::TimestampInFuture),
    ("ZeroBytes32", ContractError::ZeroBytes32),
    ("NotAdmin", ContractError::NotAdmin),
    ("NotBondOwner", ContractError::NotBondOwner),
    ("UnauthorizedAttester", ContractError::UnauthorizedAttester),
    ("NotOriginalAttester", ContractError::NotOriginalAttester),
    ("NotSigner", ContractError::NotSigner),
    (
        "UnauthorizedDepositor",
        ContractError::UnauthorizedDepositor,
    ),
    ("ContractPaused", ContractError::ContractPaused),
    ("InvalidPauseAction", ContractError::InvalidPauseAction),
    (
        "InsufficientSignatures",
        ContractError::InsufficientSignatures,
    ),
    (
        "MigrationInProgress",
        ContractError::MigrationInProgress,
    ),
    // --- Bond (200-299) ---
    ("BondNotFound", ContractError::BondNotFound),
    ("BondNotActive", ContractError::BondNotActive),
    ("InsufficientBalance", ContractError::InsufficientBalance),
    ("SlashExceedsBond", ContractError::SlashExceedsBond),
    ("StorageCapReached", ContractError::StorageCapReached),
    ("LockupNotExpired", ContractError::LockupNotExpired),
    ("NotRollingBond", ContractError::NotRollingBond),
    (
        "WithdrawalAlreadyRequested",
        ContractError::WithdrawalAlreadyRequested,
    ),
    ("ReentrancyDetected", ContractError::ReentrancyDetected),
    ("InvalidNonce", ContractError::InvalidNonce),
    ("SignatureExpired", ContractError::SignatureExpired),
    ("NegativeStake", ContractError::NegativeStake),
    (
        "EarlyExitConfigNotSet",
        ContractError::EarlyExitConfigNotSet,
    ),
    ("InvalidPenaltyBps", ContractError::InvalidPenaltyBps),
    ("LeverageExceeded", ContractError::LeverageExceeded),
    ("UnsupportedToken", ContractError::UnsupportedToken),
    ("UnsupportedDecimals", ContractError::UnsupportedDecimals),
    ("InvalidBondAmount", ContractError::InvalidBondAmount),
    ("AmountExplicitlyZero", ContractError::AmountExplicitlyZero),
    ("InvalidBondDuration", ContractError::InvalidBondDuration),
    ("InvalidNoticePeriod", ContractError::InvalidNoticePeriod),
    ("BondAlreadyExists", ContractError::BondAlreadyExists),
    // Codes 218, 219, 220, 221 — see shared Bond/Delegation block below.
    ("UnauthorizedToken", ContractError::UnauthorizedToken),
    (
        "DuplicateIdempotencyKey",
        ContractError::DuplicateIdempotencyKey,
    ),
    ("InvariantViolation", ContractError::InvariantViolation),
    ("InvalidCurrency", ContractError::InvalidCurrency),
    (
        "TreasuryNotConfigured",
        ContractError::TreasuryNotConfigured,
    ),
    ("CursorOutOfRange", ContractError::CursorOutOfRange),
    ("BatchTooLarge", ContractError::BatchTooLarge),
    ("EmptyBatch", ContractError::EmptyBatch),
    ("PromiseNotKept", ContractError::PromiseNotKept),
    // --- Shared Bond/Delegation payload mismatches ---
    // Numeric codes 219, 220, 221, 225 per `lib.rs` doc-comment.
    ("DomainMismatch", ContractError::DomainMismatch),
    ("OwnerMismatch", ContractError::OwnerMismatch),
    ("TargetMismatch", ContractError::TargetMismatch),
    ("ContractIdMismatch", ContractError::ContractIdMismatch),
    // --- Attestation (300-399) ---
    ("DuplicateAttestation", ContractError::DuplicateAttestation),
    ("AttestationNotFound", ContractError::AttestationNotFound),
    (
        "AttestationAlreadyRevoked",
        ContractError::AttestationAlreadyRevoked,
    ),
    (
        "InvalidAttestationWeight",
        ContractError::InvalidAttestationWeight,
    ),
    (
        "AttestationWeightExceedsMax",
        ContractError::AttestationWeightExceedsMax,
    ),
    // --- Registry (400-499) ---
    (
        "IdentityAlreadyRegistered",
        ContractError::IdentityAlreadyRegistered,
    ),
    (
        "BondContractAlreadyRegistered",
        ContractError::BondContractAlreadyRegistered,
    ),
    (
        "IdentityNotRegistered",
        ContractError::IdentityNotRegistered,
    ),
    (
        "BondContractNotRegistered",
        ContractError::BondContractNotRegistered,
    ),
    ("AlreadyDeactivated", ContractError::AlreadyDeactivated),
    ("AlreadyActive", ContractError::AlreadyActive),
    (
        "InvalidContractAddress",
        ContractError::InvalidContractAddress,
    ),
    (
        "ContractCodeVerificationFailed",
        ContractError::ContractCodeVerificationFailed,
    ),
    ("UnsupportedInterface", ContractError::UnsupportedInterface),
    // --- Delegation (500-599) ---
    ("ExpiryInPast", ContractError::ExpiryInPast),
    ("DelegationNotFound", ContractError::DelegationNotFound),
    ("AlreadyRevoked", ContractError::AlreadyRevoked),
    (
        "DelegationExpiryTooLong",
        ContractError::DelegationExpiryTooLong,
    ),
    ("UnknownScheme", ContractError::UnknownScheme),
    (
        "VerifierAlreadyRegistered",
        ContractError::VerifierAlreadyRegistered,
    ),
    (
        "VerifierNotRegistered",
        ContractError::VerifierNotRegistered,
    ),
    ("VerificationFailed", ContractError::VerificationFailed),
    (
        "RevocationGraceExpired",
        ContractError::RevocationGraceExpired,
    ),
    ("DelegationNotExpired", ContractError::DelegationNotExpired),
    ("PayloadTooOld", ContractError::PayloadTooOld),
    ("DelegationInactive", ContractError::DelegationInactive),
    // --- Treasury (600-699) ---
    ("AmountMustBePositive", ContractError::AmountMustBePositive),
    (
        "ThresholdExceedsSigners",
        ContractError::ThresholdExceedsSigners,
    ),
    (
        "InsufficientTreasuryBalance",
        ContractError::InsufficientTreasuryBalance,
    ),
    ("ProposalNotFound", ContractError::ProposalNotFound),
    (
        "ProposalAlreadyExecuted",
        ContractError::ProposalAlreadyExecuted,
    ),
    (
        "InsufficientApprovals",
        ContractError::InsufficientApprovals,
    ),
    (
        "InvalidFlashLoanCallback",
        ContractError::InvalidFlashLoanCallback,
    ),
    (
        "FlashLoanRepaymentFailed",
        ContractError::FlashLoanRepaymentFailed,
    ),
    ("ProposalExpired", ContractError::ProposalExpired),
    ("SlippageExceeded", ContractError::SlippageExceeded),
    (
        "TreasuryBeneficiaryMismatch",
        ContractError::TreasuryBeneficiaryMismatch,
    ),
    // --- Arithmetic (700-799) ---
    ("Overflow", ContractError::Overflow),
    ("Underflow", ContractError::Underflow),
    ("DivisionByZero", ContractError::DivisionByZero),
];
