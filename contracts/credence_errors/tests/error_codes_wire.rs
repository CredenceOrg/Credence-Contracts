use credence_errors::ContractError;

/// Wire-stable code regression tests.
///
/// Every variant's discriminant is locked here. A change to any wire code
/// is a breaking change for off-chain clients (indexers, dashboards, CLI)
/// and must never happen after deployment.
#[test]
fn test_error_code_wire_stability() {
    // Initialization (1-99)
    assert_eq!(ContractError::NotInitialized as u32, 1);
    assert_eq!(ContractError::AlreadyInitialized as u32, 2);

    // Authorization (100-199)
    assert_eq!(ContractError::NotAdmin as u32, 100);
    assert_eq!(ContractError::NotBondOwner as u32, 101);
    assert_eq!(ContractError::UnauthorizedAttester as u32, 102);
    assert_eq!(ContractError::NotOriginalAttester as u32, 103);
    assert_eq!(ContractError::NotSigner as u32, 104);
    assert_eq!(ContractError::UnauthorizedDepositor as u32, 105);
    assert_eq!(ContractError::ContractPaused as u32, 106);
    assert_eq!(ContractError::InvalidPauseAction as u32, 107);
    assert_eq!(ContractError::InsufficientSignatures as u32, 108);
    assert_eq!(ContractError::ZeroBytes32 as u32, 109);
    assert_eq!(ContractError::InvalidAdminAddress as u32, 110);
    assert_eq!(ContractError::AdminUnchanged as u32, 111);
    assert_eq!(ContractError::TimelockNotReady as u32, 112);
    assert_eq!(ContractError::AdminSuspended as u32, 113);
    assert_eq!(ContractError::BorrowFrozen as u32, 114);
    assert_eq!(ContractError::NoPendingAdmin as u32, 115);
    assert_eq!(ContractError::RoleNotHeldAtLedger as u32, 116);
    assert_eq!(ContractError::EmergencyDrainNotPermitted as u32, 117);
    assert_eq!(ContractError::TimestampInFuture as u32, 118);
    assert_eq!(ContractError::InvalidMaxPauseSigners as u32, 119);
    assert_eq!(ContractError::OutsideBusinessHours as u32, 120);
    assert_eq!(ContractError::LeaseScopeMismatch as u32, 121);
    assert_eq!(ContractError::LeaseExpired as u32, 122);
    assert_eq!(ContractError::CrossContractCallerMismatch as u32, 123);
    assert_eq!(ContractError::MigrationInProgress as u32, 124);
    assert_eq!(ContractError::MaxPauseSignersExceeded as u32, 125);
    assert_eq!(ContractError::LeaseSignerMismatch as u32, 126);
    assert_eq!(ContractError::RoleRequired as u32, 127);

    // Bond (200-299)
    assert_eq!(ContractError::BondNotFound as u32, 200);
    assert_eq!(ContractError::BondNotActive as u32, 201);
    assert_eq!(ContractError::InsufficientBalance as u32, 202);
    assert_eq!(ContractError::SlashExceedsBond as u32, 203);
    assert_eq!(ContractError::LockupNotExpired as u32, 204);
    assert_eq!(ContractError::NotRollingBond as u32, 205);
    assert_eq!(ContractError::WithdrawalAlreadyRequested as u32, 206);
    assert_eq!(ContractError::ReentrancyDetected as u32, 207);
    assert_eq!(ContractError::InvalidNonce as u32, 208);
    assert_eq!(ContractError::NegativeStake as u32, 209);
    assert_eq!(ContractError::EarlyExitConfigNotSet as u32, 210);
    assert_eq!(ContractError::InvalidPenaltyBps as u32, 211);
    assert_eq!(ContractError::LeverageExceeded as u32, 212);
    assert_eq!(ContractError::UnsupportedToken as u32, 213);
    assert_eq!(ContractError::InvalidBondAmount as u32, 214);
    assert_eq!(ContractError::AmountExplicitlyZero as u32, 215);
    assert_eq!(ContractError::InvalidBondDuration as u32, 216);
    assert_eq!(ContractError::InvalidNoticePeriod as u32, 217);
    assert_eq!(ContractError::BondAlreadyExists as u32, 218);
    assert_eq!(ContractError::OwnerMismatch as u32, 219);
    assert_eq!(ContractError::TargetMismatch as u32, 220);
    assert_eq!(ContractError::ContractIdMismatch as u32, 221);
    assert_eq!(ContractError::SignatureExpired as u32, 222);
    assert_eq!(ContractError::TreasuryNotConfigured as u32, 223);
    assert_eq!(ContractError::StorageCapReached as u32, 224);
    assert_eq!(ContractError::DomainMismatch as u32, 225);
    assert_eq!(ContractError::CursorOutOfRange as u32, 226);
    assert_eq!(ContractError::BatchTooLarge as u32, 227);
    assert_eq!(ContractError::EmptyBatch as u32, 228);
    assert_eq!(ContractError::UnsupportedDecimals as u32, 229);
    assert_eq!(ContractError::InvalidStringifiedBytes as u32, 230);
    assert_eq!(ContractError::UnauthorizedToken as u32, 231);
    assert_eq!(ContractError::DuplicateIdempotencyKey as u32, 232);
    assert_eq!(ContractError::InvariantViolation as u32, 233);
    assert_eq!(ContractError::InvalidCurrency as u32, 234);
    assert_eq!(ContractError::SnapshotGenerationMismatch as u32, 235);
    assert_eq!(ContractError::CooldownRequestAlreadyPending as u32, 236);
    assert_eq!(ContractError::CooldownRequestNotFound as u32, 237);
    assert_eq!(ContractError::CooldownPeriodNotElapsed as u32, 238);
    assert_eq!(ContractError::BytesTooLarge as u32, 239);

    // Attestation (300-399)
    assert_eq!(ContractError::DuplicateAttestation as u32, 300);
    assert_eq!(ContractError::AttestationNotFound as u32, 301);
    assert_eq!(ContractError::AttestationAlreadyRevoked as u32, 302);
    assert_eq!(ContractError::InvalidAttestationWeight as u32, 303);
    assert_eq!(ContractError::AttestationWeightExceedsMax as u32, 304);

    // Registry (400-499)
    assert_eq!(ContractError::IdentityAlreadyRegistered as u32, 400);
    assert_eq!(ContractError::BondContractAlreadyRegistered as u32, 401);
    assert_eq!(ContractError::IdentityNotRegistered as u32, 402);
    assert_eq!(ContractError::BondContractNotRegistered as u32, 403);
    assert_eq!(ContractError::AlreadyDeactivated as u32, 404);
    assert_eq!(ContractError::AlreadyActive as u32, 405);
    assert_eq!(ContractError::InvalidContractAddress as u32, 406);
    assert_eq!(ContractError::ContractCodeVerificationFailed as u32, 407);
    assert_eq!(ContractError::UnsupportedInterface as u32, 408);

    // Delegation (500-599)
    assert_eq!(ContractError::ExpiryInPast as u32, 500);
    assert_eq!(ContractError::DelegationNotFound as u32, 501);
    assert_eq!(ContractError::AlreadyRevoked as u32, 502);
    assert_eq!(ContractError::DelegationExpiryTooLong as u32, 503);
    assert_eq!(ContractError::UnknownScheme as u32, 504);
    assert_eq!(ContractError::VerifierAlreadyRegistered as u32, 505);
    assert_eq!(ContractError::VerifierNotRegistered as u32, 506);
    assert_eq!(ContractError::VerificationFailed as u32, 507);
    assert_eq!(ContractError::RevocationGraceExpired as u32, 508);
    assert_eq!(ContractError::DelegationNotExpired as u32, 509);
    assert_eq!(ContractError::PayloadTooOld as u32, 510);
    assert_eq!(ContractError::DelegationInactive as u32, 511);
    assert_eq!(ContractError::PromiseNotKept as u32, 512);
    assert_eq!(ContractError::StaleEpoch as u32, 513);
    assert_eq!(ContractError::StaleAdminEpoch as u32, 514);
    assert_eq!(ContractError::StaleSignerEpoch as u32, 515);

    // Treasury (600-699)
    assert_eq!(ContractError::AmountMustBePositive as u32, 600);
    assert_eq!(ContractError::ThresholdExceedsSigners as u32, 601);
    assert_eq!(ContractError::InsufficientTreasuryBalance as u32, 602);
    assert_eq!(ContractError::ProposalNotFound as u32, 603);
    assert_eq!(ContractError::ProposalAlreadyExecuted as u32, 604);
    assert_eq!(ContractError::InsufficientApprovals as u32, 605);
    assert_eq!(ContractError::InvalidFlashLoanCallback as u32, 606);
    assert_eq!(ContractError::FlashLoanRepaymentFailed as u32, 607);
    assert_eq!(ContractError::ProposalExpired as u32, 608);
    assert_eq!(ContractError::SlippageExceeded as u32, 609);
    assert_eq!(ContractError::TreasuryBeneficiaryMismatch as u32, 610);
    assert_eq!(ContractError::CorridorNotRegistered as u32, 611);

    // Arithmetic (700-799)
    assert_eq!(ContractError::Overflow as u32, 700);
    assert_eq!(ContractError::Underflow as u32, 701);
    assert_eq!(ContractError::DivisionByZero as u32, 702);
    assert_eq!(ContractError::InvalidPercentSplit as u32, 703);
}
