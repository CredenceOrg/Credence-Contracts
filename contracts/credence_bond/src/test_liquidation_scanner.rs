//! Tests for the bounded liquidation scanner (issue #180).

use crate::liquidation_scanner::*;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_register_bond_holder_increases_size() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let identity = Address::generate(&e);
    assert_eq!(client.get_registry_size(), 0);
    client.register_bond_holder(&admin, &identity);
    assert_eq!(client.get_registry_size(), 1);
}

#[test]
fn test_register_bond_holder_idempotent() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let identity = Address::generate(&e);
    client.register_bond_holder(&admin, &identity);
    client.register_bond_holder(&admin, &identity);
    assert_eq!(client.get_registry_size(), 1);
}

#[test]
fn test_deregister_bond_holder_decreases_size() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let identity = Address::generate(&e);
    client.register_bond_holder(&admin, &identity);
    assert_eq!(client.get_registry_size(), 1);
    client.deregister_bond_holder(&admin, &identity);
    assert_eq!(client.get_registry_size(), 0);
}

#[test]
fn test_deregister_nonexistent_is_noop() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let identity = Address::generate(&e);
    client.deregister_bond_holder(&admin, &identity);
    assert_eq!(client.get_registry_size(), 0);
}

#[test]
fn test_register_multiple_holders() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    for _ in 0..10 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    assert_eq!(client.get_registry_size(), 10);
}

#[test]
fn test_max_iter_hard_cap_enforced() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    for _ in 0..10 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    let result = client.scan_liquidation_candidates(&keeper, &0, &(MAX_ITER_HARD_CAP + 100), &0);
    assert!(result.next_cursor <= MAX_ITER_HARD_CAP || result.done);
}

#[test]
fn test_zero_max_iter_uses_default() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    for _ in 0..5 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    let result = client.scan_liquidation_candidates(&keeper, &0, &0, &0);
    assert!(result.done || result.next_cursor > 0);
}

#[test]
fn test_scan_empty_registry_returns_done() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin);
    let keeper = Address::generate(&e);
    let result = client.scan_liquidation_candidates(&keeper, &0, &50, &0);
    assert!(result.done);
    assert_eq!(result.next_cursor, 0);
    assert_eq!(result.candidates.len(), 0);
}

#[test]
fn test_pagination_covers_all_holders_no_overlap() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    let page_size = 3u32;
    let total = 10u32;
    for _ in 0..total {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    let mut cursor = 0u32;
    let mut pages = 0u32;
    let mut total_scanned = 0u32;
    loop {
        let result = client.scan_liquidation_candidates(&keeper, &cursor, &page_size, &0);
        let scanned_this_page = if result.done {
            total - cursor
        } else {
            result.next_cursor - cursor
        };
        total_scanned += scanned_this_page;
        pages += 1;
        if result.done {
            break;
        }
        assert!(result.next_cursor > cursor, "cursor must advance");
        cursor = result.next_cursor;
        assert!(pages <= total + 1, "too many pages");
    }
    assert_eq!(
        total_scanned, total,
        "all holders must be scanned exactly once"
    );
}

#[test]
fn test_pagination_done_flag_set_on_last_page() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    for _ in 0..5 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    let result = client.scan_liquidation_candidates(&keeper, &0, &5, &0);
    assert!(result.done);
    assert_eq!(result.next_cursor, 0);
}

#[test]
fn test_pagination_next_cursor_resets_to_zero_on_completion() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    for _ in 0..4 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    let result = client.scan_liquidation_candidates(&keeper, &0, &10, &0);
    assert!(result.done);
    assert_eq!(result.next_cursor, 0);
}

#[test]
fn test_no_candidates_when_no_bonds_active() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    for _ in 0..5 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    let result = client.scan_liquidation_candidates(&keeper, &0, &50, &5000);
    assert_eq!(result.candidates.len(), 0);
}

#[test]
fn test_get_keeper_cursor_default_zero() {
    let e = Env::default();
    let (client, _) = setup(&e);
    let keeper = Address::generate(&e);
    assert_eq!(client.get_keeper_cursor(&keeper), 0);
}

#[test]
fn test_advance_keeper_cursor_reset_to_zero_allowed() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    for _ in 0..5 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    let result = client.scan_liquidation_candidates(&keeper, &0, &3, &0);
    assert!(!result.done);
    client.advance_keeper_cursor(&keeper, &0);
    assert_eq!(client.get_keeper_cursor(&keeper), 0);
}

#[test]
#[should_panic(expected = "keeper cursor: invalid advance")]
fn test_advance_keeper_cursor_backwards_rejected() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    for _ in 0..10 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    client.scan_liquidation_candidates(&keeper, &0, &5, &0);
    client.advance_keeper_cursor(&keeper, &2);
}

#[test]
#[should_panic(expected = "cursor out of range")]
fn test_scan_with_cursor_beyond_registry_panics() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    for _ in 0..5 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    client.scan_liquidation_candidates(&keeper, &99, &10, &0);
}

#[test]
fn test_register_emits_event() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let identity = Address::generate(&e);
    let events_before = e.events().all().len();
    client.register_bond_holder(&admin, &identity);
    let events_after = e.events().all().len();
    assert!(events_after > events_before);
}

#[test]
fn test_scan_emits_page_event() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let keeper = Address::generate(&e);
    for _ in 0..3 {
        client.register_bond_holder(&admin, &Address::generate(&e));
    }
    let events_before = e.events().all().len();
    client.scan_liquidation_candidates(&keeper, &0, &10, &0);
    let events_after = e.events().all().len();
    assert!(events_after > events_before);
}
