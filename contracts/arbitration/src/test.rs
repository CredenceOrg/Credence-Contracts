use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Env, String};

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
    client.register_arbitrator(&arb1, &10);
    client.register_arbitrator(&arb2, &5);

    let description = String::from_str(&e, "Dispute #1");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.id, 0);
    assert_eq!(dispute.status, status::DisputeStatus::Voting);

    client.vote(&arb1, &dispute_id, &1);
    client.vote(&arb2, &dispute_id, &2);

    assert_eq!(client.get_tally(&dispute_id, &1), 10);
    assert_eq!(client.get_tally(&dispute_id, &2), 5);

    advance(&e, 3601);

    let winner = client.resolve_dispute(&dispute_id);
    assert_eq!(winner, 1);

    let resolved = client.get_dispute(&dispute_id);
    assert_eq!(resolved.status, status::DisputeStatus::Resolved);
    assert_eq!(resolved.outcome, 1);
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

    advance(&e, 3601);

    let winner = client.resolve_dispute(&dispute_id);
    assert_eq!(winner, 0); // tie → 0
}

#[test]
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
    let err = client.try_vote(&arb, &dispute_id, &1).unwrap_err().unwrap();
    assert_eq!(err, status::ArbitrationError::AlreadyVoted);
}

#[test]
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

    let err = client
        .try_vote(&non_arb, &dispute_id, &1)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::NotArbitrator);
}

#[test]
fn test_get_arbitrator_weight() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let arb = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);

    // returns NotArbitrator before registration
    let err = client.try_get_arbitrator_weight(&arb).unwrap_err().unwrap();
    assert_eq!(err, status::ArbitrationError::NotArbitrator);

    // register
    client.register_arbitrator(&arb, &15);

    // weight success case
    let weight = client.get_arbitrator_weight(&arb);
    assert_eq!(weight, 15);

    // unregister
    client.unregister_arbitrator(&arb);
    let err = client.try_get_arbitrator_weight(&arb).unwrap_err().unwrap();
    assert_eq!(err, status::ArbitrationError::NotArbitrator);
}

#[test]
fn test_has_voted() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let arb = Address::generate(&e);
    let creator = Address::generate(&e);

    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);

    let description = String::from_str(&e, "Vote check");
    let dispute_id = client.create_dispute(&creator, &description, &3600);

    // has_voted before voting (false)
    assert_eq!(client.has_voted(&dispute_id, &arb), false);

    client.vote(&arb, &dispute_id, &1);

    // has_voted after voting (true)
    assert_eq!(client.has_voted(&dispute_id, &arb), true);
}

#[test]
fn test_arbitrator_registry_and_pagination() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);

    client.initialize(&admin);

    // empty registry check
    let (page, next_cursor) = client.get_arbitrators_page(&0, &10);
    assert_eq!(page.len(), 0);
    assert_eq!(next_cursor, None);

    // register arbitrators
    let mut arbs = soroban_sdk::Vec::new(&e);
    for _ in 0..5 {
        arbs.push_back(Address::generate(&e));
    }

    // registry creation & duplicate registration protection
    for arb in arbs.iter() {
        client.register_arbitrator(&arb, &10);
        // duplicate register shouldn't add duplicate keys in registry list
        client.register_arbitrator(&arb, &20);
    }

    // check deterministic ordering & length
    let (page, next_cursor) = client.get_arbitrators_page(&0, &10);
    assert_eq!(page.len(), 5);
    assert_eq!(next_cursor, None);
    for i in 0..5 {
        assert_eq!(page.get(i).unwrap(), arbs.get(i).unwrap());
    }

    // pagination first page (limit = 2)
    let (page_1, cursor_1) = client.get_arbitrators_page(&0, &2);
    assert_eq!(page_1.len(), 2);
    assert_eq!(page_1.get(0).unwrap(), arbs.get(0).unwrap());
    assert_eq!(page_1.get(1).unwrap(), arbs.get(1).unwrap());
    assert_eq!(cursor_1, Some(2));

    // pagination middle page (cursor = 2, limit = 2)
    let (page_2, cursor_2) = client.get_arbitrators_page(&2, &2);
    assert_eq!(page_2.len(), 2);
    assert_eq!(page_2.get(0).unwrap(), arbs.get(2).unwrap());
    assert_eq!(page_2.get(1).unwrap(), arbs.get(3).unwrap());
    assert_eq!(cursor_2, Some(4));

    // pagination final page (cursor = 4, limit = 2)
    let (page_3, cursor_3) = client.get_arbitrators_page(&4, &2);
    assert_eq!(page_3.len(), 1);
    assert_eq!(page_3.get(0).unwrap(), arbs.get(4).unwrap());
    assert_eq!(cursor_3, None);

    // limit greater than cap clamps to cap (limit = 250)
    // Register 205 arbitrators to exceed cap (200)
    for _ in 0..205 {
        let arb = Address::generate(&e);
        client.register_arbitrator(&arb, &10);
    }

    let (page_cap, cursor_cap) = client.get_arbitrators_page(&0, &250);
    // Page length should be capped at 200
    assert_eq!(page_cap.len(), 200);
    assert_eq!(cursor_cap, Some(200));

    // unregister removal correctness & then re-register consistency
    let test_arb = arbs.get(0).unwrap();
    client.unregister_arbitrator(&test_arb);

    // Should compact the list, and order remains deterministic
    let (page_compact, _) = client.get_arbitrators_page(&0, &10);
    // Let's verify test_arb is not in the full list
    let mut found = false;
    let mut cursor = 0;
    loop {
        let (p, next) = client.get_arbitrators_page(&cursor, &100);
        for a in p.iter() {
            if a == test_arb {
                found = true;
            }
        }
        if let Some(n) = next {
            cursor = n;
        } else {
            break;
        }
    }
    assert_eq!(found, false);

    // Re-register test_arb and check consistency
    client.register_arbitrator(&test_arb, &30);
    let mut found_again = false;
    let mut cursor = 0;
    loop {
        let (p, next) = client.get_arbitrators_page(&cursor, &100);
        for a in p.iter() {
            if a == test_arb {
                found_again = true;
            }
        }
        if let Some(n) = next {
            cursor = n;
        } else {
            break;
        }
    }
    assert_eq!(found_again, true);
    assert_eq!(client.get_arbitrator_weight(&test_arb), 30);
}
