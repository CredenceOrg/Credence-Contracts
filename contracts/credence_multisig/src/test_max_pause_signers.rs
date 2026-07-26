#![cfg(test)]

use crate::pausable::{DEFAULT_MAX_PAUSE_SIGNERS, MAX_PAUSE_SIGNERS_HARD_CAP};
use crate::{CredenceMultiSig, CredenceMultiSigClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn setup() -> (Env, Address, Address, CredenceMultiSigClient<'static>) {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let signer = Address::generate(&e);
    let mut signers = Vec::new(&e);
    signers.push_back(signer.clone());

    let contract_id = e.register_contract(None, CredenceMultiSig);
    let client = CredenceMultiSigClient::new(&e, &contract_id);
    client.initialize(&admin, &signers, &1);

    (e, admin, signer, client)
}

// --- Defaults ---

#[test]
fn test_default_max_pause_signers_is_the_documented_default() {
    let (_e, _admin, _signer, client) = setup();
    assert_eq!(client.get_max_pause_signers(), DEFAULT_MAX_PAUSE_SIGNERS);
}

#[test]
fn test_default_does_not_block_ordinary_signer_registration() {
    // Preserves current behavior: registering a handful of pause signers
    // under the default cap must keep working exactly as before this
    // feature existed.
    let (e, admin, _signer, client) = setup();
    for _ in 0..5 {
        client.set_pause_signer(&admin, &Address::generate(&e), &true);
    }
}

// --- Valid updates ---

#[test]
fn test_admin_can_update_max_pause_signers() {
    let (_e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &5);
    assert_eq!(client.get_max_pause_signers(), 5);
}

#[test]
fn test_admin_can_set_max_pause_signers_to_hard_cap() {
    let (_e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &MAX_PAUSE_SIGNERS_HARD_CAP);
    assert_eq!(client.get_max_pause_signers(), MAX_PAUSE_SIGNERS_HARD_CAP);
}

#[test]
fn test_admin_can_set_max_pause_signers_to_one() {
    let (_e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &1);
    assert_eq!(client.get_max_pause_signers(), 1);
}

// --- Invalid values ---

#[test]
#[should_panic(expected = "Error(Contract, #119)")]
fn test_zero_max_pause_signers_rejected() {
    let (_e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #119)")]
fn test_max_pause_signers_above_hard_cap_rejected() {
    let (_e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &(MAX_PAUSE_SIGNERS_HARD_CAP + 1));
}

#[test]
fn test_rejected_update_leaves_previous_value_in_effect() {
    let (_e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &5);

    let res = client.try_set_max_pause_signers(&admin, &0);
    assert!(res.is_err());

    // The last valid configuration is untouched by the rejected call.
    assert_eq!(client.get_max_pause_signers(), 5);
}

// --- Unauthorized access ---

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn test_non_admin_cannot_update_max_pause_signers() {
    let (e, _admin, _signer, client) = setup();
    let attacker = Address::generate(&e);
    client.set_max_pause_signers(&attacker, &5);
}

#[test]
fn test_default_unchanged_after_unauthorized_attempt() {
    let (e, _admin, _signer, client) = setup();
    let attacker = Address::generate(&e);

    let res = client.try_set_max_pause_signers(&attacker, &5);
    assert!(res.is_err());
    assert_eq!(client.get_max_pause_signers(), DEFAULT_MAX_PAUSE_SIGNERS);
    let _ = e;
}

// --- Cap enforcement at signer-registration time ---

#[test]
fn test_cap_enforced_when_registering_pause_signers() {
    let (e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &2);

    client.set_pause_signer(&admin, &Address::generate(&e), &true);
    client.set_pause_signer(&admin, &Address::generate(&e), &true);

    let res = client.try_set_pause_signer(&admin, &Address::generate(&e), &true);
    assert!(res.is_err());
}

#[test]
#[should_panic(expected = "Error(Contract, #120)")]
fn test_registering_beyond_cap_panics_with_typed_error() {
    let (e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &1);

    client.set_pause_signer(&admin, &Address::generate(&e), &true);
    // Second registration exceeds the cap of 1.
    client.set_pause_signer(&admin, &Address::generate(&e), &true);
}

#[test]
fn test_disabling_a_signer_frees_capacity_under_the_cap() {
    let (e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &1);

    let first = Address::generate(&e);
    client.set_pause_signer(&admin, &first, &true);

    // At the cap: a second registration is rejected...
    let res = client.try_set_pause_signer(&admin, &Address::generate(&e), &true);
    assert!(res.is_err());

    // ...but removing the existing signer frees a slot for a new one.
    client.set_pause_signer(&admin, &first, &false);
    client.set_pause_signer(&admin, &Address::generate(&e), &true);
}

#[test]
fn test_raising_the_cap_allows_further_registration() {
    let (e, admin, _signer, client) = setup();
    client.set_max_pause_signers(&admin, &1);
    client.set_pause_signer(&admin, &Address::generate(&e), &true);

    let res = client.try_set_pause_signer(&admin, &Address::generate(&e), &true);
    assert!(res.is_err());

    client.set_max_pause_signers(&admin, &2);
    client.set_pause_signer(&admin, &Address::generate(&e), &true);
}
