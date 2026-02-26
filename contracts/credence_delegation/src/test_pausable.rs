#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup() -> (Env, Address, CredenceDelegationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&env, &contract_id);
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
    let owner = Address::generate(&env);
    let delegate = Address::generate(&env);
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));

    // State changes should fail
    assert!(client
        .try_delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64)
        .is_err());

    assert!(client.try_revoke_attestation(&owner, &delegate).is_err());

    client.unpause(&admin);
    assert!(!client.is_paused());

    // State change works again
    let _ = client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);
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
fn test_execute_requires_threshold() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let pid = client.pause(&s1).unwrap();

    assert!(client.try_execute_pause_proposal(&pid).is_err());

    client.approve_pause_proposal(&s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());
}
