//! Proptest harness for `proportional_deduction` invariants.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::treasury::proportional_deduction;
    use proptest::prelude::*;
    use soroban_sdk::Env;

    // Generate valid triples: source_balance, amount, total where total > 0.
    fn triple_strategy() -> impl Strategy<Value = (i128, i128, i128)> {
        // Keep totals within reasonable range to avoid u128 overflow when converting.
        let max_total: i128 = 1_000_000_000;
        (0i128..=max_total)
            .prop_flat_map(move |total| {
                let source_bal = 0i128..=total;
                let amount = 0i128..=total;
                (Just(total), source_bal, amount)
            })
            .prop_map(|(total, source, amount)| (source, amount, total))
    }

    proptest! {
        #[test]
        fn proportional_deduction_basic((source_balance, amount, total) in triple_strategy()) {
            let e = Env::default();
            let deduction = proportional_deduction(&e, source_balance, amount, total);
            // Invariant 1: non‑negative and never exceeds the source balance.
            prop_assert!(deduction >= 0);
            prop_assert!(deduction <= source_balance);
            // Idempotence: repeated calls give the same result.
            let deduction2 = proportional_deduction(&e, source_balance, amount, total);
            prop_assert_eq!(deduction, deduction2);
        }
    }

    // Two‑source split test – verifies sum invariant and rounding behaviour.
    proptest! {
        #[test]
        fn proportional_deduction_two_source(source_a in 0i128..=1_000_000_000,
                                             source_b in 0i128..=1_000_000_000,
                                             amount in 0i128..=2_000_000_000) {
            let total = source_a + source_b;
            // Avoid division by zero and amount > total (invariant not defined).
            if total == 0 || amount > total { return Ok(()); }
            let e = Env::default();
            let ded_a = proportional_deduction(&e, source_a, amount, total);
            let ded_b = proportional_deduction(&e, source_b, amount, total);
            // Each deduction respects its source.
            prop_assert!(ded_a <= source_a);
            prop_assert!(ded_b <= source_b);
            // The sum cannot exceed the requested amount; any remainder is at most (sources‑1).
            let sum = ded_a + ded_b;
            prop_assert!(sum <= amount);
            prop_assert!(amount - sum < 2);
        }
    }
}
