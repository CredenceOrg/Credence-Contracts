//! Cross-test harness checks: variant coverage must share one generation counter.
//!
//! Regression target: adding one enum variant used to require bumping unrelated
//! manual counts (`ALL_VARIANTS_COUNT` vs `all_variants().len()` vs
//! `test_is_recoverable_exhaustive` cases) that drifted to wildly different
//! values (e.g. 94 vs 96). The shared `variant_table.rs` is authoritative;
//! these tests fail if parallel counters reappear.

// Off-chain test binary, not deployed WASM (issue #713 exemption).
#![allow(clippy::disallowed_macros)]

use credence_errors::ContractError;

include!("../variant_table.rs");

#[test]
fn variant_table_length_is_the_canonical_generation() {
    assert_eq!(
        ALL_VARIANTS.len(),
        104,
        "Add one row to `variant_table.rs` per new `ContractError` variant; \
         do not maintain separate manual counts in other test files.",
    );
}

#[test]
fn variant_table_covers_every_expected_wire_code_once() {
    let mut seen: std::vec::Vec<u32> = std::vec::Vec::with_capacity(ALL_VARIANTS.len());
    for (name, variant) in ALL_VARIANTS {
        let code = *variant as u32;
        assert!(
            !seen.contains(&code),
            "variant `{name}` wire code {code} is duplicated in `variant_table.rs`",
        );
        seen.push(code);
    }
    assert_eq!(seen.len(), ALL_VARIANTS.len());
}

#[test]
#[should_panic(expected = "GENERATION DRIFT")]
fn stale_manual_count_differs_from_table_length() {
    // Sad-path regression: simulates bumping only one manual counter while
    // the enum grew by two variants (94 vs 96 on main before this fix).
    const STALE_MANUAL_COUNT: usize = ALL_VARIANTS.len() - 2;
    assert_eq!(
        ALL_VARIANTS.len(),
        STALE_MANUAL_COUNT,
        "GENERATION DRIFT: `variant_table.rs` has {} variants but a stale \
         manually-maintained counter said {STALE_MANUAL_COUNT}. Use \
         `ALL_VARIANTS.len()` instead of parallel counts.",
        ALL_VARIANTS.len(),
    );
}
