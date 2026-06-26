//! Tests that every state-mutating treasury entrypoint bumps instance storage TTL.

use crate::consts::INSTANCE_TTL_EXTEND_TO;
use crate::treasury::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::storage::Instance as InstanceTestutils;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceTreasuryClient<'_>, Address, Address, Address) {
    e.ledger().with_mut(|li| {
        li.max_entry_ttl = INSTANCE_TTL_EXTEND_TO + 1;
    });
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let token_id = e.register_stellar_asset_contract(Address::generate(e));
    e.mock_all_auths();
    client.initialize(&admin, &token_id);
    (client, admin, token_id, contract_id)
}

fn get_instance_ttl(e: &Env, contract_id: &Address) -> u32 {
    e.as_contract(contract_id, || {
        InstanceTestutils::get_ttl(&e.storage().instance())
    })
}

#[test]
fn test_initialize_bumps_ttl() {
    let e = Env::default();
    let (_client, _admin, _token, contract_id) = setup(&e);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after initialize, got {ttl}");
}

#[test]
fn test_add_depositor_bumps_ttl() {
    let e = Env::default();
    let (client, _admin, _token, contract_id) = setup(&e);
    let depositor = Address::generate(&e);
    client.add_depositor(&depositor);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after add_depositor, got {ttl}");
}

#[test]
fn test_add_signer_bumps_ttl() {
    let e = Env::default();
    let (client, _admin, _token, contract_id) = setup(&e);
    let signer = Address::generate(&e);
    client.add_signer(&signer);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after add_signer, got {ttl}");
}

#[test]
fn test_set_threshold_bumps_ttl() {
    let e = Env::default();
    let (client, _admin, _token, contract_id) = setup(&e);
    let signer = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(ttl > 0, "TTL should be > 0 after set_threshold, got {ttl}");
}

#[test]
fn test_propose_withdrawal_bumps_ttl() {
    let e = Env::default();
    let (client, admin, token_id, contract_id) = setup(&e);
    let signer = Address::generate(&e);
    client.add_signer(&signer);
    client.set_threshold(&1);

    let stellar_client = soroban_sdk::token::StellarAssetClient::new(&e, &token_id);
    stellar_client.mint(&admin, &1000_i128);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);

    let recipient = Address::generate(&e);
    client.propose_withdrawal(&signer, &recipient, &100);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(
        ttl > 0,
        "TTL should be > 0 after propose_withdrawal, got {ttl}"
    );
}

#[test]
fn test_set_min_liquidity_bumps_ttl() {
    let e = Env::default();
    let (client, admin, _token, contract_id) = setup(&e);
    client.set_min_liquidity(&admin, &500);
    let ttl = get_instance_ttl(&e, &contract_id);
    assert!(
        ttl > 0,
        "TTL should be > 0 after set_min_liquidity, got {ttl}"
    );
}
