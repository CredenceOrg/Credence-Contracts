#![cfg(test)]

//! # Timelock State Machine Proptests
//!
//! Generates arbitrary sequences of queue/advance-time/execute/cancel actions against a model
//! and asserts core timelock invariants after each step.
//!
//! ## Invariants Enforced:
//! - **ETA never earlier than min delay**: `op.eta >= op.queued_at + min_delay_seconds()`.
//! - **Execute at most once**: An operation cannot be executed more than once.
//! - **Cancel only before execute**: Cancel can only succeed on pending operations; cancelled
//!   operations cannot be executed, and executed operations cannot be cancelled.
//! - **is_operation_executed consistency**: `is_operation_executed(&hash)` is true if and only
//!   if at least one operation with that hash has been executed.

use proptest::prelude::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env};
use std::collections::HashSet;
use timelock::{
    min_delay_seconds, OperationStatus, QueuedOperation, TimelockContract, TimelockContractClient,
    GRACE_PERIOD,
};

/// Actions that can be performed in the timelock state machine.
#[derive(Clone, Debug)]
pub enum TimelockAction {
    /// Queue a new operation with a selected hash index and delay.
    Queue { hash_id: u8, delay: u64 },
    /// Advance the ledger timestamp by a number of seconds.
    AdvanceTime { seconds: u64 },
    /// Attempt to execute an operation by id.
    Execute { target_op: u8 },
    /// Attempt to cancel an operation by id.
    Cancel { target_op: u8 },
}

impl Arbitrary for TimelockAction {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        prop_oneof![
            // Queue action: delays spanning invalid (< 86_400) and valid (>= 86_400) ranges
            (
                0..4u8,
                prop_oneof![
                    Just(0u64),
                    Just(1u64),
                    Just(86_399u64),      // strictly below min_delay
                    Just(86_400u64),      // exact min_delay
                    Just(86_401u64),      // 1 sec above min_delay
                    86_400u64..200_000u64, // normal delay range
                ]
            )
                .prop_map(|(hash_id, delay)| TimelockAction::Queue { hash_id, delay }),

            // AdvanceTime action: combinations of small, boundary, and large jumps
            prop_oneof![
                Just(0u64),
                Just(1u64),
                Just(3600u64),
                Just(86_399u64),
                Just(86_400u64),          // min_delay / grace period
                Just(86_401u64),
                Just(172_800u64),         // min_delay + grace period
                1u64..200_000u64,
            ]
                .prop_map(|seconds| TimelockAction::AdvanceTime { seconds }),

            // Execute action targeting existing or hypothetical op_ids
            (0..8u8).prop_map(|target_op| TimelockAction::Execute { target_op }),

            // Cancel action targeting existing or hypothetical op_ids
            (0..8u8).prop_map(|target_op| TimelockAction::Cancel { target_op }),
        ]
        .boxed()
    }
}

/// Abstract status in the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelOpStatus {
    Pending,
    Executed,
    Cancelled,
}

/// Model representation of a queued operation.
#[derive(Clone, Debug)]
pub struct ModelOperation {
    pub id: u64,
    pub op_hash: [u8; 32],
    pub queued_at: u64,
    pub delay: u64,
    pub eta: u64,
    pub expires_at: u64,
    pub status: ModelOpStatus,
}

/// State machine model tracking expected timelock state.
pub struct TimelockModel {
    pub current_time: u64,
    pub operations: Vec<ModelOperation>,
    pub executed_hashes: HashSet<[u8; 32]>,
}

impl TimelockModel {
    pub fn new(initial_time: u64) -> Self {
        Self {
            current_time: initial_time,
            operations: Vec::new(),
            executed_hashes: HashSet::new(),
        }
    }
}

/// Run an action sequence against both the Soroban timelock contract and the model,
/// validating that behavior matches and all invariants hold at every step.
fn run_state_machine(actions: &[TimelockAction]) {
    let env = Env::default();
    env.mock_all_auths();

    let initial_time = 1_000u64;
    env.ledger().with_mut(|li| li.timestamp = initial_time);

    let contract_id = env.register(TimelockContract, ());
    let client = TimelockContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let mut model = TimelockModel::new(initial_time);

    for (step_idx, action) in actions.iter().enumerate() {
        match action {
            TimelockAction::Queue { hash_id, delay } => {
                let op_hash = [*hash_id; 32];
                let hash_bytes = BytesN::from_array(&env, &op_hash);

                let will_succeed = *delay >= min_delay_seconds()
                    && !model.executed_hashes.contains(&op_hash);

                let res = client.try_queue_operation(&admin, &hash_bytes, delay);
                assert_eq!(
                    res.is_ok(),
                    will_succeed,
                    "[Step {}] Queue success mismatch for hash_id={}, delay={}: expected {}, got {:?}",
                    step_idx, hash_id, delay, will_succeed, res
                );

                if will_succeed {
                    let op_id = res.unwrap().unwrap();
                    assert_eq!(
                        op_id,
                        model.operations.len() as u64,
                        "[Step {}] Operation ID mismatch", step_idx
                    );

                    let eta = model.current_time.checked_add(*delay).unwrap();
                    let expires_at = eta.checked_add(GRACE_PERIOD).unwrap();

                    model.operations.push(ModelOperation {
                        id: op_id,
                        op_hash,
                        queued_at: model.current_time,
                        delay: *delay,
                        eta,
                        expires_at,
                        status: ModelOpStatus::Pending,
                    });
                }
            }

            TimelockAction::AdvanceTime { seconds } => {
                model.current_time = model.current_time.saturating_add(*seconds);
                env.ledger().with_mut(|li| li.timestamp = model.current_time);
            }

            TimelockAction::Execute { target_op } => {
                let op_id = *target_op as u64;
                let op_idx = model.operations.iter().position(|op| op.id == op_id);

                let will_succeed = match op_idx {
                    Some(idx) => {
                        let op = &model.operations[idx];
                        op.status == ModelOpStatus::Pending
                            && model.current_time >= op.eta
                            && model.current_time <= op.expires_at
                            && !model.executed_hashes.contains(&op.op_hash)
                    }
                    None => false,
                };

                let res = client.try_execute_operation(&op_id);
                assert_eq!(
                    res.is_ok(),
                    will_succeed,
                    "[Step {}] Execute success mismatch for op_id={}: expected {}, got {:?}",
                    step_idx, op_id, will_succeed, res
                );

                if will_succeed {
                    let idx = op_idx.unwrap();
                    let op = &mut model.operations[idx];
                    op.status = ModelOpStatus::Executed;
                    model.executed_hashes.insert(op.op_hash);
                }
            }

            TimelockAction::Cancel { target_op } => {
                let op_id = *target_op as u64;
                let op_idx = model.operations.iter().position(|op| op.id == op_id);

                let will_succeed = match op_idx {
                    Some(idx) => model.operations[idx].status == ModelOpStatus::Pending,
                    None => false,
                };

                let res = client.try_cancel_operation(&admin, &op_id);
                assert_eq!(
                    res.is_ok(),
                    will_succeed,
                    "[Step {}] Cancel success mismatch for op_id={}: expected {}, got {:?}",
                    step_idx, op_id, will_succeed, res
                );

                if will_succeed {
                    let idx = op_idx.unwrap();
                    model.operations[idx].status = ModelOpStatus::Cancelled;
                }
            }
        }

        // --- Invariant assertions after each step ---
        assert_invariants(&env, &client, &admin, &model, step_idx, action);
    }
}

/// Asserts the core timelock invariants after each state machine transition.
fn assert_invariants(
    env: &Env,
    client: &TimelockContractClient,
    admin: &Address,
    model: &TimelockModel,
    step: usize,
    action: &TimelockAction,
) {
    // Operation counter invariant: exact match with model operation count
    assert_eq!(
        client.get_operation_count(),
        model.operations.len() as u64,
        "[Step {}] get_operation_count mismatch after {:?}",
        step, action
    );

    for op in &model.operations {
        let chain_op = client
            .get_operation(&op.id)
            .expect("operation present in model must exist in contract");

        // Invariant 1: ETA never earlier than min delay
        assert!(
            chain_op.eta >= op.queued_at + min_delay_seconds(),
            "[Step {}] ETA {} earlier than queued_at {} + min_delay {} for op {}",
            step, chain_op.eta, op.queued_at, min_delay_seconds(), op.id
        );
        assert_eq!(
            chain_op.expires_at,
            chain_op.eta + GRACE_PERIOD,
            "[Step {}] expires_at mismatch for op {}",
            step, op.id
        );
        assert_eq!(chain_op.eta, op.eta);
        assert_eq!(chain_op.expires_at, op.expires_at);

        // Status alignment between model and contract
        let expected_status = match op.status {
            ModelOpStatus::Pending => OperationStatus::Pending,
            ModelOpStatus::Executed => OperationStatus::Executed,
            ModelOpStatus::Cancelled => OperationStatus::Cancelled,
        };
        assert_eq!(
            chain_op.status, expected_status,
            "[Step {}] status mismatch for op {}",
            step, op.id
        );

        // Invariant 2: Execute at most once
        // If already executed, any subsequent execute attempt MUST fail
        if op.status == ModelOpStatus::Executed {
            assert!(
                client.try_execute_operation(&op.id).is_err(),
                "[Step {}] re-executing already executed op {} succeeded",
                step, op.id
            );
        }

        // Invariant 3: Cancel only before execute
        // If executed, cancel MUST fail
        if op.status == ModelOpStatus::Executed {
            assert!(
                client.try_cancel_operation(admin, &op.id).is_err(),
                "[Step {}] cancelling executed op {} succeeded",
                step, op.id
            );
        }
        // If cancelled, execute MUST fail and re-cancelling MUST fail
        if op.status == ModelOpStatus::Cancelled {
            assert!(
                client.try_execute_operation(&op.id).is_err(),
                "[Step {}] executing cancelled op {} succeeded",
                step, op.id
            );
            assert!(
                client.try_cancel_operation(admin, &op.id).is_err(),
                "[Step {}] re-cancelling cancelled op {} succeeded",
                step, op.id
            );
        }
    }

    // Invariant 4: is_operation_executed consistency
    // Checked for every hash in the generated domain [0..4]
    for hash_id in 0..4u8 {
        let hash = [hash_id; 32];
        let hash_bytes = BytesN::from_array(env, &hash);
        let on_chain_executed = client.is_operation_executed(&hash_bytes);
        let model_executed = model.executed_hashes.contains(&hash);

        assert_eq!(
            on_chain_executed, model_executed,
            "[Step {}] is_operation_executed mismatch for hash {:?}: contract={}, model={}",
            step, hash, on_chain_executed, model_executed
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        .. ProptestConfig::default()
    })]

    #[test]
    fn test_timelock_state_machine(
        ref actions in prop::collection::vec(any::<TimelockAction>(), 1..64)
    ) {
        run_state_machine(actions);
    }
}
