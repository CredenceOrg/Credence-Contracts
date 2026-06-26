//! Tests that every state-mutating timelock entrypoint bumps instance storage TTL.

use crate::consts::INSTANCE_TTL_EXTEND_TO;
use crate::{TimelockContract, TimelockContractClient};
use soroban_sdk::testutils::storage::Instance as InstanceTestutils;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env};

fn setup(e: &Env) -> (TimelockContractClient<'_>, Address, Address) {
    e.ledger().with_mut(|li| {
        li.max_entry_ttl = INSTANCE_TTL_EXTEND_TO + 1;
        li.timestamp = 0;
    });
    let contract_id = e.register(TimelockContract, ());
    let client = TimelockContractClient::new(e, &contract_id);
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
fn test_queue_operation_bumps_ttl() {
    let e = Env::default();
    let (client, admin, contract_id) = setup(&e);
    let op_hash: BytesN<32> = BytesN::from_array(&e, &[2u8; 32]);
    client.queue_operation(&admin, &op_hash, &86_400_u64);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(
        ttl > 0,
        "TTL should be > 0 after queue_operation, got {ttl}"
    );
}

#[test]
fn test_cancel_operation_bumps_ttl() {
    let e = Env::default();
    let (client, admin, contract_id) = setup(&e);
    let op_hash: BytesN<32> = BytesN::from_array(&e, &[3u8; 32]);
    let op_id = client.queue_operation(&admin, &op_hash, &86_400_u64);
    client.cancel_operation(&admin, &op_id);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(
        ttl > 0,
        "TTL should be > 0 after cancel_operation, got {ttl}"
    );
}
