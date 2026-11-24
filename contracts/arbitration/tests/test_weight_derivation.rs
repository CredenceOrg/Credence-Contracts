use credence_arbitration::status::{ArbitrationError, DisputeStatus};
use credence_arbitration::{CredenceArbitration, CredenceArbitrationClient, DataKey};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String};

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

fn setup<'a>(e: &'a Env) -> (CredenceArbitrationClient<'a>, Address, Address) {
    e.mock_all_auths();
    let admin = Address::generate(e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(e, &contract_id);
    client.initialize(&admin);
    (client, admin, contract_id)
}

#[test]
fn test_mid_dispute_weight_increase_does_not_affect_open_dispute() {
    let e = Env::default();
    let (client, _admin, _contract_id) = setup(&e);

    let arb1 = Address::generate(&e);
    let arb2 = Address::generate(&e);
    let creator = Address::generate(&e);

    client.register_arbitrator(&arb1, &10);
    client.register_arbitrator(&arb2, &20);

    let dispute_id = client.create_dispute(&creator, &String::from_str(&e, "Dispute #1"), &3600);

    // Admin increases arb1's weight mid-dispute; open dispute must ignore the change.
    client.register_arbitrator(&arb1, &100);
    assert_eq!(client.get_arbitrator_weight(&arb1), 100);

    client.vote(&arb1, &dispute_id, &1);
    client.vote(&arb2, &dispute_id, &2);

    assert_eq!(client.get_tally(&dispute_id, &1), 10);
    assert_eq!(client.get_tally(&dispute_id, &2), 20);

    advance(&e, 3601);
    let winner = client.resolve_dispute(&dispute_id);
    assert_eq!(winner, 2);
    assert_eq!(client.get_dispute(&dispute_id).status, DisputeStatus::Resolved);
}

#[test]
fn test_mid_dispute_weight_change_does_not_affect_quorum() {
    let e = Env::default();
    let (client, admin, _contract_id) = setup(&e);

    let arb1 = Address::generate(&e);
    let creator = Address::generate(&e);

    client.set_quorum(&admin, &25, &1);
    client.register_arbitrator(&arb1, &10);

    let dispute_id = client.create_dispute(&creator, &String::from_str(&e, "Quorum test"), &3600);

    // Admin bumps arb1's weight above quorum mid-dispute.
    client.register_arbitrator(&arb1, &50);

    client.vote(&arb1, &dispute_id, &1);
    assert_eq!(client.get_tally(&dispute_id, &1), 10);

    advance(&e, 3601);
    let outcome = client.resolve_dispute(&dispute_id);
    assert_eq!(outcome, 0);
    assert_eq!(client.get_dispute(&dispute_id).status, DisputeStatus::NoQuorum);
}

#[test]
fn test_mid_dispute_unregister_preserves_snapshot() {
    let e = Env::default();
    let (client, _admin, _contract_id) = setup(&e);

    let arb1 = Address::generate(&e);
    let creator = Address::generate(&e);

    client.register_arbitrator(&arb1, &15);
    let dispute_id = client.create_dispute(&creator, &String::from_str(&e, "Unregister test"), &3600);

    // Unregistering arb1 mid-dispute does not revoke their frozen weight for the open dispute.
    client.unregister_arbitrator(&arb1);

    client.vote(&arb1, &dispute_id, &1);
    assert_eq!(client.get_tally(&dispute_id, &1), 15);

    advance(&e, 3601);
    assert_eq!(client.resolve_dispute(&dispute_id), 1);
}

#[test]
fn test_mid_dispute_new_arbitrator_cannot_vote_on_open_dispute() {
    let e = Env::default();
    let (client, _admin, _contract_id) = setup(&e);

    let arb1 = Address::generate(&e);
    let arb_new = Address::generate(&e);
    let creator = Address::generate(&e);

    client.register_arbitrator(&arb1, &10);
    let dispute_id = client.create_dispute(&creator, &String::from_str(&e, "New arb test"), &3600);

    // Arbitrator registered after dispute creation is not in the frozen snapshot.
    client.register_arbitrator(&arb_new, &100);

    let res = client.try_vote(&arb_new, &dispute_id, &1);
    assert_eq!(res, Err(Ok(ArbitrationError::NotArbitrator)));
}

#[test]
fn test_weight_change_applies_to_subsequent_disputes() {
    let e = Env::default();
    let (client, _admin, _contract_id) = setup(&e);

    let arb1 = Address::generate(&e);
    let creator1 = Address::generate(&e);
    let creator2 = Address::generate(&e);

    client.register_arbitrator(&arb1, &10);
    let dispute_0 = client.create_dispute(&creator1, &String::from_str(&e, "Dispute 0"), &3600);

    client.register_arbitrator(&arb1, &50);
    let dispute_1 = client.create_dispute(&creator2, &String::from_str(&e, "Dispute 1"), &3600);

    client.vote(&arb1, &dispute_0, &1);
    client.vote(&arb1, &dispute_1, &1);

    assert_eq!(client.get_tally(&dispute_0, &1), 10);
    assert_eq!(client.get_tally(&dispute_1, &1), 50);
}

#[test]
fn test_legacy_dispute_without_snapshot_falls_back_to_live_registry() {
    let e = Env::default();
    let (client, _admin, contract_id) = setup(&e);

    let arb1 = Address::generate(&e);
    let creator = Address::generate(&e);

    client.register_arbitrator(&arb1, &12);
    let dispute_id = client.create_dispute(&creator, &String::from_str(&e, "Legacy dispute"), &3600);

    // Simulate a pre-upgrade dispute that has no persisted weight snapshot.
    e.as_contract(&contract_id, || {
        e.storage()
            .persistent()
            .remove(&DataKey::DisputeWeightSnapshot(dispute_id));
    });

    client.vote(&arb1, &dispute_id, &1);
    assert_eq!(client.get_tally(&dispute_id, &1), 12);

    advance(&e, 3601);
    assert_eq!(client.resolve_dispute(&dispute_id), 1);
}

#[test]
fn test_reopen_dispute_refreshes_weight_snapshot() {
    let e = Env::default();
    let (client, admin, _contract_id) = setup(&e);

    let arb1 = Address::generate(&e);
    let creator = Address::generate(&e);

    client.register_arbitrator(&arb1, &10);
    let dispute_id = client.create_dispute(&creator, &String::from_str(&e, "Reopen test"), &3600);

    advance(&e, 3601);
    // No votes -> Tied
    assert_eq!(client.resolve_dispute(&dispute_id), 0);
    client.archive_dispute(&admin, &dispute_id);

    // Update weight before reopening, then change again after reopening.
    client.register_arbitrator(&arb1, &30);
    client.reopen_dispute(&admin, &dispute_id, &3600);
    client.register_arbitrator(&arb1, &99);

    client.vote(&arb1, &dispute_id, &1);
    assert_eq!(client.get_tally(&dispute_id, &1), 30);
}
