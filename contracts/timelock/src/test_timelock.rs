#![cfg(test)]

use crate::{Timelock, TimelockClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

fn setup(e: &Env) -> (TimelockClient<'_>, Address, Address) {
    let contract_id = e.register(Timelock, ());
    let client = TimelockClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let governance = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin, &governance, &86400);
    (client, admin, governance)
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let e = Env::default();
    let (client, admin, governance) = setup(&e);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_governance(), governance);
    assert_eq!(client.get_min_delay(), 86400);
}

#[test]
#[should_panic(expected = "min_delay must be greater than zero")]
fn test_initialize_zero_delay() {
    let e = Env::default();
    let contract_id = e.register(Timelock, ());
    let client = TimelockClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let governance = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&admin, &governance, &0);
}

// ---------------------------------------------------------------------------
// Proposing changes
// ---------------------------------------------------------------------------

#[test]
fn test_propose_change() {
    let e = Env::default();
    let (client, admin, _gov) = setup(&e);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let key = Symbol::new(&e, "slash_rate");
    let id = client.propose_change(&admin, &key, &500);
    assert_eq!(id, 0);

    let change = client.get_change(&id);
    assert_eq!(change.parameter_key, key);
    assert_eq!(change.new_value, 500);
    assert_eq!(change.proposed_at, 1000);
    assert_eq!(change.eta, 1000 + 86400);
    assert!(!change.executed);
    assert!(!change.cancelled);
}

#[test]
#[should_panic(expected = "only admin can propose changes")]
fn test_propose_non_admin_fails() {
    let e = Env::default();
    let (client, _admin, _gov) = setup(&e);
    let other = Address::generate(&e);
    let key = Symbol::new(&e, "threshold");
    client.propose_change(&other, &key, &10);
}

// ---------------------------------------------------------------------------
// Executing changes
// ---------------------------------------------------------------------------

#[test]
fn test_execute_change_after_delay() {
    let e = Env::default();
    let (client, admin, _gov) = setup(&e);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let key = Symbol::new(&e, "fee_bps");
    let id = client.propose_change(&admin, &key, &250);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000 + 86400;
    });

    client.execute_change(&id);
    let change = client.get_change(&id);
    assert!(change.executed);
}

#[test]
#[should_panic(expected = "timelock delay has not elapsed")]
fn test_execute_before_delay_fails() {
    let e = Env::default();
    let (client, admin, _gov) = setup(&e);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let key = Symbol::new(&e, "fee_bps");
    let id = client.propose_change(&admin, &key, &250);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000 + 86399;
    });

    client.execute_change(&id);
}

#[test]
#[should_panic(expected = "change already executed")]
fn test_execute_already_executed_fails() {
    let e = Env::default();
    let (client, admin, _gov) = setup(&e);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let key = Symbol::new(&e, "fee_bps");
    let id = client.propose_change(&admin, &key, &250);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000 + 86400;
    });

    client.execute_change(&id);
    client.execute_change(&id);
}

#[test]
#[should_panic(expected = "only admin can propose changes")]
fn test_execute_non_admin_fails() {
    let e = Env::default();
    let (client, _admin, _gov) = setup(&e);
    let other = Address::generate(&e);
    let key = Symbol::new(&e, "fee_bps");
    client.propose_change(&other, &key, &250);
}

// ---------------------------------------------------------------------------
// Cancelling changes
// ---------------------------------------------------------------------------

#[test]
fn test_cancel_change_by_governance() {
    let e = Env::default();
    let (client, admin, gov) = setup(&e);

    let key = Symbol::new(&e, "cooldown");
    let id = client.propose_change(&admin, &key, &7200);

    client.cancel_change(&gov, &id);
    let change = client.get_change(&id);
    assert!(change.cancelled);
    assert!(!change.executed);
}

#[test]
#[should_panic(expected = "only governance can cancel changes")]
fn test_cancel_by_non_governance_fails() {
    let e = Env::default();
    let (client, admin, _gov) = setup(&e);
    let other = Address::generate(&e);

    let key = Symbol::new(&e, "cooldown");
    let id = client.propose_change(&admin, &key, &7200);

    client.cancel_change(&other, &id);
}

#[test]
#[should_panic(expected = "change has been cancelled")]
fn test_execute_cancelled_change_fails() {
    let e = Env::default();
    let (client, admin, gov) = setup(&e);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let key = Symbol::new(&e, "penalty");
    let id = client.propose_change(&admin, &key, &300);

    client.cancel_change(&gov, &id);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000 + 86400;
    });

    client.execute_change(&id);
}

#[test]
#[should_panic(expected = "change already cancelled")]
fn test_cancel_already_cancelled_fails() {
    let e = Env::default();
    let (client, admin, gov) = setup(&e);

    let key = Symbol::new(&e, "penalty");
    let id = client.propose_change(&admin, &key, &300);

    client.cancel_change(&gov, &id);
    client.cancel_change(&gov, &id);
}

#[test]
#[should_panic(expected = "change already executed")]
fn test_cancel_executed_change_fails() {
    let e = Env::default();
    let (client, admin, gov) = setup(&e);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let key = Symbol::new(&e, "penalty");
    let id = client.propose_change(&admin, &key, &300);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000 + 86400;
    });

    client.execute_change(&id);
    client.cancel_change(&gov, &id);
}

// ---------------------------------------------------------------------------
// Updating minimum delay
// ---------------------------------------------------------------------------

#[test]
fn test_update_min_delay() {
    let e = Env::default();
    let (client, _admin, _gov) = setup(&e);

    assert_eq!(client.get_min_delay(), 86400);
    client.update_min_delay(&172800);
    assert_eq!(client.get_min_delay(), 172800);
}

#[test]
#[should_panic(expected = "min_delay must be greater than zero")]
fn test_update_min_delay_zero_fails() {
    let e = Env::default();
    let (client, _admin, _gov) = setup(&e);
    client.update_min_delay(&0);
}

// ---------------------------------------------------------------------------
// Multiple pending changes
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_pending_changes() {
    let e = Env::default();
    let (client, admin, _gov) = setup(&e);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let key_a = Symbol::new(&e, "param_a");
    let key_b = Symbol::new(&e, "param_b");

    let id_a = client.propose_change(&admin, &key_a, &100);
    let id_b = client.propose_change(&admin, &key_b, &200);

    assert_ne!(id_a, id_b);

    let change_a = client.get_change(&id_a);
    let change_b = client.get_change(&id_b);
    assert_eq!(change_a.parameter_key, key_a);
    assert_eq!(change_a.new_value, 100);
    assert_eq!(change_b.parameter_key, key_b);
    assert_eq!(change_b.new_value, 200);

    e.ledger().with_mut(|li| {
        li.timestamp = 1000 + 86400;
    });

    client.execute_change(&id_a);
    assert!(client.get_change(&id_a).executed);
    assert!(!client.get_change(&id_b).executed);

    client.execute_change(&id_b);
    assert!(client.get_change(&id_b).executed);
}

// ---------------------------------------------------------------------------
// Query edge cases
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "change not found")]
fn test_get_change_not_found() {
    let e = Env::default();
    let (client, _admin, _gov) = setup(&e);
    let _ = client.get_change(&999);
}
