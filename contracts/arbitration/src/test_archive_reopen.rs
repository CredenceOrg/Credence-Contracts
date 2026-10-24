#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String};
use status::{ArbitrationError, DisputeStatus};

fn advance(e: &Env, secs: u64) {
    e.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: e.ledger().timestamp() + secs,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });
}

struct Setup<'a> {
    env: Env,
    admin: Address,
    arb: Address,
    creator: Address,
    client: CredenceArbitrationClient<'a>,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let arb = Address::generate(&env);
    let creator = Address::generate(&env);
    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);
    Setup {
        env,
        admin,
        arb,
        creator,
        client,
    }
}

fn create_and_resolve(s: &Setup) -> u64 {
    let desc = String::from_str(&s.env, "test dispute");
    let id = s.client.create_dispute(&s.creator, &desc, &3600);
    s.client.vote(&s.arb, &id, &1);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    id
}

fn create_and_tie(s: &Setup) -> u64 {
    let arb2 = Address::generate(&s.env);
    s.client.register_arbitrator(&arb2, &10);
    let desc = String::from_str(&s.env, "tie dispute");
    let id = s.client.create_dispute(&s.creator, &desc, &3600);
    s.client.vote(&s.arb, &id, &1);
    s.client.vote(&arb2, &id, &2);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    id
}

fn create_and_cancel(s: &Setup) -> u64 {
    let desc = String::from_str(&s.env, "cancel dispute");
    let id = s.client.create_dispute(&s.creator, &desc, &3600);
    s.client.cancel_dispute(&s.creator, &id, &None);
    id
}

#[test]
fn test_archive_resolved_dispute() {
    let s = setup();
    let id = create_and_resolve(&s);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Resolved);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

#[test]
fn test_archive_tied_dispute() {
    let s = setup();
    let id = create_and_tie(&s);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Tied);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

#[test]
fn test_archive_cancelled_dispute() {
    let s = setup();
    let id = create_and_cancel(&s);
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Cancelled);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Archived);
}

#[test]
fn test_archive_rejects_voting_dispute() {
    let s = setup();
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "d"), &3600);
    let err = s
        .client
        .try_archive_dispute(&s.admin, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_archive_rejects_non_admin() {
    let s = setup();
    let id = create_and_resolve(&s);
    let stranger = Address::generate(&s.env);
    let err = s
        .client
        .try_archive_dispute(&stranger, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAdmin);
}

#[test]
fn test_reopen_archived_dispute() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    assert_eq!(s.client.get_dispute(&id).status, DisputeStatus::Archived);
    s.client.try_reopen_dispute(&s.admin, &id, &3600).unwrap();
    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, DisputeStatus::Voting);
    assert_eq!(d.outcome, 0);
}

#[test]
fn test_reopen_rejects_non_admin() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    let stranger = Address::generate(&s.env);
    let err = s
        .client
        .try_reopen_dispute(&stranger, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAdmin);
}

#[test]
fn test_reopen_rejects_non_archived() {
    let s = setup();
    let id = create_and_resolve(&s);
    let err = s
        .client
        .try_reopen_dispute(&s.admin, &id, &3600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_cannot_archive_twice() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    let err = s
        .client
        .try_archive_dispute(&s.admin, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::InvalidTransition);
}

#[test]
fn test_reopen_allows_new_votes() {
    let s = setup();
    let id = create_and_resolve(&s);
    s.client.try_archive_dispute(&s.admin, &id).unwrap();
    s.client.try_reopen_dispute(&s.admin, &id, &3600).unwrap();
    s.client.vote(&s.arb, &id, &2);
    advance(&s.env, 3601);
    let winner = s.client.resolve_dispute(&id);
    assert_eq!(winner, 2);
}

#[test]
fn test_invalid_transition_require_transition() {
    use status::require_transition;
    assert!(require_transition(DisputeStatus::Resolved, DisputeStatus::Archived).is_ok());
    assert!(require_transition(DisputeStatus::Tied, DisputeStatus::Archived).is_ok());
    assert!(require_transition(DisputeStatus::Cancelled, DisputeStatus::Archived).is_ok());
    assert!(require_transition(DisputeStatus::Archived, DisputeStatus::Voting).is_ok());
    assert_eq!(
        require_transition(DisputeStatus::Archived, DisputeStatus::Resolved),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq!(
        require_transition(DisputeStatus::Archived, DisputeStatus::Tied),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq!(
        require_transition(DisputeStatus::Archived, DisputeStatus::Cancelled),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq!(
        require_transition(DisputeStatus::Archived, DisputeStatus::Open),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq!(
        require_transition(DisputeStatus::Resolved, DisputeStatus::Voting),
        Err(ArbitrationError::InvalidTransition)
    );
    assert_eq!(
        require_transition(DisputeStatus::Voting, DisputeStatus::Archived),
        Err(ArbitrationError::InvalidTransition)
    );
}
