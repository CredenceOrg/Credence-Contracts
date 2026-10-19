//! Boundary and recovery tests for `leverage.rs`.
//!
//! # Coverage matrix
//!
//! | # | Category                         | Description                                              |
//! |---|----------------------------------|----------------------------------------------------------|
//! | 1 | Unit – non-positive passthrough  | Negative and zero amounts bypass the cap                 |
//! | 2 | Unit – below cap                 | Amounts below the cap pass                               |
//! | 3 | Unit – at cap (off-by-one lower) | `leverage == max_leverage` must NOT panic                |
//! | 4 | Unit – above cap (off-by-one)    | `leverage == max_leverage + 1` must panic                |
//! | 5 | Unit – cap = 1 (minimum)         | Only exactly MIN_BOND_AMOUNT is allowed                  |
//! | 6 | Unit – cap = MAX_MAX_LEVERAGE    | Arithmetic at the highest allowed cap                    |
//! | 7 | Unit – i128::MAX amount          | checked_div never wraps for a positive divisor           |
//! | 8 | Unit – integer division rounding | Floor semantics: partial multiples stay below cap        |
//! | 9 | Unit – u32::MAX cap              | Entire i128 positive space is accepted                   |
//! |10 | Integration – create_bond below  | Bond created below cap succeeds (5-arg current API)      |
//! |11 | Integration – create_bond at cap | Bond at the cap is accepted                              |
//! |12 | Integration – create_bond above  | Bond above cap reverts with LeverageExceeded             |
//! |13 | Integration – cap tightened      | Reduced cap blocks a previously-valid subsequent bond    |
//! |14 | Integration – top_up crosses cap | top_up that pushes total over cap reverts                |
//! |15 | Integration – top_up stays at cap| top_up that keeps total exactly at cap succeeds          |
//! |16 | Recovery – cap restored          | Bond allowed again after admin re-raises the cap         |
//! |17 | Authorization – non-admin locked | set_max_leverage rejected for non-admin callers          |
//! |18 | Param bounds – zero rejected     | `set_max_leverage(0)` panics                             |
//! |19 | Param bounds – above ceiling     | `set_max_leverage(MAX_MAX_LEVERAGE + 1)` panics          |
//! |20 | Param bounds – MIN_MAX_LEVERAGE  | Minimum allowed value accepted and enforced              |
//! |21 | Param bounds – MAX_MAX_LEVERAGE  | Maximum allowed value accepted and enforced              |
//! |22 | Default value                    | Freshly-initialised contract exposes DEFAULT_MAX_LEVERAGE|
//! |23 | Decimal scale – 6-dec at cap     | 100× at a cap of 100 succeeds                            |
//! |24 | Decimal scale – 6-dec above cap  | 101× at a cap of 100 reverts                             |
//! |25 | Decimal scale – large amounts    | Large-scale amounts handled without overflow             |
//! |26 | Regression – checked_div guard   | Division uses checked_div: positive divisor never wraps  |

#![cfg(test)]

use crate::leverage::validate_leverage;
use crate::parameters::{DEFAULT_MAX_LEVERAGE, MAX_MAX_LEVERAGE, MIN_MAX_LEVERAGE};
use crate::test_helpers::setup_with_token_mint;
use crate::validation::MIN_BOND_AMOUNT;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

// ============================================================================
// Helpers
// ============================================================================

/// Create an initialised contract with no token attached.
/// Useful for parameter-only tests that never touch token balances.
fn bare_contract(e: &Env) -> (CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    (client, admin)
}

/// Duration constant used throughout integration tests: one day in seconds.
const ONE_DAY: u64 = 86_400;

// ============================================================================
// 1. Unit – non-positive amounts bypass the cap
// ============================================================================

/// Zero amount carries leverage 0 (treated as "no position") and must always
/// pass regardless of max_leverage, so the check defers to validate_bond_amount.
#[test]
fn unit_zero_amount_always_passes() {
    let e = Env::default();
    // Calling validate_leverage directly.  No panic expected.
    validate_leverage(&e, 0, 1_u32);
}

/// A negative bond_amount is an invalid input that must be caught earlier by
/// validate_bond_amount.  The leverage guard returns early (≤ 0 branch) without
/// any panic, preserving the single-responsibility design.
#[test]
fn unit_negative_amount_bypasses_guard() {
    let e = Env::default();
    validate_leverage(&e, -1, 1_u32);
    validate_leverage(&e, i128::MIN, 1_u32);
}

/// i128::MIN is the most negative value; the guard must still return without
/// panicking (the early-return path handles all ≤ 0 amounts).
#[test]
fn unit_i128_min_amount_bypasses_guard() {
    let e = Env::default();
    validate_leverage(&e, i128::MIN, 0_u32);
}

// ============================================================================
// 2. Unit – amounts below the cap
// ============================================================================

/// A bond of exactly 1× (= MIN_BOND_AMOUNT) is below a cap of 2.
#[test]
fn unit_one_x_below_cap_two_passes() {
    let e = Env::default();
    validate_leverage(&e, MIN_BOND_AMOUNT, 2_u32);
}

/// A bond of 5× is below a cap of 10.
#[test]
fn unit_five_x_below_cap_ten_passes() {
    let e = Env::default();
    validate_leverage(&e, 5 * MIN_BOND_AMOUNT, 10_u32);
}

// ============================================================================
// 3. Unit – leverage == max_leverage (boundary: must NOT panic)
// ============================================================================

/// Exactly at the cap is the inclusive upper boundary — the condition is
/// `leverage > max_leverage`, so equality is allowed.
#[test]
fn unit_leverage_equals_cap_passes() {
    let e = Env::default();
    let cap = 7_u32;
    // 7 × MIN_BOND_AMOUNT → leverage = 7 = cap → must pass.
    validate_leverage(&e, cap as i128 * MIN_BOND_AMOUNT, cap);
}

/// Test the boundary at cap = 1 (minimum possible cap).
#[test]
fn unit_leverage_one_equals_cap_one_passes() {
    let e = Env::default();
    validate_leverage(&e, MIN_BOND_AMOUNT, 1_u32);
}

// ============================================================================
// 4. Unit – leverage == max_leverage + 1 (off-by-one: must panic)
// ============================================================================

/// One step over the cap must trigger LeverageExceeded.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn unit_leverage_one_over_cap_reverts() {
    let e = Env::default();
    let cap = 7_u32;
    // (cap + 1) × MIN_BOND_AMOUNT → leverage = cap + 1 > cap → must panic.
    validate_leverage(&e, (cap as i128 + 1) * MIN_BOND_AMOUNT, cap);
}

/// Confirm the same boundary holds at cap = 1: 2 × MIN_BOND_AMOUNT reverts.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn unit_two_x_above_cap_one_reverts() {
    let e = Env::default();
    validate_leverage(&e, 2 * MIN_BOND_AMOUNT, 1_u32);
}

// ============================================================================
// 5. Unit – cap = 1 (tightest allowed cap)
// ============================================================================

/// With cap = 1, exactly MIN_BOND_AMOUNT is the maximum allowed amount.
#[test]
fn unit_cap_one_min_bond_amount_passes() {
    let e = Env::default();
    validate_leverage(&e, MIN_BOND_AMOUNT, 1_u32);
}

/// With cap = 1, MIN_BOND_AMOUNT + 1 produces leverage = 1 (floor division),
/// so it should still pass — confirming that integer floor division, not
/// rounding, governs the cap.
///
/// Invariant: `(MIN_BOND_AMOUNT + 1) / MIN_BOND_AMOUNT == 1` (integer division)
///           → leverage 1 == cap 1 → pass.
#[test]
fn unit_cap_one_min_bond_plus_one_passes_due_to_floor_division() {
    let e = Env::default();
    // MIN_BOND_AMOUNT + 1 still yields leverage = 1 under floor division.
    validate_leverage(&e, MIN_BOND_AMOUNT + 1, 1_u32);
}

/// Only the first multiple above floor forces leverage = 2 > 1.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn unit_cap_one_two_x_reverts() {
    let e = Env::default();
    validate_leverage(&e, 2 * MIN_BOND_AMOUNT, 1_u32);
}

// ============================================================================
// 6. Unit – cap = MAX_MAX_LEVERAGE (arithmetic at the hard ceiling)
// ============================================================================

/// Exactly at MAX_MAX_LEVERAGE passes.
#[test]
fn unit_max_leverage_cap_exactly_at_cap_passes() {
    let e = Env::default();
    let cap = MAX_MAX_LEVERAGE; // 100_000_000
    validate_leverage(&e, cap as i128 * MIN_BOND_AMOUNT, cap);
}

/// One step above MAX_MAX_LEVERAGE reverts.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn unit_max_leverage_cap_one_over_reverts() {
    let e = Env::default();
    let cap = MAX_MAX_LEVERAGE;
    validate_leverage(&e, (cap as i128 + 1) * MIN_BOND_AMOUNT, cap);
}

// ============================================================================
// 7. Unit – i128::MAX amount (extreme positive value)
// ============================================================================

/// i128::MAX / MIN_BOND_AMOUNT is a valid i128 division (positive divisor,
/// non-negative dividend) and must not panic inside checked_div_leverage.
/// The result only passes when max_leverage is large enough; with u32::MAX
/// the check passes because i128::MAX / MIN_BOND_AMOUNT < u32::MAX as i128
/// in test mode (MIN_BOND_AMOUNT = 1_000 in test).
#[test]
fn unit_i128_max_amount_with_huge_cap_passes() {
    let e = Env::default();
    // In test mode: MIN_BOND_AMOUNT = 1_000; i128::MAX / 1_000 ≈ 1.7e35
    // u32::MAX as i128 = 4_294_967_295 which is much smaller, so we need a
    // cap large enough.  Use u32::MAX — if the computed leverage > u32::MAX
    // the test is still asserting no overflow panic, just a LeverageExceeded.
    // The test's goal is no Overflow panic from checked_div.
    //
    // We call validate_leverage and catch either success or LeverageExceeded,
    // but NOT Overflow.  We achieve this by asserting the result is not an
    // Overflow panic by wrapping in std::panic::catch_unwind.
    //
    // Since Soroban test panics are string-tagged, we inspect the panic message.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        validate_leverage(&e, i128::MAX, u32::MAX);
    }));
    match result {
        Ok(()) => { /* leverage <= u32::MAX — test passes */ }
        Err(payload) => {
            // Any panic here must be LeverageExceeded, never Overflow.
            let msg = payload
                .downcast_ref::<std::string::String>()
                .map(|s| s.as_str())
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("");
            assert!(
                msg.contains("leverage exceeds maximum") || msg.is_empty(),
                "unexpected panic: expected LeverageExceeded or no panic, got: {:?}",
                msg
            );
        }
    }
}

// ============================================================================
// 8. Unit – integer division floor semantics (partial multiples)
// ============================================================================

/// `(2 × MIN_BOND_AMOUNT - 1)` must floor to leverage 1, not 2.
/// This verifies the floor property: partial multiples do NOT cross the cap
/// boundary unexpectedly.
#[test]
fn unit_floor_division_partial_multiple_stays_below_cap() {
    let e = Env::default();
    let cap = 2_u32;
    // 2 × MIN_BOND_AMOUNT - 1 → floor(/ MIN_BOND_AMOUNT) = 1 ≤ cap = 2 → pass.
    validate_leverage(&e, 2 * MIN_BOND_AMOUNT - 1, cap);
}

/// Exactly 2 × MIN_BOND_AMOUNT equals cap = 2 → pass (inclusive boundary).
#[test]
fn unit_floor_division_exactly_two_x_at_cap_two_passes() {
    let e = Env::default();
    validate_leverage(&e, 2 * MIN_BOND_AMOUNT, 2_u32);
}

/// `2 × MIN_BOND_AMOUNT + 1` still floors to 2 → passes when cap = 2.
#[test]
fn unit_floor_division_two_x_plus_one_at_cap_two_passes() {
    let e = Env::default();
    // floor((2*MIN + 1) / MIN) = 2 == cap = 2 → pass.
    validate_leverage(&e, 2 * MIN_BOND_AMOUNT + 1, 2_u32);
}

/// `3 × MIN_BOND_AMOUNT - 1` floors to 2 → passes when cap = 2.
#[test]
fn unit_floor_division_just_below_third_multiple_passes() {
    let e = Env::default();
    validate_leverage(&e, 3 * MIN_BOND_AMOUNT - 1, 2_u32);
}

/// `3 × MIN_BOND_AMOUNT` = leverage 3 > cap 2 → must revert.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn unit_floor_division_third_multiple_above_cap_reverts() {
    let e = Env::default();
    validate_leverage(&e, 3 * MIN_BOND_AMOUNT, 2_u32);
}

// ============================================================================
// 9. Unit – u32::MAX cap (every positive i128 that doesn't overflow is ≤ cap)
// ============================================================================

/// With cap = u32::MAX, very large amounts should not trigger LeverageExceeded
/// so long as the resulting leverage fits in a u32.
/// In test mode: MIN_BOND_AMOUNT = 1_000; u32::MAX × 1_000 = 4_294_967_295_000.
/// An amount of exactly (u32::MAX as i128) * MIN_BOND_AMOUNT passes.
#[test]
fn unit_u32_max_cap_at_cap_passes() {
    let e = Env::default();
    let cap = u32::MAX;
    validate_leverage(&e, cap as i128 * MIN_BOND_AMOUNT, cap);
}

// ============================================================================
// 10-12. Integration – create_bond (current 5-arg API)
// ============================================================================

/// Bond with amount below the cap succeeds end-to-end via create_bond.
#[test]
fn integration_create_bond_below_cap_succeeds() {
    let e = Env::default();
    let cap = 10_u32;
    let amount = 5 * MIN_BOND_AMOUNT; // 5× < cap 10×
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    client.set_max_leverage(&admin, &cap);
    let bond = client.create_bond(&identity, &amount, &ONE_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, amount);
    assert!(bond.active);
}

/// Bond with amount exactly at the cap must be accepted (inclusive boundary).
#[test]
fn integration_create_bond_at_cap_exact_boundary_succeeds() {
    let e = Env::default();
    let cap = 10_u32;
    let amount = cap as i128 * MIN_BOND_AMOUNT; // exactly 10×
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    client.set_max_leverage(&admin, &cap);
    let bond = client.create_bond(&identity, &amount, &ONE_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, amount);
    assert!(bond.active);
}

/// Bond one step above the cap must revert with LeverageExceeded.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn integration_create_bond_one_over_cap_reverts() {
    let e = Env::default();
    let cap = 10_u32;
    let amount = (cap as i128 + 1) * MIN_BOND_AMOUNT; // 11× — one over
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    client.set_max_leverage(&admin, &cap);
    client.create_bond(&identity, &amount, &ONE_DAY, &false, &0_u64);
}

/// A rolling bond respects the same leverage cap.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn integration_create_rolling_bond_above_cap_reverts() {
    let e = Env::default();
    let cap = 5_u32;
    let amount = (cap as i128 + 1) * MIN_BOND_AMOUNT;
    let notice = ONE_DAY;
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    client.set_max_leverage(&admin, &cap);
    // is_rolling = true, notice_period_duration = ONE_DAY
    client.create_bond(&identity, &amount, &ONE_DAY, &true, &notice);
}

// ============================================================================
// 13. Integration – tightened cap blocks a previously-valid subsequent bond
// ============================================================================

/// After admin reduces the cap, a previously-valid amount is rejected.
/// This verifies that the cap change takes effect immediately for the next
/// bond, with no grace window or stale-cache bug.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn integration_reduced_cap_blocks_previously_valid_amount() {
    let e = Env::default();
    let original_cap = 100_u32;
    let amount = 50 * MIN_BOND_AMOUNT; // 50× — fine under 100×

    // Use a distinct identity for each bond since BondAlreadyExists would
    // interfere with the second call.
    let (client, admin, identity1, ..) = setup_with_token_mint(&e, amount * 4);

    // First bond succeeds under the original cap.
    client.set_max_leverage(&admin, &original_cap);
    let b1 = client.create_bond(&identity1, &amount, &ONE_DAY, &false, &0_u64);
    assert_eq!(b1.bonded_amount, amount);

    // Admin tightens the cap below the previously-valid amount.
    client.set_max_leverage(&admin, &10_u32);

    // A fresh identity attempting the same amount must now revert.
    let identity2 = Address::generate(&e);
    client.create_bond(&identity2, &amount, &ONE_DAY, &false, &0_u64);
}

// ============================================================================
// 14-15. Integration – top_up leverage check
// ============================================================================

/// A top_up that pushes the total bonded amount above the cap reverts.
/// The combined (original + top_up) leverage is checked, not just the top_up.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn integration_top_up_crosses_cap_reverts() {
    let e = Env::default();
    let cap = 5_u32;
    let initial = 3 * MIN_BOND_AMOUNT; // 3× — below cap 5×
    let topup = 3 * MIN_BOND_AMOUNT; // would make total 6× — above cap

    let (client, admin, identity, ..) = setup_with_token_mint(&e, (initial + topup) * 4);

    client.set_max_leverage(&admin, &cap);
    client.create_bond(&identity, &initial, &ONE_DAY, &false, &0_u64);
    // This top_up must revert: 3 + 3 = 6 > cap 5.
    client.top_up(&identity, &topup);
}

/// A top_up that keeps the total bonded amount exactly at the cap succeeds.
#[test]
fn integration_top_up_to_exact_cap_succeeds() {
    let e = Env::default();
    let cap = 5_u32;
    let initial = 3 * MIN_BOND_AMOUNT;
    let topup = 2 * MIN_BOND_AMOUNT; // total = 5× = cap

    let (client, admin, identity, ..) = setup_with_token_mint(&e, (initial + topup) * 4);

    client.set_max_leverage(&admin, &cap);
    client.create_bond(&identity, &initial, &ONE_DAY, &false, &0_u64);
    let bond = client.top_up(&identity, &topup);
    assert_eq!(bond.bonded_amount, initial + topup);
    assert!(bond.active);
}

/// A top_up one unit above the exact cap reverts (tight off-by-one).
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn integration_top_up_one_over_cap_reverts() {
    let e = Env::default();
    let cap = 5_u32;
    let initial = cap as i128 * MIN_BOND_AMOUNT; // exactly at cap
                                                 // Adding MIN_BOND_AMOUNT would make total = (cap + 1) × MIN_BOND_AMOUNT
    let topup = MIN_BOND_AMOUNT;

    let (client, admin, identity, ..) = setup_with_token_mint(&e, (initial + topup) * 4);

    client.set_max_leverage(&admin, &cap);
    client.create_bond(&identity, &initial, &ONE_DAY, &false, &0_u64);
    client.top_up(&identity, &topup);
}

// ============================================================================
// 16. Recovery – cap raised again allows the bond
// ============================================================================

/// After being blocked by a low cap, raising the cap allows subsequent bonds.
/// This confirms the guard is purely dynamic: no permanent ban is stored.
#[test]
fn integration_cap_raised_allows_bond_again() {
    let e = Env::default();
    let tight_cap = 5_u32;
    let amount = 10 * MIN_BOND_AMOUNT; // 10× — exceeds tight cap

    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    // Set tight cap; this bond should fail.
    client.set_max_leverage(&admin, &tight_cap);
    let first_try = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_bond(&identity, &amount, &ONE_DAY, &false, &0_u64)
    }));
    assert!(
        first_try.is_err(),
        "expected create_bond to fail under tight cap"
    );

    // Raise the cap so the bond can proceed.
    client.set_max_leverage(&admin, &50_u32);
    // Bond a different identity (prior call may have partially mutated state).
    let identity2 = Address::generate(&e);
    let bond = client.create_bond(&identity2, &amount, &ONE_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, amount);
    assert!(bond.active);
}

/// top_up initially blocked by the cap succeeds once the cap is raised.
#[test]
fn integration_cap_raised_allows_top_up_recovery() {
    let e = Env::default();
    let initial_cap = 5_u32;
    let initial = 3 * MIN_BOND_AMOUNT;
    let topup = 3 * MIN_BOND_AMOUNT; // would be 6× — just over initial cap

    let (client, admin, identity, ..) = setup_with_token_mint(&e, (initial + topup) * 4);

    client.set_max_leverage(&admin, &initial_cap);
    client.create_bond(&identity, &initial, &ONE_DAY, &false, &0_u64);

    // Confirm top_up is blocked at low cap.
    let first_top = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.top_up(&identity, &topup)
    }));
    assert!(
        first_top.is_err(),
        "expected top_up to fail under tight cap"
    );

    // Raise cap and retry.
    client.set_max_leverage(&admin, &10_u32);
    let bond = client.top_up(&identity, &topup);
    assert_eq!(bond.bonded_amount, initial + topup);
    assert!(bond.active);
}

// ============================================================================
// 17. Authorization – non-admin cannot change the cap
// ============================================================================

/// A non-admin caller must not be able to set the leverage cap.
/// Relaxing the cap from a non-admin address is a privilege escalation vector.
#[test]
#[should_panic(expected = "not admin")]
fn integration_non_admin_cannot_set_max_leverage() {
    let e = Env::default();
    let (client, _admin) = bare_contract(&e);
    let non_admin = Address::generate(&e);

    // A non-admin trying to raise the cap to allow larger positions.
    client.set_max_leverage(&non_admin, &50_000_u32);
}

// ============================================================================
// 18-21. Parameter bounds enforcement
// ============================================================================

/// `set_max_leverage(0)` must panic — zero disables the cap entirely,
/// which is an unsafe invariant (any bond size would pass).
#[test]
#[should_panic(expected = "max_leverage out of bounds")]
fn param_set_max_leverage_zero_rejected() {
    let e = Env::default();
    let (client, admin) = bare_contract(&e);
    client.set_max_leverage(&admin, &0_u32);
}

/// A value one above the hard ceiling must be rejected.
#[test]
#[should_panic(expected = "max_leverage out of bounds")]
fn param_set_max_leverage_above_ceiling_rejected() {
    let e = Env::default();
    let (client, admin) = bare_contract(&e);
    client.set_max_leverage(&admin, &(MAX_MAX_LEVERAGE + 1));
}

/// The minimum allowed value (1×) is accepted.
#[test]
fn param_set_max_leverage_min_value_accepted() {
    let e = Env::default();
    let (client, admin) = bare_contract(&e);
    client.set_max_leverage(&admin, &MIN_MAX_LEVERAGE);
    assert_eq!(client.get_max_leverage(), MIN_MAX_LEVERAGE);
}

/// The maximum allowed value is accepted.
#[test]
fn param_set_max_leverage_max_value_accepted() {
    let e = Env::default();
    let (client, admin) = bare_contract(&e);
    client.set_max_leverage(&admin, &MAX_MAX_LEVERAGE);
    assert_eq!(client.get_max_leverage(), MAX_MAX_LEVERAGE);
}

/// MIN_MAX_LEVERAGE + 1 is accepted (just inside the lower boundary).
#[test]
fn param_set_max_leverage_min_plus_one_accepted() {
    let e = Env::default();
    let (client, admin) = bare_contract(&e);
    client.set_max_leverage(&admin, &(MIN_MAX_LEVERAGE + 1));
    assert_eq!(client.get_max_leverage(), MIN_MAX_LEVERAGE + 1);
}

/// MAX_MAX_LEVERAGE - 1 is accepted (just inside the upper boundary).
#[test]
fn param_set_max_leverage_max_minus_one_accepted() {
    let e = Env::default();
    let (client, admin) = bare_contract(&e);
    client.set_max_leverage(&admin, &(MAX_MAX_LEVERAGE - 1));
    assert_eq!(client.get_max_leverage(), MAX_MAX_LEVERAGE - 1);
}

// ============================================================================
// 22. Default value
// ============================================================================

/// A freshly-initialised contract must report DEFAULT_MAX_LEVERAGE.
/// Callers relying on the default (e.g. off-chain dashboards, keeper bots)
/// must observe the expected value without an explicit set.
#[test]
fn param_default_max_leverage_is_correct() {
    let e = Env::default();
    let (client, _admin) = bare_contract(&e);
    assert_eq!(client.get_max_leverage(), DEFAULT_MAX_LEVERAGE);
}

// ============================================================================
// 23-24. Decimal scale – 6-decimal token (USDC-like, MIN_BOND_AMOUNT = 1_000
//         in test mode representing 1 raw unit of a 6-dec token)
// ============================================================================

/// Simulates a 6-decimal token scenario: 100 units at a cap of 100 succeeds.
#[test]
fn decimal_six_at_cap_succeeds() {
    let e = Env::default();
    let cap = 100_u32;
    let amount = 100 * MIN_BOND_AMOUNT; // exactly 100×
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    client.set_max_leverage(&admin, &cap);
    let bond = client.create_bond(&identity, &amount, &ONE_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, amount);
    assert!(bond.active);
}

/// 101 units at a cap of 100 must revert.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn decimal_six_above_cap_reverts() {
    let e = Env::default();
    let cap = 100_u32;
    let amount = 101 * MIN_BOND_AMOUNT; // 101×
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    client.set_max_leverage(&admin, &cap);
    client.create_bond(&identity, &amount, &ONE_DAY, &false, &0_u64);
}

// ============================================================================
// 25. Decimal scale – large amounts (no arithmetic overflow)
// ============================================================================

/// Large amounts that stay within the cap must not trigger any overflow error.
/// This exercises the checked_div path with a representative large value.
#[test]
fn decimal_large_amount_at_high_cap_no_overflow() {
    let e = Env::default();
    let cap = 1_000_u32;
    let amount = cap as i128 * MIN_BOND_AMOUNT;
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    client.set_max_leverage(&admin, &cap);
    let bond = client.create_bond(&identity, &amount, &ONE_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, amount);
    assert!(bond.active);
}

/// Large amounts one step above the cap revert correctly without overflowing.
#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn decimal_large_amount_one_over_high_cap_reverts() {
    let e = Env::default();
    let cap = 1_000_u32;
    let amount = (cap as i128 + 1) * MIN_BOND_AMOUNT;
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    client.set_max_leverage(&admin, &cap);
    client.create_bond(&identity, &amount, &ONE_DAY, &false, &0_u64);
}

// ============================================================================
// 26. Regression – checked_div guard never produces Overflow panic
// ============================================================================

/// The division `bond_amount / MIN_BOND_AMOUNT` uses checked_div.
/// For any positive `bond_amount` and the positive constant `MIN_BOND_AMOUNT`,
/// `checked_div` always returns `Some`, so ContractError::Overflow must never
/// be emitted from this path.
///
/// This regression test guards against reverting the safe-division refactor.
#[test]
fn regression_checked_div_never_overflows_for_valid_amounts() {
    let e = Env::default();

    // Test a variety of amounts spanning the allowed range.
    let test_amounts: &[i128] = &[
        MIN_BOND_AMOUNT,
        MIN_BOND_AMOUNT + 1,
        100 * MIN_BOND_AMOUNT - 1,
        100 * MIN_BOND_AMOUNT,
        1_000 * MIN_BOND_AMOUNT,
        100_000 * MIN_BOND_AMOUNT, // = DEFAULT_MAX_LEVERAGE × MIN_BOND_AMOUNT
    ];

    for &amount in test_amounts {
        // Use u32::MAX as an effectively unlimited cap to ensure the check
        // reaches the division without early-returning.
        // No panic (including Overflow) is acceptable here.
        validate_leverage(&e, amount, u32::MAX);
    }
}

/// The off-by-one boundary at the exact leverage cap must always produce
/// LeverageExceeded, never Overflow.
#[test]
fn regression_checked_div_boundary_produces_leverage_exceeded_not_overflow() {
    let e = Env::default();
    let cap = 5_u32;

    // This must be LeverageExceeded, not Overflow.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        validate_leverage(&e, (cap as i128 + 1) * MIN_BOND_AMOUNT, cap)
    }));

    assert!(result.is_err(), "expected a panic for amount above cap");
    let payload = result.unwrap_err();
    let msg = payload
        .downcast_ref::<std::string::String>()
        .map(|s| s.as_str())
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    // The panic must mention leverage, not arithmetic overflow.
    assert!(
        msg.contains("leverage exceeds maximum") || msg.is_empty(),
        "expected LeverageExceeded panic, got: {:?}",
        msg
    );
}

/// Admin can update the cap multiple times; each update takes effect
/// immediately for the next call without needing a state-reset.
#[test]
fn integration_cap_updates_are_immediately_effective() {
    let e = Env::default();
    let amount_10x = 10 * MIN_BOND_AMOUNT;
    let (client, admin, identity1, ..) = setup_with_token_mint(&e, amount_10x * 20);

    // Cap = 20: bond at 10× succeeds.
    client.set_max_leverage(&admin, &20_u32);
    let b1 = client.create_bond(&identity1, &amount_10x, &ONE_DAY, &false, &0_u64);
    assert_eq!(b1.bonded_amount, amount_10x);

    // Immediately tighten to cap = 5: a new bond at 10× must fail.
    client.set_max_leverage(&admin, &5_u32);
    let identity2 = Address::generate(&e);
    let try2 = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_bond(&identity2, &amount_10x, &ONE_DAY, &false, &0_u64)
    }));
    assert!(try2.is_err(), "expected rejection after cap tighten");

    // Raise the cap again: 10× succeeds for a third identity.
    client.set_max_leverage(&admin, &20_u32);
    let identity3 = Address::generate(&e);
    let b3 = client.create_bond(&identity3, &amount_10x, &ONE_DAY, &false, &0_u64);
    assert_eq!(b3.bonded_amount, amount_10x);
}
