#![cfg(test)]

use dispute_resolution::{DisputeError, DisputeResolutionContract, DisputeStatus};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup() -> (
    Env,
    Address,
    dispute_resolution::DisputeResolutionContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(DisputeResolutionContract, ());
    let client = dispute_resolution::DisputeResolutionContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, admin, client)
}

#[test]
fn pause_blocks_create_resolve_and_close_but_keeps_reads_available() {
    let (env, admin, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.pause(&admin);
    assert!(client.is_paused());

    let next_creator = Address::generate(&env);
    assert_eq!(
        client
            .try_create_dispute(&next_creator, &resolver)
            .unwrap_err()
            .unwrap(),
        soroban_sdk::Error::from_contract_error(DisputeError::Paused as u32)
    );
    assert_eq!(
        client.try_resolve(&resolver, &id).unwrap_err().unwrap(),
        DisputeError::Paused
    );
    assert_eq!(
        client.try_close(&resolver, &id).unwrap_err().unwrap(),
        DisputeError::Paused
    );

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);

    client.unpause(&admin);
    assert!(!client.is_paused());
    assert_eq!(client.create_dispute(&next_creator, &resolver), id + 1);
}

#[test]
fn pause_and_unpause_follow_signer_threshold_proposals() {
    let (env, admin, client) = setup();
    let signer_one = Address::generate(&env);
    let signer_two = Address::generate(&env);

    client.set_pause_signer(&admin, &signer_one, &true);
    client.set_pause_signer(&admin, &signer_two, &true);
    client.set_pause_threshold(&admin, &2);

    let pause_proposal = client.pause(&signer_one).unwrap();
    assert!(!client.is_paused());
    assert!(client.try_execute_pause_proposal(&pause_proposal).is_err());
    client.approve_pause_proposal(&signer_two, &pause_proposal);
    client.execute_pause_proposal(&pause_proposal);
    assert!(client.is_paused());

    let unpause_proposal = client.unpause(&signer_one).unwrap();
    client.approve_pause_proposal(&signer_two, &unpause_proposal);
    client.execute_pause_proposal(&unpause_proposal);
    assert!(!client.is_paused());
}

#[test]
fn direct_pause_requires_the_initialized_admin() {
    let (env, _admin, client) = setup();
    let unauthorized = Address::generate(&env);

    assert!(client.try_pause(&unauthorized).is_err());
}
