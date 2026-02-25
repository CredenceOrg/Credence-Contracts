#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup() -> (Env, Address, CredenceRegistryClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceRegistry, ());
    let client = CredenceRegistryClient::new(&env, &contract_id);
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
    let _ = client.get_admin();

    // State change should fail
    let identity = Address::generate(&env);
    let bond_contract = Address::generate(&env);

    assert!(client.try_register(&identity, &bond_contract).is_err());

    // Unpause
    client.unpause(&admin);
    assert!(!client.is_paused());

    // State change works again
    let _entry = client.register(&identity, &bond_contract);
}

#[test]
fn test_pause_multisig_flow() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    // Propose pause (first approval auto-recorded)
    let pid = client.pause(&s1).unwrap();

    // Still not paused yet
    assert!(!client.is_paused());

    // Approve and execute
    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);

    assert!(client.is_paused());

    // Propose unpause
    let pid2 = client.unpause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pid2);
    client.execute_pause_proposal(&pid2);

    assert!(!client.is_paused());
}

#[test]
fn test_execute_requires_threshold() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();

    // Only 1 approval so far
    assert!(client.try_execute_pause_proposal(&pid).is_err());

    // Add second approval then it succeeds
    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());
}
