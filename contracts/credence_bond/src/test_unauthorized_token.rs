#![cfg(test)]

use crate::CredenceBondClient;
use credence_errors::ContractError;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

#[test]
fn test_set_token_with_unauthorized_token_rejects() {
    let e = Env::default();
    let contract_id = e.register(crate::CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let accepted_token = Address::generate(&e);
    let unauthorized_token = Address::generate(&e);

    // Initialize contract
    e.mock_all_auths();
    client.initialize(&admin, &None);

    // Set accepted tokens
    let mut accepted_tokens = Vec::new(&e);
    accepted_tokens.push_back(accepted_token.clone());
    client.set_accepted_tokens(&admin, &accepted_tokens);

    // Try to set an unauthorized token - should fail
    let result = client.try_set_token(&admin, &unauthorized_token);
    assert!(result.is_err());

    // Verify the error is UnauthorizedToken
    let err = result.unwrap_err();
    assert_eq!(err, Ok(ContractError::UnauthorizedToken.into()));
}

#[test]
fn test_set_token_with_accepted_token_succeeds() {
    let e = Env::default();
    let contract_id = e.register(crate::CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let accepted_token = Address::generate(&e);

    // Initialize contract
    e.mock_all_auths();
    client.initialize(&admin, &None);

    // Set accepted tokens
    let mut accepted_tokens = Vec::new(&e);
    accepted_tokens.push_back(accepted_token.clone());
    client.set_accepted_tokens(&admin, &accepted_tokens);

    // Set an accepted token - should succeed
    client.set_token(&admin, &accepted_token);

    // Verify token was set
    let stored_token = e.as_contract(&contract_id, || crate::token_integration::get_token(&e));
    assert_eq!(stored_token, accepted_token);
}
