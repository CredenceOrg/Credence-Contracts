use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Env, String};

#[test]
fn test_arbitration_flow() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let arb1 = Address::generate(&e);
    let arb2 = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);

    // Register arbitrators
    client.register_arbitrator(&arb1, &10); // weight 10
    client.register_arbitrator(&arb2, &5); // weight 5

    // Create dispute
    let description = String::from_str(&e, "Dispute #1");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    // Initial state
    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.id, 0);
    assert!(!dispute.resolved);

    // Voting
    client.vote(&arb1, &dispute_id, &1); // outcome 1, weight 10
    client.vote(&arb2, &dispute_id, &2); // outcome 2, weight 5

    assert_eq!(client.get_tally(&dispute_id, &1), 10);
    assert_eq!(client.get_tally(&dispute_id, &2), 5);

    // Resolve dispute (should fail if period not ended)
    // e.ledger().with_mut(|li| li.timestamp += 3601);

    // Fast forward ledger time
    e.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: e.ledger().timestamp() + 3601,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });

    let winner = client.resolve_dispute(&dispute_id);
    assert_eq!(winner, 1);

    let resolved_dispute = client.get_dispute(&dispute_id);
    assert!(resolved_dispute.resolved);
    assert_eq!(resolved_dispute.outcome, 1);
}

#[test]
fn test_tie_scenario() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let arb1 = Address::generate(&e);
    let arb2 = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);

    client.register_arbitrator(&arb1, &10);
    client.register_arbitrator(&arb2, &10);

    let description = String::from_str(&e, "Tie Test");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    client.vote(&arb1, &dispute_id, &1);
    client.vote(&arb2, &dispute_id, &2);

    e.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: e.ledger().timestamp() + 3601,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });

    let winner = client.resolve_dispute(&dispute_id);
    assert_eq!(winner, 0); // Tie results in outcome 0
}

#[test]
#[should_panic(expected = "arbitrator already voted on this dispute")]
fn test_double_voting_prevention() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let arb = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);

    let description = String::from_str(&e, "Double Vote");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    client.vote(&arb, &dispute_id, &1);
    client.vote(&arb, &dispute_id, &1); // Should panic
}

#[test]
#[should_panic(expected = "voter is not an authorized arbitrator")]
fn test_unauthorized_voter() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let non_arb = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);

    let description = String::from_str(&e, "Unauthorized Vote");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    client.vote(&non_arb, &dispute_id, &1);
}
