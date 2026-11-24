#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec};

fn setup() -> (Env, Address, CredenceDelegationClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, client)
}

fn assert_pause_invariant(env: &Env, client: &CredenceDelegationClient<'_>, addrs: &Vec<Address>) {
    env.as_contract(&client.address, || {
        let mut counted: u32 = 0;
        for a in addrs.iter() {
            let enabled: bool = env
                .storage()
                .instance()
                .get(&DataKey::PauseSigner(a.clone()))
                .unwrap_or(false);
            if enabled {
                counted = counted.saturating_add(1);
            }
        }
        let stored: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        assert_eq!(counted, stored, "PauseSignerCount mismatch");
    });
}

/// THREAT: T-030
/// Validates pause signer invariant: stored count matches actual signers (prevents unauthorized pause).
#[test]
fn test_idempotent_add_remove_pause_signer_invariant() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);

    let mut addrs: Vec<Address> = Vec::new(&env);
    addrs.push_back(s1.clone());

    // add twice
    client.set_pause_signer(&admin, &s1, &true);
    assert_pause_invariant(&env, &client, &addrs);

    client.set_pause_signer(&admin, &s1, &true);
    assert_pause_invariant(&env, &client, &addrs);

    // remove twice
    client.set_pause_signer(&admin, &s1, &false);
    assert_pause_invariant(&env, &client, &addrs);

    client.set_pause_signer(&admin, &s1, &false);
    assert_pause_invariant(&env, &client, &addrs);
}

#[test]
fn test_alternating_sequence_pause_signer_invariant() {
    let (env, admin, client) = setup();

    let mut signers: Vec<Address> = Vec::new(&env);
    for _ in 0..5 {
        signers.push_back(Address::generate(&env));
    }

    // Alternating add/remove across set
    let s0 = signers.get(0).unwrap();
    let s1 = signers.get(1).unwrap();
    let s2 = signers.get(2).unwrap();
    let s3 = signers.get(3).unwrap();

    client.set_pause_signer(&admin, &s0, &true);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &s1, &true);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &s0, &false);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &s2, &true);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &s1, &false);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &s3, &true);
    assert_pause_invariant(&env, &client, &signers);
}

#[test]
fn test_edge_cases_adding_removing_nonexistent() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    // removing non-existent signer should be fine
    let mut addrs: Vec<Address> = Vec::new(&env);
    addrs.push_back(s1.clone());
    addrs.push_back(s2.clone());

    client.set_pause_signer(&admin, &s1, &false);
    assert_pause_invariant(&env, &client, &addrs);

    // add then remove
    client.set_pause_signer(&admin, &s2, &true);
    assert_pause_invariant(&env, &client, &addrs);

    client.set_pause_signer(&admin, &s2, &false);
    assert_pause_invariant(&env, &client, &addrs);
}
