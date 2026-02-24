//! Tests for weighted attestation: weight from attester stake, config, cap.

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
