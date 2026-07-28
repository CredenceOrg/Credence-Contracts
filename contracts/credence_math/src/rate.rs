use crate::{mul_div_i128, Rounding};
use soroban_sdk::contracttype;

/// A fixed-point interest rate (in basis points).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rate {
    pub bps: u32,
}

impl Rate {
    /// Compound the given rate (in bps) over `periods` periods.
    /// Returns the compounded multiplier scaled by 10_000 (i.e., 10_000 = 1.0x).
    pub fn compound(rate: u32, periods: u32) -> i128 {
        let mut multiplier: i128 = 10_000;
        let factor = 10_000_i128 + rate as i128;
        for _ in 0..periods {
            multiplier = mul_div_i128(
                multiplier,
                factor,
                10_000,
                Rounding::Down,
                "compound overflow",
            );
        }
        multiplier
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_rate_compound_monotonicity(
            bps in 0..10_000u32,
            period_a in 0..32u32,
            period_b in 0..32u32
        ) {
            let (min_period, max_period) = if period_a <= period_b {
                (period_a, period_b)
            } else {
                (period_b, period_a)
            };

            let val_min = Rate::compound(bps, min_period);
            let val_max = Rate::compound(bps, max_period);

            prop_assert!(val_max >= val_min, "More periods should yield higher or equal compounded rate");
        }
    }
}
