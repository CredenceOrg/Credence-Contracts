//! Compile-check and smoke-test for the accrual helper and test templates.
//!
//! This integration test exists solely to wire `accrual_helper_template.rs`
//! and `accrual_tests_template.rs` into the Cargo build graph so they are
//! compiled and their logic is exercised on every `cargo test -p templates`.
//!
//! The templates themselves contain only stub/pseudocode logic; the tests here
//! verify that:
//!   1. Both files compile without errors.
//!   2. The concrete helper functions (`checked_add_i128`, `checked_mul_i128`,
//!      `ensure_accrued`, `calculate_interest`) behave correctly for simple
//!      inputs.
//!   3. The template test stubs (renamed to avoid `#[test]` conflicts) run
//!      without panicking.

// Include the template source files directly. Any syntax or type error in the
// templates will surface as a compile error in this test binary.
include!("../accrual_helper_template.rs");

mod accrual_tests {
    include!("../accrual_tests_template.rs");
}

// ---------------------------------------------------------------------------
// Concrete smoke tests for the helper functions
// ---------------------------------------------------------------------------

#[test]
fn checked_add_i128_basic() {
    assert_eq!(checked_add_i128(100, 200, "overflow"), 300);
}

#[test]
fn checked_add_i128_zero() {
    assert_eq!(checked_add_i128(0, 0, "overflow"), 0);
}

#[test]
fn checked_mul_i128_basic() {
    assert_eq!(checked_mul_i128(10, 20, "overflow"), 200);
}

#[test]
fn checked_mul_i128_zero() {
    assert_eq!(checked_mul_i128(1_000_000, 0, "overflow"), 0);
}

#[test]
fn calculate_interest_basic() {
    // principal=1_000, rate=100 per second, elapsed=5s → interest = 500_000
    let interest = accrual_tests::calculate_interest(1_000, 100, 5);
    assert_eq!(interest, 500_000);
}

#[test]
fn calculate_interest_zero_elapsed() {
    let interest = accrual_tests::calculate_interest(1_000_000, 50, 0);
    assert_eq!(interest, 0);
}

#[test]
fn calculate_interest_zero_principal() {
    let interest = accrual_tests::calculate_interest(0, 100, 100);
    assert_eq!(interest, 0);
}

// ---------------------------------------------------------------------------
// Smoke invocations for the template stubs (verify they don't panic)
// ---------------------------------------------------------------------------

#[test]
fn template_borrow_stub_compiles_and_runs() {
    accrual_tests::test_borrow_forces_accrual_template();
}

#[test]
fn template_repay_stub_compiles_and_runs() {
    accrual_tests::test_repay_forces_accrual_template();
}

#[test]
fn template_no_double_accrual_stub_compiles_and_runs() {
    accrual_tests::test_no_double_accrual_template();
}
