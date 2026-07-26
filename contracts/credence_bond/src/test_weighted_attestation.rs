//! Tests for weighted attestation: weight from attester stake, config, cap.

use crate::types::attestation::MAX_ATTESTATION_WEIGHT;
use crate::weighted_attestation;
use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Env, String};

fn setup(
    e: &Env,
) -> (
    CredenceBondClient<'_>,
    soroban_sdk::Address,
    soroban_sdk::Address,
    soroban_sdk::Address, // contract_id
) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = soroban_sdk::Address::generate(e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(e);
    client.register_attester(&attester);
    (client, admin, attester, contract_id)
}

#[test]
fn default_weight_is_one() {
    let e = Env::default();
    let (client, _admin, attester, contract_id) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(&attester);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "data"),
        &contract_id,
        &deadline,
        &nonce,
    );
    assert_eq!(att.weight, 1);
}

#[test]
fn weight_increases_with_stake() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    client.set_attester_stake(&admin, &attester, &1_000_000i128);
    client.set_weight_config(&admin, &100u32, &100_000u32);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(&attester);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "data"),
        &contract_id,
        &deadline,
        &nonce,
    );
    assert!(att.weight >= 1);
}

#[test]
fn weight_capped_by_config() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    client.set_attester_stake(&admin, &attester, &1_000_000_000_000i128);
    client.set_weight_config(&admin, &100_000u32, &500u32);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(&attester);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "capped"),
        &contract_id,
        &deadline,
        &nonce,
    );
    assert!(att.weight <= 500);
}

#[test]
fn get_weight_config_returns_set_values() {
    let e = Env::default();
    let (client, admin, _attester, _contract_id) = setup(&e);
    client.set_weight_config(&admin, &200u32, &10_000u32);
    let (mult, max) = client.get_weight_config();
    assert_eq!(mult, 200);
    assert_eq!(max, 10_000);
}

#[test]
fn get_attester_stake_default_zero() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);
    let stake = e.as_contract(&contract_id, || {
        weighted_attestation::get_attester_stake(&e, &attester)
    });
    assert_eq!(stake, 0);
}

#[test]
fn compute_weight_zero_stake_returns_default() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let _client = CredenceBondClient::new(&e, &contract_id);
    let attester = soroban_sdk::Address::generate(&e);
    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(w, 1);
}

#[test]
#[should_panic(expected = "attester stake cannot be negative")]
fn set_attester_stake_negative_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.set_attester_stake(&admin, &attester, &(-1i128));
}

#[test]
fn weight_capped_by_max_attestation_weight() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    // Use stake high enough to exceed MAX_ATTESTATION_WEIGHT but avoid overflow: 200M * 100 / 10_000 = 2M
    client.set_attester_stake(&admin, &attester, &200_000_000i128);
    let max_requested = MAX_ATTESTATION_WEIGHT + 1000u32;
    client.set_weight_config(&admin, &100u32, &max_requested);
    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(&attester);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "max_cap"),
        &contract_id,
        &deadline,
        &nonce,
    );
    assert!(att.weight <= MAX_ATTESTATION_WEIGHT);
}

#[test]
fn weight_updates_when_stake_changes() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);
    client.set_weight_config(&admin, &100u32, &100_000u32);
    let deadline = e.ledger().timestamp() + 100_000;

    client.set_attester_stake(&admin, &attester, &10_000i128);
    let subject = soroban_sdk::Address::generate(&e);
    let nonce1 = client.get_nonce(&attester);
    let att1 = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "first"),
        &contract_id,
        &deadline,
        &nonce1,
    );

    client.set_attester_stake(&admin, &attester, &1_000_000i128);
    let nonce2 = client.get_nonce(&attester);
    let att2 = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "second"),
        &contract_id,
        &deadline,
        &nonce2,
    );

    assert!(
        att2.weight > att1.weight,
        "weight should increase when stake increases"
    );
}

#[test]
fn set_weight_config_caps_max_at_protocol_limit() {
    let e = Env::default();
    let (client, admin, _attester, _contract_id) = setup(&e);
    let max_requested = MAX_ATTESTATION_WEIGHT + 5000u32;
    client.set_weight_config(&admin, &100u32, &max_requested);
    let (_mult, max) = client.get_weight_config();
    assert_eq!(max, MAX_ATTESTATION_WEIGHT);
}

// ---------------------------------------------------------------------------
// Rounding invariants
// ---------------------------------------------------------------------------

/// weight = floor(stake * multiplier_bps / 10_000).
/// Verify the formula directly against the public compute_weight helper.
#[test]
fn compute_weight_formula_floor_division() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    // stake=9_999, multiplier=100 → 9_999*100/10_000 = 99 (floor, not 100)
    client.set_attester_stake(&admin, &attester, &9_999i128);
    client.set_weight_config(&admin, &100u32, &100_000u32);

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(w, 99, "floor division: 9_999*100/10_000 must be 99");
}

#[test]
fn compute_weight_exact_boundary_no_remainder() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    // stake=10_000, multiplier=100 → 10_000*100/10_000 = 100 exactly
    client.set_attester_stake(&admin, &attester, &10_000i128);
    client.set_weight_config(&admin, &100u32, &100_000u32);

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(w, 100, "exact boundary: 10_000*100/10_000 must be 100");
}

/// Weight is always >= DEFAULT_ATTESTATION_WEIGHT (1) regardless of stake/config.
#[test]
fn weight_always_at_least_default() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    // Very small stake that rounds to 0 before the floor-max guard
    client.set_attester_stake(&admin, &attester, &1i128);
    client.set_weight_config(&admin, &1u32, &100_000u32); // 1*1/10_000 = 0 → clamped to 1

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert!(w >= 1, "weight must never be zero");
}

/// Weight is always <= MAX_ATTESTATION_WEIGHT regardless of stake/config.
#[test]
fn weight_never_exceeds_protocol_max() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    // Enormous stake + max multiplier; result must still be capped
    // Safe upper bound: u64::MAX / 10_000 to avoid overflow in bps_u64
    let safe_max_stake = (u64::MAX / 10_000) as i128;
    client.set_attester_stake(&admin, &attester, &safe_max_stake);
    client.set_weight_config(&admin, &10_000u32, &(MAX_ATTESTATION_WEIGHT + 1));

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert!(
        w <= MAX_ATTESTATION_WEIGHT,
        "weight {w} must not exceed MAX_ATTESTATION_WEIGHT {MAX_ATTESTATION_WEIGHT}"
    );
}

/// compute_weight is deterministic: same inputs always produce the same output.
#[test]
fn compute_weight_is_deterministic() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    client.set_attester_stake(&admin, &attester, &500_000i128);
    client.set_weight_config(&admin, &200u32, &100_000u32);

    let w1 = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    let w2 = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(w1, w2, "compute_weight must be deterministic");
}

/// Monotonicity: higher stake → weight is non-decreasing.
#[test]
fn weight_monotone_with_stake() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    client.set_weight_config(&admin, &100u32, &100_000u32);

    let stakes: &[i128] = &[0, 1, 100, 10_000, 100_000, 1_000_000, 10_000_000];
    let mut prev = 0u32;
    for &stake in stakes {
        client.set_attester_stake(&admin, &attester, &stake);
        let w = e.as_contract(&contract_id, || {
            weighted_attestation::compute_weight(&e, &attester)
        });
        assert!(
            w >= prev,
            "weight must be non-decreasing: stake={stake}, w={w}, prev={prev}"
        );
        prev = w;
    }
}

// ---------------------------------------------------------------------------
// Regression vectors — fixed (stake, multiplier_bps, expected_weight) triples
// ---------------------------------------------------------------------------

/// Table-driven regression vectors for compute_weight.
/// Formula: weight = max(1, min(floor(stake * mult / 10_000), config_max, MAX_ATTESTATION_WEIGHT))
#[test]
fn regression_vectors_compute_weight() {
    // (stake, multiplier_bps, config_max, expected_weight)
    let cases: &[(i128, u32, u32, u32)] = &[
        // zero stake → default weight
        (0, 100, 100_000, 1),
        // stake=1, mult=1 → floor(1/10_000)=0 → clamped to 1
        (1, 1, 100_000, 1),
        // stake=10_000, mult=100 → 100 exactly
        (10_000, 100, 100_000, 100),
        // stake=9_999, mult=100 → floor(99.99)=99
        (9_999, 100, 100_000, 99),
        // stake=10_001, mult=100 → floor(100.01)=100
        (10_001, 100, 100_000, 100),
        // stake=1_000_000, mult=100 → 10_000; config_max=5_000 → capped at 5_000
        (1_000_000, 100, 5_000, 5_000),
        // stake=50_000, mult=200 → floor(1_000)=1_000
        (50_000, 200, 100_000, 1_000),
        // stake=33_333, mult=300 → floor(999.99)=999
        (33_333, 300, 100_000, 999),
        // stake=33_334, mult=300 → floor(1_000.02)=1_000
        (33_334, 300, 100_000, 1_000),
        // multiplier=0 → 0 → clamped to 1
        (1_000_000, 0, 100_000, 1),
        // config_max=1 → always 1 (above default)
        (1_000_000, 10_000, 1, 1),
        // stake=100, mult=10_000 → floor(100)=100
        (100, 10_000, 100_000, 100),
    ];

    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    for &(stake, mult, cfg_max, expected) in cases {
        client.set_attester_stake(&admin, &attester, &stake);
        client.set_weight_config(&admin, &mult, &cfg_max);
        let w = e.as_contract(&contract_id, || {
            weighted_attestation::compute_weight(&e, &attester)
        });
        assert_eq!(
            w, expected,
            "stake={stake} mult={mult} cfg_max={cfg_max}: expected {expected}, got {w}"
        );
    }
}

/// Regression: stored attestation weight is immutable after creation even if
/// stake or config changes afterwards.
#[test]
fn regression_stored_weight_immutable_after_creation() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);

    client.set_attester_stake(&admin, &attester, &10_000i128);
    client.set_weight_config(&admin, &100u32, &100_000u32);

    let subject = soroban_sdk::Address::generate(&e);
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(&attester);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "immutable"),
        &contract_id,
        &deadline,
        &nonce,
    );
    let original_weight = att.weight;

    // Change stake and config dramatically
    client.set_attester_stake(&admin, &attester, &999_999_999i128);
    client.set_weight_config(&admin, &10_000u32, &100_000u32);

    // Re-fetch the stored attestation — weight must not have changed
    let fetched = client.get_attestation(&att.id);
    assert_eq!(
        fetched.weight, original_weight,
        "stored attestation weight must be immutable after creation"
    );
}

/// Regression: weight config max is silently clamped to MAX_ATTESTATION_WEIGHT
/// and the stored config reflects the clamped value.
#[test]
fn regression_set_weight_config_clamps_silently() {
    let e = Env::default();
    let (client, admin, _attester, _contract_id) = setup(&e);

    let over_limit = MAX_ATTESTATION_WEIGHT + 999_999;
    client.set_weight_config(&admin, &100u32, &over_limit);
    let (_mult, stored_max) = client.get_weight_config();
    assert_eq!(
        stored_max, MAX_ATTESTATION_WEIGHT,
        "config max must be silently clamped to MAX_ATTESTATION_WEIGHT"
    );
}

/// Regression: two different attesters with the same stake produce the same weight.
#[test]
fn regression_equal_stake_equal_weight() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);

    let attester_a = soroban_sdk::Address::generate(&e);
    let attester_b = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester_a);
    client.register_attester(&attester_b);

    client.set_attester_stake(&admin, &attester_a, &50_000i128);
    client.set_attester_stake(&admin, &attester_b, &50_000i128);
    client.set_weight_config(&admin, &100u32, &100_000u32);

    let wa = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester_a)
    });
    let wb = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester_b)
    });
    assert_eq!(wa, wb, "equal stake must produce equal weight");
}

// ---------------------------------------------------------------------------
// Additional rounding edge-cases and regression vectors (issue #281)
// ---------------------------------------------------------------------------

/// Multiplier of exactly BPS_DENOMINATOR (10_000) means weight == stake (before cap).
/// stake=7 * 10_000 / 10_000 = 7.
#[test]
fn regression_multiplier_equals_denominator() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    client.set_attester_stake(&admin, &attester, &7i128);
    client.set_weight_config(&admin, &10_000u32, &100_000u32);

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(w, 7, "stake=7, mult=10_000 → weight must equal stake (7)");
}

/// stake=1, multiplier=10_000 → floor(1 * 10_000 / 10_000) = 1 (no rounding loss).
#[test]
fn regression_unit_stake_full_multiplier() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    client.set_attester_stake(&admin, &attester, &1i128);
    client.set_weight_config(&admin, &10_000u32, &100_000u32);

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(w, 1, "stake=1, mult=10_000 → weight=1 (exact, no rounding)");
}

/// config_max=0 is treated as 0 by min(), but the DEFAULT floor clamps it to 1.
#[test]
fn regression_config_max_zero_clamps_to_default() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    client.set_attester_stake(&admin, &attester, &1_000_000i128);
    client.set_weight_config(&admin, &100u32, &0u32); // max=0 → clamped to 1 by DEFAULT guard

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(
        w, 1,
        "config_max=0 must still yield at least DEFAULT weight (1)"
    );
}

/// Verify weight is independent of attester address — only stake/config matter.
#[test]
fn regression_weight_independent_of_attester_address() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);

    let a1 = soroban_sdk::Address::generate(&e);
    let a2 = soroban_sdk::Address::generate(&e);
    let a3 = soroban_sdk::Address::generate(&e);
    client.register_attester(&a1);
    client.register_attester(&a2);
    client.register_attester(&a3);

    let stake = 123_456i128;
    client.set_attester_stake(&admin, &a1, &stake);
    client.set_attester_stake(&admin, &a2, &stake);
    client.set_attester_stake(&admin, &a3, &stake);
    client.set_weight_config(&admin, &150u32, &100_000u32);

    let w1 = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &a1)
    });
    let w2 = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &a2)
    });
    let w3 = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &a3)
    });

    assert_eq!(
        w1, w2,
        "same stake must yield same weight regardless of address"
    );
    assert_eq!(
        w2, w3,
        "same stake must yield same weight regardless of address"
    );
}

/// Rounding: stake just below a clean multiple always floors down.
/// stake=19_999, mult=100 → floor(199.99) = 199, not 200.
#[test]
fn regression_floor_just_below_clean_multiple() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    client.set_attester_stake(&admin, &attester, &19_999i128);
    client.set_weight_config(&admin, &100u32, &100_000u32);

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(w, 199, "floor(19_999*100/10_000) must be 199, not 200");
}

/// Rounding: stake at exact clean multiple produces no remainder.
/// stake=20_000, mult=100 → 200 exactly.
#[test]
fn regression_exact_clean_multiple() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    client.set_attester_stake(&admin, &attester, &20_000i128);
    client.set_weight_config(&admin, &100u32, &100_000u32);

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(w, 200, "20_000*100/10_000 must be exactly 200");
}

/// Protocol hard cap (MAX_ATTESTATION_WEIGHT) overrides config_max when config_max > protocol cap.
/// This is a separate path from the set_weight_config clamping — compute_weight also enforces it.
#[test]
fn regression_protocol_cap_enforced_in_compute_weight() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.register_attester(&attester);

    // set_weight_config clamps max to MAX_ATTESTATION_WEIGHT, so stored max == MAX_ATTESTATION_WEIGHT
    let over = MAX_ATTESTATION_WEIGHT + 1;
    client.set_weight_config(&admin, &10_000u32, &over);
    // stake large enough that raw weight would exceed MAX_ATTESTATION_WEIGHT
    let safe_stake = (u64::MAX / 10_000) as i128;
    client.set_attester_stake(&admin, &attester, &safe_stake);

    let w = e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(&e, &attester)
    });
    assert_eq!(
        w, MAX_ATTESTATION_WEIGHT,
        "compute_weight must cap at MAX_ATTESTATION_WEIGHT"
    );
}

/// get_weight_config returns defaults when never set.
#[test]
fn regression_default_weight_config() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);

    let (mult, max) = client.get_weight_config();
    assert_eq!(
        mult,
        weighted_attestation::DEFAULT_WEIGHT_MULTIPLIER_BPS,
        "default multiplier must be DEFAULT_WEIGHT_MULTIPLIER_BPS"
    );
    assert_eq!(
        max,
        weighted_attestation::DEFAULT_MAX_WEIGHT,
        "default max must be DEFAULT_MAX_WEIGHT"
    );
}

// ---------------------------------------------------------------------------
// Explicit clamp-boundary pinning (issue #663)
//
// These tests pin the exact lower and upper clamp thresholds of compute_weight
// and re-assert the negative-stake rejection, so a refactor that shifts a
// boundary by one is caught deterministically.
// ---------------------------------------------------------------------------

/// Helper: register an attester, set its stake/config, and compute its weight.
fn weight_for(e: &Env, stake: i128, multiplier_bps: u32, config_max: u32) -> u32 {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = soroban_sdk::Address::generate(e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(e);
    client.register_attester(&attester);
    client.set_attester_stake(&admin, &attester, &stake);
    client.set_weight_config(&admin, &multiplier_bps, &config_max);
    e.as_contract(&contract_id, || {
        weighted_attestation::compute_weight(e, &attester)
    })
}

/// Lower clamp: a raw weight of exactly 0 pins to DEFAULT_ATTESTATION_WEIGHT.
/// stake=1, mult=1 → floor(1/10_000)=0 → clamped up to 1.
#[test]
fn clamp_lower_bound_pins_at_default() {
    use crate::types::attestation::DEFAULT_ATTESTATION_WEIGHT;
    let e = Env::default();
    assert_eq!(
        weight_for(&e, 1, 1, 100_000),
        DEFAULT_ATTESTATION_WEIGHT,
        "raw weight 0 must clamp exactly to DEFAULT_ATTESTATION_WEIGHT"
    );
}

/// Just above the lower clamp: the first stake that produces raw weight 1
/// returns exactly DEFAULT_ATTESTATION_WEIGHT (no over-flooring).
/// stake=10_000, mult=1 → floor(10_000/10_000)=1.
#[test]
fn clamp_lower_bound_first_unit_above_floor() {
    use crate::types::attestation::DEFAULT_ATTESTATION_WEIGHT;
    let e = Env::default();
    assert_eq!(
        weight_for(&e, 10_000, 1, 100_000),
        DEFAULT_ATTESTATION_WEIGHT,
        "raw weight 1 must equal DEFAULT_ATTESTATION_WEIGHT exactly"
    );
}

/// Upper clamp at config_max: a raw weight above config_max pins to config_max
/// exactly (config_max < MAX_ATTESTATION_WEIGHT, so config wins).
/// stake=1_000_000, mult=100 → raw=10_000; config_max=5_000 → 5_000.
#[test]
fn clamp_upper_bound_pins_at_config_max() {
    let e = Env::default();
    assert_eq!(
        weight_for(&e, 1_000_000, 100, 5_000),
        5_000,
        "weight above config_max must pin exactly to config_max"
    );
}

/// Upper clamp exactly at config_max: a raw weight equal to config_max passes
/// through unchanged (boundary is inclusive, not off-by-one).
/// stake=500_000, mult=100 → raw=5_000 == config_max.
#[test]
fn clamp_upper_bound_exact_config_max_passes_through() {
    let e = Env::default();
    assert_eq!(
        weight_for(&e, 500_000, 100, 5_000),
        5_000,
        "raw weight equal to config_max must pass through unchanged"
    );
}

/// Host cap wins: when config_max > MAX_ATTESTATION_WEIGHT and the raw weight
/// also exceeds the protocol cap, compute_weight pins to MAX_ATTESTATION_WEIGHT,
/// not the (clamped-away) config value.
#[test]
fn clamp_upper_bound_host_cap_wins_over_config_max() {
    let e = Env::default();
    let safe_stake = (u64::MAX / 10_000) as i128;
    assert_eq!(
        weight_for(&e, safe_stake, 10_000, MAX_ATTESTATION_WEIGHT + 1),
        MAX_ATTESTATION_WEIGHT,
        "protocol cap must win when config_max exceeds MAX_ATTESTATION_WEIGHT"
    );
}

/// Negative stake is rejected before any weight is computed.
#[test]
#[should_panic(expected = "attester stake cannot be negative")]
fn clamp_set_attester_stake_rejects_negative() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);
    let attester = soroban_sdk::Address::generate(&e);
    client.set_attester_stake(&admin, &attester, &(-42i128));
}

/// Overwriting weight config replaces both fields atomically.
#[test]
fn regression_weight_config_overwrite_is_atomic() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin, &None);

    client.set_weight_config(&admin, &500u32, &50_000u32);
    client.set_weight_config(&admin, &250u32, &25_000u32);

    let (mult, max) = client.get_weight_config();
    assert_eq!(
        mult, 250,
        "second set_weight_config must overwrite multiplier"
    );
    assert_eq!(max, 25_000, "second set_weight_config must overwrite max");
}
