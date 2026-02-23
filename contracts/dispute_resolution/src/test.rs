#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

fn setup_token<'a>(
    env: &'a Env,
    admin: &Address,
    recipient: &Address,
    amount: i128,
) -> (
    Address,
    soroban_sdk::token::StellarAssetClient<'a>,
    soroban_sdk::token::Client<'a>,
) {
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(env, &token_id);
    let token_client = soroban_sdk::token::Client::new(env, &token_id);
    token_admin_client.mint(recipient, &amount);
    (token_id, token_admin_client, token_client)
}
// ── create_dispute ────────────────────────────────────────────────────────────

#[test]
fn test_create_dispute_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);

    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &3600);
    assert_eq!(dispute_id, 1);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.disputer, disputer);
    assert_eq!(dispute.slash_request_id, 1);
    assert_eq!(dispute.stake, 500);
    assert_eq!(dispute.status, DisputeStatus::Open);
    assert_eq!(dispute.outcome, DisputeOutcome::None);
    assert_eq!(dispute.votes_for_disputer, 0);
    assert_eq!(dispute.votes_for_slasher, 0);
}

#[test]
fn test_create_dispute_sets_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);

    let current_ts = env.ledger().timestamp();
    let duration = 3600_u64;

    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &duration);
    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.deadline, current_ts + duration);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_create_dispute_fails_insufficient_stake() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, _) = setup_token(&env, &token_admin, &disputer, 1000);

    client.create_dispute(&disputer, &1, &50, &token_id, &3600);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_create_dispute_fails_invalid_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, _) = setup_token(&env, &token_admin, &disputer, 1000);

    client.create_dispute(&disputer, &1, &500, &token_id, &0);
}

#[test]
fn test_create_dispute_transfers_stake_to_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let stake = 500_i128;
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &stake, &1000);
    client.create_dispute(&disputer, &1, &stake, &token_id, &3600);

    assert_eq!(token_client.balance(&disputer), 1000 - stake);
    assert_eq!(token_client.balance(&contract_id), stake);
}

#[test]
fn test_create_multiple_disputes_increments_counter() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 2000);

    token_client.approve(&disputer, &contract_id, &1000, &1000);

    let id1 = client.create_dispute(&disputer, &1, &500, &token_id, &3600);
    let id2 = client.create_dispute(&disputer, &2, &500, &token_id, &3600);

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(client.get_dispute_count(), 2);
}

// ── cast_vote ─────────────────────────────────────────────────────────────────

#[test]
fn test_cast_vote_favor_disputer() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &3600);

    client.cast_vote(&Address::generate(&env), &dispute_id, &true);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.votes_for_disputer, 1);
    assert_eq!(dispute.votes_for_slasher, 0);
}

#[test]
fn test_cast_vote_favor_slasher() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &3600);

    client.cast_vote(&Address::generate(&env), &dispute_id, &false);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.votes_for_disputer, 0);
    assert_eq!(dispute.votes_for_slasher, 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_cast_vote_fails_already_voted() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &3600);

    client.cast_vote(&arbitrator, &dispute_id, &true);
    client.cast_vote(&arbitrator, &dispute_id, &true);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_cast_vote_fails_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &100);

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    client.cast_vote(&Address::generate(&env), &dispute_id, &true);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_cast_vote_fails_dispute_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    client.cast_vote(&Address::generate(&env), &999, &true);
}

#[test]
fn test_has_voted_true_and_false() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let other = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &3600);

    assert!(!client.has_voted(&dispute_id, &arbitrator));
    client.cast_vote(&arbitrator, &dispute_id, &true);
    assert!(client.has_voted(&dispute_id, &arbitrator));
    assert!(!client.has_voted(&dispute_id, &other));
}

#[test]
fn test_multiple_arbitrators_vote() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &3600);

    for _ in 0..3 {
        client.cast_vote(&Address::generate(&env), &dispute_id, &true);
    }
    for _ in 0..2 {
        client.cast_vote(&Address::generate(&env), &dispute_id, &false);
    }

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.votes_for_disputer, 3);
    assert_eq!(dispute.votes_for_slasher, 2);
}

// ── resolve_dispute ───────────────────────────────────────────────────────────

#[test]
fn test_resolve_dispute_favor_disputer_stake_returned() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let stake = 500_i128;
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &stake, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &stake, &token_id, &100);

    client.cast_vote(&Address::generate(&env), &dispute_id, &true);
    client.cast_vote(&Address::generate(&env), &dispute_id, &false);
    client.cast_vote(&Address::generate(&env), &dispute_id, &true);

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    client.resolve_dispute(&dispute_id);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.status, DisputeStatus::Resolved);
    assert_eq!(dispute.outcome, DisputeOutcome::FavorDisputer);
    assert_eq!(token_client.balance(&disputer), 1000);
}

#[test]
fn test_resolve_dispute_favor_slasher_stake_forfeited() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let stake = 500_i128;
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &stake, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &stake, &token_id, &100);

    client.cast_vote(&Address::generate(&env), &dispute_id, &false);
    client.cast_vote(&Address::generate(&env), &dispute_id, &false);

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    client.resolve_dispute(&dispute_id);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.status, DisputeStatus::Resolved);
    assert_eq!(dispute.outcome, DisputeOutcome::FavorSlasher);
    assert_eq!(token_client.balance(&disputer), 1000 - stake);
    assert_eq!(token_client.balance(&contract_id), stake);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_resolve_dispute_fails_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &3600);

    client.resolve_dispute(&dispute_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_resolve_dispute_fails_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    client.resolve_dispute(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_resolve_dispute_fails_already_resolved() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &100);

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    client.resolve_dispute(&dispute_id);
    client.resolve_dispute(&dispute_id);
}

// ── expire_dispute ────────────────────────────────────────────────────────────

#[test]
fn test_expire_dispute_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &100);

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    client.expire_dispute(&dispute_id);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.status, DisputeStatus::Expired);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_expire_dispute_fails_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &3600);

    client.expire_dispute(&dispute_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_expire_dispute_fails_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    client.expire_dispute(&999);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_expire_already_resolved_dispute_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &100);

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    client.resolve_dispute(&dispute_id);
    client.expire_dispute(&dispute_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_cannot_vote_on_expired_dispute() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    let disputer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (token_id, _, token_client) = setup_token(&env, &token_admin, &disputer, 1000);

    token_client.approve(&disputer, &contract_id, &500, &1000);
    let dispute_id = client.create_dispute(&disputer, &1, &500, &token_id, &100);

    env.ledger().set_timestamp(env.ledger().timestamp() + 200);
    client.expire_dispute(&dispute_id);
    client.cast_vote(&Address::generate(&env), &dispute_id, &true);
}

// ── get_dispute_count ─────────────────────────────────────────────────────────

#[test]
fn test_get_dispute_count_empty() {
    let env = Env::default();
    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    assert_eq!(client.get_dispute_count(), 0);
}

#[test]
#[should_panic(expected = "Dispute not found")]
fn test_get_dispute_not_found_panics() {
    let env = Env::default();
    let contract_id = env.register(DisputeContract, ());
    let client = DisputeContractClient::new(&env, &contract_id);

    client.get_dispute(&999);
}
