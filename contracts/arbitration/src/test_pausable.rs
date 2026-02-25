#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

fn setup() -> (Env, Address, CredenceArbitrationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, client)
}

#[test]
fn test_pause_blocks_state_changes_but_allows_reads() {
    let (env, admin, client) = setup();

    assert!(!client.is_paused());
    client.pause(&admin);
    assert!(client.is_paused());

    // Read should still work
    let _ = client.get_tally(&0u64, &1u32);

    // State change should fail
    let arbitrator = Address::generate(&env);
    assert!(client.try_register_arbitrator(&arbitrator, &10_i128).is_err());

    client.unpause(&admin);
    assert!(!client.is_paused());

    // State change works again
    client.register_arbitrator(&arbitrator, &10_i128);
}

#[test]
fn test_pause_multisig_flow() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

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
fn test_pause_does_not_block_existing_tests_flow_when_unpaused() {
    let (env, _admin, client) = setup();

    let arb = Address::generate(&env);
    client.register_arbitrator(&arb, &10_i128);

    let creator = Address::generate(&env);
    let description = String::from_str(&env, "Dispute");
    let dispute_id = client.create_dispute(&creator, &description, &3600u64);
    let _ = client.get_dispute(&dispute_id);
}
