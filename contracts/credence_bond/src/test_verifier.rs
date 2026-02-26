//! Tests for verifier registration system: stake requirement, reputation, deactivation,
//! event emission, and stake withdrawal.

#![cfg(test)]

extern crate std;

use crate::test_helpers::setup_with_token;
use crate::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::token::TokenClient;
use soroban_sdk::{vec, Address, Env, IntoVal, Symbol, TryFromVal};
use std::panic::{catch_unwind, AssertUnwindSafe};

type ContractEvent = (
    Address,
    soroban_sdk::Vec<soroban_sdk::Val>,
    soroban_sdk::Val,
);

fn count_event_topics(
    events: &soroban_sdk::Vec<ContractEvent>,
    contract_id: &Address,
    topics: &soroban_sdk::Vec<soroban_sdk::Val>,
) -> u32 {
    events
        .iter()
        .filter(|(c, t, _)| c == contract_id && t == topics)
        .count() as u32
}

#[test]
fn set_and_get_verifier_stake_requirement_emits_event() {
    let e = Env::default();
    let (client, admin, _verifier, _token, contract_id) = setup_with_token(&e);

    client.set_verifier_stake_requirement(&admin, &1_000i128);
    let events = e.events().all();

    let topics = vec![&e, Symbol::new(&e, "verifier_config_updated").into_val(&e)];
    assert_eq!(count_event_topics(&events, &contract_id, &topics), 1);
    assert_eq!(client.get_verifier_stake_requirement(), 1_000i128);
}

#[test]
#[should_panic(expected = "insufficient verifier stake")]
fn register_verifier_enforces_min_stake() {
    let e = Env::default();
    let (client, admin, verifier, _token, _contract_id) = setup_with_token(&e);

    client.set_verifier_stake_requirement(&admin, &1_000i128);
    client.register_verifier(&verifier, &999i128);
}

#[test]
fn register_verifier_transfers_stake_sets_active() {
    let e = Env::default();
    let (client, admin, verifier, token, contract_id) = setup_with_token(&e);
    let token_client = TokenClient::new(&e, &token);

    client.set_verifier_stake_requirement(&admin, &1_000i128);

    let stake = 5_000i128;
    let verifier_balance_before = token_client.balance(&verifier);
    let contract_balance_before = token_client.balance(&contract_id);

    let info = client.register_verifier(&verifier, &stake);
    let events = e.events().all();

    let topics = vec![
        &e,
        Symbol::new(&e, "verifier_registered").into_val(&e),
        verifier.clone().into_val(&e),
    ];
    assert_eq!(count_event_topics(&events, &contract_id, &topics), 1);

    let (_cid, _topics, data) = events
        .iter()
        .find(|(c, t, _)| c == &contract_id && t == &topics)
        .unwrap();
    let (kind, deposited, total, min): (Symbol, i128, i128, i128) =
        <(Symbol, i128, i128, i128)>::try_from_val(&e, &data).unwrap();
    assert_eq!(kind, Symbol::new(&e, "new"));
    assert_eq!(deposited, stake);
    assert_eq!(total, stake);
    assert_eq!(min, 1_000i128);

    assert!(info.active);
    assert_eq!(info.stake, stake);

    let verifier_balance_after = token_client.balance(&verifier);
    let contract_balance_after = token_client.balance(&contract_id);
    assert_eq!(
        verifier_balance_after,
        verifier_balance_before.checked_sub(stake).unwrap()
    );
    assert_eq!(
        contract_balance_after,
        contract_balance_before.checked_add(stake).unwrap()
    );

    assert!(client.is_attester(&verifier));
    let stored = client.get_verifier_info(&verifier).unwrap();
    assert_eq!(stored.stake, stake);
    assert!(stored.active);
}

#[test]
#[should_panic(expected = "verifier already active")]
fn register_verifier_while_active_with_zero_deposit_panics() {
    let e = Env::default();
    let (client, admin, verifier, _token, _contract_id) = setup_with_token(&e);
    client.set_verifier_stake_requirement(&admin, &0i128);
    client.register_verifier(&verifier, &1_000i128);

    // Active verifier with zero deposit should be rejected (use a positive deposit to top up).
    client.register_verifier(&verifier, &0i128);
}

#[test]
fn register_verifier_top_up_increases_stake() {
    let e = Env::default();
    let (client, admin, verifier, _token, _contract_id) = setup_with_token(&e);

    client.set_verifier_stake_requirement(&admin, &1_000i128);
    let info1 = client.register_verifier(&verifier, &1_000i128);
    assert_eq!(info1.stake, 1_000i128);

    let info2 = client.register_verifier(&verifier, &500i128);
    assert_eq!(info2.stake, 1_500i128);
}

#[test]
fn deactivate_verifier_prevents_attestation_and_allows_withdrawal() {
    let e = Env::default();
    let (client, admin, verifier, token, contract_id) = setup_with_token(&e);
    let token_client = TokenClient::new(&e, &token);

    client.set_verifier_stake_requirement(&admin, &1_000i128);
    client.register_verifier(&verifier, &2_000i128);

    // Attest once successfully.
    let subject = Address::generate(&e);
    let data = soroban_sdk::String::from_str(&e, "ok");
    let nonce = client.get_nonce(&verifier);
    client.add_attestation(&verifier, &subject, &data, &nonce);

    // Deactivate.
    client.deactivate_verifier(&verifier);
    assert!(!client.is_attester(&verifier));

    // Should no longer be able to attest.
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.add_attestation(
            &verifier,
            &subject,
            &soroban_sdk::String::from_str(&e, "should fail"),
            &client.get_nonce(&verifier),
        );
    }));
    assert!(result.is_err());

    // Withdraw some stake after deactivation.
    let contract_balance_before = token_client.balance(&contract_id);
    let verifier_balance_before = token_client.balance(&verifier);

    let info = client.withdraw_verifier_stake(&verifier, &500i128);
    assert_eq!(info.stake, 1_500i128);

    let contract_balance_after = token_client.balance(&contract_id);
    let verifier_balance_after = token_client.balance(&verifier);
    assert_eq!(
        contract_balance_after,
        contract_balance_before.checked_sub(500).unwrap()
    );
    assert_eq!(
        verifier_balance_after,
        verifier_balance_before.checked_add(500).unwrap()
    );
}

#[test]
#[should_panic(expected = "verifier must be inactive to withdraw stake")]
fn withdraw_verifier_stake_while_active_panics() {
    let e = Env::default();
    let (client, admin, verifier, _token, _contract_id) = setup_with_token(&e);

    client.set_verifier_stake_requirement(&admin, &1_000i128);
    client.register_verifier(&verifier, &1_000i128);
    client.withdraw_verifier_stake(&verifier, &1i128);
}

#[test]
fn admin_can_deactivate_verifier() {
    let e = Env::default();
    let (client, admin, verifier, _token, _contract_id) = setup_with_token(&e);

    client.set_verifier_stake_requirement(&admin, &1_000i128);
    client.register_verifier(&verifier, &1_000i128);
    client.deactivate_verifier_by_admin(&admin, &verifier);
    assert!(!client.is_attester(&verifier));
}

#[test]
fn verifier_reputation_updates_on_attestation_and_revocation() {
    let e = Env::default();
    let (client, admin, verifier, _token, _contract_id) = setup_with_token(&e);

    // Make weight deterministic: weight = stake (100% multiplier) with generous cap.
    client.set_weight_config(&admin, &10_000u32, &1_000_000u32);

    client.set_verifier_stake_requirement(&admin, &1_000i128);
    client.register_verifier(&verifier, &1_000i128);

    let before = client.get_verifier_info(&verifier).unwrap();
    assert_eq!(before.reputation, 0);
    assert_eq!(before.attestations_issued, 0);
    assert_eq!(before.attestations_revoked, 0);

    let subject = Address::generate(&e);
    let data = soroban_sdk::String::from_str(&e, "rep");
    let nonce0 = client.get_nonce(&verifier);
    let att = client.add_attestation(&verifier, &subject, &data, &nonce0);
    assert_eq!(att.weight, 1_000u32);

    let after_add = client.get_verifier_info(&verifier).unwrap();
    assert_eq!(after_add.reputation, 1_000i128);
    assert_eq!(after_add.attestations_issued, 1);
    assert_eq!(after_add.attestations_revoked, 0);

    let nonce1 = client.get_nonce(&verifier);
    client.revoke_attestation(&verifier, &att.id, &nonce1);

    let after_revoke = client.get_verifier_info(&verifier).unwrap();
    assert_eq!(after_revoke.reputation, 0i128);
    assert_eq!(after_revoke.attestations_issued, 1);
    assert_eq!(after_revoke.attestations_revoked, 1);
}

#[test]
#[should_panic(expected = "insufficient verifier stake")]
fn reactivation_fails_if_withdrawn_below_min_stake() {
    let e = Env::default();
    let (client, admin, verifier, _token, _contract_id) = setup_with_token(&e);

    client.set_verifier_stake_requirement(&admin, &1_000i128);
    client.register_verifier(&verifier, &1_000i128);

    client.deactivate_verifier(&verifier);
    client.withdraw_verifier_stake(&verifier, &1i128); // remaining 999

    // Reactivation with no additional deposit should fail since stake < min.
    client.register_verifier(&verifier, &0i128);
}
