use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};

fn setup_env() -> (Env, TimelockContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, client, admin)
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
    env.ledger().with_mut(|li| li.timestamp = op_5.expires_at + 1);
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
