//! Tests that every state-mutating admin entrypoint bumps instance storage TTL.

use crate::consts::INSTANCE_TTL_EXTEND_TO;
use crate::{AdminContract, AdminContractClient, AdminRole};
use soroban_sdk::testutils::storage::Instance as InstanceTestutils;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (AdminContractClient<'_>, Address, Address) {
    e.ledger().with_mut(|li| {
        li.max_entry_ttl = INSTANCE_TTL_EXTEND_TO + 1;
        li.timestamp = 0;
    });
    let contract_id = e.register(AdminContract, ());
    let client = AdminContractClient::new(e, &contract_id);
    let super_admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1, &100);
    (client, super_admin, contract_id)
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
fn test_add_admin_bumps_ttl() {
    let e = Env::default();
    let (client, super_admin, contract_id) = setup(&e);
    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Operator);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after add_admin, got {ttl}");
}

#[test]
fn test_deactivate_and_reactivate_admin_bumps_ttl() {
    let e = Env::default();
    let (client, super_admin, contract_id) = setup(&e);
    let new_admin = Address::generate(&e);
    client.add_admin(&super_admin, &new_admin, &AdminRole::Operator);
    client.deactivate_admin(&super_admin, &new_admin);
    let ttl_after_deactivate = get_instance_ttl(&e, &contract_id);
    assert!(
        ttl_after_deactivate > 0,
        "TTL should be > 0 after deactivate_admin, got {ttl_after_deactivate}"
    );
    client.reactivate_admin(&super_admin, &new_admin);
    let ttl_after_reactivate = get_instance_ttl(&e, &contract_id);
    assert!(
        ttl_after_reactivate > 0,
        "TTL should be > 0 after reactivate_admin, got {ttl_after_reactivate}"
    );
}
