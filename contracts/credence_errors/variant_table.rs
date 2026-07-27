// Single source of truth for exhaustive `ContractError` variant coverage in tests.
//
// Included from `tests/discriminant_uniqueness.rs` and `src/test_errors.rs`.
// When adding a variant to `src/lib.rs`, add exactly one row here — do not
// maintain parallel lists or manually bumped counts elsewhere.
//
// Row order: numeric wire code within each category block.

/// Every `ContractError` variant, one row per name, in numeric-code order
/// within each category block.
pub const ALL_VARIANTS: &[(&str, ContractError)] = &[
    // --- Initialization (1-99) ---
    ("NotInitialized", ContractError::NotInitialized),           // 1
    ("AlreadyInitialized", ContractError::AlreadyInitialized),   // 2
    // --- Authorization (100-199) ---
    ("NotAdmin", ContractError::NotAdmin),                                   // 100
    ("NotBondOwner", ContractError::NotBondOwner),                           // 101
    ("UnauthorizedAttester", ContractError::UnauthorizedAttester),           // 102
    ("NotOriginalAttester", ContractError::NotOriginalAttester),             // 103
    ("NotSigner", ContractError::NotSigner),                                 // 104
    ("UnauthorizedDepositor", ContractError::UnauthorizedDepositor),         // 105
    ("ContractPaused", ContractError::ContractPaused),                       // 106
    ("InvalidPauseAction", ContractError::InvalidPauseAction),               // 107
    ("InsufficientSignatures", ContractError::InsufficientSignatures),       // 108
    ("ZeroBytes32", ContractError::ZeroBytes32),                             // 109
    ("InvalidAdminAddress", ContractError::InvalidAdminAddress),             // 110
    ("AdminUnchanged", ContractError::AdminUnchanged),                       // 111
    ("TimelockNotReady", ContractError::TimelockNotReady),                   // 112
    ("AdminSuspended", ContractError::AdminSuspended),                       // 113
    ("BorrowFrozen", ContractError::BorrowFrozen),                           // 114
    ("NoPendingAdmin", ContractError::NoPendingAdmin),                       // 115
    ("RoleNotHeldAtLedger", ContractError::RoleNotHeldAtLedger),             // 116
    ("EmergencyDrainNotPermitted", ContractError::EmergencyDrainNotPermitted), // 117
    ("TimestampInFuture", ContractError::TimestampInFuture),                 // 118
    ("InvalidMaxPauseSigners", ContractError::InvalidMaxPauseSigners),       // 119
    ("OutsideBusinessHours", ContractError::OutsideBusinessHours),           // 120
    ("LeaseScopeMismatch", ContractError::LeaseScopeMismatch),               // 121
    ("LeaseExpired", ContractError::LeaseExpired),                           // 122
    ("CrossContractCallerMismatch", ContractError::CrossContractCallerMismatch), // 123
    ("MigrationInProgress", ContractError::MigrationInProgress),             // 124
    ("MaxPauseSignersExceeded", ContractError::MaxPauseSignersExceeded),     // 125
    // --- Bond (200-299) ---
    ("BondNotFound", ContractError::BondNotFound),                           // 200
    ("BondNotActive", ContractError::BondNotActive),                         // 201
    ("InsufficientBalance", ContractError::InsufficientBalance),             // 202
    ("SlashExceedsBond", ContractError::SlashExceedsBond),                   // 203
    ("LockupNotExpired", ContractError::LockupNotExpired),                   // 204
    ("NotRollingBond", ContractError::NotRollingBond),                       // 205
    ("WithdrawalAlreadyRequested", ContractError::WithdrawalAlreadyRequested), // 206
    ("ReentrancyDetected", ContractError::ReentrancyDetected),               // 207
    ("InvalidNonce", ContractError::InvalidNonce),                           // 208
    ("NegativeStake", ContractError::NegativeStake),                         // 209
    ("EarlyExitConfigNotSet", ContractError::EarlyExitConfigNotSet),         // 210
    ("InvalidPenaltyBps", ContractError::InvalidPenaltyBps),                 // 211
    ("LeverageExceeded", ContractError::LeverageExceeded),                   // 212
    ("UnsupportedToken", ContractError::UnsupportedToken),                   // 213
    ("InvalidBondAmount", ContractError::InvalidBondAmount),                 // 214
    ("AmountExplicitlyZero", ContractError::AmountExplicitlyZero),           // 215
    ("InvalidBondDuration", ContractError::InvalidBondDuration),             // 216
    ("InvalidNoticePeriod", ContractError::InvalidNoticePeriod),             // 217
    ("BondAlreadyExists", ContractError::BondAlreadyExists),                 // 218
    // Codes 219, 220, 221, 225 — shared Bond/Delegation payload mismatches.
    ("OwnerMismatch", ContractError::OwnerMismatch),                         // 219
    ("TargetMismatch", ContractError::TargetMismatch),                       // 220
    ("ContractIdMismatch", ContractError::ContractIdMismatch),               // 221
    ("SignatureExpired", ContractError::SignatureExpired),                    // 222
    ("TreasuryNotConfigured", ContractError::TreasuryNotConfigured),         // 223
    ("StorageCapReached", ContractError::StorageCapReached),                 // 224
    ("DomainMismatch", ContractError::DomainMismatch),                       // 225
    ("CursorOutOfRange", ContractError::CursorOutOfRange),                   // 226
    ("BatchTooLarge", ContractError::BatchTooLarge),                         // 227
    ("EmptyBatch", ContractError::EmptyBatch),                               // 228
    ("UnsupportedDecimals", ContractError::UnsupportedDecimals),             // 229
    ("InvalidStringifiedBytes", ContractError::InvalidStringifiedBytes),     // 230
    ("UnauthorizedToken", ContractError::UnauthorizedToken),                 // 231
    ("DuplicateIdempotencyKey", ContractError::DuplicateIdempotencyKey),     // 232
    ("InvariantViolation", ContractError::InvariantViolation),               // 233
    ("InvalidCurrency", ContractError::InvalidCurrency),                     // 234
    ("SnapshotGenerationMismatch", ContractError::SnapshotGenerationMismatch), // 235
    // --- Attestation (300-399) ---
    ("DuplicateAttestation", ContractError::DuplicateAttestation),           // 300
    ("AttestationNotFound", ContractError::AttestationNotFound),             // 301
    ("AttestationAlreadyRevoked", ContractError::AttestationAlreadyRevoked), // 302
    ("InvalidAttestationWeight", ContractError::InvalidAttestationWeight),   // 303
    ("AttestationWeightExceedsMax", ContractError::AttestationWeightExceedsMax), // 304
    // --- Registry (400-499) ---
    ("IdentityAlreadyRegistered", ContractError::IdentityAlreadyRegistered), // 400
    ("BondContractAlreadyRegistered", ContractError::BondContractAlreadyRegistered), // 401
    ("IdentityNotRegistered", ContractError::IdentityNotRegistered),         // 402
    ("BondContractNotRegistered", ContractError::BondContractNotRegistered), // 403
    ("AlreadyDeactivated", ContractError::AlreadyDeactivated),               // 404
    ("AlreadyActive", ContractError::AlreadyActive),                         // 405
    ("InvalidContractAddress", ContractError::InvalidContractAddress),       // 406
    ("ContractCodeVerificationFailed", ContractError::ContractCodeVerificationFailed), // 407
    ("UnsupportedInterface", ContractError::UnsupportedInterface),           // 408
    // --- Delegation (500-599) ---
    ("ExpiryInPast", ContractError::ExpiryInPast),                           // 500
    ("DelegationNotFound", ContractError::DelegationNotFound),               // 501
    ("AlreadyRevoked", ContractError::AlreadyRevoked),                       // 502
    ("DelegationExpiryTooLong", ContractError::DelegationExpiryTooLong),     // 503
    ("UnknownScheme", ContractError::UnknownScheme),                         // 504
    ("VerifierAlreadyRegistered", ContractError::VerifierAlreadyRegistered), // 505
    ("VerifierNotRegistered", ContractError::VerifierNotRegistered),         // 506
    ("VerificationFailed", ContractError::VerificationFailed),               // 507
    ("RevocationGraceExpired", ContractError::RevocationGraceExpired),       // 508
    ("DelegationNotExpired", ContractError::DelegationNotExpired),           // 509
    ("PayloadTooOld", ContractError::PayloadTooOld),                         // 510
    ("DelegationInactive", ContractError::DelegationInactive),               // 511
    ("PromiseNotKept", ContractError::PromiseNotKept),                       // 512
    ("StaleEpoch", ContractError::StaleEpoch),                               // 513
    ("StaleAdminEpoch", ContractError::StaleAdminEpoch),                     // 514
    ("StaleSignerEpoch", ContractError::StaleSignerEpoch),                   // 515
    // --- Treasury (600-699) ---
    ("AmountMustBePositive", ContractError::AmountMustBePositive),           // 600
    ("ThresholdExceedsSigners", ContractError::ThresholdExceedsSigners),     // 601
    ("InsufficientTreasuryBalance", ContractError::InsufficientTreasuryBalance), // 602
    ("ProposalNotFound", ContractError::ProposalNotFound),                   // 603
    ("ProposalAlreadyExecuted", ContractError::ProposalAlreadyExecuted),     // 604
    ("InsufficientApprovals", ContractError::InsufficientApprovals),         // 605
    ("InvalidFlashLoanCallback", ContractError::InvalidFlashLoanCallback),   // 606
    ("FlashLoanRepaymentFailed", ContractError::FlashLoanRepaymentFailed),   // 607
    ("ProposalExpired", ContractError::ProposalExpired),                     // 608
    ("SlippageExceeded", ContractError::SlippageExceeded),                   // 609
    ("TreasuryBeneficiaryMismatch", ContractError::TreasuryBeneficiaryMismatch), // 610
    ("CorridorNotRegistered", ContractError::CorridorNotRegistered),         // 611
    // --- Arithmetic (700-799) ---
    ("Overflow", ContractError::Overflow),                                   // 700
    ("Underflow", ContractError::Underflow),                                 // 701
    ("DivisionByZero", ContractError::DivisionByZero),                       // 702
    ("InvalidPercentSplit", ContractError::InvalidPercentSplit),             // 703
];
