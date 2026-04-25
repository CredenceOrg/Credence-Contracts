#![cfg(test)]

use crate::timelock::{Timelock, TimelockClient};
use credence_errors::ContractError;
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

#[test]
fn test_pause_unpause() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let governance = Address::generate(&e);
    let contract_id = e.register_contract(None, Timelock);
    let client = TimelockClient::new(&e, &contract_id);

    client.initialize(&admin, &governance, &3600);

    // Initial state: not paused
    assert!(!client.is_paused());

    // Pause
    client.pause(&admin);
    assert!(client.is_paused());

    // Try a mutating action while paused
    let res = client.try_update_min_delay(&7200);
    assert!(res.is_err());

    // Unpause
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Action should now succeed
    client.update_min_delay(&7200);
    assert_eq!(client.get_min_delay(), 7200);
}

#[test]
fn test_multi_sig_pause() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let governance = Address::generate(&e);
    let signer1 = Address::generate(&e);
    let signer2 = Address::generate(&e);

    let contract_id = e.register_contract(None, Timelock);
    let client = TimelockClient::new(&e, &contract_id);

    client.initialize(&admin, &governance, &3600);

    // Set up pause signers
    client.set_pause_signer(&admin, &signer1, &true);
    client.set_pause_signer(&admin, &signer2, &true);
    client.set_pause_threshold(&admin, &2);

    // Propose pause
    let proposal_id = client.pause(&signer1).unwrap();
    assert!(!client.is_paused());

    // Approve pause
    client.approve_pause_proposal(&signer2, &proposal_id);
    assert!(!client.is_paused());

    // Execute pause
    client.execute_pause_proposal(&proposal_id);
    assert!(client.is_paused());

    // Propose unpause
    let unpause_id = client.unpause(&signer1).unwrap();
    client.approve_pause_proposal(&signer2, &unpause_id);
    client.execute_pause_proposal(&unpause_id);
    assert!(!client.is_paused());
}
