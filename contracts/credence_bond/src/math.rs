//! Re-export the shared overflow-safe arithmetic helpers used across contracts.

#[allow(unused_imports)]
pub use credence_math::{
    add_i128, bps, bps_u64, ceil_div_i128, div_i128, mul_i128, mul_u64, sat_mul_bps, split_bps,
    sub_i128, BPS_DENOMINATOR,
};
