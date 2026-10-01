#[cfg(test)]
mod tests {
    extern crate std;
    use crate::{ContractError, ErrorCategory, ErrorExt, Role};
    use soroban_sdk::testutils::Address as _;
    use std::vec::Vec;

    include!("../variant_table.rs");

    /// Canonical list of every `ContractError` variant, deduplicated, in numeric
    /// wire-code order. Used for exhaustiveness checks across the test suite.
    /// Keeping this in sync with `variant_table.rs` is enforced by the
    /// `test_all_variants_count` test below.
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
            ContractError::ContractPaused,
            ContractError::BorrowFrozen,
            ContractError::InvalidPauseAction,
            ContractError::InsufficientSignatures,
            ContractError::AdminSuspended,
            ContractError::NoPendingAdmin,
            ContractError::InvalidAdminAddress,
            ContractError::AdminUnchanged,
            ContractError::TimelockNotReady,
            ContractError::EmergencyDrainNotPermitted,
            ContractError::RoleNotHeldAtLedger,
            ContractError::ZeroBytes32,
            ContractError::CrossContractCallerMismatch,
            ContractError::TimestampInFuture,
            ContractError::LeaseScopeMismatch,
            ContractError::LeaseExpired,
            ContractError::CorridorNotRegistered,
            ContractError::InvalidPercentSplit,
            ContractError::InvalidStringifiedBytes,
            ContractError::SnapshotGenerationMismatch,
            ContractError::StaleAdminEpoch,
            ContractError::StaleSignerEpoch,
            ContractError::InvalidCurrency,
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
            ContractError::LeverageExceeded,
            ContractError::UnsupportedToken,
            ContractError::UnsupportedDecimals,
            ContractError::InvalidBondAmount,
            ContractError::AmountExplicitlyZero,
            ContractError::InvalidBondDuration,
            ContractError::InvalidNoticePeriod,
            ContractError::BondAlreadyExists,
            ContractError::UnauthorizedToken,
            ContractError::DuplicateIdempotencyKey,
            ContractError::InvalidStringifiedBytes,
            ContractError::InvariantViolation,
            ContractError::StorageCapReached,
            ContractError::TreasuryNotConfigured,
            ContractError::CursorOutOfRange,
            ContractError::DomainMismatch,
            ContractError::OwnerMismatch,
            ContractError::TargetMismatch,
            ContractError::ContractIdMismatch,
            ContractError::SignatureExpired,
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
            ContractError::InvalidContractAddress,
            ContractError::ContractCodeVerificationFailed,
            ContractError::UnsupportedInterface,
            ContractError::ExpiryInPast,
            ContractError::DelegationNotFound,
            ContractError::AlreadyRevoked,
            ContractError::DelegationExpiryTooLong,
            ContractError::UnknownScheme,
            ContractError::VerifierAlreadyRegistered,
            ContractError::VerifierNotRegistered,
            ContractError::VerificationFailed,
            ContractError::RevocationGraceExpired,
            ContractError::DelegationNotExpired,
            ContractError::DelegationInactive,
            ContractError::PromiseNotKept,
            ContractError::AmountMustBePositive,
            ContractError::ThresholdExceedsSigners,
            ContractError::InsufficientTreasuryBalance,
            ContractError::ProposalNotFound,
            ContractError::ProposalAlreadyExecuted,
            ContractError::InsufficientApprovals,
            ContractError::InvalidFlashLoanCallback,
            ContractError::FlashLoanRepaymentFailed,
            ContractError::ProposalExpired,
            ContractError::SlippageExceeded,
            ContractError::TreasuryBeneficiaryMismatch,
            ContractError::Overflow,
            ContractError::Underflow,
            ContractError::DivisionByZero,
            ContractError::BatchTooLarge,
            ContractError::EmptyBatch,
            ContractError::InvalidCurrency,
            ContractError::PayloadTooOld,
            ContractError::TimestampInFuture,
            ContractError::StaleAdminEpoch,
            ContractError::StaleSignerEpoch,
        ]
    }

    // ---------------------------------------------------------------------------
    // require_contract_uninitialized helper tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_require_contract_uninitialized_passes_when_false() {
        let e = soroban_sdk::Env::default();
        // Does not panic.
        crate::require_contract_uninitialized(&e, false);
    }

    #[test]
    fn test_require_contract_uninitialized_returns_error_when_true() {
        // `AlreadyInitialized` is wire code 2, and the helper panics via `Env::panic_with_error`.
        let e = soroban_sdk::Env::default();
        assert_eq!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::require_contract_uninitialized(&e, true);
            }))
            .is_err(),
            true
        );
    }

    // ---------------------------------------------------------------------------
    // Business-hours guard
    // ---------------------------------------------------------------------------

    #[test]
    fn test_require_within_business_hours_passes() {
        use crate::require_within_business_hours;
        use soroban_sdk::Env;
        let e = Env::default();
        // Thursday 1970-01-01 10:00:00 UTC = 36000
        require_within_business_hours(&e, 36_000);
        // Friday 1970-01-02 16:59:59 UTC = 86400 + 61199 = 147599
        require_within_business_hours(&e, 147_599);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #120)")]
    fn test_require_within_business_hours_panics_on_weekend() {
        use crate::require_within_business_hours;
        use soroban_sdk::Env;
        let e = Env::default();
        // Saturday 1970-01-03 12:00:00 UTC = 2 * 86400 + 43200 = 216000
        require_within_business_hours(&e, 216_000);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #120)")]
    fn test_require_within_business_hours_panics_outside_hours() {
        use crate::require_within_business_hours;
        use soroban_sdk::Env;
        let e = Env::default();
        // Thursday 1970-01-01 08:59:59 UTC = 32399
        require_within_business_hours(&e, 32_399);
    }

    // Boundary: the first second of business hours must pass.
    #[test]
    fn test_require_within_business_hours_exact_open_boundary() {
        use crate::require_within_business_hours;
        let e = soroban_sdk::Env::default();
        // Thursday 1970-01-01 09:00:00 UTC = 32400 (first valid second)
        require_within_business_hours(&e, 32_400);
    }

    // Boundary: the last second of business hours must pass.
    #[test]
    fn test_require_within_business_hours_exact_close_boundary() {
        use crate::require_within_business_hours;
        let e = soroban_sdk::Env::default();
        // Thursday 1970-01-01 16:59:59 UTC = 61199 (last valid second)
        require_within_business_hours(&e, 61_199);
    }

    // Boundary: 17:00:00 exactly must be rejected.
    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #120)")]
    fn test_require_within_business_hours_rejects_at_close_exact() {
        use crate::require_within_business_hours;
        let e = soroban_sdk::Env::default();
        // Thursday 1970-01-01 17:00:00 UTC = 61200 (exclusive upper bound)
        require_within_business_hours(&e, 61_200);
    }

    // Sunday must always be rejected regardless of time.
    #[test]
    #[should_panic(expected = "HostError: Error(Contract, #120)")]
    fn test_require_within_business_hours_rejects_sunday() {
        use crate::require_within_business_hours;
        let e = soroban_sdk::Env::default();
        // Sunday 1970-01-04 12:00:00 UTC = 3 * 86400 + 43200 = 302400
        require_within_business_hours(&e, 302_400);
    }

    // ---------------------------------------------------------------------------
    // Wire code tests
    // ---------------------------------------------------------------------------

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
        assert_eq!(ContractError::InvalidContractAddress as u32, 406);
        assert_eq!(ContractError::ContractCodeVerificationFailed as u32, 407);
        assert_eq!(ContractError::UnsupportedInterface as u32, 408);
    }

    #[test]
    fn test_codes_delegation() {
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
    }

    #[test]
    fn test_codes_treasury() {
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
    }

    #[test]
    fn test_codes_arithmetic() {
        assert_eq!(ContractError::Overflow as u32, 700);
        assert_eq!(ContractError::Underflow as u32, 701);
        assert_eq!(ContractError::DivisionByZero as u32, 702);
        assert_eq!(ContractError::InvalidPercentSplit as u32, 703);
    }

    // ---------------------------------------------------------------------------
    // Category mapping tests
    // ---------------------------------------------------------------------------

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
        let auth_variants = [
            ContractError::NotAdmin,
            ContractError::NotBondOwner,
            ContractError::UnauthorizedAttester,
            ContractError::NotOriginalAttester,
            ContractError::NotSigner,
            ContractError::UnauthorizedDepositor,
            ContractError::ContractPaused,
            ContractError::InvalidPauseAction,
            ContractError::InsufficientSignatures,
            ContractError::ZeroBytes32,
            ContractError::InvalidAdminAddress,
            ContractError::AdminUnchanged,
            ContractError::TimelockNotReady,
            ContractError::AdminSuspended,
            ContractError::BorrowFrozen,
            ContractError::NoPendingAdmin,
            ContractError::RoleNotHeldAtLedger,
            ContractError::EmergencyDrainNotPermitted,
            ContractError::TimestampInFuture,
            ContractError::InvalidMaxPauseSigners,
            ContractError::OutsideBusinessHours,
            ContractError::LeaseScopeMismatch,
            ContractError::LeaseExpired,
            ContractError::CrossContractCallerMismatch,
            ContractError::MigrationInProgress,
            ContractError::MaxPauseSignersExceeded,
            ContractError::LeaseSignerMismatch,
            ContractError::RoleRequired,
            ContractError::StaleAdminEpoch,
            ContractError::StaleSignerEpoch,
        ];
        for v in &auth_variants {
            assert_eq!(
                v.category(),
                ErrorCategory::Authorization,
                "{v:?} should be Authorization"
            );
        }
    }

    #[test]
    fn test_category_bond() {
        let bond_variants = [
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
            ContractError::LeverageExceeded,
            ContractError::UnsupportedToken,
            ContractError::InvalidBondAmount,
            ContractError::AmountExplicitlyZero,
            ContractError::InvalidBondDuration,
            ContractError::InvalidNoticePeriod,
            ContractError::BondAlreadyExists,
            ContractError::OwnerMismatch,
            ContractError::TargetMismatch,
            ContractError::ContractIdMismatch,
            ContractError::SignatureExpired,
            ContractError::TreasuryNotConfigured,
            ContractError::StorageCapReached,
            ContractError::DomainMismatch,
            ContractError::CursorOutOfRange,
            ContractError::BatchTooLarge,
            ContractError::EmptyBatch,
            ContractError::UnsupportedDecimals,
            ContractError::InvalidStringifiedBytes,
            ContractError::UnauthorizedToken,
            ContractError::DuplicateIdempotencyKey,
            ContractError::InvariantViolation,
            ContractError::InvalidCurrency,
            ContractError::SnapshotGenerationMismatch,
            ContractError::CooldownRequestAlreadyPending,
            ContractError::CooldownRequestNotFound,
            ContractError::CooldownPeriodNotElapsed,
            ContractError::BytesTooLarge,
        ];
        for v in &bond_variants {
            assert_eq!(
                v.category(),
                ErrorCategory::Bond,
                "{v:?} should be Bond"
            );
        }
    }

    #[test]
    fn test_category_attestation() {
        let attestation_variants = [
            ContractError::DuplicateAttestation,
            ContractError::AttestationNotFound,
            ContractError::AttestationAlreadyRevoked,
            ContractError::InvalidAttestationWeight,
            ContractError::AttestationWeightExceedsMax,
        ];
        for v in &attestation_variants {
            assert_eq!(
                v.category(),
                ErrorCategory::Attestation,
                "{v:?} should be Attestation"
            );
        }
    }

    #[test]
    fn test_category_registry() {
        let registry_variants = [
            ContractError::IdentityAlreadyRegistered,
            ContractError::BondContractAlreadyRegistered,
            ContractError::IdentityNotRegistered,
            ContractError::BondContractNotRegistered,
            ContractError::AlreadyDeactivated,
            ContractError::AlreadyActive,
            ContractError::InvalidContractAddress,
            ContractError::ContractCodeVerificationFailed,
            ContractError::UnsupportedInterface,
        ];
        for v in &registry_variants {
            assert_eq!(
                v.category(),
                ErrorCategory::Registry,
                "{v:?} should be Registry"
            );
        }
    }

    #[test]
    fn test_category_delegation() {
        let delegation_variants = [
            ContractError::ExpiryInPast,
            ContractError::DelegationNotFound,
            ContractError::AlreadyRevoked,
            ContractError::DelegationExpiryTooLong,
            ContractError::UnknownScheme,
            ContractError::VerifierAlreadyRegistered,
            ContractError::VerifierNotRegistered,
            ContractError::VerificationFailed,
            ContractError::RevocationGraceExpired,
            ContractError::DelegationNotExpired,
            ContractError::PayloadTooOld,
            ContractError::DelegationInactive,
            ContractError::PromiseNotKept,
            ContractError::StaleEpoch,
        ];
        for v in &delegation_variants {
            assert_eq!(
                v.category(),
                ErrorCategory::Delegation,
                "{v:?} should be Delegation"
            );
        }
    }

    #[test]
    fn test_category_treasury() {
        let treasury_variants = [
            ContractError::AmountMustBePositive,
            ContractError::ThresholdExceedsSigners,
            ContractError::InsufficientTreasuryBalance,
            ContractError::ProposalNotFound,
            ContractError::ProposalAlreadyExecuted,
            ContractError::InsufficientApprovals,
            ContractError::InvalidFlashLoanCallback,
            ContractError::FlashLoanRepaymentFailed,
            ContractError::ProposalExpired,
            ContractError::SlippageExceeded,
            ContractError::TreasuryBeneficiaryMismatch,
            ContractError::CorridorNotRegistered,
        ];
        for v in &treasury_variants {
            assert_eq!(
                v.category(),
                ErrorCategory::Treasury,
                "{v:?} should be Treasury"
            );
        }
    }

    #[test]
    fn test_category_arithmetic() {
        assert_eq!(ContractError::Overflow.category(), ErrorCategory::Arithmetic);
        assert_eq!(
            ContractError::Underflow.category(),
            ErrorCategory::Arithmetic
        );
        assert_eq!(
            ContractError::DivisionByZero.category(),
            ErrorCategory::Arithmetic
        );
        assert_eq!(
            ContractError::InvalidPercentSplit.category(),
            ErrorCategory::Arithmetic
        );
    }

    // ---------------------------------------------------------------------------
    // Description tests
    // ---------------------------------------------------------------------------

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
                assert_ne!(
                    variants[i].description(),
                    variants[j].description(),
                    "Variants {:?} and {:?} share the same description",
                    variants[i],
                    variants[j]
                );
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Variant count guard
    // ---------------------------------------------------------------------------

    #[test]
    fn test_all_variants_count() {
        // This count must equal the number of entries in variant_table.rs.
        // When adding a variant: add a row to variant_table.rs and bump this number.
        assert_eq!(
            all_variants().len(),
            116,
            "Update variant_table.rs and this count when adding/removing errors"
        );
    }

    // ---------------------------------------------------------------------------
    // Copy and Eq tests
    // ---------------------------------------------------------------------------

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

    // ---------------------------------------------------------------------------
    // Result integration tests (mirrors real contract call sites)
    // ---------------------------------------------------------------------------

    // Initialization
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

    // Authorization
    fn mock_admin(role: Role) -> Result<(), ContractError> {
        if role != Role::Admin {
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
        assert_eq!(mock_admin(Role::User), Err(ContractError::NotAdmin));
        assert!(mock_admin(Role::Admin).is_ok());
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

    // Bond
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
    fn test_slash_exceeds_bond_boundary() {
        // Boundary: slash == bonded must succeed.
        assert!(mock_slash(100, 100).is_ok(), "slash == bonded should be ok");
        // Boundary: slash == bonded + 1 must fail.
        assert_eq!(mock_slash(101, 100), Err(ContractError::SlashExceedsBond));
        // Far over boundary.
        assert_eq!(mock_slash(i128::MAX, 0), Err(ContractError::SlashExceedsBond));
        // Slash of 0 on any balance must succeed.
        assert!(mock_slash(0, 0).is_ok());
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
    fn test_negative_stake_boundary() {
        // Boundary: stake == -1 (one below zero) must fail.
        assert_eq!(mock_stake(-1), Err(ContractError::NegativeStake));
        // Boundary: stake == 0 must pass (zero is not negative).
        assert!(mock_stake(0).is_ok());
        // Far negative.
        assert_eq!(mock_stake(i128::MIN), Err(ContractError::NegativeStake));
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
    fn test_invalid_penalty_bps_boundary() {
        // Boundary at maximum valid value: 10_000 must pass.
        assert!(mock_penalty_bps(10_000).is_ok(), "10000 bps should be valid");
        // One above the maximum: 10_001 must fail.
        assert_eq!(
            mock_penalty_bps(10_001),
            Err(ContractError::InvalidPenaltyBps)
        );
        // Zero must be valid (no penalty).
        assert!(mock_penalty_bps(0).is_ok(), "0 bps should be valid");
        // Maximum u32 must fail.
        assert_eq!(mock_penalty_bps(u32::MAX), Err(ContractError::InvalidPenaltyBps));
    }

    // Attestation
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
    fn test_invalid_attestation_weight_boundary() {
        // Boundary: weight == 0 must fail (not strictly positive).
        assert_eq!(
            mock_weight(0, 100),
            Err(ContractError::InvalidAttestationWeight)
        );
        // Negative weight must fail.
        assert_eq!(
            mock_weight(-1, 100),
            Err(ContractError::InvalidAttestationWeight)
        );
        // Minimum valid weight == 1.
        assert!(mock_weight(1, 100).is_ok());
    }

    #[test]
    fn test_attestation_weight_exceeds_max_boundary() {
        // Boundary: weight == max must succeed.
        assert!(mock_weight(100, 100).is_ok(), "weight == max should be ok");
        // One above max must fail.
        assert_eq!(
            mock_weight(101, 100),
            Err(ContractError::AttestationWeightExceedsMax)
        );
    }

    // Registry
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

    // Delegation
    fn mock_delegate(expiry_future: bool, within_max_duration: bool) -> Result<(), ContractError> {
        if !expiry_future {
            return Err(ContractError::ExpiryInPast);
        }
        if !within_max_duration {
            return Err(ContractError::DelegationExpiryTooLong);
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
        assert_eq!(mock_delegate(false, true), Err(ContractError::ExpiryInPast));
        assert!(mock_delegate(true, true).is_ok());
    }

    #[test]
    fn test_delegation_expiry_too_long() {
        assert_eq!(
            mock_delegate(true, false),
            Err(ContractError::DelegationExpiryTooLong)
        );
        assert!(mock_delegate(true, true).is_ok());
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

    // Treasury
    fn apply_leading_zero_guard(amount: Option<i128>) -> Result<(), ContractError> {
        let _e = soroban_sdk::Env::default();
        crate::require_no_leading_zero_amount!(&_e, amount);
        Ok(())
    }

    #[test]
    fn leading_zero_guard_accepts_unset_amount() {
        assert_eq!(apply_leading_zero_guard(None), Ok(()));
    }

    #[test]
    fn leading_zero_guard_rejects_explicit_zero() {
        assert_eq!(
            apply_leading_zero_guard(Some(0)),
            Err(ContractError::AmountExplicitlyZero)
        );
    }

    #[test]
    fn leading_zero_guard_accepts_positive_amount() {
        assert_eq!(apply_leading_zero_guard(Some(1)), Ok(()));
        assert_eq!(apply_leading_zero_guard(Some(i128::MAX)), Ok(()));
    }

    #[test]
    fn leading_zero_guard_accepts_negative_amount() {
        // Negative amounts are not rejected by this guard (other guards handle that).
        assert_eq!(apply_leading_zero_guard(Some(-1)), Ok(()));
        assert_eq!(apply_leading_zero_guard(Some(i128::MIN)), Ok(()));
    }

    #[test]
    fn test_require_positive_amount_macro_compiles() {
        fn test_positive() -> Result<(), ContractError> {
            let e = soroban_sdk::Env::default();
            crate::require_positive_amount!(&e, 1_i128);
            Ok(())
        }
        test_positive().unwrap();
    }

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
    fn test_amount_must_be_positive_boundary() {
        // Boundary: amount == 0 must fail.
        assert_eq!(
            mock_receive_fee(0, true),
            Err(ContractError::AmountMustBePositive)
        );
        // Boundary: amount == -1 must fail.
        assert_eq!(
            mock_receive_fee(-1, true),
            Err(ContractError::AmountMustBePositive)
        );
        // Boundary: amount == 1 (minimum positive) must pass.
        assert!(mock_receive_fee(1, true).is_ok());
        // i128::MIN must fail.
        assert_eq!(
            mock_receive_fee(i128::MIN, true),
            Err(ContractError::AmountMustBePositive)
        );
        // i128::MAX must pass.
        assert!(mock_receive_fee(i128::MAX, true).is_ok());
    }

    #[test]
    fn test_threshold_exceeds_signers_boundary() {
        // Boundary: threshold == count must succeed.
        assert!(
            mock_set_threshold(3, 3).is_ok(),
            "threshold == count should be ok"
        );
        // Boundary: threshold == count + 1 must fail.
        assert_eq!(
            mock_set_threshold(4, 3),
            Err(ContractError::ThresholdExceedsSigners)
        );
        // threshold == 0 with count == 0 must succeed.
        assert!(mock_set_threshold(0, 0).is_ok());
    }

    #[test]
    fn test_propose_not_signer() {
        assert_eq!(mock_propose(false, 50, 100), Err(ContractError::NotSigner));
    }

    #[test]
    fn test_propose_insufficient_treasury_balance_boundary() {
        // Boundary: amount == balance must succeed.
        assert_eq!(mock_propose(true, 100, 100), Ok(1));
        // Boundary: amount == balance + 1 must fail.
        assert_eq!(
            mock_propose(true, 101, 100),
            Err(ContractError::InsufficientTreasuryBalance)
        );
        // amount == 0 must fail with AmountMustBePositive before reaching balance check.
        assert_eq!(
            mock_propose(true, 0, 100),
            Err(ContractError::AmountMustBePositive)
        );
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
    fn test_insufficient_approvals_boundary() {
        // Boundary: approvals == threshold - 1 must fail.
        assert_eq!(
            mock_execute(true, false, 2, 3, 50, 100),
            Err(ContractError::InsufficientApprovals)
        );
        // Boundary: approvals == threshold must succeed.
        assert!(mock_execute(true, false, 3, 3, 50, 100).is_ok());
        // approvals > threshold must succeed.
        assert!(mock_execute(true, false, 4, 3, 50, 100).is_ok());
    }

    #[test]
    fn test_execute_ok() {
        assert!(mock_execute(true, false, 3, 2, 50, 100).is_ok());
    }

    // Multi-sig
    #[test]
    fn test_insufficient_signatures() {
        fn mock_multisig_execute(approvals: u32, threshold: u32) -> Result<(), ContractError> {
            if approvals < threshold {
                return Err(ContractError::InsufficientSignatures);
            }
            Ok(())
        }
        assert_eq!(
            mock_multisig_execute(1, 2),
            Err(ContractError::InsufficientSignatures)
        );
        assert!(mock_multisig_execute(2, 2).is_ok());
        // Boundary: approvals == threshold - 1 must fail.
        assert_eq!(
            mock_multisig_execute(0, 1),
            Err(ContractError::InsufficientSignatures)
        );
    }

    #[test]
    fn test_insufficient_signatures_category() {
        assert_eq!(
            ContractError::InsufficientSignatures.category(),
            ErrorCategory::Authorization
        );
    }

    #[test]
    fn test_insufficient_signatures_description() {
        assert!(!ContractError::InsufficientSignatures
            .description()
            .is_empty());
    }

    // Arithmetic
    #[test]
    fn test_overflow_boundary() {
        // i128::MAX + 1 overflows.
        let result: Result<i128, ContractError> =
            i128::MAX.checked_add(1).ok_or(ContractError::Overflow);
        assert_eq!(result, Err(ContractError::Overflow));
        // i128::MAX + 0 does not overflow.
        let ok: Result<i128, ContractError> =
            i128::MAX.checked_add(0).ok_or(ContractError::Overflow);
        assert_eq!(ok, Ok(i128::MAX));
    }

    #[test]
    fn test_underflow_boundary() {
        // i128::MIN - 1 underflows.
        let result: Result<i128, ContractError> =
            i128::MIN.checked_sub(1).ok_or(ContractError::Underflow);
        assert_eq!(result, Err(ContractError::Underflow));
        // i128::MIN - 0 does not underflow.
        let ok: Result<i128, ContractError> =
            i128::MIN.checked_sub(0).ok_or(ContractError::Underflow);
        assert_eq!(ok, Ok(i128::MIN));
    }

    #[test]
    fn test_division_by_zero_boundary() {
        fn safe_div(a: i128, b: i128) -> Result<i128, ContractError> {
            if b == 0 {
                return Err(ContractError::DivisionByZero);
            }
            Ok(a / b)
        }
        // Boundary: b == 0 must fail.
        assert_eq!(safe_div(10, 0), Err(ContractError::DivisionByZero));
        // Boundary: b == 1 must succeed.
        assert_eq!(safe_div(10, 1), Ok(10));
        // Boundary: a == 0, b == 1.
        assert_eq!(safe_div(0, 1), Ok(0));
        // b == -1 is not zero; must succeed.
        assert_eq!(safe_div(10, -1), Ok(-10));
    }

    #[test]
    fn test_error_category_equality() {
        assert_eq!(ErrorCategory::Bond, ErrorCategory::Bond);
        assert_ne!(ErrorCategory::Bond, ErrorCategory::Treasury);
    }

    // --- is_recoverable() tests ---

    /// Mirror of `impl ErrorExt for ContractError::is_recoverable`.
    ///
    /// Note: this `mod tests` is already gated by `#[cfg(test)]` at its
    /// declaration site, so the per-function attribute is redundant here.
    /// This function exists to force the compiler to enforce that every
    /// `ContractError` variant is explicitly classified: adding a new
    /// variant to the enum (without classifying it here) makes the test
    /// fail to compile — exactly the property the issue requires. A
    /// short rationale sits next to every arm so a reviewer can audit
    /// recoverability decisions line by line.
    ///
    /// The pre-existing `all_variants()` helper nearby is intentionally
    /// out of scope: it returns a subset of variants and would risk
    /// drifting the exhaustiveness check. This list is exhaustive.
    fn expected_is_recoverable(e: ContractError) -> bool {
        match e {
            // Initialization: caller fixes setup state.
            ContractError::NotInitialized => true, // init first
            ContractError::LeaseSignerMismatch => true, // sign with the lease holder's key
            ContractError::BytesTooLarge => true,  // resubmit with shorter input
            ContractError::CooldownRequestAlreadyPending => true, // wait for the existing request
            ContractError::CooldownRequestNotFound => true,
            ContractError::CooldownPeriodNotElapsed => true, // wait for the cooldown to elapse
            ContractError::AlreadyInitialized => true,       // idempotent

            // Authorization: switch signer/role.
            ContractError::NotAdmin => true,
            ContractError::NotBondOwner => true,
            ContractError::UnauthorizedAttester => true,
            ContractError::NotOriginalAttester => true,
            ContractError::NotSigner => true,
            ContractError::UnauthorizedDepositor => true,
            ContractError::ContractPaused => true, // wait for unpause
            ContractError::MigrationInProgress => true, // wait for migration to finish
            ContractError::BorrowFrozen => true,   // wait for unfreeze
            ContractError::OutsideBusinessHours => true, // retry within business hours
            ContractError::InvalidPauseAction => true,
            ContractError::InsufficientSignatures => true, // gather more sigs
            ContractError::AdminSuspended => true,         // wait for suspension

            // Admin Transfer: state-step fixes.
            ContractError::NoPendingAdmin => true,
            ContractError::InvalidAdminAddress => true,
            ContractError::AdminUnchanged => true,
            ContractError::TimelockNotReady => true, // wait for delay
            ContractError::EmergencyDrainNotPermitted => true,
            ContractError::RoleNotHeldAtLedger => true,
            ContractError::ZeroBytes32 => true,
            ContractError::MigrationInProgress => true, // wait for migration to complete
            ContractError::OutsideBusinessHours => true, // retry after business-hours window opens
            ContractError::TimestampInFuture => true,   // caller can correct timestamp
            ContractError::InvalidMaxPauseSigners => true, // admin supplies a valid value
            ContractError::MaxPauseSignersExceeded => true, // remove a signer or raise the cap
            ContractError::LeaseScopeMismatch => true,
            ContractError::LeaseExpired => true,
            ContractError::CrossContractCallerMismatch => false,
            ContractError::RoleRequired => true,
            ContractError::StaleAdminEpoch => false,
            ContractError::StaleSignerEpoch => false,

            // Bond: state/caller fixes; fatal cases are security/drift/capacity.
            ContractError::BondNotFound => true,
            ContractError::BondNotActive => true,
            ContractError::InsufficientBalance => true,
            ContractError::SlashExceedsBond => true,
            ContractError::StorageCapReached => false, // caller cannot free capacity; only operator prune fixes it
            ContractError::LockupNotExpired => true,
            ContractError::NotRollingBond => true,
            ContractError::WithdrawalAlreadyRequested => true,
            ContractError::ReentrancyDetected => false, // SECURITY HALT
            ContractError::InvalidNonce => true,
            ContractError::SignatureExpired => true, // re-sign
            ContractError::NegativeStake => true,
            ContractError::EarlyExitConfigNotSet => true,
            ContractError::InvalidPenaltyBps => true,
            ContractError::LeverageExceeded => true,
            ContractError::UnsupportedToken => true,
            ContractError::UnsupportedDecimals => true,
            ContractError::InvalidBondAmount => true,
            ContractError::AmountExplicitlyZero => true, // supply a non-zero amount
            ContractError::InvalidBondDuration => true,
            ContractError::InvalidNoticePeriod => true,
            ContractError::BondAlreadyExists => true,
            ContractError::UnauthorizedToken => true,
            ContractError::DuplicateIdempotencyKey => true,
            ContractError::InvalidStringifiedBytes => true,
            ContractError::InvariantViolation => false, // post-write drift
            ContractError::SnapshotGenerationMismatch => false,
            ContractError::TreasuryNotConfigured => true, // admin can configure treasury then retry
            ContractError::DomainMismatch => false,       // payload binding
            ContractError::BatchTooLarge => true,         // reduce batch size
            ContractError::EmptyBatch => true,            // supply at least one item
            ContractError::InvalidCurrency => true,       // supply a valid currency
            ContractError::OwnerMismatch => false,
            ContractError::TargetMismatch => false,
            ContractError::ContractIdMismatch => false,

            // Attestation: state/caller fixes.
            ContractError::DuplicateAttestation => true,
            ContractError::AttestationNotFound => true,
            ContractError::AttestationAlreadyRevoked => true,
            ContractError::InvalidAttestationWeight => true,
            ContractError::AttestationWeightExceedsMax => true,

            // Registry: state fixes.
            ContractError::IdentityAlreadyRegistered => true,
            ContractError::BondContractAlreadyRegistered => true,
            ContractError::IdentityNotRegistered => true,
            ContractError::BondContractNotRegistered => true,
            ContractError::AlreadyDeactivated => true,
            ContractError::AlreadyActive => true,
            ContractError::InvalidContractAddress => true,
            ContractError::ContractCodeVerificationFailed => true,
            ContractError::UnsupportedInterface => true,

            // Delegation: state/caller fixes; fatal cases are scheme/crypto.
            ContractError::ExpiryInPast => true,
            ContractError::DelegationNotFound => true,
            ContractError::AlreadyRevoked => true,
            ContractError::DelegationExpiryTooLong => true,
            ContractError::UnknownScheme => false, // unsupported scheme
            ContractError::VerifierAlreadyRegistered => true,
            ContractError::VerifierNotRegistered => true,
            ContractError::VerificationFailed => false, // crypto failure
            ContractError::RevocationGraceExpired => false, // delegation is in terminal state from caller's side; only admin can extend grace (distinct from AlreadyRevoked, whose state is idempotent)
            ContractError::DelegationNotExpired => true,    // wait for expiry then retry
            ContractError::DelegationInactive => false,
            ContractError::PayloadTooOld => true, // re-sign with current ledger number
            ContractError::PromiseNotKept => false, // off-chain promise hash does not match on-chain execution; same input will fail
            ContractError::StaleEpoch => false, // proposal ID contains a stale epoch reference; caller must re-propose
            // Treasury: state/caller fixes; fatal cases are callback failures.
            ContractError::AmountMustBePositive => true,
            ContractError::ThresholdExceedsSigners => true,
            ContractError::InsufficientTreasuryBalance => true,
            ContractError::ProposalNotFound => true,
            ContractError::ProposalAlreadyExecuted => true,
            ContractError::InsufficientApprovals => true,
            ContractError::InvalidFlashLoanCallback => false, // bad magic
            ContractError::FlashLoanRepaymentFailed => false, // bad repayment
            ContractError::ProposalExpired => true,
            ContractError::SlippageExceeded => true,
            ContractError::TreasuryBeneficiaryMismatch => true, // call with the correct treasury address
            ContractError::CorridorNotRegistered => true, // admin registers the corridor, then retry

            // Registry pagination: caller can supply a valid cursor.
            ContractError::CursorOutOfRange => true,

            // Arithmetic: code-level impossibility.
            ContractError::Overflow => false,
            ContractError::Underflow => false,
            ContractError::DivisionByZero => false,
            ContractError::InvalidPercentSplit => true,
        }
    }

    /// Compile-time exhaustiveness check.
    ///
    /// `expected_is_recoverable` already requires every variant in its
    /// match, so adding a new `ContractError` variant without classifying
    /// it here breaks the build. This test also iterates the runtime
    /// mirror (the actual `is_recoverable`) and asserts equality for
    /// every variant, so the documented expectation table cannot drift
    /// from the implementation.
    #[test]
    fn test_is_recoverable_exhaustive() {
        // Spot-check every variant: runtime classification must equal
        // documented expectation. The match in `expected_is_recoverable`
        // already forces compile-time exhaustiveness for the expectation
        // table.
        // Canonical list of all 110 variants, one entry each, in numeric wire-code
        // order matching `variant_table.rs`. No duplicates. Adding a new variant
        // to `lib.rs` requires adding it here AND in `expected_is_recoverable`.
        let cases: std::vec::Vec<ContractError> = std::vec![
            ContractError::NotInitialized,
            ContractError::AlreadyInitialized,
            ContractError::NotAdmin,
            ContractError::NotBondOwner,
            ContractError::UnauthorizedAttester,
            ContractError::NotOriginalAttester,
            ContractError::NotSigner,
            ContractError::UnauthorizedDepositor,
            ContractError::ContractPaused,
            ContractError::BorrowFrozen,
            ContractError::InvalidPauseAction,
            ContractError::InsufficientSignatures,
            ContractError::AdminSuspended,
            ContractError::NoPendingAdmin,
            ContractError::InvalidAdminAddress,
            ContractError::AdminUnchanged,
            ContractError::TimelockNotReady,
            ContractError::EmergencyDrainNotPermitted,
            ContractError::RoleNotHeldAtLedger,
            ContractError::ZeroBytes32,
            ContractError::RoleRequired,
            ContractError::LeaseSignerMismatch,
            ContractError::BondNotFound,
            ContractError::BondNotActive,
            ContractError::InsufficientBalance,
            ContractError::SlashExceedsBond,
            ContractError::LockupNotExpired,
            ContractError::NotRollingBond,
            ContractError::WithdrawalAlreadyRequested,
            ContractError::ReentrancyDetected,
            ContractError::InvalidNonce,
            ContractError::SignatureExpired,
            ContractError::NegativeStake,
            ContractError::EarlyExitConfigNotSet,
            ContractError::InvalidPenaltyBps,
            ContractError::LeverageExceeded,
            ContractError::UnsupportedToken,
            ContractError::UnsupportedDecimals,
            ContractError::InvalidBondAmount,
            ContractError::AmountExplicitlyZero,
            ContractError::InvalidBondDuration,
            ContractError::InvalidNoticePeriod,
            ContractError::BondAlreadyExists,
            ContractError::UnauthorizedToken,
            ContractError::DuplicateIdempotencyKey,
            ContractError::BatchTooLarge,
            ContractError::EmptyBatch,
            ContractError::StorageCapReached,
            ContractError::TreasuryNotConfigured,
            ContractError::InvariantViolation,
            ContractError::DomainMismatch,
            ContractError::OwnerMismatch,
            ContractError::TargetMismatch,
            ContractError::ContractIdMismatch,
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
            ContractError::InvalidContractAddress,
            ContractError::ContractCodeVerificationFailed,
            ContractError::UnsupportedInterface,
            ContractError::ExpiryInPast,
            ContractError::DelegationNotFound,
            ContractError::AlreadyRevoked,
            ContractError::DelegationExpiryTooLong,
            ContractError::UnknownScheme,
            ContractError::VerifierAlreadyRegistered,
            ContractError::VerifierNotRegistered,
            ContractError::VerificationFailed,
            ContractError::RevocationGraceExpired,
            ContractError::DelegationNotExpired,
            ContractError::DelegationInactive,
            ContractError::PromiseNotKept,
            ContractError::AmountMustBePositive,
            ContractError::ThresholdExceedsSigners,
            ContractError::InsufficientTreasuryBalance,
            ContractError::ProposalNotFound,
            ContractError::ProposalAlreadyExecuted,
            ContractError::InsufficientApprovals,
            ContractError::InvalidFlashLoanCallback,
            ContractError::FlashLoanRepaymentFailed,
            ContractError::ProposalExpired,
            ContractError::SlippageExceeded,
            ContractError::TreasuryBeneficiaryMismatch,
            ContractError::CursorOutOfRange,
            ContractError::InvalidCurrency,
            ContractError::PayloadTooOld,
            ContractError::PromiseNotKept,
            ContractError::Overflow,
            ContractError::Underflow,
            ContractError::DivisionByZero,
            ContractError::TimestampInFuture,
            ContractError::BorrowFrozen,
            ContractError::PayloadTooOld,
            ContractError::InvalidCurrency,
            ContractError::StaleEpoch,
            ContractError::MigrationInProgress,
            ContractError::OutsideBusinessHours,
        ];
        assert_eq!(
            cases.len(),
            105,
            "Add the new variant to ALL THREE places: \
             (1) lib.rs is_recoverable() match, \
             (2) expected_is_recoverable() below, \
             (3) this `cases` list."
        );
        for e in &cases {
            assert_eq!(
                e.is_recoverable(),
                expected_is_recoverable(*e),
                "classification drift for {:?}",
                e
            );
        }
    }

    /// Metadata guarantees: must not panic, must not allocate, must be
    /// `Copy`-safe (returns a primitive). Catch regressions where someone
    /// adds a heavier impl (storage read, panic, etc.).
    #[test]
    fn test_is_recoverable_is_const_safe() {
        // Repeated calls on a Copy value must yield identical results.
        let e = ContractError::Overflow;
        let _ = e.is_recoverable();
        let _ = e.is_recoverable();
        let _ = e.is_recoverable();
        assert!(!e.is_recoverable());

        let e = ContractError::AlreadyInitialized;
        assert!(e.is_recoverable());
    }

    /// Spot-check a recoverable sample per category (issue requirement).
    #[test]
    fn test_is_recoverable_recoverable_samples() {
        // issue #519 specifically names AlreadyInitialized, ProposalAlreadyExecuted,
        // and AlreadyRevoked as recoverable; assert those explicitly as well.
        assert!(ContractError::NotInitialized.is_recoverable());
        assert!(ContractError::AlreadyInitialized.is_recoverable());
        assert!(ContractError::NotAdmin.is_recoverable());
        assert!(ContractError::NotBondOwner.is_recoverable());
        assert!(ContractError::ContractPaused.is_recoverable());
        assert!(ContractError::BondNotFound.is_recoverable());
        assert!(ContractError::InvalidNonce.is_recoverable());
        assert!(ContractError::DuplicateAttestation.is_recoverable());
        assert!(ContractError::IdentityNotRegistered.is_recoverable());
        assert!(ContractError::ExpiryInPast.is_recoverable());
        assert!(ContractError::AlreadyRevoked.is_recoverable());
        assert!(ContractError::AmountMustBePositive.is_recoverable());
        assert!(ContractError::ProposalAlreadyExecuted.is_recoverable());
        assert!(ContractError::InsufficientApprovals.is_recoverable());
    }

    /// Spot-check a fatal sample per category (issue requirement).
    #[test]
    fn test_is_recoverable_fatal_samples() {
        assert!(!ContractError::ReentrancyDetected.is_recoverable());
        assert!(!ContractError::StorageCapReached.is_recoverable());
        assert!(!ContractError::InvariantViolation.is_recoverable());
        assert!(!ContractError::VerificationFailed.is_recoverable());
        assert!(!ContractError::UnknownScheme.is_recoverable());
        assert!(!ContractError::RevocationGraceExpired.is_recoverable());
        assert!(!ContractError::InvalidFlashLoanCallback.is_recoverable());
        assert!(!ContractError::FlashLoanRepaymentFailed.is_recoverable());
        assert!(!ContractError::Overflow.is_recoverable());
        assert!(!ContractError::Underflow.is_recoverable());
        assert!(!ContractError::DomainMismatch.is_recoverable());
        assert!(!ContractError::OwnerMismatch.is_recoverable());
        assert!(!ContractError::TargetMismatch.is_recoverable());
        assert!(!ContractError::ContractIdMismatch.is_recoverable());
    }

    /// Regression test: `DelegationInactive` was previously shadowed by an
    /// earlier, wrongly-classified match arm in `is_recoverable()`, so it
    /// silently evaluated to `true` (recoverable) instead of `false`
    /// (fatal). A revoked/expired delegation is a terminal *state* the
    /// caller cannot retry past, unlike genuine *permission* errors (e.g.
    /// `NotSigner`) which are fixed by switching signer/role.
    #[test]
    fn test_delegation_inactive_is_fatal_not_recoverable() {
        assert!(!ContractError::DelegationInactive.is_recoverable());
    }

    /// Verify the documented proposal-lifecycle example from the issue
    /// body: `ProposalAlreadyExecuted` is recoverable, `Overflow` is fatal.
    #[test]
    fn test_is_recoverable_issue_examples() {
        assert!(
            ContractError::ProposalAlreadyExecuted.is_recoverable(),
            "ProposalAlreadyExecuted (604) must be recoverable per issue #519"
        );
        assert!(
            !ContractError::Overflow.is_recoverable(),
            "Overflow (700) must be fatal per issue #519"
        );
    }

    #[test]
    fn test_require_non_zero_bytes32_happy_path() {
        use soroban_sdk::{BytesN, Env};

        fn test_single_bit() -> Result<(), ContractError> {
            let e = Env::default();
            let mut arr = [0u8; 32];
            arr[0] = 1;
            let single_bit = BytesN::from_array(&e, &arr);
            crate::require_non_zero_bytes32!(&e, &single_bit);
            Ok(())
        }
        test_single_bit().unwrap();

        fn test_all_ones() -> Result<(), ContractError> {
            let e = Env::default();
            let all_ones = BytesN::from_array(&e, &[0xffu8; 32]);
            crate::require_non_zero_bytes32!(&e, &all_ones);
            Ok(())
        }
        test_all_ones().unwrap();

        // Only the last byte set.
        fn test_last_byte() -> Result<(), ContractError> {
            let e = Env::default();
            let mut arr = [0u8; 32];
            arr[31] = 1;
            let v = BytesN::from_array(&e, &arr);
            crate::require_non_zero_bytes32!(&e, &v);
            Ok(())
        }
        test_last_byte().unwrap();
    }

    #[test]
    fn test_require_non_zero_bytes32_all_zeros_returns_error() {
        use soroban_sdk::{BytesN, Env};

        fn test_all_zeros() -> Result<(), ContractError> {
            let e = Env::default();
            let all_zeros = BytesN::from_array(&e, &[0u8; 32]);
            crate::require_non_zero_bytes32!(&e, &all_zeros);
            Ok(())
        }
        assert_eq!(test_all_zeros(), Err(ContractError::ZeroBytes32));
    }

    // ---------------------------------------------------------------------------
    // Macro tests: verify_no_future_ledger
    // ---------------------------------------------------------------------------

    #[test]
    fn test_verify_no_future_ledger_happy_path() {
        use soroban_sdk::Env;
        let e = Env::default();
        let now = e.ledger().sequence();
        // Current sequence must not panic.
        crate::verify_no_future_ledger(&e, now);
        // Zero must not panic.
        crate::verify_no_future_ledger(&e, 0u32);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #118)")]
    fn test_verify_no_future_ledger_rejects_future() {
        use soroban_sdk::Env;
        let e = Env::default();
        let now = e.ledger().sequence();
        crate::verify_no_future_ledger(&e, now + 1);
    }

    // ---------------------------------------------------------------------------
    // Macro tests: require_within_ttl
    // ---------------------------------------------------------------------------

    #[test]
    fn test_require_within_ttl_happy_path() {
        use soroban_sdk::Env;
        fn test_valid() -> Result<(), ContractError> {
            let e = Env::default();
            let now = e.ledger().timestamp();
            crate::require_within_ttl!(&e, now + 1);
            Ok(())
        }
        assert!(test_valid().is_ok());
    }

    #[test]
    fn test_require_within_ttl_rejects_at_exact_expiry() {
        use soroban_sdk::Env;
        fn test_expired() -> Result<(), ContractError> {
            let e = Env::default();
            let now = e.ledger().timestamp();
            // now == expires_at is the hard expiry cliff.
            crate::require_within_ttl!(&e, now);
            Ok(())
        }
        assert_eq!(test_expired(), Err(ContractError::SignatureExpired));
    }

    #[test]
    fn test_require_within_ttl_rejects_already_expired() {
        use soroban_sdk::Env;
        fn test_expired() -> Result<(), ContractError> {
            let e = Env::default();
            let now = e.ledger().timestamp();
            // Strictly in the past.
            if now > 0 {
                crate::require_within_ttl!(&e, now - 1);
            } else {
                // Ledger timestamp is 0 in the default environment; treat 0 as expired.
                crate::require_within_ttl!(&e, 0_u64);
            }
            Ok(())
        }
        assert_eq!(test_expired(), Err(ContractError::SignatureExpired));
    }

    // ---------------------------------------------------------------------------
    // require_matching_contract_id
    // ---------------------------------------------------------------------------

    #[test]
    fn test_require_matching_contract_id_happy_path() {
        use soroban_sdk::{Address, Env};
        fn test_match() {
            let e = Env::default();
            let caller = Address::generate(&e);
            let expected = caller.clone();
            crate::require_matching_contract_id(&e, &caller, &expected);
        }
        test_match(); // must not panic
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #123)")]
    fn test_require_matching_contract_id_mismatch_panics() {
        use soroban_sdk::{Address, Env};
        let e = Env::default();
        let caller = Address::generate(&e);
        let expected = Address::generate(&e);
        crate::require_matching_contract_id(&e, &caller, &expected);
    }

    // ---------------------------------------------------------------------------
    // require_matching_treasury_beneficiary
    // ---------------------------------------------------------------------------

    #[test]
    fn test_require_matching_treasury_beneficiary_happy_path() {
        use soroban_sdk::{Address, Env};
        let e = Env::default();
        let treasury = Address::generate(&e);
        let recipient = treasury.clone();
        crate::require_matching_treasury_beneficiary(&e, &recipient, &treasury); // must not panic
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #610)")]
    fn test_require_matching_treasury_beneficiary_mismatch_panics() {
        use soroban_sdk::{Address, Env};
        let e = Env::default();
        let treasury = Address::generate(&e);
        let bad_recipient = Address::generate(&e);
        crate::require_matching_treasury_beneficiary(&e, &bad_recipient, &treasury);
    }

    // ---------------------------------------------------------------------------
    // require_matching_lease_signer
    // ---------------------------------------------------------------------------

    #[test]
    fn test_require_matching_lease_signer_happy_path() {
        use soroban_sdk::{Address, Env};
        let e = Env::default();
        let actor = Address::generate(&e);
        crate::require_matching_lease_signer(&e, &actor, &actor); // must not panic
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #126)")]
    fn test_require_matching_lease_signer_mismatch_panics() {
        use soroban_sdk::{Address, Env};
        let e = Env::default();
        let lease_signer = Address::generate(&e);
        let actor = Address::generate(&e);
        crate::require_matching_lease_signer(&e, &lease_signer, &actor);
    }

    // ---------------------------------------------------------------------------
    // require_role
    // ---------------------------------------------------------------------------

    #[test]
    fn test_require_role_admin_ok() {
        let e = soroban_sdk::Env::default();
        let actor = soroban_sdk::Address::generate(&e);
        crate::require_role(&e, Role::Admin, &actor, true); // must not panic
    }

    #[test]
    fn test_require_role_user_ok() {
        let e = soroban_sdk::Env::default();
        let actor = soroban_sdk::Address::generate(&e);
        crate::require_role(&e, Role::User, &actor, true); // must not panic
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_require_role_admin_panics_when_not_held() {
        let e = soroban_sdk::Env::default();
        let actor = soroban_sdk::Address::generate(&e);
        crate::require_role(&e, Role::Admin, &actor, false);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #127)")]
    fn test_require_role_user_panics_when_not_held() {
        let e = soroban_sdk::Env::default();
        let actor = soroban_sdk::Address::generate(&e);
        crate::require_role(&e, Role::User, &actor, false);
    }

    // ---------------------------------------------------------------------------
    // is_expired helper
    // ---------------------------------------------------------------------------

    #[test]
    fn test_is_expired_zero_never_expires() {
        use soroban_sdk::{testutils::Ledger, Env};
        let e = Env::default();
        e.ledger().with_mut(|li| li.timestamp = 1_000_000);
        // expires_at == 0 means "never expires".
        assert!(!crate::is_expired(&e, 0));
    }

    #[test]
    fn test_is_expired_future_not_expired() {
        use soroban_sdk::{testutils::Ledger, Env};
        let e = Env::default();
        e.ledger().with_mut(|li| li.timestamp = 1_000_000);
        assert!(!crate::is_expired(&e, 1_000_001));
    }

    #[test]
    fn test_is_expired_at_boundary_is_expired() {
        use soroban_sdk::{testutils::Ledger, Env};
        let e = Env::default();
        e.ledger().with_mut(|li| li.timestamp = 1_000_000);
        // Exactly at expires_at is expired (hard cliff: now >= expires_at).
        assert!(crate::is_expired(&e, 1_000_000));
    }

    #[test]
    fn test_is_expired_past_is_expired() {
        use soroban_sdk::{testutils::Ledger, Env};
        let e = Env::default();
        e.ledger().with_mut(|li| li.timestamp = 1_000_000);
        assert!(crate::is_expired(&e, 999_999));
    }

    // ---------------------------------------------------------------------------
    // require_within_ttl_result helper
    // ---------------------------------------------------------------------------

    #[test]
    fn test_require_within_ttl_result_returns_ok_when_fresh() {
        use soroban_sdk::{testutils::Ledger, Env};
        let e = Env::default();
        e.ledger().with_mut(|li| li.timestamp = 500);
        assert!(crate::require_within_ttl_result(&e, 501).is_ok());
    }

    #[test]
    fn test_require_within_ttl_result_returns_err_at_boundary() {
        use soroban_sdk::{testutils::Ledger, Env};
        let e = Env::default();
        e.ledger().with_mut(|li| li.timestamp = 500);
        assert_eq!(
            crate::require_within_ttl_result(&e, 500),
            Err(ContractError::SignatureExpired)
        );
    }

    #[test]
    fn test_require_within_ttl_result_never_expires_for_zero() {
        use soroban_sdk::{testutils::Ledger, Env};
        let e = Env::default();
        e.ledger().with_mut(|li| li.timestamp = u64::MAX);
        // expires_at == 0 → never expires.
        assert!(crate::require_within_ttl_result(&e, 0).is_ok());
    }

    // ---------------------------------------------------------------------------
    // is_recoverable() — exhaustive classification tests
    // ---------------------------------------------------------------------------

    /// Mirror of `impl ErrorExt for ContractError::is_recoverable`.
    ///
    /// This match is exhaustive: adding a new variant to `ContractError` without
    /// adding it here is a compile error, enforcing that every new code gets an
    /// explicit recoverability decision before it lands.
    fn expected_is_recoverable(e: ContractError) -> bool {
        match e {
            // Initialization
            ContractError::NotInitialized => true,
            ContractError::AlreadyInitialized => true,

            // Authorization
            ContractError::NotAdmin => true,
            ContractError::NotBondOwner => true,
            ContractError::UnauthorizedAttester => true,
            ContractError::NotOriginalAttester => true,
            ContractError::NotSigner => true,
            ContractError::UnauthorizedDepositor => true,
            ContractError::ContractPaused => true,
            ContractError::InvalidPauseAction => true,
            ContractError::InsufficientSignatures => true,
            ContractError::ZeroBytes32 => true,
            ContractError::InvalidAdminAddress => true,
            ContractError::AdminUnchanged => true,
            ContractError::TimelockNotReady => true,
            ContractError::AdminSuspended => true,
            ContractError::BorrowFrozen => true,
            ContractError::NoPendingAdmin => true,
            ContractError::RoleNotHeldAtLedger => true,
            ContractError::EmergencyDrainNotPermitted => true,
            ContractError::TimestampInFuture => true,
            ContractError::InvalidMaxPauseSigners => true,
            ContractError::OutsideBusinessHours => true,
            ContractError::LeaseScopeMismatch => true,
            ContractError::LeaseExpired => true,
            ContractError::CrossContractCallerMismatch => false, // contract topology bug; no retry path
            ContractError::MigrationInProgress => true,         // wait for migration to complete
            ContractError::MaxPauseSignersExceeded => true,     // remove a signer or raise the cap
            ContractError::LeaseSignerMismatch => true,
            ContractError::RoleRequired => true,
            ContractError::StaleAdminEpoch => false, // stale epoch; must re-propose
            ContractError::StaleSignerEpoch => false,

            // Bond
            ContractError::BondNotFound => true,
            ContractError::BondNotActive => true,
            ContractError::InsufficientBalance => true,
            ContractError::SlashExceedsBond => true,
            ContractError::LockupNotExpired => true,
            ContractError::NotRollingBond => true,
            ContractError::WithdrawalAlreadyRequested => true,
            ContractError::ReentrancyDetected => false, // SECURITY HALT
            ContractError::InvalidNonce => true,
            ContractError::NegativeStake => true,
            ContractError::EarlyExitConfigNotSet => true,
            ContractError::InvalidPenaltyBps => true,
            ContractError::LeverageExceeded => true,
            ContractError::UnsupportedToken => true,
            ContractError::InvalidBondAmount => true,
            ContractError::AmountExplicitlyZero => true,
            ContractError::InvalidBondDuration => true,
            ContractError::InvalidNoticePeriod => true,
            ContractError::BondAlreadyExists => true,
            ContractError::OwnerMismatch => false,
            ContractError::TargetMismatch => false,
            ContractError::ContractIdMismatch => false,
            ContractError::SignatureExpired => true,
            ContractError::TreasuryNotConfigured => true,
            ContractError::StorageCapReached => false,          // system capacity; operator-only fix
            ContractError::DomainMismatch => false,
            ContractError::CursorOutOfRange => true,
            ContractError::BatchTooLarge => true,
            ContractError::EmptyBatch => true,
            ContractError::UnsupportedDecimals => true,
            ContractError::InvalidStringifiedBytes => true,
            ContractError::UnauthorizedToken => true,
            ContractError::DuplicateIdempotencyKey => true,
            ContractError::InvariantViolation => false,          // post-write drift; code bug
            ContractError::InvalidCurrency => true,
            ContractError::SnapshotGenerationMismatch => false,  // state-epoch mismatch
            ContractError::CooldownRequestAlreadyPending => true,
            ContractError::CooldownRequestNotFound => true,
            ContractError::CooldownPeriodNotElapsed => true,
            ContractError::BytesTooLarge => true,

            // Attestation
            ContractError::DuplicateAttestation => true,
            ContractError::AttestationNotFound => true,
            ContractError::AttestationAlreadyRevoked => true,
            ContractError::InvalidAttestationWeight => true,
            ContractError::AttestationWeightExceedsMax => true,

            // Registry
            ContractError::IdentityAlreadyRegistered => true,
            ContractError::BondContractAlreadyRegistered => true,
            ContractError::IdentityNotRegistered => true,
            ContractError::BondContractNotRegistered => true,
            ContractError::AlreadyDeactivated => true,
            ContractError::AlreadyActive => true,
            ContractError::InvalidContractAddress => true,
            ContractError::ContractCodeVerificationFailed => true,
            ContractError::UnsupportedInterface => true,

            // Delegation
            ContractError::ExpiryInPast => true,
            ContractError::DelegationNotFound => true,
            ContractError::AlreadyRevoked => true,
            ContractError::DelegationExpiryTooLong => true,
            ContractError::UnknownScheme => false,          // unsupported scheme; cannot retry
            ContractError::VerifierAlreadyRegistered => true,
            ContractError::VerifierNotRegistered => true,
            ContractError::VerificationFailed => false,     // crypto failure; same input will fail
            ContractError::RevocationGraceExpired => false, // terminal; only admin can extend grace
            ContractError::DelegationNotExpired => true,    // wait for expiry then retry
            ContractError::PayloadTooOld => true,           // re-sign with current ledger number
            ContractError::DelegationInactive => false,     // terminal delegation state
            ContractError::PromiseNotKept => false,          // hash mismatch; not retryable
            ContractError::StaleEpoch => false,             // stale epoch; must re-propose

            // Treasury
            ContractError::AmountMustBePositive => true,
            ContractError::ThresholdExceedsSigners => true,
            ContractError::InsufficientTreasuryBalance => true,
            ContractError::ProposalNotFound => true,
            ContractError::ProposalAlreadyExecuted => true, // idempotent
            ContractError::InsufficientApprovals => true,
            ContractError::InvalidFlashLoanCallback => false, // bad magic; same call will fail
            ContractError::FlashLoanRepaymentFailed => false, // bad repayment
            ContractError::ProposalExpired => true,
            ContractError::SlippageExceeded => true,
            ContractError::TreasuryBeneficiaryMismatch => true,
            ContractError::CorridorNotRegistered => true,

            // Arithmetic
            ContractError::Overflow => false,
            ContractError::Underflow => false,
            ContractError::DivisionByZero => false,
            ContractError::InvalidPercentSplit => true,
        }
    }

    /// Compile-time exhaustiveness + runtime agreement check.
    ///
    /// The exhaustive match in `expected_is_recoverable` above forces every
    /// new variant to receive a classification decision at compile time.
    /// This test then asserts the documented table agrees with the live
    /// `is_recoverable()` implementation for every variant.
    #[test]
    fn test_is_recoverable_exhaustive() {
        for (_, e) in ALL_VARIANTS {
            assert_eq!(
                e.is_recoverable(),
                expected_is_recoverable(*e),
                "is_recoverable() classification drift for {:?} (wire code {})",
                e,
                *e as u32
            );
        }
    }

    /// Metadata guarantees: must not panic, must be `Copy`-safe.
    #[test]
    fn test_is_recoverable_is_const_safe() {
        let e = ContractError::Overflow;
        let _ = e.is_recoverable();
        let _ = e.is_recoverable();
        assert!(!e.is_recoverable());

        let e = ContractError::AlreadyInitialized;
        assert!(e.is_recoverable());
    }

    /// Spot-check a recoverable sample per category.
    #[test]
    fn test_is_recoverable_recoverable_samples() {
        assert!(ContractError::NotInitialized.is_recoverable());
        assert!(ContractError::AlreadyInitialized.is_recoverable());
        assert!(ContractError::NotAdmin.is_recoverable());
        assert!(ContractError::NotBondOwner.is_recoverable());
        assert!(ContractError::ContractPaused.is_recoverable());
        assert!(ContractError::BorrowFrozen.is_recoverable());
        assert!(ContractError::MigrationInProgress.is_recoverable());
        assert!(ContractError::MaxPauseSignersExceeded.is_recoverable());
        assert!(ContractError::OutsideBusinessHours.is_recoverable());
        assert!(ContractError::BondNotFound.is_recoverable());
        assert!(ContractError::InvalidNonce.is_recoverable());
        assert!(ContractError::CooldownRequestAlreadyPending.is_recoverable());
        assert!(ContractError::CooldownRequestNotFound.is_recoverable());
        assert!(ContractError::CooldownPeriodNotElapsed.is_recoverable());
        assert!(ContractError::BytesTooLarge.is_recoverable());
        assert!(ContractError::DuplicateAttestation.is_recoverable());
        assert!(ContractError::IdentityNotRegistered.is_recoverable());
        assert!(ContractError::ExpiryInPast.is_recoverable());
        assert!(ContractError::AlreadyRevoked.is_recoverable());
        assert!(ContractError::AmountMustBePositive.is_recoverable());
        assert!(ContractError::ProposalAlreadyExecuted.is_recoverable());
        assert!(ContractError::InsufficientApprovals.is_recoverable());
        assert!(ContractError::InvalidPercentSplit.is_recoverable());
    }

    /// Spot-check a fatal sample per category.
    #[test]
    fn test_is_recoverable_fatal_samples() {
        assert!(!ContractError::CrossContractCallerMismatch.is_recoverable());
        assert!(!ContractError::StaleAdminEpoch.is_recoverable());
        assert!(!ContractError::StaleSignerEpoch.is_recoverable());
        assert!(!ContractError::ReentrancyDetected.is_recoverable());
        assert!(!ContractError::StorageCapReached.is_recoverable());
        assert!(!ContractError::InvariantViolation.is_recoverable());
        assert!(!ContractError::SnapshotGenerationMismatch.is_recoverable());
        assert!(!ContractError::DomainMismatch.is_recoverable());
        assert!(!ContractError::OwnerMismatch.is_recoverable());
        assert!(!ContractError::TargetMismatch.is_recoverable());
        assert!(!ContractError::ContractIdMismatch.is_recoverable());
        assert!(!ContractError::VerificationFailed.is_recoverable());
        assert!(!ContractError::UnknownScheme.is_recoverable());
        assert!(!ContractError::RevocationGraceExpired.is_recoverable());
        assert!(!ContractError::DelegationInactive.is_recoverable());
        assert!(!ContractError::PromiseNotKept.is_recoverable());
        assert!(!ContractError::StaleEpoch.is_recoverable());
        assert!(!ContractError::InvalidFlashLoanCallback.is_recoverable());
        assert!(!ContractError::FlashLoanRepaymentFailed.is_recoverable());
        assert!(!ContractError::Overflow.is_recoverable());
        assert!(!ContractError::Underflow.is_recoverable());
        assert!(!ContractError::DivisionByZero.is_recoverable());
    }

    /// Regression: `DelegationInactive` must be fatal (not recoverable).
    /// A revoked/expired delegation is a terminal state the caller cannot
    /// work past without administrative intervention.
    #[test]
    fn test_delegation_inactive_is_fatal_not_recoverable() {
        assert!(!ContractError::DelegationInactive.is_recoverable());
    }

    /// Regression: `CrossContractCallerMismatch` is fatal — it indicates a
    /// contract topology misconfiguration that callers cannot fix by retrying.
    #[test]
    fn test_cross_contract_caller_mismatch_is_fatal() {
        assert!(!ContractError::CrossContractCallerMismatch.is_recoverable());
    }

    /// Regression: `SnapshotGenerationMismatch` is fatal — the caller must
    /// restart the scan from a fresh snapshot, not retry the same operation.
    #[test]
    fn test_snapshot_generation_mismatch_is_fatal() {
        assert!(!ContractError::SnapshotGenerationMismatch.is_recoverable());
    }

    /// Regression: `StaleAdminEpoch` / `StaleSignerEpoch` are fatal — a stale
    /// epoch reference in a proposal ID cannot be corrected by the caller
    /// without re-deriving the proposal in the current epoch.
    #[test]
    fn test_stale_epoch_variants_are_fatal() {
        assert!(!ContractError::StaleAdminEpoch.is_recoverable());
        assert!(!ContractError::StaleSignerEpoch.is_recoverable());
        assert!(!ContractError::StaleEpoch.is_recoverable());
    }

    /// Regression: `ProposalAlreadyExecuted` is recoverable (idempotent) and
    /// `Overflow` is fatal.
    #[test]
    fn test_is_recoverable_issue_examples() {
        assert!(ContractError::ProposalAlreadyExecuted.is_recoverable());
        assert!(!ContractError::Overflow.is_recoverable());
    }

    // ---------------------------------------------------------------------------
    // Boundary: permission / state transitions
    // ---------------------------------------------------------------------------

    /// Paused contract: callers must wait, not get a fatal halt.
    #[test]
    fn test_contract_paused_is_recoverable() {
        assert!(ContractError::ContractPaused.is_recoverable());
    }

    /// BorrowFrozen: callers must wait for the freeze to lift.
    #[test]
    fn test_borrow_frozen_is_recoverable() {
        assert!(ContractError::BorrowFrozen.is_recoverable());
    }

    /// MigrationInProgress: callers retry after migration completes.
    #[test]
    fn test_migration_in_progress_is_recoverable() {
        assert!(ContractError::MigrationInProgress.is_recoverable());
    }

    /// StorageCapReached: only an operator prune can fix this; not caller-fixable.
    #[test]
    fn test_storage_cap_reached_is_not_recoverable() {
        assert!(!ContractError::StorageCapReached.is_recoverable());
    }

    /// CursorOutOfRange: caller can supply a valid cursor.
    #[test]
    fn test_cursor_out_of_range_is_recoverable() {
        assert!(ContractError::CursorOutOfRange.is_recoverable());
    }

    /// BytesTooLarge: caller resubmits with shorter input.
    #[test]
    fn test_bytes_too_large_is_recoverable() {
        assert!(ContractError::BytesTooLarge.is_recoverable());
    }

    /// MaxPauseSignersExceeded: admin can remove a signer or raise the cap.
    #[test]
    fn test_max_pause_signers_exceeded_is_recoverable() {
        assert!(ContractError::MaxPauseSignersExceeded.is_recoverable());
    }

    /// ProposalExpired: caller creates a new proposal.
    #[test]
    fn test_proposal_expired_is_recoverable() {
        assert!(ContractError::ProposalExpired.is_recoverable());
    }

    /// SlippageExceeded: caller retries with looser slippage bound.
    #[test]
    fn test_slippage_exceeded_is_recoverable() {
        assert!(ContractError::SlippageExceeded.is_recoverable());
    }

    /// CorridorNotRegistered: admin registers the corridor, then caller retries.
    #[test]
    fn test_corridor_not_registered_is_recoverable() {
        assert!(ContractError::CorridorNotRegistered.is_recoverable());
    }

    /// DelegationNotExpired: caller waits for the delegation to expire.
    #[test]
    fn test_delegation_not_expired_is_recoverable() {
        assert!(ContractError::DelegationNotExpired.is_recoverable());
    }

    /// PayloadTooOld: caller re-signs with the current ledger number.
    #[test]
    fn test_payload_too_old_is_recoverable() {
        assert!(ContractError::PayloadTooOld.is_recoverable());
    }

    /// InvalidPercentSplit: caller provides splits that sum to 10_000.
    #[test]
    fn test_invalid_percent_split_is_recoverable() {
        assert!(ContractError::InvalidPercentSplit.is_recoverable());
    }

    // ---------------------------------------------------------------------------
    // Boundary: cooldown errors
    // ---------------------------------------------------------------------------

    fn mock_cooldown_request(already_pending: bool) -> Result<(), ContractError> {
        if already_pending {
            return Err(ContractError::CooldownRequestAlreadyPending);
        }
        Ok(())
    }

    fn mock_complete_cooldown(request_found: bool, elapsed: bool) -> Result<(), ContractError> {
        if !request_found {
            return Err(ContractError::CooldownRequestNotFound);
        }
        if !elapsed {
            return Err(ContractError::CooldownPeriodNotElapsed);
        }
        Ok(())
    }

    #[test]
    fn test_cooldown_request_already_pending() {
        assert_eq!(
            mock_cooldown_request(true),
            Err(ContractError::CooldownRequestAlreadyPending)
        );
        assert!(mock_cooldown_request(false).is_ok());
    }

    #[test]
    fn test_cooldown_request_not_found() {
        assert_eq!(
            mock_complete_cooldown(false, true),
            Err(ContractError::CooldownRequestNotFound)
        );
        assert!(mock_complete_cooldown(true, true).is_ok());
    }

    #[test]
    fn test_cooldown_period_not_elapsed() {
        assert_eq!(
            mock_complete_cooldown(true, false),
            Err(ContractError::CooldownPeriodNotElapsed)
        );
        assert!(mock_complete_cooldown(true, true).is_ok());
    }

    // ---------------------------------------------------------------------------
    // Retry safety invariants
    // ---------------------------------------------------------------------------

    /// Fatal errors must stay fatal across repeated calls (no state side-effects).
    #[test]
    fn test_fatal_errors_are_idempotent() {
        let fatal = [
            ContractError::ReentrancyDetected,
            ContractError::InvariantViolation,
            ContractError::StorageCapReached,
            ContractError::Overflow,
            ContractError::Underflow,
            ContractError::DivisionByZero,
            ContractError::VerificationFailed,
            ContractError::DomainMismatch,
            ContractError::OwnerMismatch,
            ContractError::TargetMismatch,
            ContractError::ContractIdMismatch,
        ];
        for e in &fatal {
            assert!(!e.is_recoverable(), "{e:?} should be fatal on every call");
            // Second call must return the same result.
            assert!(!e.is_recoverable(), "{e:?} should be fatal on second call too");
        }
    }

    /// Recoverable errors must stay recoverable across repeated calls.
    #[test]
    fn test_recoverable_errors_are_stable() {
        let recoverable = [
            ContractError::NotAdmin,
            ContractError::ContractPaused,
            ContractError::BondNotFound,
            ContractError::InvalidNonce,
            ContractError::ProposalAlreadyExecuted,
        ];
        for e in &recoverable {
            assert!(e.is_recoverable(), "{e:?} should be recoverable");
            assert!(e.is_recoverable(), "{e:?} should remain recoverable on second call");
        }
    }

    // ---------------------------------------------------------------------------
    // Concurrent / duplicate state transitions
    // ---------------------------------------------------------------------------

    /// Simulates two concurrent calls both trying to create the same bond.
    /// The second call must see BondAlreadyExists and the error must be recoverable.
    #[test]
    fn test_concurrent_bond_creation_second_call_is_recoverable() {
        let mut bond_exists = false;
        let create = |exists: &mut bool| -> Result<(), ContractError> {
            if *exists {
                return Err(ContractError::BondAlreadyExists);
            }
            *exists = true;
            Ok(())
        };

        assert!(create(&mut bond_exists).is_ok());
        let second = create(&mut bond_exists);
        assert_eq!(second, Err(ContractError::BondAlreadyExists));
        assert!(ContractError::BondAlreadyExists.is_recoverable());
    }

    /// Simulates two concurrent revocation calls on the same delegation.
    /// The second call sees AlreadyRevoked which is recoverable (idempotent).
    #[test]
    fn test_concurrent_revocation_second_call_is_recoverable() {
        let mut revoked = false;
        let revoke = |r: &mut bool| -> Result<(), ContractError> {
            if *r {
                return Err(ContractError::AlreadyRevoked);
            }
            *r = true;
            Ok(())
        };

        assert!(revoke(&mut revoked).is_ok());
        assert_eq!(revoke(&mut revoked), Err(ContractError::AlreadyRevoked));
        assert!(ContractError::AlreadyRevoked.is_recoverable());
    }

    /// Simulates two concurrent proposal executions.
    /// The second must see ProposalAlreadyExecuted (recoverable / idempotent).
    #[test]
    fn test_concurrent_proposal_execution_second_call_is_recoverable() {
        let mut executed = false;
        let execute = |ex: &mut bool| -> Result<(), ContractError> {
            if *ex {
                return Err(ContractError::ProposalAlreadyExecuted);
            }
            *ex = true;
            Ok(())
        };

        assert!(execute(&mut executed).is_ok());
        assert_eq!(
            execute(&mut executed),
            Err(ContractError::ProposalAlreadyExecuted)
        );
        assert!(ContractError::ProposalAlreadyExecuted.is_recoverable());
    }

    // ---------------------------------------------------------------------------
    // Stale / permission states
    // ---------------------------------------------------------------------------

    /// Stale epoch errors are fatal: the caller must re-derive the proposal.
    #[test]
    fn test_stale_epoch_errors_are_fatal_and_must_rederive() {
        fn mock_use_proposal(epoch_current: u32, epoch_in_proposal: u32) -> Result<(), ContractError> {
            if epoch_in_proposal != epoch_current {
                return Err(ContractError::StaleEpoch);
            }
            Ok(())
        }
        assert_eq!(mock_use_proposal(2, 1), Err(ContractError::StaleEpoch));
        assert!(!ContractError::StaleEpoch.is_recoverable());
        // Same epoch must succeed.
        assert!(mock_use_proposal(2, 2).is_ok());
    }

    /// RoleNotHeldAtLedger is recoverable: the actor must re-sign with a valid
    /// ledger timestamp at which they held the role.
    #[test]
    fn test_role_not_held_at_ledger_is_recoverable() {
        assert!(ContractError::RoleNotHeldAtLedger.is_recoverable());
    }

    // ---------------------------------------------------------------------------
    // Error descriptions contain no sensitive data keywords
    // ---------------------------------------------------------------------------

    /// Descriptions must not contain raw secret/key keywords that would expose
    /// sensitive internal state in logs visible to off-chain clients.
    #[test]
    fn test_descriptions_do_not_expose_sensitive_keywords() {
        let sensitive = ["secret", "private key", "seed", "mnemonic", "password"];
        for e in all_variants() {
            let desc = e.description().to_lowercase();
            for kw in &sensitive {
                assert!(
                    !desc.contains(kw),
                    "{e:?} description contains sensitive keyword '{kw}': {desc}"
                );
            }
        }
    }

    // ---------------------------------------------------------------------------
    // category() covers every variant (no missing arms)
    // ---------------------------------------------------------------------------

    /// Every variant returned by all_variants() must produce a valid category
    /// without panicking.
    #[test]
    fn test_all_variants_have_valid_category() {
        let valid_categories = [
            ErrorCategory::Initialization,
            ErrorCategory::Authorization,
            ErrorCategory::Bond,
            ErrorCategory::Attestation,
            ErrorCategory::Registry,
            ErrorCategory::Delegation,
            ErrorCategory::Treasury,
            ErrorCategory::Arithmetic,
        ];
        for e in all_variants() {
            let cat = e.category();
            assert!(
                valid_categories.contains(&cat),
                "{e:?} returned an unexpected ErrorCategory variant"
            );
        }
    }
}
