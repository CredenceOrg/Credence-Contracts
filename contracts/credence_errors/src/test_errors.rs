#[cfg(test)]
mod tests {
    extern crate std;
    use crate::{ContractError, ErrorCategory, ErrorExt};
    use std::vec::Vec;

    fn all_variants() -> Vec<ContractError> {
        std::vec![
            ContractError::NotInitialized,
            ContractError::AlreadyInitialized,
            ContractError::NotAdmin,
            ContractError::NotBondOwner,
            ContractError::UnauthorizedAttester,
            ContractError::NotOriginalAttester,
            ContractError::NotSigner,
            ContractError::UnauthorizedDepositor,
            ContractError::BondNotFound,
            ContractError::BondNotActive,
            ContractError::InsufficientBalance,
            ContractError::SlashExceedsBond,
            ContractError::LockupNotExpired,
            ContractError::NotRollingBond,
            ContractError::WithdrawalAlreadyRequested,
            ContractError::ReentrancyDetected,
            ContractError::InvalidNonce,
            ContractError::NegativeStake,
            ContractError::EarlyExitConfigNotSet,
            ContractError::InvalidPenaltyBps,
            ContractError::DuplicateAttestation,
            ContractError::AttestationNotFound,
            ContractError::AttestationAlreadyRevoked,
            ContractError::InvalidAttestationWeight,
            ContractError::AttestationWeightExceedsMax,
            ContractError::IdentityAlreadyRegistered,
            ContractError::BondContractAlreadyRegistered,
            ContractError::IdentityNotRegistered,
            ContractError::BondContractNotRegistered,
            ContractError::AlreadyDeactivated,
            ContractError::AlreadyActive,
            ContractError::ExpiryInPast,
            ContractError::DelegationNotFound,
            ContractError::AlreadyRevoked,
            ContractError::AmountMustBePositive,
            ContractError::ThresholdExceedsSigners,
            ContractError::InsufficientTreasuryBalance,
            ContractError::ProposalNotFound,
            ContractError::ProposalAlreadyExecuted,
            ContractError::InsufficientApprovals,
            ContractError::Overflow,
            ContractError::Underflow,
        ]
    }

    // --- Wire code tests ---

    #[test]
    fn test_codes_initialization() {
        assert_eq!(ContractError::NotInitialized as u32, 1);
        assert_eq!(ContractError::AlreadyInitialized as u32, 2);
    }

    #[test]
    fn test_codes_authorization() {
        assert_eq!(ContractError::NotAdmin as u32, 100);
        assert_eq!(ContractError::NotBondOwner as u32, 101);
        assert_eq!(ContractError::UnauthorizedAttester as u32, 102);
        assert_eq!(ContractError::NotOriginalAttester as u32, 103);
        assert_eq!(ContractError::NotSigner as u32, 104);
        assert_eq!(ContractError::UnauthorizedDepositor as u32, 105);
    }

    #[test]
    fn test_codes_bond() {
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
    }

    #[test]
    fn test_codes_attestation() {
        assert_eq!(ContractError::DuplicateAttestation as u32, 300);
        assert_eq!(ContractError::AttestationNotFound as u32, 301);
        assert_eq!(ContractError::AttestationAlreadyRevoked as u32, 302);
        assert_eq!(ContractError::InvalidAttestationWeight as u32, 303);
        assert_eq!(ContractError::AttestationWeightExceedsMax as u32, 304);
    }

    #[test]
    fn test_codes_registry() {
        assert_eq!(ContractError::IdentityAlreadyRegistered as u32, 400);
        assert_eq!(ContractError::BondContractAlreadyRegistered as u32, 401);
        assert_eq!(ContractError::IdentityNotRegistered as u32, 402);
        assert_eq!(ContractError::BondContractNotRegistered as u32, 403);
        assert_eq!(ContractError::AlreadyDeactivated as u32, 404);
        assert_eq!(ContractError::AlreadyActive as u32, 405);
    }

    #[test]
    fn test_codes_delegation() {
        assert_eq!(ContractError::ExpiryInPast as u32, 500);
        assert_eq!(ContractError::DelegationNotFound as u32, 501);
        assert_eq!(ContractError::AlreadyRevoked as u32, 502);
    }

    #[test]
    fn test_codes_treasury() {
        assert_eq!(ContractError::AmountMustBePositive as u32, 600);
        assert_eq!(ContractError::ThresholdExceedsSigners as u32, 601);
        assert_eq!(ContractError::InsufficientTreasuryBalance as u32, 602);
        assert_eq!(ContractError::ProposalNotFound as u32, 603);
        assert_eq!(ContractError::ProposalAlreadyExecuted as u32, 604);
        assert_eq!(ContractError::InsufficientApprovals as u32, 605);
    }

    #[test]
    fn test_codes_arithmetic() {
        assert_eq!(ContractError::Overflow as u32, 700);
        assert_eq!(ContractError::Underflow as u32, 701);
    }

    // --- Category mapping tests ---

    #[test]
    fn test_category_initialization() {
        assert_eq!(
            ContractError::NotInitialized.category(),
            ErrorCategory::Initialization
        );
        assert_eq!(
            ContractError::AlreadyInitialized.category(),
            ErrorCategory::Initialization
        );
    }

    #[test]
    fn test_category_authorization() {
        assert_eq!(
            ContractError::NotAdmin.category(),
            ErrorCategory::Authorization
        );
        assert_eq!(
            ContractError::NotBondOwner.category(),
            ErrorCategory::Authorization
        );
        assert_eq!(
            ContractError::UnauthorizedAttester.category(),
            ErrorCategory::Authorization
        );
        assert_eq!(
            ContractError::NotOriginalAttester.category(),
            ErrorCategory::Authorization
        );
        assert_eq!(
            ContractError::NotSigner.category(),
            ErrorCategory::Authorization
        );
        assert_eq!(
            ContractError::UnauthorizedDepositor.category(),
            ErrorCategory::Authorization
        );
    }

    #[test]
    fn test_category_bond() {
        assert_eq!(ContractError::BondNotFound.category(), ErrorCategory::Bond);
        assert_eq!(ContractError::BondNotActive.category(), ErrorCategory::Bond);
        assert_eq!(
            ContractError::InsufficientBalance.category(),
            ErrorCategory::Bond
        );
        assert_eq!(
            ContractError::SlashExceedsBond.category(),
            ErrorCategory::Bond
        );
        assert_eq!(
            ContractError::LockupNotExpired.category(),
            ErrorCategory::Bond
        );
        assert_eq!(
            ContractError::NotRollingBond.category(),
            ErrorCategory::Bond
        );
        assert_eq!(
            ContractError::WithdrawalAlreadyRequested.category(),
            ErrorCategory::Bond
        );
        assert_eq!(
            ContractError::ReentrancyDetected.category(),
            ErrorCategory::Bond
        );
        assert_eq!(ContractError::InvalidNonce.category(), ErrorCategory::Bond);
        assert_eq!(ContractError::NegativeStake.category(), ErrorCategory::Bond);
        assert_eq!(
            ContractError::EarlyExitConfigNotSet.category(),
            ErrorCategory::Bond
        );
        assert_eq!(
            ContractError::InvalidPenaltyBps.category(),
            ErrorCategory::Bond
        );
    }

    #[test]
    fn test_category_attestation() {
        assert_eq!(
            ContractError::DuplicateAttestation.category(),
            ErrorCategory::Attestation
        );
        assert_eq!(
            ContractError::AttestationNotFound.category(),
            ErrorCategory::Attestation
        );
        assert_eq!(
            ContractError::AttestationAlreadyRevoked.category(),
            ErrorCategory::Attestation
        );
        assert_eq!(
            ContractError::InvalidAttestationWeight.category(),
            ErrorCategory::Attestation
        );
        assert_eq!(
            ContractError::AttestationWeightExceedsMax.category(),
            ErrorCategory::Attestation
        );
    }

    #[test]
    fn test_category_registry() {
        assert_eq!(
            ContractError::IdentityAlreadyRegistered.category(),
            ErrorCategory::Registry
        );
        assert_eq!(
            ContractError::BondContractAlreadyRegistered.category(),
            ErrorCategory::Registry
        );
        assert_eq!(
            ContractError::IdentityNotRegistered.category(),
            ErrorCategory::Registry
        );
        assert_eq!(
            ContractError::BondContractNotRegistered.category(),
            ErrorCategory::Registry
        );
        assert_eq!(
            ContractError::AlreadyDeactivated.category(),
            ErrorCategory::Registry
        );
        assert_eq!(
            ContractError::AlreadyActive.category(),
            ErrorCategory::Registry
        );
    }

    #[test]
    fn test_category_delegation() {
        assert_eq!(
            ContractError::ExpiryInPast.category(),
            ErrorCategory::Delegation
        );
        assert_eq!(
            ContractError::DelegationNotFound.category(),
            ErrorCategory::Delegation
        );
        assert_eq!(
            ContractError::AlreadyRevoked.category(),
            ErrorCategory::Delegation
        );
    }

    #[test]
    fn test_category_treasury() {
        assert_eq!(
            ContractError::AmountMustBePositive.category(),
            ErrorCategory::Treasury
        );
        assert_eq!(
            ContractError::ThresholdExceedsSigners.category(),
            ErrorCategory::Treasury
        );
        assert_eq!(
            ContractError::InsufficientTreasuryBalance.category(),
            ErrorCategory::Treasury
        );
        assert_eq!(
            ContractError::ProposalNotFound.category(),
            ErrorCategory::Treasury
        );
        assert_eq!(
            ContractError::ProposalAlreadyExecuted.category(),
            ErrorCategory::Treasury
        );
        assert_eq!(
            ContractError::InsufficientApprovals.category(),
            ErrorCategory::Treasury
        );
    }

    #[test]
    fn test_category_arithmetic() {
        assert_eq!(
            ContractError::Overflow.category(),
            ErrorCategory::Arithmetic
        );
        assert_eq!(
            ContractError::Underflow.category(),
            ErrorCategory::Arithmetic
        );
    }

    // --- Description tests ---

    #[test]
    fn test_descriptions_non_empty() {
        for e in all_variants() {
            assert!(!e.description().is_empty(), "{:?} has empty description", e);
        }
    }

    #[test]
    fn test_descriptions_unique() {
        let variants = all_variants();
        for i in 0..variants.len() {
            for j in (i + 1)..variants.len() {
                assert_ne!(variants[i].description(), variants[j].description());
            }
        }
    }

    // --- Variant count guard ---

    #[test]
    fn test_all_variants_count() {
        assert_eq!(
            all_variants().len(),
            42,
            "Update all_variants() and this count when adding new errors"
        );
    }

    // --- Copy and Eq tests ---

    #[test]
    fn test_copy_semantics() {
        let a = ContractError::BondNotFound;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn test_equality() {
        assert_eq!(ContractError::NotAdmin, ContractError::NotAdmin);
        assert_ne!(ContractError::NotAdmin, ContractError::NotBondOwner);
    }

    // --- Result integration tests (mirrors real contract call sites) ---

    // initialization
    fn mock_require_init(initialized: bool) -> Result<(), ContractError> {
        if !initialized {
            return Err(ContractError::NotInitialized);
        }
        Ok(())
    }

    fn mock_init_once(already: bool) -> Result<(), ContractError> {
        if already {
            return Err(ContractError::AlreadyInitialized);
        }
        Ok(())
    }

    #[test]
    fn test_not_initialized() {
        assert_eq!(mock_require_init(false), Err(ContractError::NotInitialized));
        assert!(mock_require_init(true).is_ok());
    }

    #[test]
    fn test_already_initialized() {
        assert_eq!(mock_init_once(true), Err(ContractError::AlreadyInitialized));
        assert!(mock_init_once(false).is_ok());
    }

    // authorization
    fn mock_admin(is_admin: bool) -> Result<(), ContractError> {
        if !is_admin {
            return Err(ContractError::NotAdmin);
        }
        Ok(())
    }

    fn mock_bond_owner(is_owner: bool) -> Result<(), ContractError> {
        if !is_owner {
            return Err(ContractError::NotBondOwner);
        }
        Ok(())
    }

    fn mock_attester(authorized: bool) -> Result<(), ContractError> {
        if !authorized {
            return Err(ContractError::UnauthorizedAttester);
        }
        Ok(())
    }

    fn mock_signer(is_signer: bool) -> Result<(), ContractError> {
        if !is_signer {
            return Err(ContractError::NotSigner);
        }
        Ok(())
    }

    fn mock_depositor(authorized: bool) -> Result<(), ContractError> {
        if !authorized {
            return Err(ContractError::UnauthorizedDepositor);
        }
        Ok(())
    }

    #[test]
    fn test_not_admin() {
        assert_eq!(mock_admin(false), Err(ContractError::NotAdmin));
        assert!(mock_admin(true).is_ok());
    }

    #[test]
    fn test_not_bond_owner() {
        assert_eq!(mock_bond_owner(false), Err(ContractError::NotBondOwner));
        assert!(mock_bond_owner(true).is_ok());
    }

    #[test]
    fn test_unauthorized_attester() {
        assert_eq!(
            mock_attester(false),
            Err(ContractError::UnauthorizedAttester)
        );
        assert!(mock_attester(true).is_ok());
    }

    #[test]
    fn test_not_signer() {
        assert_eq!(mock_signer(false), Err(ContractError::NotSigner));
        assert!(mock_signer(true).is_ok());
    }

    #[test]
    fn test_unauthorized_depositor() {
        assert_eq!(
            mock_depositor(false),
            Err(ContractError::UnauthorizedDepositor)
        );
        assert!(mock_depositor(true).is_ok());
    }

    // bond
    fn mock_get_bond(exists: bool) -> Result<(), ContractError> {
        if !exists {
            return Err(ContractError::BondNotFound);
        }
        Ok(())
    }

    fn mock_bond_active(active: bool) -> Result<(), ContractError> {
        if !active {
            return Err(ContractError::BondNotActive);
        }
        Ok(())
    }

    fn mock_balance(enough: bool) -> Result<(), ContractError> {
        if !enough {
            return Err(ContractError::InsufficientBalance);
        }
        Ok(())
    }

    fn mock_slash(slash: i128, bonded: i128) -> Result<(), ContractError> {
        if slash > bonded {
            return Err(ContractError::SlashExceedsBond);
        }
        Ok(())
    }

    fn mock_lockup(expired: bool) -> Result<(), ContractError> {
        if !expired {
            return Err(ContractError::LockupNotExpired);
        }
        Ok(())
    }

    fn mock_rolling(is_rolling: bool) -> Result<(), ContractError> {
        if !is_rolling {
            return Err(ContractError::NotRollingBond);
        }
        Ok(())
    }

    fn mock_withdrawal_requested(already: bool) -> Result<(), ContractError> {
        if already {
            return Err(ContractError::WithdrawalAlreadyRequested);
        }
        Ok(())
    }

    fn mock_reentrancy(locked: bool) -> Result<(), ContractError> {
        if locked {
            return Err(ContractError::ReentrancyDetected);
        }
        Ok(())
    }

    fn mock_nonce(valid: bool) -> Result<(), ContractError> {
        if !valid {
            return Err(ContractError::InvalidNonce);
        }
        Ok(())
    }

    fn mock_stake(new_stake: i128) -> Result<(), ContractError> {
        if new_stake < 0 {
            return Err(ContractError::NegativeStake);
        }
        Ok(())
    }

    fn mock_early_exit(config_set: bool) -> Result<(), ContractError> {
        if !config_set {
            return Err(ContractError::EarlyExitConfigNotSet);
        }
        Ok(())
    }

    fn mock_penalty_bps(bps: u32) -> Result<(), ContractError> {
        if bps > 10_000 {
            return Err(ContractError::InvalidPenaltyBps);
        }
        Ok(())
    }

    #[test]
    fn test_bond_not_found() {
        assert_eq!(mock_get_bond(false), Err(ContractError::BondNotFound));
        assert!(mock_get_bond(true).is_ok());
    }

    #[test]
    fn test_bond_not_active() {
        assert_eq!(mock_bond_active(false), Err(ContractError::BondNotActive));
        assert!(mock_bond_active(true).is_ok());
    }

    #[test]
    fn test_insufficient_balance() {
        assert_eq!(mock_balance(false), Err(ContractError::InsufficientBalance));
        assert!(mock_balance(true).is_ok());
    }

    #[test]
    fn test_slash_exceeds_bond() {
        assert_eq!(mock_slash(101, 100), Err(ContractError::SlashExceedsBond));
        assert!(mock_slash(100, 100).is_ok());
    }

    #[test]
    fn test_lockup_not_expired() {
        assert_eq!(mock_lockup(false), Err(ContractError::LockupNotExpired));
        assert!(mock_lockup(true).is_ok());
    }

    #[test]
    fn test_not_rolling_bond() {
        assert_eq!(mock_rolling(false), Err(ContractError::NotRollingBond));
        assert!(mock_rolling(true).is_ok());
    }

    #[test]
    fn test_withdrawal_already_requested() {
        assert_eq!(
            mock_withdrawal_requested(true),
            Err(ContractError::WithdrawalAlreadyRequested)
        );
        assert!(mock_withdrawal_requested(false).is_ok());
    }

    #[test]
    fn test_reentrancy_detected() {
        assert_eq!(
            mock_reentrancy(true),
            Err(ContractError::ReentrancyDetected)
        );
        assert!(mock_reentrancy(false).is_ok());
    }

    #[test]
    fn test_invalid_nonce() {
        assert_eq!(mock_nonce(false), Err(ContractError::InvalidNonce));
        assert!(mock_nonce(true).is_ok());
    }

    #[test]
    fn test_negative_stake() {
        assert_eq!(mock_stake(-1), Err(ContractError::NegativeStake));
        assert!(mock_stake(0).is_ok());
    }

    #[test]
    fn test_early_exit_config_not_set() {
        assert_eq!(
            mock_early_exit(false),
            Err(ContractError::EarlyExitConfigNotSet)
        );
        assert!(mock_early_exit(true).is_ok());
    }

    #[test]
    fn test_invalid_penalty_bps() {
        assert_eq!(
            mock_penalty_bps(10_001),
            Err(ContractError::InvalidPenaltyBps)
        );
        assert!(mock_penalty_bps(10_000).is_ok());
    }

    // attestation
    fn mock_attest(duplicate: bool) -> Result<(), ContractError> {
        if duplicate {
            return Err(ContractError::DuplicateAttestation);
        }
        Ok(())
    }

    fn mock_get_attestation(exists: bool) -> Result<(), ContractError> {
        if !exists {
            return Err(ContractError::AttestationNotFound);
        }
        Ok(())
    }

    fn mock_revoke(is_original: bool, already_revoked: bool) -> Result<(), ContractError> {
        if !is_original {
            return Err(ContractError::NotOriginalAttester);
        }
        if already_revoked {
            return Err(ContractError::AttestationAlreadyRevoked);
        }
        Ok(())
    }

    fn mock_weight(weight: i128, max: i128) -> Result<(), ContractError> {
        if weight <= 0 {
            return Err(ContractError::InvalidAttestationWeight);
        }
        if weight > max {
            return Err(ContractError::AttestationWeightExceedsMax);
        }
        Ok(())
    }

    #[test]
    fn test_duplicate_attestation() {
        assert_eq!(mock_attest(true), Err(ContractError::DuplicateAttestation));
        assert!(mock_attest(false).is_ok());
    }

    #[test]
    fn test_attestation_not_found() {
        assert_eq!(
            mock_get_attestation(false),
            Err(ContractError::AttestationNotFound)
        );
        assert!(mock_get_attestation(true).is_ok());
    }

    #[test]
    fn test_not_original_attester() {
        assert_eq!(
            mock_revoke(false, false),
            Err(ContractError::NotOriginalAttester)
        );
    }

    #[test]
    fn test_attestation_already_revoked() {
        assert_eq!(
            mock_revoke(true, true),
            Err(ContractError::AttestationAlreadyRevoked)
        );
        assert!(mock_revoke(true, false).is_ok());
    }

    #[test]
    fn test_invalid_attestation_weight() {
        assert_eq!(
            mock_weight(0, 100),
            Err(ContractError::InvalidAttestationWeight)
        );
        assert_eq!(
            mock_weight(-1, 100),
            Err(ContractError::InvalidAttestationWeight)
        );
    }

    #[test]
    fn test_attestation_weight_exceeds_max() {
        assert_eq!(
            mock_weight(101, 100),
            Err(ContractError::AttestationWeightExceedsMax)
        );
        assert!(mock_weight(100, 100).is_ok());
    }

    // registry
    fn mock_register_identity(already: bool) -> Result<(), ContractError> {
        if already {
            return Err(ContractError::IdentityAlreadyRegistered);
        }
        Ok(())
    }

    fn mock_register_bond_contract(already: bool) -> Result<(), ContractError> {
        if already {
            return Err(ContractError::BondContractAlreadyRegistered);
        }
        Ok(())
    }

    fn mock_get_identity(exists: bool) -> Result<(), ContractError> {
        if !exists {
            return Err(ContractError::IdentityNotRegistered);
        }
        Ok(())
    }

    fn mock_get_bond_contract(exists: bool) -> Result<(), ContractError> {
        if !exists {
            return Err(ContractError::BondContractNotRegistered);
        }
        Ok(())
    }

    fn mock_deactivate(active: bool) -> Result<(), ContractError> {
        if !active {
            return Err(ContractError::AlreadyDeactivated);
        }
        Ok(())
    }

    fn mock_reactivate(active: bool) -> Result<(), ContractError> {
        if active {
            return Err(ContractError::AlreadyActive);
        }
        Ok(())
    }

    #[test]
    fn test_identity_already_registered() {
        assert_eq!(
            mock_register_identity(true),
            Err(ContractError::IdentityAlreadyRegistered)
        );
        assert!(mock_register_identity(false).is_ok());
    }

    #[test]
    fn test_bond_contract_already_registered() {
        assert_eq!(
            mock_register_bond_contract(true),
            Err(ContractError::BondContractAlreadyRegistered)
        );
        assert!(mock_register_bond_contract(false).is_ok());
    }

    #[test]
    fn test_identity_not_registered() {
        assert_eq!(
            mock_get_identity(false),
            Err(ContractError::IdentityNotRegistered)
        );
        assert!(mock_get_identity(true).is_ok());
    }

    #[test]
    fn test_bond_contract_not_registered() {
        assert_eq!(
            mock_get_bond_contract(false),
            Err(ContractError::BondContractNotRegistered)
        );
        assert!(mock_get_bond_contract(true).is_ok());
    }

    #[test]
    fn test_already_deactivated() {
        assert_eq!(
            mock_deactivate(false),
            Err(ContractError::AlreadyDeactivated)
        );
        assert!(mock_deactivate(true).is_ok());
    }

    #[test]
    fn test_already_active() {
        assert_eq!(mock_reactivate(true), Err(ContractError::AlreadyActive));
        assert!(mock_reactivate(false).is_ok());
    }

    // delegation
    fn mock_delegate(expiry_future: bool) -> Result<(), ContractError> {
        if !expiry_future {
            return Err(ContractError::ExpiryInPast);
        }
        Ok(())
    }

    fn mock_get_delegation(exists: bool) -> Result<(), ContractError> {
        if !exists {
            return Err(ContractError::DelegationNotFound);
        }
        Ok(())
    }

    fn mock_revoke_delegation(revoked: bool) -> Result<(), ContractError> {
        if revoked {
            return Err(ContractError::AlreadyRevoked);
        }
        Ok(())
    }

    #[test]
    fn test_expiry_in_past() {
        assert_eq!(mock_delegate(false), Err(ContractError::ExpiryInPast));
        assert!(mock_delegate(true).is_ok());
    }

    #[test]
    fn test_delegation_not_found() {
        assert_eq!(
            mock_get_delegation(false),
            Err(ContractError::DelegationNotFound)
        );
        assert!(mock_get_delegation(true).is_ok());
    }

    #[test]
    fn test_already_revoked() {
        assert_eq!(
            mock_revoke_delegation(true),
            Err(ContractError::AlreadyRevoked)
        );
        assert!(mock_revoke_delegation(false).is_ok());
    }

    // treasury
    fn mock_receive_fee(amount: i128, authorized: bool) -> Result<(), ContractError> {
        if amount <= 0 {
            return Err(ContractError::AmountMustBePositive);
        }
        if !authorized {
            return Err(ContractError::UnauthorizedDepositor);
        }
        Ok(())
    }

    fn mock_set_threshold(threshold: u32, count: u32) -> Result<(), ContractError> {
        if threshold > count {
            return Err(ContractError::ThresholdExceedsSigners);
        }
        Ok(())
    }

    fn mock_propose(is_signer: bool, amount: i128, balance: i128) -> Result<u64, ContractError> {
        if !is_signer {
            return Err(ContractError::NotSigner);
        }
        if amount <= 0 {
            return Err(ContractError::AmountMustBePositive);
        }
        if amount > balance {
            return Err(ContractError::InsufficientTreasuryBalance);
        }
        Ok(1)
    }

    fn mock_execute(
        found: bool,
        executed: bool,
        approvals: u32,
        threshold: u32,
        amount: i128,
        balance: i128,
    ) -> Result<(), ContractError> {
        if !found {
            return Err(ContractError::ProposalNotFound);
        }
        if executed {
            return Err(ContractError::ProposalAlreadyExecuted);
        }
        if approvals < threshold {
            return Err(ContractError::InsufficientApprovals);
        }
        if balance < amount {
            return Err(ContractError::InsufficientTreasuryBalance);
        }
        Ok(())
    }

    #[test]
    fn test_amount_must_be_positive() {
        assert_eq!(
            mock_receive_fee(0, true),
            Err(ContractError::AmountMustBePositive)
        );
        assert_eq!(
            mock_receive_fee(-1, true),
            Err(ContractError::AmountMustBePositive)
        );
        assert!(mock_receive_fee(1, true).is_ok());
    }

    #[test]
    fn test_threshold_exceeds_signers() {
        assert_eq!(
            mock_set_threshold(4, 3),
            Err(ContractError::ThresholdExceedsSigners)
        );
        assert!(mock_set_threshold(3, 3).is_ok());
    }

    #[test]
    fn test_propose_not_signer() {
        assert_eq!(mock_propose(false, 50, 100), Err(ContractError::NotSigner));
    }

    #[test]
    fn test_propose_insufficient_treasury_balance() {
        assert_eq!(
            mock_propose(true, 101, 100),
            Err(ContractError::InsufficientTreasuryBalance)
        );
        assert_eq!(mock_propose(true, 100, 100), Ok(1));
    }

    #[test]
    fn test_proposal_not_found() {
        assert_eq!(
            mock_execute(false, false, 3, 2, 50, 100),
            Err(ContractError::ProposalNotFound)
        );
    }

    #[test]
    fn test_proposal_already_executed() {
        assert_eq!(
            mock_execute(true, true, 3, 2, 50, 100),
            Err(ContractError::ProposalAlreadyExecuted)
        );
    }

    #[test]
    fn test_insufficient_approvals() {
        assert_eq!(
            mock_execute(true, false, 1, 3, 50, 100),
            Err(ContractError::InsufficientApprovals)
        );
    }

    #[test]
    fn test_execute_ok() {
        assert!(mock_execute(true, false, 3, 2, 50, 100).is_ok());
    }

    // arithmetic
    #[test]
    fn test_overflow() {
        let result: Result<i128, ContractError> =
            i128::MAX.checked_add(1).ok_or(ContractError::Overflow);
        assert_eq!(result, Err(ContractError::Overflow));
    }

    #[test]
    fn test_underflow() {
        let result: Result<i128, ContractError> =
            i128::MIN.checked_sub(1).ok_or(ContractError::Underflow);
        assert_eq!(result, Err(ContractError::Underflow));
    }

    #[test]
    fn test_error_category_equality() {
        assert_eq!(ErrorCategory::Bond, ErrorCategory::Bond);
        assert_ne!(ErrorCategory::Bond, ErrorCategory::Treasury);
    }
}
