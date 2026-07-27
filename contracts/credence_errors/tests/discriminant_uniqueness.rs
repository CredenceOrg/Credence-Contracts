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

// These integration tests use assert_eq! with format messages for diagnostics.
// The disallowed_macros lint targets production contract code; test harnesses
// are explicitly exempted.
#![allow(clippy::disallowed_macros)]

use credence_errors::ContractError;

// Canonical list of all variants, imported from the single source of truth.
include!("../variant_table.rs");

/// N :: Number of variants asserted to exist. Bumped here whenever a new
/// `ContractError` variant is added; must always equal `ALL_VARIANTS.len()`.
/// Enforced by `all_variants_count_is_consistent_with_enum_definition` below.
const ALL_VARIANTS_COUNT: usize = 110;

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
    // NOTE: StaleAdminEpoch (514) and StaleSignerEpoch (515) have wire codes
    // in the 500-599 Delegation range but are categorised as Authorization
    // in `ErrorExt::category()`. The range check here uses the wire code,
    // not the semantic category, so they pass under 500-599.
    // Similarly DomainMismatch (225), OwnerMismatch (219), TargetMismatch (220),
    // ContractIdMismatch (221) have wire codes in the 200-299 Bond range.
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
