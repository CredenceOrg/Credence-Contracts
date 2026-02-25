//! Tests for overflow-safe arithmetic helpers.

#![cfg(test)]

use crate::math;

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
#[should_panic(expected = "sub underflow")]
fn test_sub_underflow_panics() {
    let _ = math::sub_i128(i128::MIN, 1, "sub underflow");
}

#[test]
#[should_panic(expected = "mul overflow")]
fn test_mul_overflow_panics() {
    let _ = math::mul_i128(i128::MAX, 2, "mul overflow");
}

#[test]
#[should_panic(expected = "attestation weight overflow")]
fn test_mul_u64_overflow_panics() {
    let _ = math::mul_u64(u64::MAX, 2, "attestation weight overflow");
}
