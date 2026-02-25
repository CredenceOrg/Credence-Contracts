//! Tests for weighted attestation: weight from attester stake, config, cap.

#![cfg(test)]

use crate::types::attestation::MAX_ATTESTATION_WEIGHT;
use crate::weighted_attestation;
use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Env, String};

fn setup(
    e: &Env,
) -> (
    CredenceBondClient,
    soroban_sdk::Address,
    soroban_sdk::Address,
) {
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = soroban_sdk::Address::generate(e);
    client.initialize(&admin);
    let attester = soroban_sdk::Address::generate(e);
    client.register_attester(&attester);
    (client, admin, attester)
}

#[test]
fn default_weight_is_one() {
    let e = Env::default();
    let (client, _admin, attester) = setup(&e);
    let subject = soroban_sdk::Address::generate(&e);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "data"),
        &client.get_nonce(&attester),
    );
    assert_eq!(att.weight, 1);
}

#[test]
fn weight_increases_with_stake() {
    let e = Env::default();
    let (client, admin, attester) = setup(&e);
    client.set_attester_stake(&admin, &attester, &1_000_000i128);
    client.set_weight_config(&admin, &100u32, &100_000u32);
    let subject = soroban_sdk::Address::generate(&e);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "data"),
        &client.get_nonce(&attester),
    );
    assert!(att.weight >= 1);
}

#[test]
fn weight_capped_by_config() {
    let e = Env::default();
    let (client, admin, attester) = setup(&e);
    client.set_attester_stake(&admin, &attester, &1_000_000_000_000i128);
    client.set_weight_config(&admin, &100_000u32, &500u32);
    let subject = soroban_sdk::Address::generate(&e);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "capped"),
        &client.get_nonce(&attester),
    );
    assert!(att.weight <= 500);
}

#[test]
fn get_weight_config_returns_set_values() {
    let e = Env::default();
    let (client, admin, _attester) = setup(&e);
    client.set_weight_config(&admin, &200u32, &10_000u32);
    let (mult, max) = client.get_weight_config();
    assert_eq!(mult, 200);
    assert_eq!(max, 10_000);
}

#[test]
fn get_attester_stake_default_zero() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin);
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
    let contract_id = e.register_contract(None, CredenceBond);
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
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = soroban_sdk::Address::generate(&e);
    client.initialize(&admin);
    let attester = soroban_sdk::Address::generate(&e);
    client.set_attester_stake(&admin, &attester, &(-1i128));
}

#[test]
fn weight_capped_by_max_attestation_weight() {
    let e = Env::default();
    let (client, admin, attester) = setup(&e);
    // Use stake high enough to exceed MAX_ATTESTATION_WEIGHT but avoid overflow: 200M * 100 / 10_000 = 2M
    client.set_attester_stake(&admin, &attester, &200_000_000i128);
    let max_requested = MAX_ATTESTATION_WEIGHT + 1000u32;
    client.set_weight_config(&admin, &100u32, &max_requested);
    let subject = soroban_sdk::Address::generate(&e);
    let att = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "max_cap"),
        &client.get_nonce(&attester),
    );
    assert!(att.weight <= MAX_ATTESTATION_WEIGHT);
}

#[test]
fn weight_updates_when_stake_changes() {
    let e = Env::default();
    let (client, admin, attester) = setup(&e);
    client.set_weight_config(&admin, &100u32, &100_000u32);

    client.set_attester_stake(&admin, &attester, &10_000i128);
    let subject = soroban_sdk::Address::generate(&e);
    let att1 = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "first"),
        &client.get_nonce(&attester),
    );

    client.set_attester_stake(&admin, &attester, &1_000_000i128);
    let att2 = client.add_attestation(
        &attester,
        &subject,
        &String::from_str(&e, "second"),
        &client.get_nonce(&attester),
    );

    assert!(
        att2.weight > att1.weight,
        "weight should increase when stake increases"
    );
}

#[test]
fn set_weight_config_caps_max_at_protocol_limit() {
    let e = Env::default();
    let (client, admin, _attester) = setup(&e);
    let max_requested = MAX_ATTESTATION_WEIGHT + 5000u32;
    client.set_weight_config(&admin, &100u32, &max_requested);
    let (_mult, max) = client.get_weight_config();
    assert_eq!(max, MAX_ATTESTATION_WEIGHT);
}
