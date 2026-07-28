//! Event schema regression tests for the Timelock contract.
//!
//! These tests pin the expected event field counts and ordering so that
//! indexers can detect breaking changes immediately. Any change to event
//! topic or data layout must be accompanied by a version bump in the
//! corresponding emit function.
//!
//! SDK 22 events API:
//!   `env.events().all()` → `Vec<(Address, Vec<Val>, Val)>`
//!   tuple fields: (contract_id, topics, data)

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    Address, BytesN, Env, Symbol, TryFromVal,
};

// ── helpers ──────────────────────────────────────────────────────────────

/// Decode a `Val` into `T` or panic with a helpful message.
macro_rules! decode {
    ($env:expr, $val:expr, $ty:ty, $msg:literal) => {
        <$ty>::try_from_val($env, &$val).expect($msg)
    };
}

/// Retrieve the single event emitted during the test.
fn only_event(env: &Env) -> (soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val) {
    let all = env.events().all();
    assert_eq!(all.len(), 1, "expected exactly one event");
    let ev = all.iter().next().unwrap();
    (ev.1, ev.2)
}

// ── operation_queued ─────────────────────────────────────────────────────

#[test]
fn operation_queued_event_schema_matches() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(TimelockContract, ());
    let client = TimelockContractClient::new(&e, &contract_id);
    client.initialize(&admin);

    let op_hash = BytesN::from_array(&e, &[1; 32]);
    let delay = min_delay_seconds();

    // Drain init events
    let _ = e.events().all();

    client.queue_operation(&admin, &op_hash, &delay);

    let (topics, data) = only_event(&e);

    // Topics: operation_queued (Symbol), op_id (u64) = 2
    assert_eq!(topics.len(), 2, "operation_queued must have 2 topics");
    let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "topic[0] Symbol");
    assert_eq!(t0, Symbol::new(&e, "operation_queued"));
    let _t1 = decode!(&e, topics.get(1).unwrap(), u64, "topic[1] u64");

    // Data: proposer (Address), op_hash (BytesN<32>), eta (u64), expires_at (u64) = 4 tuple
    let (_proposer, _op_hash_val, _eta, _expires_at): (Address, BytesN<32>, u64, u64) =
        decode!(&e, data, (Address, BytesN<32>, u64, u64), "data 4-tuple");
}

// ── operation_executed ───────────────────────────────────────────────────

#[test]
fn operation_executed_event_schema_matches() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(TimelockContract, ());
    let client = TimelockContractClient::new(&e, &contract_id);
    client.initialize(&admin);

    let op_hash = BytesN::from_array(&e, &[2; 32]);
    let delay = min_delay_seconds();
    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();
    e.ledger().with_mut(|li| li.timestamp = op.eta);

    let _ = e.events().all();

    client.execute_operation(&op_id);

    let (topics, data) = only_event(&e);

    // Topics: operation_executed (Symbol), op_id (u64) = 2
    assert_eq!(topics.len(), 2, "operation_executed must have 2 topics");
    let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "topic[0] Symbol");
    assert_eq!(t0, Symbol::new(&e, "operation_executed"));
    let _t1 = decode!(&e, topics.get(1).unwrap(), u64, "topic[1] u64");

    // Data: op_hash (BytesN<32>) = 1
    let _hash: BytesN<32> = decode!(&e, data, BytesN<32>, "data BytesN<32>");
}

// ── operation_cancelled ──────────────────────────────────────────────────

#[test]
fn operation_cancelled_event_schema_matches() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(TimelockContract, ());
    let client = TimelockContractClient::new(&e, &contract_id);
    client.initialize(&admin);

    let op_hash = BytesN::from_array(&e, &[3; 32]);
    let delay = min_delay_seconds();
    let op_id = client.queue_operation(&admin, &op_hash, &delay);

    let _ = e.events().all();

    client.cancel_operation(&admin, &op_id);

    let (topics, data) = only_event(&e);

    // Topics: operation_cancelled (Symbol), op_id (u64) = 2
    assert_eq!(topics.len(), 2, "operation_cancelled must have 2 topics");
    let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "topic[0] Symbol");
    assert_eq!(t0, Symbol::new(&e, "operation_cancelled"));
    let _t1 = decode!(&e, topics.get(1).unwrap(), u64, "topic[1] u64");

    // Data: op_hash (BytesN<32>) = 1
    let _hash: BytesN<32> = decode!(&e, data, BytesN<32>, "data BytesN<32>");
}

// ── admin_transferred ────────────────────────────────────────────────────

#[test]
fn admin_transferred_event_schema_matches() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(TimelockContract, ());
    let client = TimelockContractClient::new(&e, &contract_id);
    client.initialize(&admin);

    let new_admin = Address::generate(&e);

    let _ = e.events().all();

    client.transfer_admin(&new_admin);

    let (topics, data) = only_event(&e);

    // Topics: admin_transferred (Symbol) = 1
    assert_eq!(topics.len(), 1, "admin_transferred must have 1 topic");
    let t0 = decode!(&e, topics.get(0).unwrap(), Symbol, "topic[0] Symbol");
    assert_eq!(t0, Symbol::new(&e, "admin_transferred"));

    // Data: current_admin (Address), new_admin (Address) = 2
    let (_current, _new): (Address, Address) =
        decode!(&e, data, (Address, Address), "data (Address, Address)");
}

// ── schema-change detection ──────────────────────────────────────────────

/// Verify that event counts are predictable — one event per operation.
#[test]
fn schema_event_count_is_predictable() {
    let e = Env::default();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let contract_id = e.register(TimelockContract, ());
    let client = TimelockContractClient::new(&e, &contract_id);
    client.initialize(&admin);

    let op_hash = BytesN::from_array(&e, &[99; 32]);
    let delay = min_delay_seconds();

    // Queue emits exactly 1 event
    let _ = e.events().all();
    client.queue_operation(&admin, &op_hash, &delay);
    let after_queue = e.events().all();
    assert_eq!(after_queue.len(), 1, "queue_operation must emit exactly 1 event");
}
