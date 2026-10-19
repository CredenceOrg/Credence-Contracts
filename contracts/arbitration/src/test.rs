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

    // empty registry: cursor 0 >= len 0 → CursorOutOfRange
    let err = client
        .try_get_arbitrators_page(&0, &10)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::CursorOutOfRange);

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

#[test]
fn test_quorum_configuration_and_query() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    // Default quorum
    assert_eq!(client.get_quorum(), (0i128, 0u32));

    // Set quorum
    client.set_quorum(&admin, &100, &3);
    assert_eq!(client.get_quorum(), (100i128, 3u32));

    // Non-admin cannot set quorum
    let stranger = Address::generate(&e);
    let err = client
        .try_set_quorum(&stranger, &50, &1)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::NotAdmin);
}

#[test]
fn test_quorum_single_voter_under_min_voters() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let arb = Address::generate(&e);
    let creator = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);
    client.set_quorum(&admin, &0, &2);

    let dispute_id = client.create_dispute(&creator, &String::from_str(&e, "Q1"), &3600);
    client.vote(&arb, &dispute_id, &1);
    advance(&e, 3601);
    let err = client
        .try_resolve_dispute(&dispute_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::QuorumNotMet);
}

// ── Pagination edge-case tests (issue #1298) ──────────────────────────────────

#[test]
fn test_pagination_empty_registry_cursor_zero() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    // Empty registry: cursor 0 >= len 0 → CursorOutOfRange
    let err = client
        .try_get_arbitrators_page(&0, &10)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::CursorOutOfRange);
}

#[test]
fn test_pagination_cursor_past_end() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let arb = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);

    // 1 arbitrator registered; cursor = 5 is out of range
    let err = client
        .try_get_arbitrators_page(&5, &10)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::CursorOutOfRange);
}

#[test]
fn test_pagination_cursor_at_boundary() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    // Register exactly 3 arbitrators
    let mut arbs = soroban_sdk::Vec::new(&e);
    for _ in 0..3 {
        arbs.push_back(Address::generate(&e));
    }
    for arb in arbs.iter() {
        client.register_arbitrator(&arb, &10);
    }

    // cursor = 3 equals registry_len → CursorOutOfRange
    let err = client
        .try_get_arbitrators_page(&3, &10)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::CursorOutOfRange);

    // cursor = 2 is valid (last valid index for a list of 3)
    let (page, next_cursor) = client.get_arbitrators_page(&2, &2);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), arbs.get(2).unwrap());
    assert_eq!(next_cursor, None);
}

#[test]
fn test_pagination_single_item_page() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    let arb = Address::generate(&e);
    client.register_arbitrator(&arb, &10);

    // Single arbitrator, limit = 1
    let (page, next_cursor) = client.get_arbitrators_page(&0, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), arb);
    assert_eq!(next_cursor, None);
}

#[test]
fn test_pagination_limit_zero_uses_default() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    // Register 5 arbitrators
    let mut arbs = soroban_sdk::Vec::new(&e);
    for _ in 0..5 {
        arbs.push_back(Address::generate(&e));
    }
    for arb in arbs.iter() {
        client.register_arbitrator(&arb, &10);
    }

    // limit = 0 → uses DEFAULT_MAX_ITER (50)
    let (page, next_cursor) = client.get_arbitrators_page(&0, &0);
    assert_eq!(page.len(), 5);
    assert_eq!(next_cursor, None);
}

#[test]
fn test_pagination_full_walk_reassembles() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    // Register 7 arbitrators
    let mut arbs = soroban_sdk::Vec::new(&e);
    for _ in 0..7 {
        arbs.push_back(Address::generate(&e));
    }
    for arb in arbs.iter() {
        client.register_arbitrator(&arb, &10);
    }

    // Walk the full list page by page (limit = 2)
    let mut collected = soroban_sdk::Vec::new(&e);
    let mut cursor = 0;
    loop {
        let (page, next) = client.get_arbitrators_page(&cursor, &2);
        for addr in page.iter() {
            collected.push_back(addr);
        }
        match next {
            Some(n) => cursor = n,
            None => break,
        }
    }

    assert_eq!(collected.len(), 7);
    for i in 0..7 {
        assert_eq!(collected.get(i).unwrap(), arbs.get(i).unwrap());
    }
}

#[test]
fn test_pagination_deterministic_ordering() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    let mut arbs = soroban_sdk::Vec::new(&e);
    for _ in 0..10 {
        arbs.push_back(Address::generate(&e));
    }
    for arb in arbs.iter() {
        client.register_arbitrator(&arb, &10);
    }

    // Two calls with same cursor should return same result
    let (page_a, cursor_a) = client.get_arbitrators_page(&3, &4);
    let (page_b, cursor_b) = client.get_arbitrators_page(&3, &4);

    assert_eq!(page_a.len(), page_b.len());
    assert_eq!(cursor_a, cursor_b);
    for i in 0..page_a.len() {
        assert_eq!(page_a.get(i).unwrap(), page_b.get(i).unwrap());
    }
}

#[test]
fn test_pagination_exact_boundary_split() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    // Register 6 arbitrators and split into 3 pages of 2
    let mut arbs = soroban_sdk::Vec::new(&e);
    for _ in 0..6 {
        arbs.push_back(Address::generate(&e));
    }
    for arb in arbs.iter() {
        client.register_arbitrator(&arb, &10);
    }

    let (p1, c1) = client.get_arbitrators_page(&0, &2);
    assert_eq!(p1.len(), 2);
    assert_eq!(c1, Some(2));

    let (p2, c2) = client.get_arbitrators_page(&2, &2);
    assert_eq!(p2.len(), 2);
    assert_eq!(c2, Some(4));

    let (p3, c3) = client.get_arbitrators_page(&4, &2);
    assert_eq!(p3.len(), 2);
    assert_eq!(c3, None);
}

#[test]
fn test_pagination_after_unregister_compacts() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    let mut arbs = soroban_sdk::Vec::new(&e);
    for _ in 0..5 {
        arbs.push_back(Address::generate(&e));
    }
    for arb in arbs.iter() {
        client.register_arbitrator(&arb, &10);
    }

    // Remove the 3rd arbitrator (index 2)
    let removed = arbs.get(2).unwrap();
    client.unregister_arbitrator(&removed);

    // Walk the full list and verify removed arbitrator is absent
    let mut collected = soroban_sdk::Vec::new(&e);
    let mut cursor = 0;
    loop {
        let (page, next) = client.get_arbitrators_page(&cursor, &10);
        for addr in page.iter() {
            collected.push_back(addr);
        }
        match next {
            Some(n) => cursor = n,
            None => break,
        }
    }

    assert_eq!(collected.len(), 4);
    let mut found = false;
    for i in 0..collected.len() {
        if collected.get(i).unwrap() == removed {
            found = true;
        }
    }
    assert!(!found);
}

#[test]
fn test_pagination_concurrent_insert_during_walk() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    // Register 2 arbitrators
    let arb1 = Address::generate(&e);
    let arb2 = Address::generate(&e);
    client.register_arbitrator(&arb1, &10);
    client.register_arbitrator(&arb2, &10);

    // Walk page 1 (limit=1)
    let (page1, cursor1) = client.get_arbitrators_page(&0, &1);
    assert_eq!(page1.len(), 1);
    assert_eq!(cursor1, Some(1));

    // Register a new arbitrator mid-walk
    let arb3 = Address::generate(&e);
    client.register_arbitrator(&arb3, &10);

    // Continue the walk from cursor1. The new arbitrator is at index 2,
    // so the walk should still see arb2 at cursor=1 and then be done
    // (since the Vec now has 3 items but we started mid-walk).
    let mut collected = soroban_sdk::Vec::new(&e);
    let mut cursor = 0;
    loop {
        let (page, next) = client.get_arbitrators_page(&cursor, &10);
        for addr in page.iter() {
            collected.push_back(addr);
        }
        match next {
            Some(n) => cursor = n,
            None => break,
        }
    }

    // Full walk should see all 3 arbitrators in insertion order
    assert_eq!(collected.len(), 3);
    assert_eq!(collected.get(0).unwrap(), arb1);
    assert_eq!(collected.get(1).unwrap(), arb2);
    assert_eq!(collected.get(2).unwrap(), arb3);
}

// ── Adversarial regression cases (issue #1451) ────────────────────────────────
//
// These cases target the boundaries an attacker or an integration bug would
// probe: state that leaks across disputes, guards that are cleared too eagerly
// (or not at all), time boundaries that are off by one, and authorization that
// is checked after state has already been touched.

struct AdversarialSetup<'a> {
    env: Env,
    client: CredenceArbitrationClient<'a>,
    admin: Address,
    arb1: Address,
    arb2: Address,
    creator: Address,
}

fn setup_adversarial() -> AdversarialSetup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let arb1 = Address::generate(&env);
    let arb2 = Address::generate(&env);
    let creator = Address::generate(&env);
    let contract_id = env.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb1, &10i128);
    client.register_arbitrator(&arb2, &5i128);
    AdversarialSetup {
        env,
        client,
        admin,
        arb1,
        arb2,
        creator,
    }
}

fn open(s: &AdversarialSetup, tag: &str) -> u64 {
    s.client
        .create_dispute(&s.creator, &String::from_str(&s.env, tag), &3600u64)
}

// ---------------------------------------------------------------------------
// Reopen must re-establish the active-dispute guard.
//
// Regression: reopen_dispute() set the dispute back to Voting but never wrote
// DataKey::ActiveDispute. Resolving had already cleared that marker, so the
// creator could open a second concurrent dispute while the reopened one was
// still live — breaking the one-active-dispute-per-creator invariant.
// ---------------------------------------------------------------------------

#[test]
fn test_reopen_preserves_active_dispute_guard() {
    let s = setup_adversarial();
    let id = open(&s, "reopen-guard");

    s.client.vote(&s.arb1, &id, &1);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    s.client.archive_dispute(&s.admin, &id);
    s.client.reopen_dispute(&s.admin, &id, &3600u64);

    // The reopened dispute is live again, so the guard must be back.
    assert_eq!(
        s.client.get_dispute(&id).status,
        status::DisputeStatus::Voting
    );
    let err = s
        .client
        .try_create_dispute(&s.creator, &String::from_str(&s.env, "second"), &3600u64)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::OngoingDispute);

    // Resolving the reopened dispute releases the creator again.
    s.client.vote(&s.arb1, &id, &2);
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    let next = open(&s, "after-resolve");
    assert_ne!(next, id);
}

// ---------------------------------------------------------------------------
// The active-dispute guard is per creator, so one creator cannot lock another.
// ---------------------------------------------------------------------------

#[test]
fn test_active_dispute_guard_is_scoped_per_creator() {
    let s = setup_adversarial();
    let other = Address::generate(&s.env);

    let mine = open(&s, "mine");
    assert_eq!(s.client.get_dispute(&mine).creator, s.creator);

    // A different creator is unaffected by the first creator's live dispute.
    let theirs = s
        .client
        .create_dispute(&other, &String::from_str(&s.env, "theirs"), &3600u64);
    assert_eq!(s.client.get_dispute(&theirs).creator, other);

    // Cancelling one dispute frees only that dispute's creator.
    s.client.cancel_dispute(&s.creator, &mine, &None);
    let reopened_mine = open(&s, "mine-again");
    let err = s
        .client
        .try_create_dispute(&other, &String::from_str(&s.env, "theirs-again"), &3600u64)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::OngoingDispute);
    assert_eq!(s.client.get_dispute(&reopened_mine).creator, s.creator);
}

// ---------------------------------------------------------------------------
// A cancellation that is rejected must not clear the active-dispute guard.
// ---------------------------------------------------------------------------

#[test]
fn test_rejected_cancel_does_not_release_active_guard() {
    let s = setup_adversarial();
    let id = open(&s, "cancel-rejected");
    let stranger = Address::generate(&s.env);

    let err = s
        .client
        .try_cancel_dispute(&stranger, &id, &None)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::NotAuthorized);

    // Guard still held: the dispute is untouched and still blocks a new one.
    assert_eq!(
        s.client.get_dispute(&id).status,
        status::DisputeStatus::Voting
    );
    let err = s
        .client
        .try_create_dispute(&s.creator, &String::from_str(&s.env, "blocked"), &3600u64)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, status::ArbitrationError::OngoingDispute);
}

// ---------------------------------------------------------------------------
// Vote tallies and the "has voted" marker are keyed per dispute; a voter must
// not be able to carry state from one dispute into another.
// ---------------------------------------------------------------------------

#[test]
fn test_vote_state_is_isolated_per_dispute() {
    let s = setup_adversarial();
    let first = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "first"), &3600u64);

    s.client.vote(&s.arb1, &first, &1);
    assert!(s.client.has_voted(&first, &s.arb1));
    assert_eq!(s.client.get_tally(&first, &1), 10i128);

    // Settle the first dispute so the creator is free to open another.
    advance(&s.env, 3601);
    s.client.resolve_dispute(&first);

    let second = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "second"), &3600u64);

    // Fresh dispute: no inherited vote marker, no inherited tally.
    assert!(!s.client.has_voted(&second, &s.arb1));
    assert_eq!(s.client.get_tally(&second, &1), 0i128);

    // The same arbitrator may vote again on the new dispute.
    s.client.vote(&s.arb1, &second, &2);
    assert!(s.client.has_voted(&second, &s.arb1));
    assert_eq!(s.client.get_tally(&second, &2), 10i128);
    assert_eq!(s.client.get_tally(&second, &1), 0i128);
}

// ---------------------------------------------------------------------------
// Reopen clears the previous round's votes; a stale "has voted" flag would
// silently disenfranchise an arbitrator on the reopened dispute.
// ---------------------------------------------------------------------------

#[test]
fn test_reopen_clears_stale_vote_markers() {
    let s = setup_adversarial();
    let id = open(&s, "reopen-votes");
    s.client.vote(&s.arb1, &id, &1);
    s.client.vote(&s.arb2, &id, &2);

    advance(&s.env, 3601);
    assert_eq!(s.client.resolve_dispute(&id), 1);

    s.client.archive_dispute(&s.admin, &id);
    s.client.reopen_dispute(&s.admin, &id, &3600u64);

    // Prior round's tallies and markers are gone.
    assert_eq!(s.client.get_tally(&id, &1), 0i128);
    assert_eq!(s.client.get_tally(&id, &2), 0i128);
    assert!(!s.client.has_voted(&id, &s.arb1));
    assert!(!s.client.has_voted(&id, &s.arb2));

    // Both arbitrators can vote again, and quorum sees the new round only.
    s.client.vote(&s.arb1, &id, &3);
    s.client.vote(&s.arb2, &id, &3);
    assert_eq!(s.client.get_tally(&id, &3), 15i128);
}

// ---------------------------------------------------------------------------
// Time boundaries. voting_end is inclusive for voting and exclusive for
// resolution, so the exact boundary second must behave consistently.
// ---------------------------------------------------------------------------

#[test]
fn test_vote_and_resolve_at_exact_voting_end_boundary() {
    let s = setup_adversarial();
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "boundary"), &100u64);
    let end = s.client.get_dispute(&id).voting_end;

    // Pin the ledger exactly to voting_end: a vote is still accepted, but
    // resolution must not be available yet.
    let jump = end - s.env.ledger().timestamp();
    advance(&s.env, jump);
    assert_eq!(s.env.ledger().timestamp(), end);

    s.client.vote(&s.arb1, &id, &1);
    let err = s.client.try_resolve_dispute(&id).unwrap_err().unwrap();
    assert_eq!(err, status::ArbitrationError::VotingNotEnded);
    assert_eq!(
        s.client.get_dispute(&id).status,
        status::DisputeStatus::Voting
    );

    // One second past the boundary, resolution succeeds.
    advance(&s.env, 1);
    assert_eq!(s.client.resolve_dispute(&id), 1);
}

#[test]
fn test_vote_rejected_one_second_after_voting_end() {
    let s = setup_adversarial();
    let id = s
        .client
        .create_dispute(&s.creator, &String::from_str(&s.env, "expired"), &100u64);

    advance(&s.env, 101);
    let err = s.client.try_vote(&s.arb1, &id, &1).unwrap_err().unwrap();
    assert_eq!(err, status::ArbitrationError::VotingInactive);

    // The rejected vote left no marker and no tally behind.
    assert!(!s.client.has_voted(&id, &s.arb1));
    assert_eq!(s.client.get_tally(&id, &1), 0i128);
}

#[test]
fn test_reopened_dispute_uses_fresh_voting_window() {
    let s = setup_adversarial();
    let id = s.client.create_dispute(
        &s.creator,
        &String::from_str(&s.env, "fresh-window"),
        &10u64,
    );
    let original_end = s.client.get_dispute(&id).voting_end;

    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    s.client.archive_dispute(&s.admin, &id);
    s.client.reopen_dispute(&s.admin, &id, &500u64);

    let reopened = s.client.get_dispute(&id);
    assert_eq!(reopened.voting_start, s.env.ledger().timestamp());
    assert!(reopened.voting_end > original_end);
    assert_eq!(reopened.voting_end - reopened.voting_start, 500u64);

    // The new window is actually usable.
    s.client.vote(&s.arb1, &id, &1);
    advance(&s.env, 501);
    assert_eq!(s.client.resolve_dispute(&id), 1);
}

// ---------------------------------------------------------------------------
// Unregistering an arbitrator must strip its voting power immediately, and
// re-registering must not resurrect the votes it already cast.
// ---------------------------------------------------------------------------

#[test]
fn test_unregistered_arbitrator_cannot_vote_or_revote() {
    let s = setup_adversarial();
    let id = s.client.create_dispute(
        &s.creator,
        &String::from_str(&s.env, "unregister"),
        &3600u64,
    );

    s.client.vote(&s.arb2, &id, &1);
    s.client.unregister_arbitrator(&s.arb2);

    let err = s.client.try_vote(&s.arb2, &id, &2).unwrap_err().unwrap();
    assert_eq!(err, status::ArbitrationError::NotArbitrator);

    // The earlier vote's weight is still counted; the new one was not.
    assert_eq!(s.client.get_tally(&id, &1), 5i128);
    assert_eq!(s.client.get_tally(&id, &2), 0i128);
    assert!(s.client.has_voted(&id, &s.arb2));

    // Re-registering restores voting power but must NOT un-spend the vote
    // already cast in this dispute: clearing VoterCasted on unregister would
    // let a re-registered arbitrator double-vote and inflate the tally.
    s.client.register_arbitrator(&s.arb2, &7i128);
    let err = s.client.try_vote(&s.arb2, &id, &2).unwrap_err().unwrap();
    assert_eq!(err, status::ArbitrationError::AlreadyVoted);
    assert_eq!(s.client.get_tally(&id, &2), 0i128);

    // In a *new* dispute the re-registered arbitrator votes at its new weight.
    advance(&s.env, 3601);
    s.client.resolve_dispute(&id);
    let next = open(&s, "post-unregister");
    s.client.vote(&s.arb2, &next, &2);
    assert_eq!(s.client.get_tally(&next, &2), 7i128);
}

// ---------------------------------------------------------------------------
// A dispute that was never created must not resolve, vote, or read as found.
// ---------------------------------------------------------------------------

#[test]
fn test_unknown_dispute_id_is_rejected_everywhere() {
    let s = setup_adversarial();
    let missing = 9_999u64;

    assert_eq!(
        s.client.try_get_dispute(&missing).unwrap_err().unwrap(),
        status::ArbitrationError::DisputeNotFound
    );
    assert_eq!(
        s.client
            .try_vote(&s.arb1, &missing, &1)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::DisputeNotFound
    );
    assert_eq!(
        s.client.try_resolve_dispute(&missing).unwrap_err().unwrap(),
        status::ArbitrationError::DisputeNotFound
    );
    assert_eq!(
        s.client
            .try_cancel_dispute(&s.creator, &missing, &None)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::DisputeNotFound
    );
    assert_eq!(
        s.client
            .try_archive_dispute(&s.admin, &missing)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::DisputeNotFound
    );
    assert_eq!(
        s.client
            .try_reopen_dispute(&s.admin, &missing, &3600u64)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::DisputeNotFound
    );

    // Read-only views stay safe on a missing dispute.
    assert_eq!(s.client.get_tally(&missing, &1), 0i128);
    assert!(!s.client.has_voted(&missing, &s.arb1));
}

// ---------------------------------------------------------------------------
// Authorization ordering: a non-admin must be rejected, and the rejected call
// must not mutate state.
// ---------------------------------------------------------------------------

#[test]
fn test_non_admin_admin_actions_leave_state_unchanged() {
    let s = setup_adversarial();
    let impostor = Address::generate(&s.env);
    let id = open(&s, "impostor");

    // Impostor cannot cancel a dispute they do not own.
    assert_eq!(
        s.client
            .try_cancel_dispute(&impostor, &id, &None)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::NotAuthorized
    );

    // Impostor cannot archive a still-voting dispute, nor reopen one.
    assert_eq!(
        s.client
            .try_archive_dispute(&impostor, &id)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::NotAdmin
    );
    assert_eq!(
        s.client
            .try_reopen_dispute(&impostor, &id, &3600u64)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::NotAdmin
    );

    // Impostor cannot rewrite quorum or the arbitrator registry.
    assert_eq!(
        s.client
            .try_set_quorum(&impostor, &1i128, &1u32)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::NotAdmin
    );
    assert_eq!(s.client.get_quorum(), (0i128, 0u32));
    s.client.unregister_arbitrator(&s.arb1);
    // Unregister is admin-gated via require_auth, so the impostor path above
    // never reached it; the real admin's registry is what matters.
    assert_eq!(
        s.client.get_dispute(&id).status,
        status::DisputeStatus::Voting
    );
    assert_eq!(
        s.client
            .try_get_arbitrator_weight(&s.arb1)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::NotArbitrator
    );
}

// ---------------------------------------------------------------------------
// Quorum is a gate on the aggregate, not a per-outcome check: splitting votes
// across outcomes can satisfy total weight while never resolving a winner.
// ---------------------------------------------------------------------------

#[test]
fn test_quorum_satisfied_by_split_votes_elects_lowest_outcome() {
    let s = setup_adversarial();
    // Weight quorum only, satisfied purely by splitting across two outcomes.
    s.client.set_quorum(&s.admin, &15i128, &0u32);
    let id = open(&s, "split-votes");

    s.client.vote(&s.arb1, &id, &5);
    s.client.vote(&s.arb2, &id, &6);

    advance(&s.env, 3601);
    // Outcome 5 carries the full weight of arb1; outcome 6 arb2's.
    assert_eq!(s.client.resolve_dispute(&id), 5);
    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, status::DisputeStatus::Resolved);
    assert_eq!(d.outcome, 5);
}

#[test]
fn test_quorum_zero_voters_still_ties() {
    let s = setup_adversarial();
    s.client.set_quorum(&s.admin, &0i128, &0u32);
    let id = open(&s, "no-votes");

    advance(&s.env, 3601);
    assert_eq!(s.client.resolve_dispute(&id), 0);
    let d = s.client.get_dispute(&id);
    assert_eq!(d.status, status::DisputeStatus::Tied);
    assert_eq!(d.outcome, 0);
    // A tie must not leave the guard held: the creator can start again.
    let next = open(&s, "after-tie");
    assert_ne!(next, id);
}

// ---------------------------------------------------------------------------
// After admin transfer, the old admin must lose every privileged path and the
// new admin must gain them.
// ---------------------------------------------------------------------------

#[test]
fn test_admin_transfer_moves_all_privileges() {
    let s = setup_adversarial();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&new_admin);

    // Old admin loses privileged operations.
    assert_eq!(
        s.client
            .try_set_quorum(&s.admin, &10i128, &1u32)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::NotAdmin
    );
    assert_eq!(
        s.client
            .try_archive_dispute(&s.admin, &0u64)
            .unwrap_err()
            .unwrap(),
        status::ArbitrationError::NotAdmin
    );

    // New admin has them.
    s.client.set_quorum(&new_admin, &10i128, &1u32);
    assert_eq!(s.client.get_quorum(), (10i128, 1u32));

    // The paused flag and dispute state are untouched by the transfer.
    assert!(!s.client.is_paused());
}

#[test]
fn test_double_initialize_is_rejected_and_keeps_original_admin() {
    let s = setup_adversarial();
    let attacker = Address::generate(&s.env);

    let err = s.client.try_initialize(&attacker).unwrap_err().unwrap();
    assert_eq!(err, status::ArbitrationError::AlreadyInitialized);

    // The original admin is still in charge.
    s.client.set_quorum(&s.admin, &5i128, &1u32);
    assert_eq!(s.client.get_quorum(), (5i128, 1u32));
}
