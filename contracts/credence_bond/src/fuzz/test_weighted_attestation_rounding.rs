//! Property-based tests for weighted-attestation rounding and clamp invariants.
//!
//! The weight path is deliberately exercised through the public contract-facing
//! helpers so these tests cover config storage clamping plus the arithmetic in
//! `compute_weight`.

extern crate std;

use crate::types::attestation::{DEFAULT_ATTESTATION_WEIGHT, MAX_ATTESTATION_WEIGHT};
use crate::weighted_attestation::{self, MAX_WEIGHT_MULTIPLIER_BPS};
use crate::{CredenceBond, CredenceBondClient};
use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

const BPS_DENOMINATOR: u128 = 10_000;

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    let attester = Address::generate(e);
    client.register_attester(&attester);
    (client, admin, attester, contract_id)
}

fn computed_weight(stake: i128, multiplier_bps: u32, max_weight: u32) -> (u32, u32, u32) {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);

    client.set_weight_config(&admin, &multiplier_bps, &max_weight);
    client.set_attester_stake(&admin, &attester, &stake);

    let stored_config = client.get_weight_config();
    let weight = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });

    (weight, stored_config.0, stored_config.1)
}

fn expected_weight(stake: i128, multiplier_bps: u32, max_weight: u32) -> u32 {
    if stake <= 0 {
        return DEFAULT_ATTESTATION_WEIGHT;
    }

    let stake = stake.unsigned_abs();
    let multiplier = multiplier_bps.min(MAX_WEIGHT_MULTIPLIER_BPS) as u128;
    let raw = (stake / BPS_DENOMINATOR) * multiplier
        + (stake % BPS_DENOMINATOR) * multiplier / BPS_DENOMINATOR;
    let cap = u128::from(max_weight.min(MAX_ATTESTATION_WEIGHT));
    raw.min(cap)
        .min(u128::from(MAX_ATTESTATION_WEIGHT))
        .max(u128::from(DEFAULT_ATTESTATION_WEIGHT)) as u32
}

#[test]
fn default_config_and_missing_stake_use_documented_floors() {
    let e = Env::default();
    let (client, _admin, attester, contract_id) = setup(&e);

    assert_eq!(
        client.get_weight_config(),
        (
            weighted_attestation::DEFAULT_WEIGHT_MULTIPLIER_BPS,
            weighted_attestation::DEFAULT_MAX_WEIGHT,
        )
    );

    let (stake, weight) = e.as_contract(&contract_id, || {
        (
            weighted_attestation::get_attester_stake(&e, &attester),
            weighted_attestation::compute_weight(&e, &attester),
        )
    });

    assert_eq!(stake, 0);
    assert_eq!(weight, DEFAULT_ATTESTATION_WEIGHT);
}

#[test]
#[should_panic(expected = "attester stake cannot be negative")]
fn negative_attester_stake_is_rejected() {
    let e = Env::default();
    let (_client, _admin, attester, contract_id) = setup(&e);

    e.as_contract(&contract_id, || {
        weighted_attestation::set_attester_stake(&e, &attester, -1);
    });
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Invariant: for fixed config, increasing stake never decreases the
    /// derived weight. Saturating the second stake keeps generated cases inside
    /// the supported non-negative `i128` domain.
    #[test]
    fn prop_weight_monotonic_non_decreasing_in_stake(
        stake in 0_i128..=i128::MAX,
        delta in 0_i128..=1_000_000_000_000_i128,
        multiplier_bps in 0_u32..=u32::MAX,
        max_weight in 0_u32..=u32::MAX,
    ) {
        let greater_stake = stake.saturating_add(delta);

        let (w1, _, _) = computed_weight(stake, multiplier_bps, max_weight);
        let (w2, _, _) = computed_weight(greater_stake, multiplier_bps, max_weight);

        prop_assert!(
            w2 >= w1,
            "weight decreased: stake={stake} w1={w1}, greater_stake={greater_stake} w2={w2}, multiplier_bps={multiplier_bps}, max_weight={max_weight}"
        );
    }

    /// Invariant: positive attestation weights never exceed the effective
    /// ceiling. The protocol also enforces a minimum stored attestation weight
    /// of 1, so `max_weight=0` still yields the default weight.
    #[test]
    fn prop_weight_respects_effective_clamp(
        stake in 0_i128..=i128::MAX,
        multiplier_bps in 0_u32..=u32::MAX,
        max_weight in 0_u32..=u32::MAX,
    ) {
        let (weight, stored_multiplier, stored_max) =
            computed_weight(stake, multiplier_bps, max_weight);
        let effective_ceiling = stored_max.max(DEFAULT_ATTESTATION_WEIGHT);

        prop_assert!(stored_multiplier <= MAX_WEIGHT_MULTIPLIER_BPS);
        prop_assert!(stored_max <= MAX_ATTESTATION_WEIGHT);
        prop_assert!(weight <= effective_ceiling);
        prop_assert!(weight <= MAX_ATTESTATION_WEIGHT);
        prop_assert_eq!(weight, expected_weight(stake, multiplier_bps, max_weight));
    }

    /// Invariant: zero stake takes the documented default floor, independently
    /// of multiplier and max-weight settings, and never panics.
    #[test]
    fn prop_zero_stake_uses_default_weight(
        multiplier_bps in 0_u32..=u32::MAX,
        max_weight in 0_u32..=u32::MAX,
    ) {
        let (weight, _, _) = computed_weight(0, multiplier_bps, max_weight);

        prop_assert_eq!(weight, DEFAULT_ATTESTATION_WEIGHT);
    }

    /// Invariant: weight derivation uses integer floor division across the
    /// generated domain. This catches accidental ceil/round-nearest changes.
    #[test]
    fn prop_rounding_is_floor_division(
        stake in 1_i128..=i128::MAX,
        multiplier_bps in 0_u32..=MAX_WEIGHT_MULTIPLIER_BPS,
        max_weight in 1_u32..=MAX_ATTESTATION_WEIGHT,
    ) {
        let (weight, _, _) = computed_weight(stake, multiplier_bps, max_weight);

        prop_assert_eq!(weight, expected_weight(stake, multiplier_bps, max_weight));
    }
}

/// Regression: max-range inputs must clamp instead of overflowing before the
/// clamp is applied.
#[test]
fn regression_max_range_inputs_do_not_overflow() {
    let cases: &[(i128, u32, u32, u32)] = &[
        (
            i128::MAX,
            MAX_WEIGHT_MULTIPLIER_BPS,
            MAX_ATTESTATION_WEIGHT,
            MAX_ATTESTATION_WEIGHT,
        ),
        (i128::MAX, u32::MAX, u32::MAX, MAX_ATTESTATION_WEIGHT),
        (
            i128::MAX,
            0,
            MAX_ATTESTATION_WEIGHT,
            DEFAULT_ATTESTATION_WEIGHT,
        ),
        (0, u32::MAX, u32::MAX, DEFAULT_ATTESTATION_WEIGHT),
    ];

    for &(stake, multiplier_bps, max_weight, expected) in cases {
        let (weight, stored_multiplier, stored_max) =
            computed_weight(stake, multiplier_bps, max_weight);
        assert_eq!(weight, expected);
        assert!(stored_multiplier <= MAX_WEIGHT_MULTIPLIER_BPS);
        assert!(stored_max <= MAX_ATTESTATION_WEIGHT);
    }
}

/// Regression: boundary stakes around the exact max-weight threshold clamp
/// monotonically and only at/after the floor threshold.
#[test]
fn regression_boundary_vectors_at_max_weight_threshold() {
    let cases: &[(i128, u32, u32, u32)] = &[
        // max=500, multiplier=100 (1%): threshold stake is 50_000.
        (49_999, 100, 500, 499),
        (50_000, 100, 500, 500),
        (50_001, 100, 500, 500),
        // multiplier upper bound: threshold stake for max=500 is 500.
        (499, MAX_WEIGHT_MULTIPLIER_BPS, 500, 499),
        (500, MAX_WEIGHT_MULTIPLIER_BPS, 500, 500),
        (501, MAX_WEIGHT_MULTIPLIER_BPS, 500, 500),
        // multiplier above the configured bound is stored and computed as 10_000.
        (499, u32::MAX, 500, 499),
        (500, u32::MAX, 500, 500),
        (501, u32::MAX, 500, 500),
        // edge caps.
        (1_000_000, 0, 500, DEFAULT_ATTESTATION_WEIGHT),
        (1_000_000, 100, 0, DEFAULT_ATTESTATION_WEIGHT),
    ];

    for &(stake, multiplier_bps, max_weight, expected) in cases {
        let (weight, stored_multiplier, stored_max) =
            computed_weight(stake, multiplier_bps, max_weight);
        assert_eq!(
            weight, expected,
            "stake={stake} multiplier_bps={multiplier_bps} max_weight={max_weight}"
        );
        assert!(stored_multiplier <= MAX_WEIGHT_MULTIPLIER_BPS);
        assert!(stored_max <= MAX_ATTESTATION_WEIGHT);
    }
}

/// Regression: fractional basis-point products are floored, not ceiled.
#[test]
fn regression_rounding_direction_floor_vectors() {
    let cases: &[(i128, u32, u32, u32)] = &[
        (9_999, 100, 100_000, 99),
        (10_000, 100, 100_000, 100),
        (10_001, 100, 100_000, 100),
        (33_333, 300, 100_000, 999),
        (33_334, 300, 100_000, 1_000),
    ];

    for &(stake, multiplier_bps, max_weight, expected) in cases {
        let (weight, _, _) = computed_weight(stake, multiplier_bps, max_weight);
        assert_eq!(
            weight, expected,
            "floor rounding changed for stake={stake} multiplier_bps={multiplier_bps}"
        );
    }
}
