//! Re-export the shared overflow-safe arithmetic helpers used across contracts.

#[allow(unused_imports)]
pub use credence_math::{
    add_i128, bps, bps_u64, ceil_div_i128, checked_add_i128, checked_ceil_div_i128,
    checked_div_i128, checked_mul_div_i128, checked_mul_i128, checked_mul_u64, checked_sub_i128,
    div_i128, mul_i128, mul_u64, sat_mul_bps, split_bps, sub_i128, unwrap_or_panic_error,
    BPS_DENOMINATOR,
};
