use credence_errors::ContractError;

#[test]
fn test_error_code_wire_stability() {
    assert_eq!(ContractError::NotInitialized as u32, 1);
    assert_eq!(ContractError::AlreadyInitialized as u32, 2);

    assert_eq!(ContractError::NotAdmin as u32, 100);
    assert_eq!(ContractError::NotBondOwner as u32, 101);
    assert_eq!(ContractError::UnauthorizedAttester as u32, 102);
    assert_eq!(ContractError::NotOriginalAttester as u32, 103);
    assert_eq!(ContractError::NotSigner as u32, 104);
    assert_eq!(ContractError::UnauthorizedDepositor as u32, 105);
    assert_eq!(ContractError::ContractPaused as u32, 106);
    assert_eq!(ContractError::InvalidPauseAction as u32, 107);
    assert_eq!(ContractError::InsufficientSignatures as u32, 108);

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
    assert_eq!(ContractError::UnsupportedDecimals as u32, 229);
    assert_eq!(ContractError::InvalidBondAmount as u32, 214);
    assert_eq!(ContractError::InvalidBondDuration as u32, 215);
    assert_eq!(ContractError::InvalidNoticePeriod as u32, 216);
    assert_eq!(ContractError::BondAlreadyExists as u32, 217);
    assert_eq!(ContractError::InvariantViolation as u32, 218);
    assert_eq!(ContractError::TreasuryNotConfigured as u32, 223);

    assert_eq!(ContractError::DuplicateAttestation as u32, 300);
    assert_eq!(ContractError::AttestationNotFound as u32, 301);
    assert_eq!(ContractError::AttestationAlreadyRevoked as u32, 302);
    assert_eq!(ContractError::InvalidAttestationWeight as u32, 303);
    assert_eq!(ContractError::AttestationWeightExceedsMax as u32, 304);

    assert_eq!(ContractError::IdentityAlreadyRegistered as u32, 400);
    assert_eq!(ContractError::BondContractAlreadyRegistered as u32, 401);
    assert_eq!(ContractError::IdentityNotRegistered as u32, 402);
    assert_eq!(ContractError::BondContractNotRegistered as u32, 403);
    assert_eq!(ContractError::AlreadyDeactivated as u32, 404);
    assert_eq!(ContractError::AlreadyActive as u32, 405);
    assert_eq!(ContractError::InvalidContractAddress as u32, 406);
    assert_eq!(ContractError::ContractCodeVerificationFailed as u32, 407);

    assert_eq!(ContractError::ExpiryInPast as u32, 500);
    assert_eq!(ContractError::DelegationNotFound as u32, 501);
    assert_eq!(ContractError::AlreadyRevoked as u32, 502);
    assert_eq!(ContractError::DelegationExpiryTooLong as u32, 503);
    assert_eq!(ContractError::PayloadTooOld as u32, 510);

    assert_eq!(ContractError::AmountMustBePositive as u32, 600);
    assert_eq!(ContractError::ThresholdExceedsSigners as u32, 601);
    assert_eq!(ContractError::InsufficientTreasuryBalance as u32, 602);
    assert_eq!(ContractError::ProposalNotFound as u32, 603);
    assert_eq!(ContractError::ProposalAlreadyExecuted as u32, 604);
    assert_eq!(ContractError::InsufficientApprovals as u32, 605);
    assert_eq!(ContractError::InvalidFlashLoanCallback as u32, 606);
    assert_eq!(ContractError::FlashLoanRepaymentFailed as u32, 607);
    assert_eq!(ContractError::ProposalExpired as u32, 608);

    assert_eq!(ContractError::Overflow as u32, 700);
    assert_eq!(ContractError::Underflow as u32, 701);
}
