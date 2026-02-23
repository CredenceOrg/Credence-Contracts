#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Env;

#[test]
fn test_delegate_attestation() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let d = client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);

    assert_eq!(d.owner, owner);
    assert_eq!(d.delegate, delegate);
    assert_eq!(d.expires_at, 86400);
    assert!(!d.revoked);
    assert!(matches!(d.delegation_type, DelegationType::Attestation));
}

#[test]
fn test_delegate_management() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    let d = client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64);

    assert_eq!(d.owner, owner);
    assert_eq!(d.delegate, delegate);
    assert!(matches!(d.delegation_type, DelegationType::Management));
}

#[test]
fn test_get_delegation() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert_eq!(d.owner, owner);
    assert_eq!(d.delegate, delegate);
    assert_eq!(d.expires_at, 86400);
}

#[test]
fn test_revoke_delegation() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation);

    let d = client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
    assert!(d.revoked);
}

#[test]
fn test_is_valid_delegate() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);

    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
fn test_is_valid_delegate_not_found() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
fn test_is_valid_delegate_after_revoke() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Management);

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
fn test_is_valid_delegate_after_expiry() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &100_u64);

    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));

    // Advance ledger past expiry
    e.ledger().with_mut(|li| {
        li.timestamp = 200;
    });

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
}

#[test]
fn test_independent_delegation_types() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);
    client.delegate(&owner, &delegate, &DelegationType::Management, &86400_u64);

    // Revoke only attestation
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation);

    assert!(!client.is_valid_delegate(&owner, &delegate, &DelegationType::Attestation));
    assert!(client.is_valid_delegate(&owner, &delegate, &DelegationType::Management));
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_double_initialize() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let admin2 = Address::generate(&e);
    client.initialize(&admin2);
}

#[test]
#[should_panic(expected = "expiry must be in the future")]
fn test_delegate_with_past_expiry() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &500_u64);
}

#[test]
#[should_panic(expected = "delegation not found")]
fn test_get_nonexistent_delegation() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.get_delegation(&owner, &delegate, &DelegationType::Attestation);
}

#[test]
#[should_panic(expected = "already revoked")]
fn test_double_revoke() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceDelegation, ());
    let client = CredenceDelegationClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let owner = Address::generate(&e);
    let delegate = Address::generate(&e);
    client.delegate(&owner, &delegate, &DelegationType::Attestation, &86400_u64);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation);
    client.revoke_delegation(&owner, &delegate, &DelegationType::Attestation);
}
