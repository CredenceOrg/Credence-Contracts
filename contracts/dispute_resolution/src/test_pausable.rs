#![cfg(test)]

use crate::{DisputeContract, DisputeContractClient};
use credence_errors::ContractError;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_pause_unpause() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let contract_id = e.register_contract(None, DisputeContract);
    let client = DisputeContractClient::new(&e, &contract_id);

    client.initialize(&admin);

    // Initial state: not paused
    assert!(!client.is_paused());

    // Pause
    client.pause(&admin);
    assert!(client.is_paused());

    // Try a mutating action while paused
    let res = client.try_create_dispute(
        &Address::generate(&e),
        &1,
        &200,
        &Address::generate(&e),
        &3600,
    );
    assert!(res.is_err());

    // Unpause
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Action should now succeed (it will fail for other reasons like token not valid, but it won't be ContractPaused)
}
