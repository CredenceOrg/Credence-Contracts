use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, BytesN, Env, IntoVal,
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

/// Boundary test: timestamp exactly at eta.
/// Execution MUST succeed and transition operation to Executed.
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
}
