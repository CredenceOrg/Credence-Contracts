#![cfg(test)]

use crate::{CredenceMultiSig, CredenceMultiSigClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

#[test]
fn test_pause_unpause() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let signer = Address::generate(&e);
    let mut signers = Vec::new(&e);
    signers.push_back(signer.clone());

    let contract_id = e.register_contract(None, CredenceMultiSig);
    let client = CredenceMultiSigClient::new(&e, &contract_id);

    client.initialize(&admin, &signers, &1);

    // Initial state: not paused
    assert!(!client.is_paused());

    // Pause
    client.pause(&admin);
    assert!(client.is_paused());

    // Try a mutating action while paused
    let res = client.try_add_signer(&admin, &Address::generate(&e));
    assert!(res.is_err());

    // Unpause
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Action should now succeed
    client.add_signer(&admin, &Address::generate(&e));
}
