//! Tests that every state-mutating multisig entrypoint bumps instance storage TTL.

use crate::consts::INSTANCE_TTL_EXTEND_TO;
use crate::multisig::{CredenceMultiSig, CredenceMultiSigClient};
use soroban_sdk::testutils::storage::Instance as InstanceTestutils;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

fn setup(e: &Env) -> (CredenceMultiSigClient, Address, Vec<Address>, Address) {
    e.ledger().with_mut(|li| {
        li.max_entry_ttl = INSTANCE_TTL_EXTEND_TO + 1;
    });
    let contract_id = e.register(CredenceMultiSig, ());
    let client = CredenceMultiSigClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let signer1 = Address::generate(e);
    let signer2 = Address::generate(e);
    let mut signers = Vec::new(e);
    signers.push_back(signer1);
    signers.push_back(signer2);
    e.mock_all_auths();
    client.initialize(&admin, &signers, &1);
    (client, admin, signers, contract_id)
}

fn get_instance_ttl(e: &Env, contract_id: &Address) -> u32 {
    e.as_contract(contract_id, || {
        InstanceTestutils::get_ttl(&e.storage().instance())
    })
}

#[test]
fn test_initialize_bumps_ttl() {
    let e = Env::default();
    let (_client, _admin, _signers, contract_id) = setup(&e);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after initialize, got {ttl}");
}

#[test]
fn test_add_signer_bumps_ttl() {
    let e = Env::default();
    let (client, admin, _signers, contract_id) = setup(&e);
    let new_signer = Address::generate(&e);
    client.add_signer(&admin, &new_signer);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after add_signer, got {ttl}");
}

#[test]
fn test_set_threshold_bumps_ttl() {
    let e = Env::default();
    let (client, admin, _signers, contract_id) = setup(&e);
    client.set_threshold(&admin, &1);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after set_threshold, got {ttl}");
}

#[test]
fn test_submit_proposal_bumps_ttl() {
    let e = Env::default();
    let (client, _admin, signers, contract_id) = setup(&e);
    let proposer = signers.get(0).unwrap();
    let op_hash: BytesN<32> = BytesN::from_array(&e, &[1u8; 32]);
    client.submit_proposal(
        &proposer,
        &crate::multisig::ActionType::ConfigChange,
        &None,
        &None,
        &None,
        &String::from_str(&e, "test proposal"),
        &0_u64,
        &None,
        &op_hash,
    );
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(
        ttl > 0,
        "TTL should be > 0 after submit_proposal, got {ttl}"
    );
}
