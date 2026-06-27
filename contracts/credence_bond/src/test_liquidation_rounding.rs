//! Bug condition exploration and regression tests for liquidation rounding.
//!
//! ## Task 1 — Bug Condition Exploration
//! These tests assert the CORRECT (post-fix) behaviour. On unfixed code they FAIL,
//! confirming the floor-division bug. After the fix they PASS.
//!
//! Known failing pairs (floor_ratio_bps < true_ratio_bps):
//!   A: bonded=3,      slashed=2,    threshold=6667  (floor=6666, ceil=6667)
//!   B: bonded=10_001, slashed=5_001, threshold=5001  (floor=5000, ceil=5001)
//!   C: bonded=7,      slashed=3,    threshold=4286  (floor=4285, ceil=4286)

#![cfg(test)]

extern crate std;

use crate::test_helpers;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

// ---------------------------------------------------------------------------
// Helper: set up a bond with the given bonded/slashed amounts directly in
// storage, register the holder, then run the scanner.
// ---------------------------------------------------------------------------
fn scan_with_bond(
    bonded: i128,
    slashed: i128,
    min_slash_ratio_bps: u32,
) -> crate::liquidation_scanner::ScanResult {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(crate::CredenceBond, ());
    let client = crate::CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    client.initialize(&admin, &None);

    // Write bond state directly into storage (bypasses token transfer).
    e.as_contract(&contract_id, || {
        let bond = crate::IdentityBond {
            identity: identity.clone(),
            bonded_amount: bonded,
            bond_start: 1_000,
            bond_duration: 86_400,
            slashed_amount: slashed,
            active: true,
            is_rolling: false,
            withdrawal_requested_at: 0,
            notice_period_duration: 0,
        };
        e.storage().instance().set(&crate::DataKey::Bond, &bond);
    });

    // Register the holder so the scanner can find it.
    client.register_bond_holder(&admin, &identity);

    let keeper = Address::generate(&e);
    client.scan_liquidation_candidates(&keeper, &0_u32, &200_u32, &min_slash_ratio_bps)
}

// ---------------------------------------------------------------------------
// Task 1 — Bug Condition Exploration Tests
// These FAIL on unfixed code (floor division misses boundary positions).
// They PASS after the fix (ceiling division correctly classifies them).
// ---------------------------------------------------------------------------

/// Test case A: bonded=3, slashed=2, threshold=6667
/// floor(2*10_000/3) = 6666 < 6667  → scanner misses on unfixed code
/// ceil(2*10_000/3)  = 6667 >= 6667 → scanner includes on fixed code
#[test]
fn bug_exploration_a_bonded_3_slashed_2_threshold_6667() {
    let result = scan_with_bond(3, 2, 6667);
    assert_eq!(
        result.candidates.len(),
        1,
        "COUNTEREXAMPLE: bonded=3, slashed=2 at threshold 6667 — \
         floor_ratio=6666 < 6667, position was NOT included (floor-division bug confirmed)"
    );
}

/// Test case B: bonded=10_001, slashed=5_001, threshold=5001
/// floor(5001*10_000/10_001) = 5000 < 5001 → scanner misses on unfixed code
/// ceil(5001*10_000/10_001)  = 5001 >= 5001 → scanner includes on fixed code
#[test]
fn bug_exploration_b_bonded_10001_slashed_5001_threshold_5001() {
    let result = scan_with_bond(10_001, 5_001, 5001);
    assert_eq!(
        result.candidates.len(),
        1,
        "COUNTEREXAMPLE: bonded=10_001, slashed=5_001 at threshold 5001 — \
         floor_ratio=5000 < 5001, position was NOT included (floor-division bug confirmed)"
    );
}

/// Test case C: bonded=7, slashed=3, threshold=4286
/// floor(3*10_000/7) = 4285 < 4286 → scanner misses on unfixed code
/// ceil(3*10_000/7)  = 4286 >= 4286 → scanner includes on fixed code
#[test]
fn bug_exploration_c_bonded_7_slashed_3_threshold_4286() {
    let result = scan_with_bond(7, 3, 4286);
    assert_eq!(
        result.candidates.len(),
        1,
        "COUNTEREXAMPLE: bonded=7, slashed=3 at threshold 4286 — \
         floor_ratio=4285 < 4286, position was NOT included (floor-division bug confirmed)"
    );
}

// ---------------------------------------------------------------------------
// Task 3.5 — Regression vectors (must pass on every CI run after the fix)
// ---------------------------------------------------------------------------

#[test]
fn regression_bonded_3_slashed_2() {
    let result = scan_with_bond(3, 2, 6667);
    assert_eq!(
        result.candidates.len(),
        1,
        "regression: bonded=3, slashed=2 at threshold 6667"
    );
}

#[test]
fn regression_bonded_10001_slashed_5001() {
    let result = scan_with_bond(10_001, 5_001, 5001);
    assert_eq!(
        result.candidates.len(),
        1,
        "regression: bonded=10_001, slashed=5_001 at threshold 5001"
    );
}

#[test]
fn regression_bonded_7_slashed_3() {
    let result = scan_with_bond(7, 3, 4286);
    assert_eq!(
        result.candidates.len(),
        1,
        "regression: bonded=7, slashed=3 at threshold 4286"
    );
}

// ---------------------------------------------------------------------------
// Exact-divisible sanity check — must pass both before and after fix
// ---------------------------------------------------------------------------

/// bonded=100, slashed=50, threshold=5000: exact division, no rounding gap.
/// Must be included both before and after fix.
#[test]
fn exact_divisible_no_regression() {
    let result = scan_with_bond(100, 50, 5000);
    assert_eq!(
        result.candidates.len(),
        1,
        "exact-divisible position must always be included"
    );
}

/// Position genuinely below threshold must NOT be included.
#[test]
fn below_threshold_excluded() {
    // bonded=100, slashed=40, ratio=4000 bps < threshold 5000
    let result = scan_with_bond(100, 40, 5000);
    assert_eq!(
        result.candidates.len(),
        0,
        "below-threshold position must be excluded"
    );
}
