#![cfg(test)]

use crate::{FixedDurationBond, FixedDurationBondClient};
use credence_errors::ContractError;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_pause_unpause() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let token = Address::generate(&e);
    let contract_id = e.register_contract(None, FixedDurationBond);
    let client = FixedDurationBondClient::new(&e, &contract_id);

    client.initialize(&admin, &token);

    // Initial state: not paused
    assert!(!client.is_paused());

    // Pause
    client.pause(&admin);
    assert!(client.is_paused());

    // Try a mutating action while paused
    let res = client.try_set_penalty_config(&admin, &500);
    assert!(res.is_err());

    // Unpause
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Action should now succeed
    client.set_penalty_config(&admin, &500);
}
