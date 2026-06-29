//! Precision-loss regression tests for the early-exit penalty time-decay formula.
//!
//! ## Background
//!
//! The penalty at [`early_exit_penalty::calculate_penalty`] is a chained
//! multiply/divide with truncation at each step:
//!
//! ```text
//! charge  = floor(amount × penalty_bps / 10_000)
//! penalty = floor(charge × remaining / duration)
//! ```
//!
//! Each truncating division can independently round the intermediate to zero,
//! creating a **zero-penalty zone** for dust-amount withdrawals.
//!
//! ## Zero-penalty boundaries
//!
//! | Division | Condition | Example (bps=1) |
//! |----------|-----------|-----------------|
//! | 1st      | `amount × bps < 10_000` | 9 999 → charge=0 |
//! | 2nd      | `charge × remaining < duration` | charge=1, rem=1, dur=86 400 → pen=0 |
//!
//! ## Exploit
//!
//! A single 200-token withdrawal at 100 bps (1 %) halfway through a bond pays
//! penalty = 1.  Splitting into 20 × 10-token withdrawals pays penalty = 0 for
//! every slice because each produces charge = 0 at the first division.
//! Dust splitting thus reduces the intended penalty to zero.
//!
//! ## Proposed fix (behaviour change)
//!
//! Replace the chained floor division with a single round-up via
//! [`credence_math::mul_div_i128`]:
//!
//! ```text
//! penalty = ceil(amount × penalty_bps × remaining / (10_000 × duration))
//! ```
//!
//! This guarantees any positive (amount, bps, remaining) tuple produces at
//! least one unit of penalty.  The proposed helper is shown below but **not**
//! wired into production code — reviewers must confirm the economic impact
//! before adopting it.
//!
//! ⚠️ Behaviour change — the round-up formula increases penalty for every
//! non-zero input.  Reviewers should verify the cap at 10 000 bps still
//! bounds the penalty to ≤ amount.

extern crate std;

use crate::early_exit_penalty;

// ---------------------------------------------------------------------------
// 1.  Zero-penalty boundary — first division truncation
// ---------------------------------------------------------------------------

#[test]
fn div1_zero_when_amount_bps_below_denominator() {
    // charge = floor(9_999 × 1 / 10_000) = 0  →  penalty = 0
    let p = early_exit_penalty::calculate_penalty(9_999, 86_400, 86_400, 1);
    assert_eq!(p, 0);
}

#[test]
fn div1_tips_when_amount_bps_equals_denominator() {
    // charge = floor(10_000 × 1 / 10_000) = 1  →  penalty = 1
    let p = early_exit_penalty::calculate_penalty(10_000, 86_400, 86_400, 1);
    assert_eq!(p, 1);
}

#[test]
fn div1_zero_for_bps_100_amount_99() {
    // charge = floor(99 × 100 / 10_000) = floor(9_900 / 10_000) = 0  →  penalty = 0
    let p = early_exit_penalty::calculate_penalty(99, 86_400, 86_400, 100);
    assert_eq!(p, 0);
}

#[test]
fn div1_tips_at_amount_100_bps_100() {
    // charge = floor(100 × 100 / 10_000) = floor(10_000 / 10_000) = 1  →  penalty = 1
    let p = early_exit_penalty::calculate_penalty(100, 86_400, 86_400, 100);
    assert_eq!(p, 1);
}

#[test]
fn div1_zero_bps_always_zero() {
    let p = early_exit_penalty::calculate_penalty(1_000_000, 86_400, 86_400, 0);
    assert_eq!(p, 0);
}

// ---------------------------------------------------------------------------
// 2.  Zero-penalty boundary — second division truncation
// ---------------------------------------------------------------------------

#[test]
fn div2_zero_when_charge_remaining_below_duration() {
    // charge = 1, remaining = 1, duration = 86_400  →  1 × 1 / 86_400 = 0
    let p = early_exit_penalty::calculate_penalty(10_000, 1, 86_400, 1);
    assert_eq!(p, 0);
}

#[test]
fn div2_tips_when_charge_remaining_equals_duration() {
    // charge = 1, remaining = 86_400, duration = 86_400  →  1 × 86_400 / 86_400 = 1
    let p = early_exit_penalty::calculate_penalty(10_000, 86_400, 86_400, 1);
    assert_eq!(p, 1);
}

#[test]
fn div2_small_remaining_at_moderate_bps() {
    // charge = floor(10_000 × 100 / 10_000) = 100
    // remaining = 1, duration = 86_400  →  100 × 1 / 86_400 = 0
    let p = early_exit_penalty::calculate_penalty(10_000, 1, 86_400, 100);
    assert_eq!(p, 0);
}

// ---------------------------------------------------------------------------
// 3.  Regression vectors
// ---------------------------------------------------------------------------

#[test]
fn regression_typical_half_duration() {
    // 1000 tokens, 10 % (1000 bps), half duration elapsed
    // charge = floor(1000 × 1000 / 10_000) = 100
    // penalty = floor(100 × 43_200 / 86_400) = 50
    assert_eq!(
        early_exit_penalty::calculate_penalty(1000, 43_200, 86_400, 1000),
        50,
    );
}

#[test]
fn regression_full_penalty_at_issuance() {
    // At issuance remaining = duration → penalty = same as bps charge
    assert_eq!(
        early_exit_penalty::calculate_penalty(2000, 86_400, 86_400, 500),
        100, // 5 % of 2000
    );
}

#[test]
fn regression_max_penalty_cap() {
    // 10_000 bps = 100 %, full remaining → penalty = amount
    assert_eq!(
        early_exit_penalty::calculate_penalty(500, 86_400, 86_400, 10_000),
        500,
    );
}

#[test]
fn regression_max_penalty_half_remaining() {
    // 100 %, half duration → penalty = floor(500 × 43_200 / 86_400) = 250
    assert_eq!(
        early_exit_penalty::calculate_penalty(500, 43_200, 86_400, 10_000),
        250,
    );
}

#[test]
fn regression_duration_zero() {
    // Special-case early return.
    assert_eq!(
        early_exit_penalty::calculate_penalty(1000, 86_400, 0, 1000),
        0,
    );
}

#[test]
fn regression_tiny_amount_max_duration() {
    // amount = 1, bps = 10_000 → charge = floor(1 × 10_000 / 10_000) = 1
    // remaining = 1, duration = u64::MAX → 1 × 1 / u64::MAX = 0
    assert_eq!(
        early_exit_penalty::calculate_penalty(1, 1, u64::MAX, 10_000),
        0,
    );
}

/// Regression: the threshold amount that survivors can withdraw before the
/// penalty tips to a positive value.
#[test]
fn regression_max_dust_before_penalty_tips() {
    // At 1 bps any amount < 10_000 produces charge = 0 → penalty = 0.
    assert_eq!(
        early_exit_penalty::calculate_penalty(9_999, 86_400, 86_400, 1),
        0,
        "9_999 is the largest dust amount at 1 bps that pays zero penalty",
    );
    // At the same rate 10 000 produces penalty = 1.
    assert_eq!(
        early_exit_penalty::calculate_penalty(10_000, 86_400, 86_400, 1),
        1,
    );
}

// ---------------------------------------------------------------------------
// 4.  Dust-splitting exploit demonstration
// ---------------------------------------------------------------------------

#[test]
fn dust_split_exploit_unit() {
    // Single withdrawal of 200 at 100 bps (1 %), half-duration elapsed:
    //   charge = floor(200 × 100 / 10_000) = 2
    //   penalty = floor(2 × 43_200 / 86_400) = 1
    let single = early_exit_penalty::calculate_penalty(200, 43_200, 86_400, 100);
    assert_eq!(single, 1, "single 200-token withdrawal pays penalty = 1");

    // 20 dust withdrawals of 10 each, same parameters:
    //   each charge = floor(10 × 100 / 10_000) = 0 → penalty = 0
    let dust_sum: i128 = (0..20)
        .map(|_| early_exit_penalty::calculate_penalty(10, 43_200, 86_400, 100))
        .sum();
    assert_eq!(dust_sum, 0, "20 × 10-token dust pays total penalty = 0");

    // The exploiter saved 100 % of the intended penalty.
    assert!(
        dust_sum < single,
        "dust splitting bypasses the penalty entirely"
    );
}

// ---------------------------------------------------------------------------
// 5.  Cumulative-penalty fairness assertion
// ---------------------------------------------------------------------------

/// A large withdrawal and the sum of dust slices that sum to the same amount
/// must produce at least the same total penalty.  If the dust sum is lower
/// the rounding direction is exploitable.
///
/// This property currently FAILS (dust pays less), which confirms the
/// exploitable floor-division behaviour.
#[test]
fn cumulative_penalty_fairness() {
    let amounts: &[i128] = &[5000, 2000, 1000, 500, 200, 100];
    let dust_slice = 10_i128;

    for &bulk in amounts {
        let single = early_exit_penalty::calculate_penalty(bulk, 43_200, 86_400, 100);
        let n_slices = bulk / dust_slice;
        let dust_total: i128 = (0..n_slices)
            .map(|_| early_exit_penalty::calculate_penalty(dust_slice, 43_200, 86_400, 100))
            .sum();

        assert!(
            dust_total <= single,
            "bulk={bulk}: dust_total={dust_total} > single={single} — unexpected",
        );
        if dust_total < single {
            // This confirms the exploit: dust splitting pays strictly less penalty.
            assert!(
                dust_total < single,
                "bulk={bulk}: dust_total={dust_total} < single={single} — exploit confirmed",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 6.  Proposed fix (illustration only — not wired into production code)
// ---------------------------------------------------------------------------

/// Proposed replacement for [`early_exit_penalty::calculate_penalty`].
///
/// Uses [`credence_math::mul_div_i128`] with `Rounding::Up` so that any
/// positive (amount, bps, remaining) tuple produces at least one unit of
/// penalty.  This eliminates the dust-splitting exploit at the cost of a
/// behaviour change — every non-zero input yields a strictly higher penalty
/// than the current formula.
///
/// # Behaviour change
///
/// Reviewers must confirm the economic impact before adopting this formula.
/// The cap at 10_000 bps still bounds the penalty to ≤ amount, so the
/// maximum possible penalty is unchanged.
#[allow(unused)]
fn calculate_penalty_ceil(amount: i128, remaining: u64, duration: u64, penalty_bps: u32) -> i128 {
    if duration == 0 || amount == 0 || penalty_bps == 0 {
        return 0;
    }
    credence_math::mul_div_i128(
        amount,
        (penalty_bps as i128) * (remaining as i128),
        credence_math::BPS_DENOMINATOR * (duration as i128),
        credence_math::Rounding::Up,
        "early_exit_penalty_ceil",
    )
}

#[test]
fn proposed_fix_eliminates_div1_dust() {
    // Current floor formula gives 0; proposed ceiling gives 1.
    let floor = early_exit_penalty::calculate_penalty(9_999, 86_400, 86_400, 1);
    let ceil = calculate_penalty_ceil(9_999, 86_400, 86_400, 1);
    assert_eq!(floor, 0, "current: dust pays zero");
    assert_eq!(ceil, 1, "proposed: smallest dust pays at least 1");
}

#[test]
fn proposed_fix_eliminates_div2_dust() {
    // Current: charge=1, remaining=1, duration=86_400 → 0.
    // Proposed: ceil(10_000 × 1 × 1 / (10_000 × 86_400)) = ceil(1/86_400) = 1.
    let floor = early_exit_penalty::calculate_penalty(10_000, 1, 86_400, 1);
    let ceil = calculate_penalty_ceil(10_000, 1, 86_400, 1);
    assert_eq!(floor, 0, "current: near-expiry tiny remaining pays zero");
    assert_eq!(ceil, 1, "proposed: near-expiry tiny remaining pays 1");
}

#[test]
fn proposed_fix_keeps_zero_for_zero_inputs() {
    let cases: &[(i128, u64, u64, u32)] = &[
        (0, 86_400, 86_400, 100),
        (1000, 0, 86_400, 100), // remaining = 0
        (1000, 86_400, 0, 100), // duration = 0
        (1000, 86_400, 86_400, 0),
    ];
    for &(amount, remaining, duration, bps) in cases {
        let floor = early_exit_penalty::calculate_penalty(amount, remaining, duration, bps);
        let ceil = calculate_penalty_ceil(amount, remaining, duration, bps);
        assert_eq!(floor, 0, "current: zero input");
        assert_eq!(ceil, 0, "proposed: zero input");
    }
}

#[test]
fn proposed_fix_identical_for_exact_divisions() {
    // When the division is exact both formulas agree.
    let cases: &[(i128, u64, u64, u32)] = &[
        (10_000, 86_400, 86_400, 1),
        (100, 86_400, 86_400, 100),
        (500, 86_400, 86_400, 10_000),
    ];
    for &(amount, remaining, duration, bps) in cases {
        let floor = early_exit_penalty::calculate_penalty(amount, remaining, duration, bps);
        let ceil = calculate_penalty_ceil(amount, remaining, duration, bps);
        assert_eq!(
            floor, ceil,
            "exact division: floor must equal ceil for (amount={amount}, remaining={remaining}, duration={duration}, bps={bps})",
        );
    }
}

#[test]
fn proposed_fix_never_below_floor() {
    // The ceiling formula must produce >= floor formula for all inputs where
    // both are defined.
    let amounts: &[i128] = &[0, 1, 10, 100, 1_000, 10_000, 100_000];
    let remainings: &[u64] = &[0, 1, 43_200, 86_400];
    let durations: &[u64] = &[1, 86_400, 365 * 86_400];
    let bpss: &[u32] = &[0, 1, 10, 100, 1000, 10_000];

    for &amt in amounts {
        for &rem in remainings {
            for &dur in durations {
                for &bps in bpss {
                    let floor = early_exit_penalty::calculate_penalty(amt, rem, dur, bps);
                    let ceil = calculate_penalty_ceil(amt, rem, dur, bps);
                    assert!(
                        ceil >= floor,
                        "proposed ceil must never be below floor: \
                         amount={amt}, remaining={rem}, duration={dur}, bps={bps}, \
                         floor={floor}, ceil={ceil}",
                    );
                }
            }
        }
    }
}
