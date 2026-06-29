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

fn assert_pause_invariant(env: &Env, client: &CredenceDelegationClient<'_>, addrs: &[Address]) {
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

    // add twice
    client.set_pause_signer(&admin, &s1, &true);
    assert_pause_invariant(&env, &client, &[s1.clone()]);

    client.set_pause_signer(&admin, &s1, &true);
    assert_pause_invariant(&env, &client, &[s1.clone()]);

    // remove twice
    client.set_pause_signer(&admin, &s1, &false);
    assert_pause_invariant(&env, &client, &[s1.clone()]);

    client.set_pause_signer(&admin, &s1, &false);
    assert_pause_invariant(&env, &client, &[s1.clone()]);
}

#[test]
fn test_alternating_sequence_pause_signer_invariant() {
    let (env, admin, client) = setup();

    let signers: Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();

    // Alternating add/remove across set
    client.set_pause_signer(&admin, &signers[0], &true);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &signers[1], &true);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &signers[0], &false);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &signers[2], &true);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &signers[1], &false);
    assert_pause_invariant(&env, &client, &signers);

    client.set_pause_signer(&admin, &signers[3], &true);
    assert_pause_invariant(&env, &client, &signers);
}

#[test]
fn test_edge_cases_adding_removing_nonexistent() {
    let (env, admin, client) = setup();

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);

    // removing non-existent signer should be fine
    client.set_pause_signer(&admin, &s1, &false);
    assert_pause_invariant(&env, &client, &[s1.clone(), s2.clone()]);

    // add then remove
    client.set_pause_signer(&admin, &s2, &true);
    assert_pause_invariant(&env, &client, &[s1.clone(), s2.clone()]);

    client.set_pause_signer(&admin, &s2, &false);
    assert_pause_invariant(&env, &client, &[s1.clone(), s2.clone()]);
}
