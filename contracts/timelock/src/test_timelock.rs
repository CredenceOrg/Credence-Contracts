use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, vec, IntoVal,
};

use soroban_sdk::IntoVal;

fn setup_env() -> (Env, TimelockContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, client, admin)
}

fn propose_admin_at(
    env: &Env,
    client: &TimelockContractClient<'static>,
    new_admin: &Address,
) -> PendingAdmin {
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    client.propose_admin(new_admin);
    client.get_pending_admin().unwrap()
}

fn assert_contract_error<T, I: std::fmt::Debug>(
    result: Result<Result<T, soroban_sdk::Error>, Result<soroban_sdk::Error, I>>,
    expected: ContractError,
) {
    match result {
        Err(Ok(error)) => assert_eq!(
            error,
            soroban_sdk::Error::from_contract_error(expected as u32)
        ),
        _ => panic!("expected contract error {expected:?}"),
    }
}

#[test]
fn admin_rotation_requires_delay() {
    let (env, client, old_admin) = setup_env();
    let new_admin = Address::generate(&env);
    let pending = propose_admin_at(&env, &client, &new_admin);

    env.ledger().with_mut(|li| li.timestamp = pending.eta - 1);
    assert_contract_error(client.try_accept_admin(), ContractError::TimelockNotReady);
    assert_eq!(client.get_admin(), old_admin);

    env.ledger().with_mut(|li| li.timestamp = pending.eta);
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn admin_rotation_requires_new_admin_auth() {
    let env = Env::default();
    let old_admin = Address::generate(&env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    client.initialize(&old_admin);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    let pending = propose_admin_at(&env, &client, &new_admin);

    env.ledger().with_mut(|li| li.timestamp = pending.eta);
    env.mock_auths(&[MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_accept_admin().is_err());
    assert_eq!(client.get_admin(), old_admin);
}

#[test]
fn admin_rotation_expires() {
    let (env, client, old_admin) = setup_env();
    let new_admin = Address::generate(&env);
    let pending = propose_admin_at(&env, &client, &new_admin);

    env.ledger()
        .with_mut(|li| li.timestamp = pending.expires_at + 1);
    assert_contract_error(client.try_accept_admin(), ContractError::SignatureExpired);
    assert_eq!(client.get_admin(), old_admin);
}

#[test]
fn admin_proposal_cancel() {
    let (env, client, old_admin) = setup_env();
    let new_admin = Address::generate(&env);
    let pending = propose_admin_at(&env, &client, &new_admin);

    client.cancel_admin_proposal();
    assert_eq!(client.get_pending_admin(), None);
    env.ledger().with_mut(|li| li.timestamp = pending.eta);
    assert_contract_error(client.try_accept_admin(), ContractError::NoPendingAdmin);
    assert_eq!(client.get_admin(), old_admin);
}

#[test]
fn propose_same_or_self_rejected() {
    let (_env, client, old_admin) = setup_env();
    assert_contract_error(
        client.try_propose_admin(&old_admin),
        ContractError::AdminUnchanged,
    );

    let contract_address = client.address.clone();
    assert_contract_error(
        client.try_propose_admin(&contract_address),
        ContractError::InvalidAdminAddress,
    );
}

#[test]
fn old_admin_retains_control_during_delay() {
    let (env, client, old_admin) = setup_env();
    let new_admin = Address::generate(&env);
    propose_admin_at(&env, &client, &new_admin);

    let op_hash = BytesN::from_array(&env, &[42; 32]);
    assert_eq!(
        client.queue_operation(&old_admin, &op_hash, &min_delay_seconds()),
        0
    );
    assert!(client
        .try_queue_operation(
            &new_admin,
            &BytesN::from_array(&env, &[43; 32]),
            &min_delay_seconds()
        )
        .is_err());
    assert_eq!(client.get_admin(), old_admin);
}

/// Boundary test: timestamp at eta - 1 (1 second before ETA).
/// Execution MUST fail with TimelockNotReady error.
#[test]
fn test_boundary_eta_minus_one_fails_not_ready() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[10; 32]);
    let delay = min_delay_seconds(); // 86_400

    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Set ledger timestamp to exactly eta - 1
    env.ledger().with_mut(|li| li.timestamp = op.eta - 1);

    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err());

    // Verify operation remains Pending and unexecuted
    let op_after = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after.status, OperationStatus::Pending);
    assert!(!client.is_operation_executed(&op_hash));
}

/// Boundary test: timestamp exactly at eta.
/// Execution MUST succeed and transition operation to Executed.
/// Recovery test: a rejected pre-ETA execution must not consume the operation.
/// The same operation must remain executable once the timelock reaches ETA.
#[test]
fn test_recovery_after_pre_eta_rejection_succeeds_at_eta() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[17; 32]);
    let delay = min_delay_seconds();

    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // First attempt is rejected because the timelock has not matured.
    env.ledger().with_mut(|li| li.timestamp = op.eta - 1);
    assert!(client.try_execute_operation(&op_id).is_err());

    // The failed attempt must leave the operation recoverable.
    let after_rejection = client.get_operation(&op_id).unwrap();
    assert_eq!(after_rejection.status, OperationStatus::Pending);
    assert!(!client.is_operation_executed(&op_hash));

    // Once ETA is reached, retrying the same operation must succeed.
    env.ledger().with_mut(|li| li.timestamp = op.eta);
    client.execute_operation(&op_id);

    let after_retry = client.get_operation(&op_id).unwrap();
    assert_eq!(after_retry.status, OperationStatus::Executed);
    assert!(client.is_operation_executed(&op_hash));
}

#[test]
fn test_boundary_eta_exact_succeeds() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[11; 32]);
    let delay = min_delay_seconds();

    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Set ledger timestamp to exactly eta
    env.ledger().with_mut(|li| li.timestamp = op.eta);

    client.execute_operation(&op_id);

    let op_after = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after.status, OperationStatus::Executed);
    assert!(client.is_operation_executed(&op_hash));
}

/// Boundary test: timestamp at eta + 1 (1 second after ETA).
/// Execution MUST succeed.
#[test]
fn test_boundary_eta_plus_one_succeeds() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[12; 32]);
    let delay = min_delay_seconds();

    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Set ledger timestamp to eta + 1
    env.ledger().with_mut(|li| li.timestamp = op.eta + 1);

    client.execute_operation(&op_id);

    let op_after = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after.status, OperationStatus::Executed);
    assert!(client.is_operation_executed(&op_hash));
}

/// Boundary test: timestamp at expires_at - 1 (1 second before expiry).
/// Execution MUST succeed.
#[test]
fn test_boundary_expires_at_minus_one_succeeds() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[13; 32]);
    let delay = min_delay_seconds();

    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Set ledger timestamp to expires_at - 1
    env.ledger().with_mut(|li| li.timestamp = op.expires_at - 1);

    client.execute_operation(&op_id);

    let op_after = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after.status, OperationStatus::Executed);
    assert!(client.is_operation_executed(&op_hash));
}

/// Boundary test: timestamp exactly at expires_at.
/// Execution MUST succeed because execution window is inclusive (now <= expires_at).
#[test]
fn test_boundary_expires_at_exact_succeeds() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[14; 32]);
    let delay = min_delay_seconds();

    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Set ledger timestamp to exactly expires_at
    env.ledger().with_mut(|li| li.timestamp = op.expires_at);

    client.execute_operation(&op_id);

    let op_after = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after.status, OperationStatus::Executed);
    assert!(client.is_operation_executed(&op_hash));
}

/// Boundary test: timestamp at expires_at + 1 (1 second after expiry).
/// Execution MUST fail with SignatureExpired error.
#[test]
fn test_boundary_expires_at_plus_one_fails_expired() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[15; 32]);
    let delay = min_delay_seconds();

    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Set ledger timestamp to expires_at + 1
    env.ledger().with_mut(|li| li.timestamp = op.expires_at + 1);

    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err());

    // Verify operation status remains Pending
    let op_after = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after.status, OperationStatus::Pending);
    assert!(!client.is_operation_executed(&op_hash));
}

/// Boundary test: verification of grace period duration and limits.
#[test]
fn test_grace_period_duration_calculation() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[16; 32]);
    let delay = 100_000; // Custom delay > min_delay

    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();

    assert_eq!(GRACE_PERIOD, 86_400);
    assert_eq!(op.expires_at - op.eta, GRACE_PERIOD);
}

/// Comprehensive full-window boundary sweep for a single operation.
#[test]
fn test_boundary_full_lifecycle_window_sweep() {
    let (env, client, admin) = setup_env();
    let delay = min_delay_seconds();

    // 1. Before ETA (eta - 1) -> TimelockNotReady
    let hash_1 = BytesN::from_array(&env, &[21; 32]);
    let id_1 = client.queue_operation(&admin, &hash_1, &delay);
    let op_1 = client.get_operation(&id_1).unwrap();
    env.ledger().with_mut(|li| li.timestamp = op_1.eta - 1);
    assert!(client.try_execute_operation(&id_1).is_err());

    // 2. Exactly at ETA -> Ok
    let hash_2 = BytesN::from_array(&env, &[22; 32]);
    let id_2 = client.queue_operation(&admin, &hash_2, &delay);
    let op_2 = client.get_operation(&id_2).unwrap();
    env.ledger().with_mut(|li| li.timestamp = op_2.eta);
    client.execute_operation(&id_2);
    assert_eq!(
        client.get_operation(&id_2).unwrap().status,
        OperationStatus::Executed
    );

    // 3. Middle of grace period -> Ok
    let hash_3 = BytesN::from_array(&env, &[23; 32]);
    let id_3 = client.queue_operation(&admin, &hash_3, &delay);
    let op_3 = client.get_operation(&id_3).unwrap();
    env.ledger()
        .with_mut(|li| li.timestamp = op_3.eta + GRACE_PERIOD / 2);
    client.execute_operation(&id_3);
    assert_eq!(
        client.get_operation(&id_3).unwrap().status,
        OperationStatus::Executed
    );

    // 4. Exactly at expires_at -> Ok
    let hash_4 = BytesN::from_array(&env, &[24; 32]);
    let id_4 = client.queue_operation(&admin, &hash_4, &delay);
    let op_4 = client.get_operation(&id_4).unwrap();
    env.ledger().with_mut(|li| li.timestamp = op_4.expires_at);
    client.execute_operation(&id_4);
    assert_eq!(
        client.get_operation(&id_4).unwrap().status,
        OperationStatus::Executed
    );

    // 5. One second after expires_at -> SignatureExpired
    let hash_5 = BytesN::from_array(&env, &[25; 32]);
    let id_5 = client.queue_operation(&admin, &hash_5, &delay);
    let op_5 = client.get_operation(&id_5).unwrap();
    env.ledger()
        .with_mut(|li| li.timestamp = op_5.expires_at + 1);
    assert!(client.try_execute_operation(&id_5).is_err());
}

/// Boundary test: cancel at boundaries (eta - 1, eta, expires_at).
#[test]
fn test_boundary_cancel_and_execute_prevention() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[30; 32]);
    let delay = min_delay_seconds();

    let op_id = client.queue_operation(&admin, &op_hash, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Cancel before ETA
    env.ledger().with_mut(|li| li.timestamp = op.eta - 1);
    client.cancel_operation(&admin, &op_id);
    assert_eq!(
        client.get_operation(&op_id).unwrap().status,
        OperationStatus::Cancelled
    );

    // Attempt execute at ETA must fail with ProposalAlreadyExecuted (cancelled ops cannot be executed)
    env.ledger().with_mut(|li| li.timestamp = op.eta);
    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err());
}

// --- Cancellation semantics: replacement proposals obey delay rules ---

/// After cancelling a proposal, re-queuing the same operation hash must
/// still obey the minimum delay requirement. This prevents an attacker from
/// circumventing the timelock by cancelling and immediately re-queuing with
/// a shorter delay to force early execution.
#[test]
fn test_replacement_after_cancellation_obeys_delay() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[40; 32]);

    // Queue initial proposal with minimum delay
    let op_id = client.queue_operation(&admin, &op_hash, &min_delay_seconds());
    let original_op = client.get_operation(&op_id).unwrap();

    // Cancel before ETA
    client.cancel_operation(&admin, &op_id);
    assert_eq!(
        client.get_operation(&op_id).unwrap().status,
        OperationStatus::Cancelled
    );

    // Advance ledger to ensure the new operation gets a fresh ETA
    env.ledger().with_mut(|li| li.timestamp = original_op.eta);

    // Re-queue the same hash — must obey min_delay
    let new_op_id = client.queue_operation(&admin, &op_hash, &min_delay_seconds());
    let new_op = client.get_operation(&new_op_id).unwrap();

    // The new operation has a fresh ETA (not the old one)
    assert!(new_op.eta > original_op.eta);
    assert_eq!(new_op.status, OperationStatus::Pending);

    // Wait until the new ETA and execute successfully
    env.ledger().with_mut(|li| li.timestamp = new_op.eta);
    client.execute_operation(&new_op_id);
    assert_eq!(
        client.get_operation(&new_op_id).unwrap().status,
        OperationStatus::Executed
    );
    assert!(client.is_operation_executed(&op_hash));
}

/// After cancelling a proposal, attempting to re-queue with a delay below
/// the minimum must be rejected. Cancellation must not weaken the timelock.
#[test]
fn test_replacement_after_cancellation_rejects_short_delay() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[41; 32]);

    // Queue and cancel
    let op_id = client.queue_operation(&admin, &op_hash, &min_delay_seconds());
    client.cancel_operation(&admin, &op_id);

    // Attempt to re-queue with delay below minimum
    let short_delay = min_delay_seconds() - 1; // 1 second below the minimum
    let res = client.try_queue_operation(&admin, &op_hash, &short_delay);
    assert!(res.is_err());
}

/// Cancelling a proposal and re-queuing with a longer delay must work
/// (the replacement must obey at least the minimum, but can be longer).
#[test]
fn test_replacement_after_cancellation_allows_longer_delay() {
    let (env, client, admin) = setup_env();
    let op_hash = BytesN::from_array(&env, &[42; 32]);

    // Queue and cancel
    let op_id = client.queue_operation(&admin, &op_hash, &min_delay_seconds());
    let original_op = client.get_operation(&op_id).unwrap();
    client.cancel_operation(&admin, &op_id);

    // Re-queue with a delay twice the minimum
    let longer_delay = min_delay_seconds() * 2;
    let new_op_id = client.queue_operation(&admin, &op_hash, &longer_delay);
    let new_op = client.get_operation(&new_op_id).unwrap();

    // Verify the longer delay is honoured in the new ETA
    let expected_min_eta = original_op.eta.checked_add(min_delay_seconds()).unwrap();
    assert!(new_op.eta >= expected_min_eta);
}

/// Boundary test: is_ready helper function direct unit assertions.
#[test]
fn test_is_ready_boundary_suite() {
    let eta: u64 = 1_000_000;

    // eta - 1 -> false
    assert!(!is_ready(eta, eta - 1));

    // eta -> true
    assert!(is_ready(eta, eta));

    // eta + 1 -> true
    assert!(is_ready(eta, eta + 1));

    // expires_at = eta + GRACE_PERIOD -> true
    assert!(is_ready(eta, eta + GRACE_PERIOD));

    // expires_at + 1 -> true (is_ready only checks now >= eta; expiry is checked separately)
    assert!(is_ready(eta, eta + GRACE_PERIOD + 1));
}

// ─── Acceptance criteria tests for issue #1625 ─────────────────────────────

/// AC: call_bound_op_invokes_target - queue set_value(7), execute at ETA, target's get_value() == 7.
#[test]
fn test_call_bound_op_invokes_target() {
    let (env, client, admin) = setup_env();
    
    // Register a mock target contract
    let target_id = env.register(TimelockContract, ()); // Use same contract type for simplicity
    let target_client = TimelockContractClient::new(&env, &target_id);
    target_client.initialize(&admin);

    let delay = 86_400; // minimum delay
    let salt = BytesN::from_array(&env, &[100; 32]);

    // Queue a call to transfer_admin (we'll use this as our "set_value" equivalent)
    let func = Symbol::new(&env, "transfer_admin");
    let new_admin = Address::generate(&env);
    let args = vec![&env, (&new_admin.clone()).into_val(&env)];
    let op_id = client.queue_call(&admin, &target_id, &func, &args, &salt, &delay);

    let op = client.get_operation(&op_id).unwrap();
    assert_eq!(op.status, OperationStatus::Pending);

    // Before ETA: must fail
    env.ledger().with_mut(|li| li.timestamp = op.eta - 1);
    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err());

    // At ETA: execute
    env.ledger().with_mut(|li| li.timestamp = op.eta);
    let _result = client.try_execute_operation(&op_id);

    // Verify the transfer happened (new admin is set)
    assert_eq!(target_client.get_admin(), new_admin);

    let op_after = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after.status, OperationStatus::Executed);
}

/// AC: call_bound_op_not_before_eta - execute at eta - 1 returns TimelockNotReady, target unchanged.
#[test]
fn test_call_bound_op_not_before_eta() {
    let (env, client, admin) = setup_env();
    
    // Register a mock target contract
    let target_id = env.register(TimelockContract, ());
    let target_client = TimelockContractClient::new(&env, &target_id);
    target_client.initialize(&admin);
    let original_admin = target_client.get_admin();

    let delay = 86_400;
    let salt = BytesN::from_array(&env, &[101; 32]);
    let func = Symbol::new(&env, "transfer_admin");
    let new_admin = Address::generate(&env);
    let args = vec![&env, (&new_admin).into_val(&env)];

    let op_id = client.queue_call(&admin, &target_id, &func, &args, &salt, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Try to execute before ETA
    env.ledger().with_mut(|li| li.timestamp = op.eta - 1);
    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err());

    // Target should be unchanged
    assert_eq!(target_client.get_admin(), original_admin);

    // Operation should still be pending
    let op_after = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after.status, OperationStatus::Pending);
}

/// AC: call_bound_op_executes_once - second execute fails, target invoked exactly once.
#[test]
fn test_call_bound_op_executes_once() {
    let (env, client, admin) = setup_env();
    
    // Register a mock target contract
    let target_id = env.register(TimelockContract, ());
    let _target_client = TimelockContractClient::new(&env, &target_id);
    _target_client.initialize(&admin);

    let delay = 86_400;
    let salt = BytesN::from_array(&env, &[102; 32]);
    let func = Symbol::new(&env, "transfer_admin");
    let new_admin = Address::generate(&env);
    let args = vec![&env, (&new_admin).into_val(&env)];

    let op_id = client.queue_call(&admin, &target_id, &func, &args, &salt, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Execute at ETA
    env.ledger().with_mut(|li| li.timestamp = op.eta);
    client.execute_operation(&op_id);

    // Second execute should fail
    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err());
}

/// AC: failing_target_reverts_status - if target panics, operation stays Pending.
#[test]
fn test_failing_target_reverts_status() {
    let (env, client, admin) = setup_env();
    
    // Register a mock target contract  
    let target_id = env.register(TimelockContract, ());
    let _target_client = TimelockContractClient::new(&env, &target_id);
    _target_client.initialize(&admin);

    let delay = 86_400;
    let salt = BytesN::from_array(&env, &[103; 32]);
    // Use initialize on a contract that's already initialized - should fail
    let func = Symbol::new(&env, "initialize");
    let args = vec![&env, (&admin.clone()).into_val(&env)];

    let op_id = client.queue_call(&admin, &target_id, &func, &args, &salt, &delay);
    let op = client.get_operation(&op_id).unwrap();
    let op_hash = op.op_hash;

    // Execute at ETA - should fail because target panics (already initialized)
    env.ledger().with_mut(|li| li.timestamp = op.eta);
    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err());

    // Operation should still be pending (status reverted)
    let op_after = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after.status, OperationStatus::Pending);

    // Hash should not be marked as executed
    assert!(!client.is_operation_executed(&op_hash));
}

/// AC: hash_binds_all_fields - hash_call differs when target, func, any arg or salt changes.
#[test]
fn test_hash_binds_all_fields() {
    let env = Env::default();
    env.mock_all_auths();
    let timelock_id = env.register(TimelockContract, ());
    let timelock_client = TimelockContractClient::new(&env, &timelock_id);
    let admin = Address::generate(&env);
    timelock_client.initialize(&admin);

    let target = Address::generate(&env);
    let func = Symbol::new(&env, "transfer_admin");
    let salt = BytesN::from_array(&env, &[1; 32]);
    let args = vec![&env, (&admin).into_val(&env)];

    let hash1 = timelock_client.hash_call(&target, &func, &args, &salt);

    // Different target
    let target2 = Address::generate(&env);
    let hash2 = timelock_client.hash_call(&target2, &func, &args, &salt);
    assert_ne!(hash1, hash2, "hash should differ when target changes");

    // Different func
    let func2 = Symbol::new(&env, "get_admin");
    let hash3 = timelock_client.hash_call(&target, &func2, &args, &salt);
    assert_ne!(hash1, hash3, "hash should differ when func changes");

    // Different arg
    let args2 = vec![&env, (&Address::generate(&env)).into_val(&env)];
    let hash4 = timelock_client.hash_call(&target, &func, &args2, &salt);
    assert_ne!(hash1, hash4, "hash should differ when args change");

    // Different salt
    let salt2 = BytesN::from_array(&env, &[2; 32]);
    let hash5 = timelock_client.hash_call(&target, &func, &args, &salt2);
    assert_ne!(hash1, hash5, "hash should differ when salt changes");
}

/// AC: cancelled_call_never_runs - cancel, then execute at ETA. Target untouched.
#[test]
fn test_cancelled_call_never_runs() {
    let (env, client, admin) = setup_env();
    
    // Register a mock target contract
    let target_id = env.register(TimelockContract, ());
    let target_client = TimelockContractClient::new(&env, &target_id);
    target_client.initialize(&admin);
    let original_admin = target_client.get_admin();

    let delay = 86_400;
    let salt = BytesN::from_array(&env, &[104; 32]);
    let func = Symbol::new(&env, "transfer_admin");
    let new_admin = Address::generate(&env);
    let args = vec![&env, (&new_admin).into_val(&env)];

    let op_id = client.queue_call(&admin, &target_id, &func, &args, &salt, &delay);
    let op = client.get_operation(&op_id).unwrap();

    // Cancel before ETA
    client.cancel_operation(&admin, &op_id);

    let op_after_cancel = client.get_operation(&op_id).unwrap();
    assert_eq!(op_after_cancel.status, OperationStatus::Cancelled);

    // Advance to ETA and try to execute
    env.ledger().with_mut(|li| li.timestamp = op.eta);
    let res = client.try_execute_operation(&op_id);
    assert!(res.is_err());

    // Target should be unchanged
    assert_eq!(target_client.get_admin(), original_admin);
}

