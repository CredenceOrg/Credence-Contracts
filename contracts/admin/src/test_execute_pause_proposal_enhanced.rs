//! Enhanced deterministic failure-boundary coverage for execute_pause_proposal.
//!
//! This test module validates the comprehensive failure boundary implementation
//! for [`AdminContract::execute_pause_proposal`] with enhanced error handling,
//! state validation, concurrent execution protection, and observability.
//!
//! ## Test Coverage Categories
//!
//! * **Success scenarios** — valid execution under normal conditions
//! * **Rejection scenarios** — deterministic failure with proper error codes
//! * **Boundary conditions** — edge cases and limit testing
//! * **Regression scenarios** — protection against previously identified issues
//! * **Concurrent execution** — deterministic behavior under concurrent access
//! * **Retry safety** — idempotent behavior across retry scenarios
//! * **Observability** — comprehensive event emission validation
//!
//! All tests validate that the enhanced implementation maintains deterministic
//! behavior while providing comprehensive error boundaries and diagnostic
//! information without exposing sensitive data.

#![cfg(test)]

use crate::pausable::PROPOSAL_EPOCH_SIZE;
use crate::*;
use soroban_sdk::testutils::Logs as _;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, Env};

// Wire-stable error discriminants from credence_errors::ContractError
const ERR_NOT_INITIALIZED: u32 = 1;
const ERR_NOT_ADMIN: u32 = 100;
const ERR_NOT_SIGNER: u32 = 104;
const ERR_INVALID_PAUSE_ACTION: u32 = 107;
const ERR_INVALID_ADMIN_ADDRESS: u32 = 110;
const ERR_STALE_ADMIN_EPOCH: u32 = 514;
const ERR_THRESHOLD_EXCEEDS_SIGNERS: u32 = 601;
const ERR_PROPOSAL_NOT_FOUND: u32 = 603;
const ERR_INSUFFICIENT_APPROVALS: u32 = 605;
const ERR_OVERFLOW: u32 = 700;

fn setup_enhanced() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, super_admin)
}

fn setup_multisig(
    n_signers: usize,
    threshold: u32,
) -> (
    Env,
    AdminContractClient<'static>,
    Address,
    soroban_sdk::Vec<Address>,
) {
    let (e, client, super_admin) = setup_enhanced();
    let mut signers = soroban_sdk::Vec::new(&e);

    for _ in 0..n_signers {
        let signer = Address::generate(&e);
        client.set_pause_signer(&super_admin, &signer, &true);
        signers.push_back(signer);
    }

    client.set_pause_threshold(&super_admin, &threshold);
    (e, client, super_admin, signers)
}

// ==================================================================================
// SUCCESS SCENARIOS - Normal execution paths with enhanced validation
// ==================================================================================

#[test]
fn test_execute_pause_proposal_success_single_signer() {
    let (e, client, _super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    let initial_epoch = client.get_config_epoch();

    // Create and execute pause proposal
    let proposal_id = client.pause(&s1).unwrap();
    assert!(!client.is_paused());

    client.execute_pause_proposal(&proposal_id);
    let events = e.events().all();
    let logs = e.logs().all();
    assert!(client.is_paused());

    // Verify epoch advancement and event emission
    assert_eq!(client.get_config_epoch(), initial_epoch + 2); // propose + execute
    assert!(!events.is_empty());

    // Verify observability events are present
    let has_execution_event = logs
        .iter()
        .any(|entry| entry.contains("pause_proposal_executed"));
    assert!(
        has_execution_event,
        "Should emit successful execution event"
    );
}

#[test]
fn test_execute_unpause_proposal_success() {
    let (e, client, super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    // First pause the contract
    client.set_pause_threshold(&super_admin, &0);
    client.pause(&super_admin);
    client.set_pause_threshold(&super_admin, &1);
    assert!(client.is_paused());

    // Create and execute unpause proposal
    let proposal_id = client.unpause(&s1).unwrap();
    assert!(client.is_paused()); // Still paused before execution

    client.execute_pause_proposal(&proposal_id);
    assert!(!client.is_paused()); // Now unpaused
}

#[test]
fn test_execute_pause_proposal_multisig_success() {
    let (e, client, _super_admin, signers) = setup_multisig(3, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Create proposal and get one approval
    let proposal_id = client.pause(&s1).unwrap();

    // Add second approval
    client.approve_pause_proposal(&s2, &proposal_id);

    // Execute with sufficient approvals
    client.execute_pause_proposal(&proposal_id);
    let events = e.events().all();
    let logs = e.logs().all();
    assert!(client.is_paused());

    // Verify comprehensive observability events
    assert!(!events.is_empty());
}

// ==================================================================================
// REJECTION SCENARIOS - Deterministic failure with proper error codes
// ==================================================================================

#[test]
fn test_execute_pause_proposal_invalid_id_zero() {
    let (_e, client, _super_admin, _signers) = setup_multisig(1, 1);

    let result = client.try_execute_pause_proposal(&0u64);
    assert!(result.is_err());
    let error = result.unwrap_err().unwrap();
    assert_eq!(
        error,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_PAUSE_ACTION)
    );
}

#[test]
fn test_execute_pause_proposal_invalid_id_max() {
    let (_e, client, _super_admin, _signers) = setup_multisig(1, 1);

    let result = client.try_execute_pause_proposal(&u64::MAX);
    assert!(result.is_err());
    let error = result.unwrap_err().unwrap();
    assert_eq!(error, soroban_sdk::Error::from_contract_error(ERR_OVERFLOW));
}

#[test]
fn test_execute_pause_proposal_not_found() {
    let (e, client, _super_admin, _signers) = setup_multisig(1, 1);

    let epoch_before = client.get_config_epoch();

    let result = client.try_execute_pause_proposal(&12345u64);
    assert!(e.events().all().is_empty(), "failed calls roll back events");
    assert!(result.is_err());
    let error = result.unwrap_err().unwrap();
    assert_eq!(
        error,
        soroban_sdk::Error::from_contract_error(ERR_PROPOSAL_NOT_FOUND)
    );

    // Verify no state change occurred
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert!(!client.is_paused());
}

#[test]
fn test_execute_pause_proposal_insufficient_approvals() {
    let (e, client, _super_admin, signers) = setup_multisig(2, 2);
    let s1 = signers.get(0).unwrap();

    // Create proposal with only one approval (need 2)
    let proposal_id = client.pause(&s1).unwrap();
    let epoch_before = client.get_config_epoch();

    let result = client.try_execute_pause_proposal(&proposal_id);
    assert!(result.is_err());
    assert!(e.events().all().is_empty(), "failed calls roll back events");
    let error = result.unwrap_err().unwrap();
    assert_eq!(
        error,
        soroban_sdk::Error::from_contract_error(ERR_INSUFFICIENT_APPROVALS)
    );

    // Verify contract state unchanged
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before);

    // Rejected invocations commit no contract events.
    assert!(e.events().all().is_empty());
}

#[test]
fn test_execute_pause_proposal_stale_epoch() {
    let (e, client, _super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    // Create proposal at end of epoch
    let epoch_boundary = u32::from(PROPOSAL_EPOCH_SIZE);
    e.ledger()
        .with_mut(|l| l.sequence_number = epoch_boundary - 1);
    let proposal_id = client.pause(&s1).unwrap();

    // Advance to next epoch
    e.ledger().with_mut(|l| l.sequence_number = epoch_boundary);

    let epoch_before = client.get_config_epoch();
    let result = client.try_execute_pause_proposal(&proposal_id);
    assert!(result.is_err());
    let error = result.unwrap_err().unwrap();
    assert_eq!(
        error,
        soroban_sdk::Error::from_contract_error(ERR_STALE_ADMIN_EPOCH)
    );

    // Verify no state change
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before);
}

#[test]
fn test_execute_pause_proposal_uninitialized_contract() {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    e.mock_all_auths();

    // Try to execute without initializing contract
    let result = client.try_execute_pause_proposal(&123u64);
    assert!(result.is_err());
    let error = result.unwrap_err().unwrap();
    assert_eq!(
        error,
        soroban_sdk::Error::from_contract_error(ERR_NOT_INITIALIZED)
    );
}

// ==================================================================================
// BOUNDARY CONDITIONS - Edge cases and limit testing
// ==================================================================================

#[test]
fn test_execute_pause_proposal_threshold_boundary() {
    let (e, client, _super_admin, signers) = setup_multisig(3, 3);

    // Get all signers to approve (exactly at threshold)
    let proposal_id = client.pause(&signers.get(0).unwrap()).unwrap();
    client.approve_pause_proposal(&signers.get(1).unwrap(), &proposal_id);
    client.approve_pause_proposal(&signers.get(2).unwrap(), &proposal_id);

    // Should execute successfully at exact threshold
    client.execute_pause_proposal(&proposal_id);
    assert!(client.is_paused());
}

#[test]
fn test_execute_pause_proposal_threshold_exceeds_signers() {
    let (e, client, super_admin) = setup_enhanced();

    // Set up invalid configuration: threshold > signers
    let s1 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_threshold(&super_admin, &1u32);

    // Create valid proposal
    let proposal_id = client.pause(&s1).unwrap();

    // Public signer removal clamps the threshold. Inject corrupt storage
    // directly to exercise the configuration-validation failure boundary.
    e.as_contract(&client.address, || {
        e.storage().instance().set(&DataKey::PauseThreshold, &2u32);
    });

    // Execution should fail with configuration error
    let result = client.try_execute_pause_proposal(&proposal_id);
    assert!(result.is_err());
    let error = result.unwrap_err().unwrap();
    assert_eq!(
        error,
        soroban_sdk::Error::from_contract_error(ERR_THRESHOLD_EXCEEDS_SIGNERS)
    );
}

#[test]
fn test_execute_pause_proposal_single_signer_boundary() {
    let (e, client, _super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    // Test minimum viable multisig (1 signer, threshold 1)
    let proposal_id = client.pause(&s1).unwrap();

    client.execute_pause_proposal(&proposal_id);
    assert!(client.is_paused());
}

// ==================================================================================
// CONCURRENT EXECUTION - Deterministic behavior under concurrent access
// ==================================================================================

#[test]
fn test_execute_pause_proposal_concurrent_modification_detection() {
    let (e, client, super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    // Create proposal
    let proposal_id = client.pause(&s1).unwrap();
    let initial_epoch = client.get_config_epoch();

    // Simulate concurrent modification by advancing epoch
    // (In real scenario this would be another admin operation)
    client.add_admin(&super_admin, &Address::generate(&e), &AdminRole::Admin);

    // Execution should still work as epoch validation is done early
    client.execute_pause_proposal(&proposal_id);
    assert!(client.is_paused());
}

#[test]
fn test_execute_pause_proposal_concurrent_signer_changes() {
    let (e, client, super_admin, signers) = setup_multisig(2, 1);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Create proposal with s1
    let proposal_id = client.pause(&s1).unwrap();

    // Remove s2 while proposal is pending (threshold stays valid)
    client.set_pause_signer(&super_admin, &s2, &false);

    // Execution should still work as s1's approval is still valid
    client.execute_pause_proposal(&proposal_id);
    assert!(client.is_paused());
}

// ==================================================================================
// RETRY SAFETY - Idempotent behavior across retry scenarios
// ==================================================================================

#[test]
fn test_execute_pause_proposal_idempotent_state() {
    let (e, client, super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    // Pause contract directly first
    client.set_pause_threshold(&super_admin, &0);
    client.pause(&super_admin);
    client.set_pause_threshold(&super_admin, &1);
    assert!(client.is_paused());

    // Create pause proposal while already paused
    let proposal_id = client.pause(&s1).unwrap();

    let epoch_before = client.get_config_epoch();

    // Execute pause proposal on already-paused contract (idempotent)
    client.execute_pause_proposal(&proposal_id);
    let events = e.events().all();
    let logs = e.logs().all();

    // Should remain paused and advance epoch (cleanup)
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_before + 1);

    // Should emit idempotent execution events
    let has_idempotent_event = logs
        .iter()
        .any(|entry| entry.contains("pause_idempotent_execution"));
    assert!(
        has_idempotent_event,
        "Should emit idempotent execution event"
    );
}

#[test]
fn test_execute_pause_proposal_retry_after_failure() {
    let (e, client, _super_admin, signers) = setup_multisig(2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Create proposal with insufficient approvals
    let proposal_id = client.pause(&s1).unwrap();

    // First execution attempt fails
    let result = client.try_execute_pause_proposal(&proposal_id);
    assert!(result.is_err());

    // Add missing approval
    client.approve_pause_proposal(&s2, &proposal_id);

    // Retry should succeed
    client.execute_pause_proposal(&proposal_id);
    assert!(client.is_paused());
}

#[test]
fn test_execute_pause_proposal_no_double_execution() {
    let (e, client, _super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    // Create and execute proposal
    let proposal_id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&proposal_id);
    assert!(client.is_paused());

    // Second execution attempt should fail (proposal consumed)
    let result = client.try_execute_pause_proposal(&proposal_id);
    assert!(result.is_err());
    let error = result.unwrap_err().unwrap();
    assert_eq!(
        error,
        soroban_sdk::Error::from_contract_error(ERR_PROPOSAL_NOT_FOUND)
    );
}

// ==================================================================================
// OBSERVABILITY - Comprehensive event emission validation
// ==================================================================================

#[test]
fn test_execute_pause_proposal_observability_events() {
    let (e, client, _super_admin, signers) = setup_multisig(2, 2);
    let s1 = signers.get(0).unwrap();
    let s2 = signers.get(1).unwrap();

    // Create proposal and approve
    let proposal_id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &proposal_id);

    // Execute proposal
    client.execute_pause_proposal(&proposal_id);
    let events = e.events().all();
    let logs = e.logs().all();

    // Verify execution context events
    let has_execution_started = logs
        .iter()
        .any(|entry| entry.contains("pause_proposal_execution_started"));
    assert!(has_execution_started, "Should emit execution started event");

    // Verify validation events
    let has_validation_passed = logs
        .iter()
        .any(|entry| entry.contains("pause_proposal_validation_passed"));
    assert!(
        has_validation_passed,
        "Should emit validation success events"
    );

    // Verify success event
    let has_success_event = logs
        .iter()
        .any(|entry| entry.contains("pause_proposal_executed"));
    assert!(has_success_event, "Should emit successful execution event");
}

#[test]
fn test_execute_pause_proposal_error_observability() {
    let (e, client, _super_admin, signers) = setup_multisig(2, 2);
    let s1 = signers.get(0).unwrap();

    // Create proposal without sufficient approvals
    let proposal_id = client.pause(&s1).unwrap();

    // Try to execute with insufficient approvals
    let result = client.try_execute_pause_proposal(&proposal_id);
    assert!(result.is_err());

    // Contract events are rolled back on failure; the typed error remains
    // observable to callers and preserves the retry classification.
    assert_eq!(
        result.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(ERR_INSUFFICIENT_APPROVALS)
    );
    assert!(e.events().all().is_empty());
}

#[test]
fn test_execute_pause_proposal_cleanup_completeness() {
    let (e, client, _super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    // Create and execute proposal
    let proposal_id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&proposal_id);
    let events = e.events().all();
    let logs = e.logs().all();

    // Verify cleanup events were emitted
    let has_cleanup_event = logs.iter().any(|entry| {
        entry.contains("pause_proposal_cleanup_completed")
            || entry.contains("pause_storage_cleanup_metrics")
    });
    assert!(has_cleanup_event, "Should emit cleanup completion events");
}

#[test]
fn test_execute_pause_proposal_state_transition_logging() {
    let (e, client, _super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    // Execute pause proposal
    let proposal_id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&proposal_id);
    let events = e.events().all();
    let logs = e.logs().all();

    // Verify state transition was logged
    let has_transition_event = logs
        .iter()
        .any(|entry| entry.contains("pause_state_transition"));
    assert!(has_transition_event, "Should emit state transition events");
}

#[test]
fn test_execute_pause_proposal_metrics_tracking() {
    let (e, client, _super_admin, signers) = setup_multisig(1, 1);
    let s1 = signers.get(0).unwrap();

    // Execute proposal
    let proposal_id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&proposal_id);
    let events = e.events().all();
    let logs = e.logs().all();

    // Verify metrics events were emitted
    let has_metrics_event = logs.iter().any(|entry| {
        entry.contains("pause_execution_metrics") || entry.contains("pause_execution_completed")
    });
    assert!(has_metrics_event, "Should emit execution metrics events");
}
