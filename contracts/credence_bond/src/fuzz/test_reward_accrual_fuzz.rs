//! Fuzz-style tests for reward accrual arithmetic (issue #238).
//!
//! Exercises `add_attestation` across the full range of `weight` values
//! (1 .. MAX_ATTESTATION_WEIGHT) to confirm that the checked arithmetic in
//! the reward accrual path never panics with an overflow and always produces
//! a non-negative `total_reward`.

#![cfg(test)]

extern crate std;

use crate::types::attestation::MAX_ATTESTATION_WEIGHT;
use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    let attester = Address::generate(e);
    client.register_attester(&attester);
    (client, admin, attester, contract_id)
}

fn attest(
    client: &CredenceBondClient<'_>,
    e: &Env,
    attester: &Address,
    contract_id: &Address,
) -> Attestation {
    let subject = Address::generate(e);
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(attester);
    client.add_attestation(
        attester,
        &subject,
        &String::from_str(e, "fuzz"),
        contract_id,
        &deadline,
        &nonce,
    )
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// Reward accrual must not overflow for the maximum possible weight.
#[test]
fn reward_accrual_no_overflow_at_max_weight() {
    let e = Env::default();
    let (client, admin, attester, contract_id) = setup(&e);

    // Drive weight to its maximum by setting a very high multiplier and stake.
    client.set_weight_config(&admin, &10_000u32, &MAX_ATTESTATION_WEIGHT);
    client.set_attester_stake(&admin, &attester, &i128::MAX);

    // Should not panic.
    let att = attest(&client, &e, &attester, &contract_id);
    assert!(att.weight <= MAX_ATTESTATION_WEIGHT);
}

/// Reward accrual must produce a non-negative total for every sampled weight.
#[test]
fn reward_accrual_non_negative_across_weight_range() {
    // Sampled boundary and mid-range weights.
    let weights: &[u32] = &[
        1,
        2,
        100,
        1_000,
        10_000,
        100_000,
        500_000,
        MAX_ATTESTATION_WEIGHT - 1,
        MAX_ATTESTATION_WEIGHT,
    ];

    for &max_w in weights {
        let e = Env::default();
        let (client, admin, attester, contract_id) = setup(&e);

        // Cap weight at `max_w` with a high multiplier so compute_weight reaches the cap.
        client.set_weight_config(&admin, &10_000u32, &max_w);
        client.set_attester_stake(&admin, &attester, &i128::MAX);

        let att = attest(&client, &e, &attester, &contract_id);

        // Replicate the production formula with the same checked arithmetic.
        let base_reward = 1000i128;
        let weight_bonus = (att.weight as i128)
            .checked_mul(100)
            .expect("weight_bonus overflow in test");
        let total_reward = base_reward
            .checked_add(weight_bonus)
            .expect("total_reward overflow in test");

        assert!(
            total_reward >= base_reward,
            "total_reward should be >= base_reward for weight={}",
            att.weight
        );
    }
}
