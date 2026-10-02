//! Tests for overflow-safe arithmetic helpers.
//!
//! This module verifies the deterministic behavior, boundary handling, and failure
//! recovery invariants of the arithmetic operations re-exported in `math.rs`.
//! All operations must handle valid, invalid, and boundary-case inputs without
//! producing unsafe or inconsistent results.

use crate::math;

// --- bps ---

#[test]
fn test_bps_basic() {
    let fee = math::bps(10_000_i128, 100_u32, "mul", "div");
    assert_eq!(fee, 100);
}

#[test]
#[should_panic(expected = "fee calculation overflow")]
fn test_bps_overflow_panics() {
    // i128::MAX * 10_000 overflows.
    let _ = math::bps(i128::MAX, 10_000_u32, "fee calculation overflow", "div");
}

#[test]
fn test_bps_boundaries() {
    // 0 amount
    assert_eq!(math::bps(0, 10_000, "mul", "div"), 0);
    // 0 bps
    assert_eq!(math::bps(10_000, 0, "mul", "div"), 0);
    // Negative amount
    assert_eq!(math::bps(-10_000, 100, "mul", "div"), -100);
}

// --- bps_u64 ---

#[test]
fn test_bps_u64_basic() {
    assert_eq!(math::bps_u64(10_000, 100, "mul fail", "div fail"), 100);
}

#[test]
#[should_panic(expected = "mul u64 overflow")]
fn test_bps_u64_overflow_panics() {
    let _ = math::bps_u64(u64::MAX, 10_000, "mul u64 overflow", "div fail");
}

#[test]
fn test_bps_u64_boundaries() {
    // 0 amount
    assert_eq!(math::bps_u64(0, 10_000, "mul", "div"), 0);
    // 0 bps
    assert_eq!(math::bps_u64(10_000, 0, "mul", "div"), 0);
    // Max amount with 0 bps
    assert_eq!(math::bps_u64(u64::MAX, 0, "mul", "div"), 0);
}

// --- add_i128 ---

#[test]
fn test_add_basic() {
    assert_eq!(math::add_i128(100, 200, "add fail"), 300);
}

#[test]
#[should_panic(expected = "add overflow")]
fn test_add_overflow_panics() {
    let _ = math::add_i128(i128::MAX, 1, "add overflow");
}

#[test]
#[should_panic(expected = "add underflow")]
fn test_add_underflow_panics() {
    let _ = math::add_i128(i128::MIN, -1, "add underflow");
}

#[test]
fn test_add_boundaries() {
    assert_eq!(math::add_i128(i128::MAX, 0, "add fail"), i128::MAX);
    assert_eq!(math::add_i128(i128::MIN, 0, "add fail"), i128::MIN);
    assert_eq!(math::add_i128(i128::MAX, -1, "add fail"), i128::MAX - 1);
    assert_eq!(math::add_i128(i128::MIN, 1, "add fail"), i128::MIN + 1);
}

// --- sub_i128 ---

#[test]
fn test_sub_basic() {
    assert_eq!(math::sub_i128(500, 200, "sub fail"), 300);
}

#[test]
#[should_panic(expected = "sub underflow")]
fn test_sub_underflow_panics() {
    let _ = math::sub_i128(i128::MIN, 1, "sub underflow");
}

#[test]
#[should_panic(expected = "sub overflow")]
fn test_sub_overflow_panics() {
    let _ = math::sub_i128(i128::MAX, -1, "sub overflow");
}

#[test]
fn test_sub_boundaries() {
    assert_eq!(math::sub_i128(i128::MAX, 0, "sub fail"), i128::MAX);
    assert_eq!(math::sub_i128(i128::MIN, 0, "sub fail"), i128::MIN);
    assert_eq!(math::sub_i128(i128::MAX, 1, "sub fail"), i128::MAX - 1);
    assert_eq!(math::sub_i128(i128::MIN, -1, "sub fail"), i128::MIN + 1);
    assert_eq!(math::sub_i128(0, 0, "sub fail"), 0);
}

// --- mul_i128 ---

#[test]
fn test_mul_basic() {
    assert_eq!(math::mul_i128(50, 2, "mul fail"), 100);
}

#[test]
#[should_panic(expected = "mul overflow")]
fn test_mul_overflow_panics() {
    let _ = math::mul_i128(i128::MAX, 2, "mul overflow");
}

#[test]
#[should_panic(expected = "mul underflow")]
fn test_mul_underflow_panics() {
    let _ = math::mul_i128(i128::MAX, -2, "mul underflow");
}

#[test]
fn test_mul_boundaries() {
    assert_eq!(math::mul_i128(i128::MAX, 1, "mul fail"), i128::MAX);
    assert_eq!(math::mul_i128(i128::MIN, 1, "mul fail"), i128::MIN);
    assert_eq!(math::mul_i128(i128::MAX, 0, "mul fail"), 0);
    assert_eq!(math::mul_i128(i128::MIN, 0, "mul fail"), 0);
    assert_eq!(math::mul_i128(-1, -1, "mul fail"), 1);
}

// --- mul_u64 ---

#[test]
fn test_mul_u64_basic() {
    assert_eq!(math::mul_u64(50, 2, "mul fail"), 100);
}

#[test]
#[should_panic(expected = "attestation weight overflow")]
fn test_mul_u64_overflow_panics() {
    let _ = math::mul_u64(u64::MAX, 2, "attestation weight overflow");
}

#[test]
fn test_mul_u64_boundaries() {
    assert_eq!(math::mul_u64(u64::MAX, 1, "mul fail"), u64::MAX);
    assert_eq!(math::mul_u64(u64::MAX, 0, "mul fail"), 0);
    assert_eq!(math::mul_u64(0, u64::MAX, "mul fail"), 0);
}

// --- div_i128 ---

#[test]
fn test_div_basic() {
    assert_eq!(math::div_i128(100, 3, "div fail"), 33);
    assert_eq!(math::div_i128(-100, 3, "div fail"), -33);
}

#[test]
#[should_panic(expected = "divide by zero")]
fn test_div_by_zero_panics() {
    let _ = math::div_i128(100, 0, "divide by zero");
}

#[test]
#[should_panic(expected = "div overflow")]
fn test_div_overflow_panics() {
    // i128::MIN / -1 overflows.
    let _ = math::div_i128(i128::MIN, -1, "div overflow");
}

#[test]
fn test_div_boundaries() {
    assert_eq!(math::div_i128(0, 100, "div fail"), 0);
    assert_eq!(math::div_i128(i128::MAX, 1, "div fail"), i128::MAX);
    assert_eq!(math::div_i128(i128::MIN, 1, "div fail"), i128::MIN);
}

// --- ceil_div_i128 ---

#[test]
fn test_ceil_div_basic() {
    assert_eq!(math::ceil_div_i128(100, 3, "ceil div fail"), 34);
    assert_eq!(math::ceil_div_i128(100, 4, "ceil div fail"), 25);
    assert_eq!(math::ceil_div_i128(0, 5, "ceil div fail"), 0);
}

#[test]
#[should_panic(expected = "divide by zero")]
fn test_ceil_div_by_zero_panics() {
    let _ = math::ceil_div_i128(100, 0, "divide by zero");
}

#[test]
#[should_panic(expected = "ceil div overflow")]
fn test_ceil_div_overflow_panics() {
    // i128::MAX + (b - 1) overflows when b > 1
    let _ = math::ceil_div_i128(i128::MAX, 2, "ceil div overflow");
}

#[test]
fn test_ceil_div_boundaries() {
    assert_eq!(math::ceil_div_i128(i128::MAX, 1, "ceil div fail"), i128::MAX);
    assert_eq!(math::ceil_div_i128(0, 1, "ceil div fail"), 0);
}

// --- sat_mul_bps ---

#[test]
fn test_sat_mul_bps_basic() {
    assert_eq!(math::sat_mul_bps(10_000, 5_000), 5_000); // 50% of 10000
    assert_eq!(math::sat_mul_bps(-10_000, 5_000), -5_000); // 50% of -10000
}

#[test]
fn test_sat_mul_bps_saturation() {
    // Should saturate to i128::MAX instead of panicking, ensuring recovery and safety at limits
    assert_eq!(math::sat_mul_bps(i128::MAX, 20_000), i128::MAX);
    // Saturates to i128::MIN
    assert_eq!(math::sat_mul_bps(i128::MIN, 20_000), i128::MIN);
}

#[test]
fn test_sat_mul_bps_boundaries() {
    assert_eq!(math::sat_mul_bps(0, 20_000), 0);
    assert_eq!(math::sat_mul_bps(10_000, 0), 0);
}

// --- split_bps ---

#[test]
fn test_split_bps_basic() {
    let (part, remainder) = math::split_bps(1000, 2000, "mul fail", "div fail", "sub fail");
    assert_eq!(part, 200);
    assert_eq!(remainder, 800);
}

#[test]
fn test_split_bps_boundaries() {
    // 0%
    let (part_zero, rem_zero) = math::split_bps(1000, 0, "mul", "div", "sub");
    assert_eq!(part_zero, 0);
    assert_eq!(rem_zero, 1000);

    // 100%
    let (part_full, rem_full) = math::split_bps(1000, 10_000, "mul", "div", "sub");
    assert_eq!(part_full, 1000);
    assert_eq!(rem_full, 0);

    // Amount is 0
    let (part_zero_amt, rem_zero_amt) = math::split_bps(0, 5_000, "mul", "div", "sub");
    assert_eq!(part_zero_amt, 0);
    assert_eq!(rem_zero_amt, 0);

    // Negative amount
    let (part_neg, rem_neg) = math::split_bps(-1000, 2000, "mul", "div", "sub");
    assert_eq!(part_neg, -200);
    assert_eq!(rem_neg, -800);
}

#[test]
#[should_panic(expected = "mul fail")]
fn test_split_bps_overflow_panics() {
    let _ = math::split_bps(i128::MAX, 10_000, "mul fail", "div fail", "sub fail");
}

// --- constants ---

#[test]
fn test_bps_denominator() {
    assert_eq!(math::BPS_DENOMINATOR, 10_000);
}
