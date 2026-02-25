#![cfg(test)]

use crate::{BondTier, CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_pause_blocks_state_changes_but_allows_reads() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    assert!(!client.is_paused());
    client.pause(&admin);
    assert!(client.is_paused());

    // Read should still work
    let random_addr = Address::generate(&e);
    assert!(!client.is_attester(&random_addr));

    // State changes should fail
    let identity = Address::generate(&e);
    assert!(client
        .try_create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64)
        .is_err());

    let attester = Address::generate(&e);
    assert!(client.try_register_attester(&attester).is_err());

    client.unpause(&admin);
    assert!(!client.is_paused());

    client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);
}

#[test]
fn test_pause_multisig_flow() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();
    assert!(!client.is_paused());

    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());

    let pid2 = client.unpause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pid2);
    client.execute_pause_proposal(&pid2);
    assert!(!client.is_paused());
}

#[test]
fn test_execute_requires_threshold() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();

    assert!(client.try_execute_pause_proposal(&pid).is_err());

    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());
}
