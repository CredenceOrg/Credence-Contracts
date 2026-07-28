//! Cross-module tests verifying consistent BPS_DENOMINATOR usage across fee calculations.
//!
//! These tests confirm that all modules producing fee or penalty outputs agree on
//! 10 000 as the single basis-point denominator and produce identical results to
//! direct credence_math calculations for the same inputs.

use crate::math;
use credence_math::BPS_DENOMINATOR;

// ---------------------------------------------------------------------------
// Denominator constant
// ---------------------------------------------------------------------------

#[test]
fn bps_denominator_is_ten_thousand() {
    assert_eq!(BPS_DENOMINATOR, 10_000_i128);
}

// ---------------------------------------------------------------------------
// credence_math::bps / split_bps regression vectors
// ---------------------------------------------------------------------------

#[test]
fn bps_one_percent_of_ten_thousand() {
    // 100 bps = 1 %; 1% of 10 000 = 100
    let fee = math::bps(10_000, 100, "mul", "div");
    assert_eq!(fee, 100);
}

#[test]
fn bps_ten_percent_of_one_million() {
    // 1 000 bps = 10 %; 10% of 1 000 000 = 100 000
    let fee = math::bps(1_000_000, 1_000, "mul", "div");
    assert_eq!(fee, 100_000);
}

#[test]
fn bps_max_rate_full_amount() {
    // 10 000 bps = 100 %; all of amount returned as fee
    let fee = math::bps(500, BPS_DENOMINATOR as u32, "mul", "div");
    assert_eq!(fee, 500);
}

#[test]
fn bps_zero_rate_returns_zero() {
    let fee = math::bps(999_999, 0, "mul", "div");
    assert_eq!(fee, 0);
}

#[test]
fn split_bps_net_plus_fee_equals_amount() {
    // For any (amount, bps) the invariant fee + net == amount must hold.
    let cases: &[(i128, u32)] = &[
        (1_000_000, 50),
        (10_000, 100),
        (10_000, 1_000),
        (999_999, 333),
        (1, 1),
        (0, 500),
    ];
    for &(amount, bps_value) in cases {
        let (fee, net) = math::split_bps(amount, bps_value, "mul", "div", "sub");
        assert_eq!(
            fee + net,
            amount,
            "fee+net != amount for amount={amount} bps={bps_value}"
        );
    }
}

// ---------------------------------------------------------------------------
// early_exit_penalty::calculate_penalty uses BPS_DENOMINATOR via math::bps
// ---------------------------------------------------------------------------

#[test]
fn early_exit_penalty_matches_direct_bps() {
    use crate::early_exit_penalty::calculate_penalty;

    let amount = 1_000_000_i128;
    let penalty_bps = 500_u32; // 5 %
    let total_duration = 86_400_u64; // 1 day in seconds
    let remaining_time = 43_200_u64; // half a day

    // Expected: base = 5% of 1_000_000 = 50_000; scaled by 0.5 = 25_000
    let expected_base = math::bps(amount, penalty_bps, "mul", "div");
    let expected = expected_base * remaining_time as i128 / total_duration as i128;

    let actual = calculate_penalty(amount, remaining_time, total_duration, penalty_bps);
    assert_eq!(actual, expected);
}

#[test]
fn early_exit_penalty_full_remaining_equals_base_penalty() {
    use crate::early_exit_penalty::calculate_penalty;

    let amount = 100_000_i128;
    let penalty_bps = 200_u32; // 2 %
                               // remaining == total => penalty == base
    let duration = 100_u64;
    let penalty = calculate_penalty(amount, duration, duration, penalty_bps);
    let base = math::bps(amount, penalty_bps, "mul", "div");
    assert_eq!(penalty, base);
}

// ---------------------------------------------------------------------------
// fees::calculate_fee uses BPS_DENOMINATOR via math::split_bps
// (unit-level: call the library function directly, not through the contract)
// ---------------------------------------------------------------------------

#[test]
fn fee_calculation_one_percent() {
    // Directly test the math used by fees::calculate_fee.
    let amount = 10_000_i128;
    let fee_bps = 100_u32; // 1 %
    let (fee, net) = math::split_bps(amount, fee_bps, "mul", "div", "sub");
    assert_eq!(fee, 100);
    assert_eq!(net, 9_900);
}

#[test]
fn fee_calculation_half_percent() {
    let amount = 1_000_000_i128;
    let fee_bps = 50_u32; // 0.5 %
    let (fee, net) = math::split_bps(amount, fee_bps, "mul", "div", "sub");
    assert_eq!(fee, 5_000);
    assert_eq!(net, 995_000);
}

#[test]
fn fee_calculation_max_bps_takes_full_amount() {
    // MAX_FEE_BPS == BPS_DENOMINATOR == 10 000
    let amount = 1_000_i128;
    let fee_bps = BPS_DENOMINATOR as u32;
    let (fee, net) = math::split_bps(amount, fee_bps, "mul", "div", "sub");
    assert_eq!(fee, amount);
    assert_eq!(net, 0);
}

// ---------------------------------------------------------------------------
// Cross-module: fee and penalty modules share the same denominator
// ---------------------------------------------------------------------------

#[test]
fn fee_and_penalty_use_same_denominator_for_equal_rates() {
    // Given the same rate (bps) and amount, fee calculation and penalty base
    // must produce identical results — both ultimately call math::bps with
    // BPS_DENOMINATOR.
    use crate::early_exit_penalty::calculate_penalty;

    let amount = 500_000_i128;
    let rate_bps = 300_u32; // 3 %

    let (fee, _net) = math::split_bps(amount, rate_bps, "mul", "div", "sub");
    // Penalty with remaining == total collapses to the base bps value.
    let penalty = calculate_penalty(amount, 1, 1, rate_bps);

    assert_eq!(
        fee, penalty,
        "fee and penalty base diverge for rate={rate_bps} bps"
    );
}

// ---------------------------------------------------------------------------
// Documentation-matching regression vectors
// ---------------------------------------------------------------------------

#[test]
fn early_exit_penalty_regression_vectors() {
    use crate::early_exit_penalty::calculate_penalty;

    // Case 1: 5% penalty rate, half time remaining
    // amount = 1000, penalty_bps = 500 (5%), remaining = 50, total = 100
    // base = 1000 * 500 / 10000 = 50
    // penalty = 50 * 50 / 100 = 25
    assert_eq!(calculate_penalty(1000, 50, 100, 500), 25);

    // Case 2: 2.5% penalty rate, 75% time remaining
    // amount = 1000000, penalty_bps = 250 (2.5%), remaining = 75, total = 100
    // base = 1000000 * 250 / 10000 = 25000
    // penalty = 25000 * 75 / 100 = 18750
    assert_eq!(calculate_penalty(1_000_000, 75, 100, 250), 18_750);

    // Case 3: 10% penalty rate, 10% time remaining
    // amount = 10000, penalty_bps = 1000 (10%), remaining = 1, total = 10
    // base = 10000 * 1000 / 10000 = 1000
    // penalty = 1000 * 1 / 10 = 100
    assert_eq!(calculate_penalty(10000, 1, 10, 1000), 100);

    // Case 4: docs/early-exit.md example — 10% rate, half duration
    // amount = 500, penalty_bps = 1000 (10%), remaining = 183, total = 365
    // base = 500 * 1000 / 10000 = 50
    // penalty = 50 * 183 / 365 = 25 (floor)
    assert_eq!(calculate_penalty(500, 183, 365, 1000), 25);

    // Case 5: 100% penalty at full remaining
    // amount = 10_000, penalty_bps = 10_000, remaining = total = 1000
    // base = 10_000 * 10_000 / 10_000 = 10_000
    // penalty = 10_000 * 1000 / 1000 = 10_000
    assert_eq!(calculate_penalty(10_000, 1000, 1000, 10_000), 10_000);

    // Case 6: zero penalty rate
    // amount = 1_000_000, penalty_bps = 0, remaining = 50, total = 100
    // base = 0 (multiply by 0), penalty = 0
    assert_eq!(calculate_penalty(1_000_000, 50, 100, 0), 0);
}

#[test]
fn fee_math_edge_cases() {
    // Zero amount
    let (fee, net) = math::split_bps(0, 500, "mul", "div", "sub");
    assert_eq!(fee, 0);
    assert_eq!(net, 0);

    // Zero BPS
    let (fee, net) = math::split_bps(1_000_000, 0, "mul", "div", "sub");
    assert_eq!(fee, 0);
    assert_eq!(net, 1_000_000);

    // Max BPS (100%)
    let (fee, net) = math::split_bps(1_000_000, BPS_DENOMINATOR as u32, "mul", "div", "sub");
    assert_eq!(fee, 1_000_000);
    assert_eq!(net, 0);
}
