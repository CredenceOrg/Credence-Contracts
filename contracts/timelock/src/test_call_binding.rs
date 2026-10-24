//! Tests for call-bound operations: a queued operation is bound to a concrete
//! `(target, func, args)` triple and `execute_operation` invokes it.

use super::*;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

// ── mock target contract ─────────────────────────────────────────────────

#[contract]
pub struct MockTarget;

#[contractimpl]
impl MockTarget {
    /// The timelock contract address is this contract's admin, so the target
    /// authorizes the timelock (the caller) exactly like a real integration.
    pub fn initialize(e: Env, admin: Address) {
        e.storage().instance().set(&Symbol::new(&e, "admin"), &admin);
        e.storage().instance().set(&Symbol::new(&e, "value"), &0_u32);
        e.storage().instance().set(&Symbol::new(&e, "calls"), &0_u32);
    }

    pub fn set_value(e: Env, value: u32) {
        let admin: Address = e.storage().instance().get(&Symbol::new(&e, "admin")).unwrap();
        admin.require_auth();
        e.storage().instance().set(&Symbol::new(&e, "value"), &value);
        let calls: u32 = e.storage().instance().get(&Symbol::new(&e, "calls")).unwrap_or(0);
        e.storage().instance().set(&Symbol::new(&e, "calls"), &(calls + 1));
    }

    pub fn get_value(e: Env) -> u32 {
        e.storage().instance().get(&Symbol::new(&e, "value")).unwrap()
    }

    pub fn call_count(e: Env) -> u32 {
        e.storage().instance().get(&Symbol::new(&e, "calls")).unwrap_or(0)
    }

    pub fn panic_call(_e: Env) {
        panic!("target panicked");
    }
}

fn setup() -> (Env, TimelockContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, client, admin)
}

/// Registers the mock target with the timelock as its admin.
fn setup_with_target() -> (
    Env,
    TimelockContractClient<'static>,
    Address,
    MockTargetClient<'static>,
) {
    let (env, client, admin) = setup();
    let target_id = env.register(MockTarget, ());
    let target = MockTargetClient::new(&env, &target_id);
    target.initialize(&client.address);
    (env, client, admin, target)
}

fn args_u32(env: &Env, value: u32) -> Vec<Val> {
    Vec::from_array(env, [value.into_val(env)])
}

fn empty_args(env: &Env) -> Vec<Val> {
    Vec::new(env)
}

// ── acceptance criteria ──────────────────────────────────────────────────

#[test]
fn call_bound_op_invokes_target() {
    let (env, client, admin, target) = setup_with_target();
    let func = Symbol::new(&env, "set_value");
    let args = args_u32(&env, 7);

    let op_id = client.queue_call(&admin, &target.address, &func, &args, &min_delay_seconds());
    let op = client.get_operation(&op_id).unwrap();

    // The queued call is inspectable and bound to the operation.
    let call = client.get_call(&op_id).unwrap();
    assert_eq!(call.target, target.address);
    assert_eq!(call.func, func);
    assert_eq!(call.args, args);
    assert_eq!(client.hash_call(&call.target, &call.func, &call.args, &call.salt), op.op_hash);

    env.ledger().with_mut(|li| li.timestamp = op.eta);
    client.execute_operation(&op_id);

    assert_eq!(target.get_value(), 7);
    assert_eq!(target.call_count(), 1);
    assert_eq!(
        client.get_operation(&op_id).unwrap().status,
        OperationStatus::Executed
    );
}

#[test]
fn call_bound_op_not_before_eta() {
    let (env, client, admin, target) = setup_with_target();
    let func = Symbol::new(&env, "set_value");
    let args = args_u32(&env, 7);

    let op_id = client.queue_call(&admin, &target.address, &func, &args, &min_delay_seconds());
    let op = client.get_operation(&op_id).unwrap();

    env.ledger().with_mut(|li| li.timestamp = op.eta - 1);
    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err(), "execution before ETA must fail");

    // Target is untouched and the operation is still pending.
    assert_eq!(target.get_value(), 0);
    assert_eq!(target.call_count(), 0);
    assert_eq!(
        client.get_operation(&op_id).unwrap().status,
        OperationStatus::Pending
    );
}

#[test]
fn call_bound_op_executes_once() {
    let (env, client, admin, target) = setup_with_target();
    let func = Symbol::new(&env, "set_value");
    let args = args_u32(&env, 7);

    let op_id = client.queue_call(&admin, &target.address, &func, &args, &min_delay_seconds());
    let op = client.get_operation(&op_id).unwrap();

    env.ledger().with_mut(|li| li.timestamp = op.eta);
    client.execute_operation(&op_id);

    let second = client.try_execute_operation(&op_id);
    assert!(second.is_err(), "an operation may only execute once");

    // The target was invoked exactly once.
    assert_eq!(target.call_count(), 1);
    assert!(client.is_operation_executed(&op.op_hash));
}

#[test]
fn failing_target_reverts_status() {
    let (env, client, admin, target) = setup_with_target();
    let func = Symbol::new(&env, "panic_call");
    let args = empty_args(&env);

    let op_id = client.queue_call(&admin, &target.address, &func, &args, &min_delay_seconds());
    let op = client.get_operation(&op_id).unwrap();

    env.ledger().with_mut(|li| li.timestamp = op.eta);
    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err(), "a panicking target must revert the execution");

    // Effects written before the interaction were rolled back with the call.
    assert_eq!(
        client.get_operation(&op_id).unwrap().status,
        OperationStatus::Pending
    );
    assert!(!client.is_operation_executed(&op.op_hash));
}

#[test]
fn hash_binds_all_fields() {
    let (env, client, _admin, target) = setup_with_target();
    let other_target = Address::generate(&env);
    let func = Symbol::new(&env, "set_value");
    let other_func = Symbol::new(&env, "set_value_v2");
    let salt = BytesN::from_array(&env, &[1; 32]);
    let other_salt = BytesN::from_array(&env, &[2; 32]);
    let args = args_u32(&env, 7);

    let base = client.hash_call(&target.address, &func, &args, &salt);

    // Different target.
    assert_ne!(base, client.hash_call(&other_target, &func, &args, &salt));
    // Different func.
    assert_ne!(
        base,
        client.hash_call(&target.address, &other_func, &args, &salt)
    );
    // Different argument list length.
    assert_ne!(
        base,
        client.hash_call(&target.address, &func, &empty_args(&env), &salt)
    );
    // Different argument value (same length).
    assert_ne!(
        base,
        client.hash_call(&target.address, &func, &args_u32(&env, 8), &salt)
    );
    // Different salt.
    assert_ne!(
        base,
        client.hash_call(&target.address, &func, &args, &other_salt)
    );
    // Same inputs hash identically.
    assert_eq!(
        base,
        client.hash_call(&target.address, &func, &args, &salt)
    );
}

#[test]
fn cancelled_call_never_runs() {
    let (env, client, admin, target) = setup_with_target();
    let func = Symbol::new(&env, "set_value");
    let args = args_u32(&env, 7);

    let op_id = client.queue_call(&admin, &target.address, &func, &args, &min_delay_seconds());
    let op = client.get_operation(&op_id).unwrap();

    client.cancel_operation(&admin, &op_id);

    env.ledger().with_mut(|li| li.timestamp = op.eta);
    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err(), "a cancelled call must never run");

    assert_eq!(target.get_value(), 0);
    assert_eq!(target.call_count(), 0);
    assert!(!client.is_operation_executed(&op.op_hash));
}

// ── supporting behaviour ─────────────────────────────────────────────────

#[test]
fn queue_call_requires_admin() {
    let (env, client, _admin, target) = setup_with_target();
    let stranger = Address::generate(&env);
    let func = Symbol::new(&env, "set_value");
    let args = args_u32(&env, 7);

    let res = client.try_queue_call(&stranger, &target.address, &func, &args, &min_delay_seconds());
    assert!(res.is_err(), "only the admin may queue calls");
}

#[test]
fn queue_call_obeys_minimum_delay() {
    let (env, client, admin, target) = setup_with_target();
    let func = Symbol::new(&env, "set_value");
    let args = args_u32(&env, 7);

    let res = client.try_queue_call(
        &admin,
        &target.address,
        &func,
        &args,
        &(min_delay_seconds() - 1),
    );
    assert!(res.is_err(), "delays below the minimum must be rejected");
}

#[test]
fn hash_only_operation_still_has_no_call() {
    let (env, client, admin, _target) = setup_with_target();
    let op_hash = BytesN::from_array(&env, &[42; 32]);

    let op_id = client.queue_operation(&admin, &op_hash, &min_delay_seconds());
    assert!(client.get_call(&op_id).is_none());

    let op = client.get_operation(&op_id).unwrap();
    env.ledger().with_mut(|li| li.timestamp = op.eta);
    client.execute_operation(&op_id);

    assert_eq!(
        client.get_operation(&op_id).unwrap().status,
        OperationStatus::Executed
    );
    assert!(client.is_operation_executed(&op_hash));
}

#[test]
fn call_bound_execution_emits_target_and_func() {
    use soroban_sdk::testutils::Events as _;

    let (env, client, admin, target) = setup_with_target();
    let func = Symbol::new(&env, "set_value");
    let args = args_u32(&env, 7);

    let op_id = client.queue_call(&admin, &target.address, &func, &args, &min_delay_seconds());
    let op = client.get_operation(&op_id).unwrap();

    env.ledger().with_mut(|li| li.timestamp = op.eta);
    let _ = env.events().all();
    client.execute_operation(&op_id);

    let all = env.events().all();
    assert_eq!(all.len(), 1, "call-bound execute emits exactly one event");
    let ev = all.iter().next().unwrap();
    // Topics: (operation_executed, op_id)
    assert_eq!(ev.1.len(), 2);
    let topics = ev.1;
    let op_id_emitted: u64 = u64::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(op_id_emitted, op_id);
    // Data: (op_hash, target, func)
    let (hash, emitted_target, emitted_func): (BytesN<32>, Address, Symbol) =
        <(BytesN<32>, Address, Symbol)>::try_from_val(&env, &ev.2).unwrap();
    assert_eq!(hash, op.op_hash);
    assert_eq!(emitted_target, target.address);
    assert_eq!(emitted_func, func);
}
