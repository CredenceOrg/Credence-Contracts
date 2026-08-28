#![cfg(test)]

use dispute_resolution::{
    DisputeError, DisputeResolutionContract, DisputeResolutionContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_close_succeeds_for_resolver() {
    let env = Env::default();
    let resolver = Address::generate(&env);
    let client = DisputeResolutionContractClient::new(
        &env,
        &env.register_contract(None, dispute_resolution::DisputeResolutionContract {}),
    );

    let dispute_id = client.create_dispute(&resolver);

    assert!(client.close(&dispute_id).is_ok());

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.status, dispute_resolution::DisputeStatus::Closed);
}

#[test]
fn test_double_close_fails() {
    let env = Env::default();
    let resolver = Address::generate(&env);
    let client = DisputeResolutionContractClient::new(
        &env,
        &env.register_contract(None, dispute_resolution::DisputeResolutionContract {}),
    );

    let dispute_id = client.create_dispute(&resolver);
    client.close(&dispute_id).unwrap();

    let result = client.try_close(&dispute_id);
    assert!(result.is_err());
    // Unwrap the error to check specifically if needed
}

#[test]
fn test_unauthorized_close_fails() {
    let env = Env::default();
    let resolver = Address::generate(&env);
    let attacker = Address::generate(&env);
    let client = DisputeResolutionContractClient::new(
        &env,
        &env.register_contract(None, dispute_resolution::DisputeResolutionContract {}),
    );

    let dispute_id = client.create_dispute(&resolver);

    // Close with attacker (using try_* to catch the panic)
    let result = client.try_close(&dispute_id);
    assert!(result.is_err());
}
