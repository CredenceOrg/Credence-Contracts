//! Tests that every state-mutating arbitration entrypoint bumps instance storage TTL.

use crate::consts::INSTANCE_TTL_EXTEND_TO;
use crate::{CredenceArbitration, CredenceArbitrationClient};
use soroban_sdk::testutils::storage::Instance as InstanceTestutils;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, String};

fn setup(e: &Env) -> (CredenceArbitrationClient<'_>, Address, Address) {
    e.ledger().with_mut(|li| {
        li.max_entry_ttl = INSTANCE_TTL_EXTEND_TO + 1;
        li.timestamp = 0;
    });
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin);
    (client, admin, contract_id)
}

fn get_instance_ttl(e: &Env, contract_id: &Address) -> u32 {
    e.as_contract(contract_id, || {
        InstanceTestutils::get_ttl(&e.storage().instance())
    })
}

#[test]
fn test_initialize_bumps_ttl() {
    let e = Env::default();
    let (_client, _admin, contract_id) = setup(&e);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after initialize, got {ttl}");
}

#[test]
fn test_register_arbitrator_bumps_ttl() {
    let e = Env::default();
    let (client, _admin, contract_id) = setup(&e);
    let arbitrator = Address::generate(&e);
    client.register_arbitrator(&arbitrator, &10_i128);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(
        ttl > 0,
        "TTL should be > 0 after register_arbitrator, got {ttl}"
    );
}

#[test]
fn test_create_dispute_bumps_ttl() {
    let e = Env::default();
    let (client, _admin, contract_id) = setup(&e);
    let creator = Address::generate(&e);
    client.create_dispute(&creator, &String::from_str(&e, "test dispute"), &3600_u64);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after create_dispute, got {ttl}");
}

#[test]
fn test_vote_bumps_ttl() {
    let e = Env::default();
    let (client, _admin, contract_id) = setup(&e);
    let arbitrator = Address::generate(&e);
    client.register_arbitrator(&arbitrator, &10_i128);
    let creator = Address::generate(&e);
    let dispute_id = client.create_dispute(&creator, &String::from_str(&e, "vote test"), &3600_u64);
    client.vote(&arbitrator, &dispute_id, &1_u32);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after vote, got {ttl}");
}
