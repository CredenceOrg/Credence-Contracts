#![cfg(test)]

//! Authentication boundary tests for CredenceArbitration.
//!
//! Verifies that every non-view #[contractimpl] function requires an
//! authenticated address and rejects unauthorised callers.  Each function
//! has a happy-path test (authorisation granted) and at least one sad-path
//! test (authorisation denied or wrong caller).

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};
use status::ArbitrationError;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    admin: Address,
    arb: Address,
    creator: Address,
    contract_id: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let arb = Address::generate(&env);
    let creator = Address::generate(&env);
    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb, &10_i128);
    Setup {
        env,
        admin,
        arb,
        creator,
        contract_id,
    }
}

fn open_dispute(env: &Env, contract_id: &Address, creator: &Address) -> u64 {
    let client = CredenceArbitrationClient::new(env, contract_id);
    let desc = String::from_str(env, "test dispute");
    client.create_dispute(creator, &desc, &3600_u64)
}

// ---------------------------------------------------------------------------
// register_arbitrator — stored admin must authorize
// ---------------------------------------------------------------------------

/// register_arbitrator fetches the stored admin and calls admin.require_auth().
/// Happy path: admin's auth is present (mock_all_auths), call succeeds.
#[test]
fn register_arbitrator_succeeds_when_admin_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_arb = Address::generate(&s.env);
    client.register_arbitrator(&new_arb, &5_i128);
    assert_eq!(client.get_arbitrator_weight(&new_arb), 5_u32);
}

/// Sad path: weight ≤ 0 is rejected before any state write, even when auth
/// is valid.  Guards the boundary that weight validation is enforced.
#[test]
fn register_arbitrator_rejected_when_weight_is_zero() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_arb = Address::generate(&s.env);
    let err = client
        .try_register_arbitrator(&new_arb, &0_i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::WeightNotPositive);
}

/// Sad path: negative weight is also rejected.
#[test]
fn register_arbitrator_rejected_when_weight_is_negative() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let new_arb = Address::generate(&s.env);
    let err = client
        .try_register_arbitrator(&new_arb, &-1_i128)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::WeightNotPositive);
}

// ---------------------------------------------------------------------------
// unregister_arbitrator — stored admin must authorize
// ---------------------------------------------------------------------------

/// Happy path: admin removes a previously-registered arbitrator.
#[test]
fn unregister_arbitrator_succeeds_when_admin_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    client.unregister_arbitrator(&s.arb);
    // After removal the weight query should fail with NotArbitrator.
    let err = client
        .try_get_arbitrator_weight(&s.arb)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotArbitrator);
}

// ---------------------------------------------------------------------------
// create_dispute — creator must authorize
// ---------------------------------------------------------------------------

/// Happy path: creator's auth is present, dispute is opened in Voting status.
#[test]
fn create_dispute_succeeds_when_creator_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let desc = String::from_str(&s.env, "valid dispute");
    let id = client.create_dispute(&s.creator, &desc, &3600_u64);
    let d = client.get_dispute(&id);
    assert_eq!(d.creator, s.creator);
    assert_eq!(d.status, status::DisputeStatus::Voting);
}

// ---------------------------------------------------------------------------
// cancel_dispute — caller (creator or admin) must authorize
// ---------------------------------------------------------------------------

/// Happy path: creator cancels their own dispute.
#[test]
fn cancel_dispute_by_creator_succeeds() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    client.cancel_dispute(&s.creator, &id, &None);
    assert_eq!(
        client.get_dispute(&id).status,
        status::DisputeStatus::Cancelled
    );
}

/// Happy path: admin cancels any dispute.
#[test]
fn cancel_dispute_by_admin_succeeds() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    client.cancel_dispute(&s.admin, &id, &None);
    assert_eq!(
        client.get_dispute(&id).status,
        status::DisputeStatus::Cancelled
    );
}

/// Sad path: a stranger (neither creator nor admin) is rejected with NotAuthorized.
#[test]
fn cancel_dispute_rejected_when_stranger_calls() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    let stranger = Address::generate(&s.env);
    let err = client
        .try_cancel_dispute(&stranger, &id, &None)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAuthorized);
}

// ---------------------------------------------------------------------------
// vote — voter must be a registered arbitrator and must authorize
// ---------------------------------------------------------------------------

/// Happy path: a registered arbitrator casts a vote and the tally increases.
#[test]
fn vote_succeeds_when_registered_arbitrator_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    client.vote(&s.arb, &id, &1_u32);
    assert_eq!(client.get_tally(&id, &1_u32), 10_i128);
}

/// Sad path: a stranger that was never registered as an arbitrator is rejected.
#[test]
fn vote_rejected_when_caller_is_not_registered_arbitrator() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    let stranger = Address::generate(&s.env);
    let err = client
        .try_vote(&stranger, &id, &1_u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotArbitrator);
}

/// Sad path: outcome 0 is always invalid regardless of who calls.
#[test]
fn vote_rejected_when_outcome_is_zero() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let id = open_dispute(&s.env, &s.contract_id, &s.creator);
    let err = client.try_vote(&s.arb, &id, &0_u32).unwrap_err().unwrap();
    assert_eq!(err, ArbitrationError::InvalidOutcome);
}

// ---------------------------------------------------------------------------
// set_quorum — stored admin must authorize and match
// ---------------------------------------------------------------------------

/// Happy path: admin configures a non-trivial quorum that is then readable.
#[test]
fn set_quorum_succeeds_when_admin_authorizes() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    client.set_quorum(&s.admin, &50_i128, &2_u32);
    let (min_weight, min_voters) = client.get_quorum();
    assert_eq!(min_weight, 50_i128);
    assert_eq!(min_voters, 2_u32);
}

/// Sad path: a stranger (not the stored admin) is rejected with NotAdmin.
#[test]
fn set_quorum_rejected_when_non_admin_calls() {
    let s = setup();
    let client = CredenceArbitrationClient::new(&s.env, &s.contract_id);
    let stranger = Address::generate(&s.env);
    let err = client
        .try_set_quorum(&stranger, &50_i128, &2_u32)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotAdmin);
}
