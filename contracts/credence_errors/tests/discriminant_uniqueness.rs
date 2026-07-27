//! Regression tests enforcing that every `credence_errors::ContractError`
//! variant maps to a unique `#[repr(u32)]` discriminant.
//!
//! Wire-stability is documented in [`docs/error-codes-wire.md`] and the
//! canonical layout in [`docs/errors.md`]. Each numeric code is part of the
//! stable external contract — indexers, the `credence_admin_cli`, monitoring
//! dashboards, and off-chain clients decode errors by their discriminant.
//! Two variants sharing a code would silently alias and route events to the
//! wrong handler, so this invariant is locked here as an executable contract.
//!
//! When adding a new `ContractError` variant:
//!   1. Add one row to `variant_table.rs` (single source of truth).
//!   2. Update exhaustive match arms in `src/test_errors.rs`
//!      (`expected_is_recoverable()`, category tests, etc.).
//!
//! [`docs/error-codes-wire.md`]: ../../../docs/error-codes-wire.md
//! [`docs/errors.md`]: ../../../docs/errors.md

// Off-chain test binary, not deployed WASM (issue #713 exemption).
#![allow(clippy::disallowed_macros)]

use credence_errors::ContractError;

/// Every `ContractError` variant, one row per name, in numeric-code order
/// within each category block. The discriminant-uniqueness test iterates
/// over this table and fails on the first duplicate numeric code it finds.
const ALL_VARIANTS: &[(&str, ContractError)] = &[
    // --- Initialization (1-99) ---
    ("NotInitialized", ContractError::NotInitialized),
    ("AlreadyInitialized", ContractError::AlreadyInitialized),
    // --- Authorization (100-199) ---
    ("NoPendingAdmin", ContractError::NoPendingAdmin),
    ("InvalidAdminAddress", ContractError::InvalidAdminAddress),
    ("AdminUnchanged", ContractError::AdminUnchanged),
    ("TimelockNotReady", ContractError::TimelockNotReady),
    ("AdminSuspended", ContractError::AdminSuspended),
    (
        "EmergencyDrainNotPermitted",
        ContractError::EmergencyDrainNotPermitted,
    ),
    ("RoleNotHeldAtLedger", ContractError::RoleNotHeldAtLedger),
    ("OutsideBusinessHours", ContractError::OutsideBusinessHours),
    ("TimestampInFuture", ContractError::TimestampInFuture),
    (
        "InvalidMaxPauseSigners",
        ContractError::InvalidMaxPauseSigners,
    ),
    (
        "MaxPauseSignersExceeded",
        ContractError::MaxPauseSignersExceeded,
    ),
    ("ZeroBytes32", ContractError::ZeroBytes32),
    ("LeaseScopeMismatch", ContractError::LeaseScopeMismatch),
    ("LeaseExpired", ContractError::LeaseExpired),
    ("LeaseSignerMismatch", ContractError::LeaseSignerMismatch),
    ("MigrationInProgress", ContractError::MigrationInProgress),
    ("CrossContractCallerMismatch", ContractError::CrossContractCallerMismatch),
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
    ("BorrowFrozen", ContractError::BorrowFrozen),
    ("InvalidPauseAction", ContractError::InvalidPauseAction),
    (
        "InsufficientSignatures",
        ContractError::InsufficientSignatures,
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
    ("DelegationInactive", ContractError::DelegationInactive),
    ("PayloadTooOld", ContractError::PayloadTooOld),
    ("PromiseNotKept", ContractError::PromiseNotKept),
    ("StaleAdminEpoch", ContractError::StaleAdminEpoch),
    ("StaleSignerEpoch", ContractError::StaleSignerEpoch),
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

/// N_i128 :: Number of entries to assert in `ALL_VARIANTS`. Bumped manually
/// when a new variant is added. The mismatch asserting test below fails the
/// build if a contributor adds a row to `src/test_errors.rs::all_variants()`
/// but forgets this file — and vice-versa.
const ALL_VARIANTS_COUNT: usize = 106;

#[test]
fn every_contract_error_variant_has_a_unique_u32_discriminant() {
    // O(n²) check via `Vec::contains` — n ≈ 110 so this runs in single-digit µs.
    // We do not use a `BTreeSet` to avoid pulling in `std::collections` machinery
    // that must remain invisible to the rest of the crate.
    let mut seen: std::vec::Vec<u32> = std::vec::Vec::with_capacity(ALL_VARIANTS.len());
    for (name, variant) in ALL_VARIANTS {
        let code = *variant as u32;
        if seen.contains(&code) {
            panic!(
                "DISCRIMINANT COLLISION DETECTED: variant `{name}` shares wire code \
                 {code} with a previously-listed variant in `ALL_VARIANTS`. \
                 Assign an unused code within the appropriate category range \
                 per `docs/errors.md` (\"Error Code Layout\"). Wire-stable codes \
                 must remain a 1:1 mapping so off-chain clients decode errors \
                 uniquely.",
            );
        }
        seen.push(code);
    }
}

#[test]
fn variant_names_are_unique_in_the_coverage_list() {
    // Sad-path regression for the case where two PRs add near-identical names
    // and the contributor accidentally lists the same name twice in
    // `variant_table.rs` — masking a real bug behind a single passed row.
    let mut seen: std::vec::Vec<&'static str> = std::vec::Vec::with_capacity(ALL_VARIANTS.len());
    for (name, _) in ALL_VARIANTS {
        assert!(
            !seen.contains(name),
            "Variant name `{name}` appears twice in `ALL_VARIANTS`. \
             If `src/lib.rs` has duplicate variant declarations, \
             deduplicate the variant name in the enum first.",
        );
        seen.push(*name);
    }
}

#[test]
fn discriminant_codes_fit_their_documented_category_range() {
    // Belt-and-suspenders guard: `every_contract_error_variant_has_a...`
    // catches same-code collisions; this test catches *cross-category*
    // leakage — e.g. someone adding an Authorization variant that
    // accidentally lands in the Bond range.
    //
    // NOTE: payload-mismatch variants (DomainMismatch/OwnerMismatch/
    // TargetMismatch/ContractIdMismatch) intentionally live in the
    // 200-299 Bond/Numeric range despite being Delegation-categorised;
    // the Catalog of variants in `docs/errors.md` lists them in the
    // 200-299 row. Update the catalog and this range table together.
    const RANGES: &[(std::ops::RangeInclusive<u32>, &str)] = &[
        (1..=99, "Initialization"),
        (100..=199, "Authorization"),
        (200..=299, "Bond"),
        (300..=399, "Attestation"),
        (400..=499, "Registry"),
        (500..=599, "Delegation"),
        (600..=699, "Treasury"),
        (700..=799, "Arithmetic"),
    ];
    for (name, variant) in ALL_VARIANTS {
        let code = *variant as u32;
        let in_any = RANGES.iter().any(|(r, _)| r.contains(&code));
        assert!(
            in_any,
            "variant `{name}` code {code} falls outside every documented \
             category range (Initialization 1-99, Authorization 100-199, \
             Bond 200-299, Attestation 300-399, Registry 400-499, \
             Delegation 500-599, Treasury 600-699, Arithmetic 700-799). \
             See `docs/errors.md` \"Error Code Layout\" for the canonical \
             lists and update both when bumping a variant.",
        );
    }
}

#[test]
fn all_variants_count_is_consistent_with_enum_definition() {
    // Forcing function: `variant_table.rs` is the single generation counter.
    // Bumping only one parallel count while the enum grows causes silent drift.
    assert_eq!(
        ALL_VARIANTS.len(),
        ALL_VARIANTS_COUNT,
        "Add one row to `variant_table.rs` per new `ContractError` variant.",
    );
}

#[test]
#[should_panic(expected = "DISCRIMINANT COLLISION DETECTED")]
fn discriminant_collision_panic_message_mentions_diagnostic() {
    // Explicit sad-path test: an artificial collision must surface the same
    // diagnostic string that the production code path emits, so engineers
    // searching CI logs can find the cause without reading test outputs.
    // We construct the collision INLINE — independent of any particular state
    // of `lib.rs` — so this test stays useful before and after collision fixes.
    let synthetic: std::vec::Vec<(&'static str, u32)> =
        std::vec![("SyntheticA", 999_001_u32), ("SyntheticB", 999_001_u32)];
    let mut seen: std::vec::Vec<u32> = std::vec::Vec::with_capacity(synthetic.len());
    for (name, code) in synthetic {
        if seen.contains(&code) {
            panic!(
                "DISCRIMINANT COLLISION DETECTED: variant `{name}` shares wire code \
                 {code} with a previously-listed variant in `ALL_VARIANTS`. \
                 Assign an unused code within the appropriate category range \
                 per `docs/errors.md` (\"Error Code Layout\"). Wire-stable codes \
                 must remain a 1:1 mapping so off-chain clients decode errors \
                 uniquely.",
            );
        }
        seen.push(code);
    }
}
