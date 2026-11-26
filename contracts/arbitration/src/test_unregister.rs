#![cfg(test)]

//! Unregistration policy tests.
//!
//! Policy under test: unregistration is always permitted. Votes already cast
//! keep their snapshot weight in the dispute tally and continue to count
//! toward quorum and resolution. Unregistered arbitrators cannot cast new
//! votes, and `get_arbitrator_weight` returns `NotArbitrator` for them.

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
    arb1: Address,
    arb2: Address,
    creator: Address,
    client: CredenceArbitrationClient<'a>,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let arb1 = Address::generate(&env);
    let arb2 = Address::generate(&env);
    let creator = Address::generate(&env);
    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb1, &10);
    client.register_arbitrator(&arb2, &5);
    Setup {
        env,
        admin,
        arb1,
        arb2,
        creator,
        client,
    }
}

fn open_dispute(s: &Setup, creator: &Address) -> u64 {
    let desc = String::from_str(&s.env, "unregister policy dispute");
    s.client.create_dispute(creator, &desc, &3600)
}

#[test]
fn cast_vote_survives_unregistration_and_decides() {
    let s = setup();
    let dispute_id = open_dispute(&s, &s.creator);

    s.client.vote(&s.arb1, &dispute_id, &1);
    s.client.vote(&s.arb2, &dispute_id, &2);

    // Admin unregisters arb1 while its vote is outstanding on an open dispute.
    s.client.unregister_arbitrator(&s.arb1);

    // Snapshot weight remains visible in the tally.
    assert_eq!(s.client.get_tally(&dispute_id, &1), 10);
    assert_eq!(s.client.get_tally(&dispute_id, &2), 5);
    assert_eq!(
        s.client.try_get_arbitrator_weight(&s.arb1).unwrap_err(),
        ArbitrationError::NotArbitrator
    );

    advance(&s.env, 3601);
    assert_eq!(s.client.resolve_dispute(&dispute_id), 1);

    let dispute = s.client.get_dispute(&dispute_id);
    assert_eq!(dispute.status, DisputeStatus::Resolved);
    assert_eq!(dispute.outcome, 1);
}

#[test]
fn unregistered_vote_still_counts_toward_quorum() {
    let s = setup();
    // Quorum requires weight 10 from a single voter; only arb1 (weight 10)
    // votes, then is unregistered before resolution.
    s.client.set_quorum(&s.admin, &10, &1);

    let dispute_id = open_dispute(&s, &s.creator);
    s.client.vote(&s.arb1, &dispute_id, &1);
    s.client.unregister_arbitrator(&s.arb1);

    advance(&s.env, 3601);
    // Quorum is met by the snapshot weight, so the dispute resolves rather
    // than finalizing as NoQuorum.
    assert_eq!(s.client.resolve_dispute(&dispute_id), 1);
    assert_eq!(
        s.client.get_dispute(&dispute_id).status,
        DisputeStatus::Resolved
    );
}

#[test]
fn quorum_math_ignores_unregistered_weight_for_new_votes() {
    let s = setup();
    // Quorum requires weight 15 across 2 voters. arb1 votes then is
    // unregistered; arb2's 5 is not enough — unregistration must not inflate
    // or silently drop the tally.
    s.client.set_quorum(&s.admin, &15, &2);

    let dispute_id = open_dispute(&s, &s.creator);
    s.client.vote(&s.arb1, &dispute_id, &1);
    s.client.vote(&s.arb2, &dispute_id, &1);
    s.client.unregister_arbitrator(&s.arb1);

    advance(&s.env, 3601);
    assert_eq!(s.client.resolve_dispute(&dispute_id), 0);
    assert_eq!(
        s.client.get_dispute(&dispute_id).status,
        DisputeStatus::NoQuorum
    );
}

#[test]
fn unregistered_arbitrator_cannot_vote_on_new_dispute() {
    let s = setup();
    let dispute_id = open_dispute(&s, &s.creator);
    s.client.vote(&s.arb1, &dispute_id, &1);
    s.client.unregister_arbitrator(&s.arb1);

    // A different creator avoids the one-active-dispute-per-creator guard.
    let other_creator = Address::generate(&s.env);
    let dispute2 = open_dispute(&s, &other_creator);

    let err = s
        .client
        .try_vote(&s.arb1, &dispute2, &1)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ArbitrationError::NotArbitrator);
}

#[test]
fn reregistered_arbitrator_can_vote_again_after_reopen() {
    let s = setup();
    let dispute_id = open_dispute(&s, &s.creator);
    s.client.vote(&s.arb1, &dispute_id, &1);

    // Resolve and archive, then unregister + re-register arb1 before reopen.
    advance(&s.env, 3601);
    assert_eq!(s.client.resolve_dispute(&dispute_id), 1);
    s.client.archive_dispute(&s.admin, &dispute_id);
    s.client.unregister_arbitrator(&s.arb1);
    s.client.register_arbitrator(&s.arb1, &10);

    s.client.reopen_dispute(&s.admin, &dispute_id, &3600);

    // VoterCasted must have been cleared via the per-dispute voter list even
    // though arb1 was absent from the registry at reopen time.
    s.client.vote(&s.arb1, &dispute_id, &2);
    assert_eq!(s.client.get_tally(&dispute_id, &2), 10);
}
