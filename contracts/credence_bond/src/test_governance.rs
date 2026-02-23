//! Integration tests for complete governance flow
//! 
//! This module contains comprehensive integration tests covering:
//! - Slash request submission by governance members
//! - Multi-sig approval flow with multiple approvers
//! - Slash execution after sufficient approvals
//! - Dispute resolution workflow
//! - State consistency verification
//! - Multi-actor scenarios
//!
//! ## Test Coverage Goals
//! - Minimum 95% code coverage
//! - End-to-end workflow validation
//! - Edge case handling

#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::vec;
use soroban_sdk::Env;

use crate::{
    CredenceBond, CredenceBondClient, 
    SlashRequestStatus,
};

/// Create a test environment with governance configured
fn create_governance_env<'a>() -> (Env, CredenceBondClient<'a>, Vec<Address>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);
    
    // Generate governance members
    let gov_member1 = Address::generate(&e);
    let gov_member2 = Address::generate(&e);
    let gov_member3 = Address::generate(&e);
    let admin = Address::generate(&e);
    
    let governance_members = vec![&e, gov_member1.clone(), gov_member2.clone(), gov_member3.clone()];
    
    // Initialize with governance
    client.initialize_with_governance(&admin, &2, &governance_members);
    
    (e, client, governance_members, admin)
}

/// Create test environment with a bond already created
fn create_env_with_bond<'a>() -> (Env, CredenceBondClient<'a>, Vec<Address>, Address, Address) {
    let (e, client, governance_members, admin) = create_governance_env();
    
    // Create a bond for an identity
    let identity = Address::generate(&e);
    let amount = 1000_i128;
    let duration = 100_u64;
    
    client.create_bond(&identity, &amount, &duration);
    
    (e, client, governance_members, admin, identity)
}

// ============================================================================
// SLASH REQUEST SUBMISSION TESTS
// ============================================================================

/// Test that governance member can submit a slash request
#[test]
fn test_integration_slash_request_submission() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    let request_id = client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    assert_eq!(request_id, 1);
    
    let request = client.get_slash_request();
    assert_eq!(request.id, 1);
    assert_eq!(request.requester, governance_members.get(0).unwrap());
    assert_eq!(request.identity, identity);
    assert_eq!(request.amount, 500);
    assert_eq!(request.status, SlashRequestStatus::Pending);
}

/// Test that non-governance member cannot submit slash request
#[test]
#[should_panic(expected = "only governance members can submit slash requests")]
fn test_integration_non_member_cannot_submit() {
    let (e, client, _governance_members, _admin, identity) = create_env_with_bond();
    
    let non_member = Address::generate(&e);
    client.submit_slash_request(
        &non_member,
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
}

/// Test slash request with different amounts
#[test]
fn test_integration_slash_different_amounts() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    // Partial slash
    let request_id1 = client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &300,
        &Symbol::new(&e, "partial_violation"),
    );
    assert_eq!(request_id1, 1);
    
    let request1 = client.get_slash_request();
    assert_eq!(request1.amount, 300);
    
    // Full slash
    let request_id2 = client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &1000,
        &Symbol::new(&e, "major_violation"),
    );
    assert_eq!(request_id2, 2);
    
    let request2 = client.get_slash_request();
    assert_eq!(request2.amount, 1000);
}

// ============================================================================
// MULTI-SIG APPROVAL FLOW TESTS
// ============================================================================

/// Test single approval - with self-approval already included
/// The requester is automatically self-approved, so one additional approval meets threshold
#[test]
fn test_integration_single_approval_with_self_approval() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    // The requester is self-approved, so with 1 more approval we have 2 total = meets threshold
    let approved = client.approve_slash_request(&governance_members.get(1).unwrap());
    assert!(approved); // Has 2 approvals now (requester + approver), meets threshold of 2
    
    let status = client.get_slash_request_status();
    assert_eq!(status, SlashRequestStatus::Approved);
}

/// Test multiple approvals meet threshold
#[test]
fn test_integration_multiple_approvals_sufficient() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    // First approval (in addition to self-approval from requester)
    let approved1 = client.approve_slash_request(&governance_members.get(1).unwrap());
    assert!(approved1); // Now has 2 approvals (requester + approver), meets threshold of 2
    
    let status = client.get_slash_request_status();
    assert_eq!(status, SlashRequestStatus::Approved);
}

/// Test duplicate approval is rejected
#[test]
fn test_integration_duplicate_approval_rejected() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    // Try to approve again with same member (already self-approved)
    let approved = client.approve_slash_request(&governance_members.get(0).unwrap());
    assert!(!approved); // Should return false for duplicate
}

/// Test non-member cannot approve
#[test]
#[should_panic(expected = "only governance members can approve slash requests")]
fn test_integration_non_member_approval_panics() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    let non_member = Address::generate(&e);
    client.approve_slash_request(&non_member);
}

/// Test approval tracking
#[test]
fn test_integration_approval_tracking() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    let request = client.get_slash_request();
    assert_eq!(request.approvals.len(), 1); // Only self-approval
    
    client.approve_slash_request(&governance_members.get(1).unwrap());
    
    let request_after = client.get_slash_request();
    assert_eq!(request_after.approvals.len(), 2);
}

// ============================================================================
// SLASH EXECUTION TESTS
// ============================================================================

/// Test slash execution after approval
#[test]
fn test_integration_slash_execution_after_approval() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    // Submit and approve
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    
    // Execute slash
    let bond = client.execute_slash();
    
    assert_eq!(bond.slashed_amount, 500);
    assert!(bond.active); // Still active with remaining bond
    
    let status = client.get_slash_request_status();
    assert_eq!(status, SlashRequestStatus::Executed);
}

/// Test full bond slash
#[test]
fn test_integration_full_bond_slash() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &1000, // Full amount
        &Symbol::new(&e, "major_violation"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    
    let bond = client.execute_slash();
    
    assert_eq!(bond.slashed_amount, 1000);
    assert!(!bond.active); // Bond is fully slashed, no longer active
}

/// Test execution without approval panics
#[test]
#[should_panic(expected = "request not approved")]
fn test_integration_execution_without_approval_panics() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    // Try to execute without approval
    client.execute_slash();
}

/// Test request status after execution
#[test]
fn test_integration_status_after_execution() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    
    client.execute_slash();
    
    let request = client.get_slash_request();
    assert_eq!(request.status, SlashRequestStatus::Executed);
}

// ============================================================================
// DISPUTE RESOLUTION TESTS
// ============================================================================

/// Test dispute on pending request
#[test]
fn test_integration_dispute_pending_request() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    let disputed = client.dispute_slash_request(
        &governance_members.get(1).unwrap(),
        &Symbol::new(&e, "unfair"),
    );
    
    assert!(disputed);
    
    let status = client.get_slash_request_status();
    assert_eq!(status, SlashRequestStatus::Disputed);
    
    let request = client.get_slash_request();
    assert!(request.disputed);
    assert_eq!(request.dispute_reason, Symbol::new(&e, "unfair"));
}

/// Test dispute on approved request
#[test]
fn test_integration_dispute_approved_request() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    
    let disputed = client.dispute_slash_request(
        &governance_members.get(2).unwrap(),
        &Symbol::new(&e, "new_evidence"),
    );
    
    assert!(disputed);
    
    let status = client.get_slash_request_status();
    assert_eq!(status, SlashRequestStatus::Disputed);
}

/// Test dispute resolution - approved
#[test]
fn test_integration_dispute_resolution_approved() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.dispute_slash_request(
        &governance_members.get(2).unwrap(),
        &Symbol::new(&e, "review"),
    );
    
    // Resolve with approval (has enough approvals)
    let resolved_status = client.resolve_dispute(&true);
    assert_eq!(resolved_status, SlashRequestStatus::Approved);
    
    let request = client.get_slash_request();
    assert!(!request.disputed);
}

/// Test dispute resolution - rejected
#[test]
fn test_integration_dispute_resolution_rejected() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    client.dispute_slash_request(
        &governance_members.get(1).unwrap(),
        &Symbol::new(&e, "unfair"),
    );
    
    // Resolve with rejection
    let resolved_status = client.resolve_dispute(&false);
    assert_eq!(resolved_status, SlashRequestStatus::Rejected);
    
    let request = client.get_slash_request();
    assert!(!request.disputed);
}

/// Test non-member cannot dispute
#[test]
#[should_panic(expected = "only governance members can dispute slash requests")]
fn test_integration_non_member_dispute_panics() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    let non_member = Address::generate(&e);
    client.dispute_slash_request(&non_member, &Symbol::new(&e, "unfair"));
}

/// Test cannot dispute executed request
#[test]
#[should_panic(expected = "cannot dispute in current state")]
fn test_integration_cannot_dispute_executed() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();
    
    client.dispute_slash_request(
        &governance_members.get(2).unwrap(),
        &Symbol::new(&e, "too_late"),
    );
}

// ============================================================================
// STATE CONSISTENCY TESTS
// ============================================================================

/// Test bond state consistency after multiple operations
#[test]
fn test_integration_bond_state_consistency() {
    let (e, client, governance_members, _admin) = create_governance_env();
    
    // Create first identity and bond
    let identity1 = Address::generate(&e);
    client.create_bond(&identity1, &1000, &100);
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity1,
        &300,
        &Symbol::new(&e, "violation1"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();
    
    // Create second identity and bond
    let identity2 = Address::generate(&e);
    client.create_bond(&identity2, &2000, &100);
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity2,
        &500,
        &Symbol::new(&e, "violation2"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();
    
    // Verify state consistency
    let config = client.get_governance_config();
    assert_eq!(config.slash_request_counter, 2);
}

/// Test governance config consistency
#[test]
fn test_integration_governance_config_consistency() {
    let (e, client, governance_members, admin) = create_governance_env();
    
    let config = client.get_governance_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.required_approvals, 2);
    assert_eq!(config.governance_members.len(), 3);
    assert_eq!(config.slash_request_counter, 0);
    
    // Submit a request and verify counter increments
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000, &100);
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "test"),
    );
    
    let config_after = client.get_governance_config();
    assert_eq!(config_after.slash_request_counter, 1);
}

/// Test slash request state transitions
#[test]
fn test_integration_request_state_transitions() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    // Initial state - Pending
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    assert_eq!(client.get_slash_request_status(), SlashRequestStatus::Pending);
    
    // After approval - Approved
    client.approve_slash_request(&governance_members.get(1).unwrap());
    assert_eq!(client.get_slash_request_status(), SlashRequestStatus::Approved);
    
    // After execution - Executed
    client.execute_slash();
    assert_eq!(client.get_slash_request_status(), SlashRequestStatus::Executed);
}

// ============================================================================
// MULTI-ACTOR SCENARIO TESTS
// ============================================================================

/// Test complex multi-actor governance scenario
#[test]
fn test_integration_multi_actor_complex_scenario() {
    let (e, client, governance_members, admin) = create_governance_env();
    
    // Setup multiple identities with bonds
    let identity1 = Address::generate(&e);
    let identity2 = Address::generate(&e);
    let identity3 = Address::generate(&e);
    
    client.create_bond(&identity1, &1000, &100);
    client.create_bond(&identity2, &2000, &100);
    client.create_bond(&identity3, &500, &100);
    
    // Scenario 1: Slash identity1 - approved and executed
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity1,
        &500,
        &Symbol::new(&e, "minor_violation"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();
    
    // Scenario 2: Slash identity2 - disputed then resolved
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity2,
        &1000,
        &Symbol::new(&e, "major_violation"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.dispute_slash_request(
        &governance_members.get(2).unwrap(),
        &Symbol::new(&e, "need_review"),
    );
    client.resolve_dispute(&true); // Approve the resolution
    client.execute_slash();
    
    // Scenario 3: Slash identity3 - rejected
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity3,
        &100,
        &Symbol::new(&e, "minor_issue"),
    );
    // Don't approve, just reject
    let _rejected = client.reject_slash_request();
    
    // Verify final state
    let config = client.get_governance_config();
    assert_eq!(config.slash_request_counter, 3);
    
    // Verify admin is still correct
    assert_eq!(config.admin, admin);
}

/// Test approval from different members
#[test]
fn test_integration_approval_from_different_members() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    
    // Get approval from member 1
    client.approve_slash_request(&governance_members.get(1).unwrap());
    
    // Verify both approvals are recorded
    let request = client.get_slash_request();
    assert_eq!(request.approvals.len(), 2);
    
    // Verify both members are in approvals
    let request_approver0 = request.approvals.get(0).unwrap();
    let request_approver1 = request.approvals.get(1).unwrap();
    let has_requester = request_approver0 == governance_members.get(0).unwrap() 
        || request_approver1 == governance_members.get(0).unwrap();
    let has_approver = request_approver0 == governance_members.get(1).unwrap() 
        || request_approver1 == governance_members.get(1).unwrap();
    assert!(has_requester);
    assert!(has_approver);
}

/// Test zero amount slash
#[test]
fn test_integration_zero_amount_slash() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &0,
        &Symbol::new(&e, "warning"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    
    let bond = client.execute_slash();
    assert_eq!(bond.slashed_amount, 0);
}

/// Test slash amount exceeds bond
#[test]
fn test_integration_slash_exceeds_bond() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &2000, // More than bonded amount
        &Symbol::new(&e, "major_violation"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    
    let bond = client.execute_slash();
    // Should be capped at bonded amount
    assert_eq!(bond.slashed_amount, 1000);
    assert!(!bond.active);
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

/// Test multiple slash requests in sequence
#[test]
fn test_integration_multiple_slash_sequence() {
    let (e, client, governance_members, _admin) = create_governance_env();
    
    let identity = Address::generate(&e);
    client.create_bond(&identity, &3000, &100);
    
    // First slash
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &1000,
        &Symbol::new(&e, "first"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();
    
    // Second slash
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &1000,
        &Symbol::new(&e, "second"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();
    
    // Verify final state
    let config = client.get_governance_config();
    assert_eq!(config.slash_request_counter, 2);
    
    // Bond should have 2000 total slashed (but capped at bonded amount)
    let bond = client.get_identity_state();
    assert_eq!(bond.slashed_amount, 2000);
}

/// Test governance member verification
#[test]
fn test_integration_governance_member_verification() {
    let (e, client, governance_members, _admin) = create_governance_env();
    
    // All governance members should be recognized
    assert!(client.is_governance_member(&governance_members.get(0).unwrap()));
    assert!(client.is_governance_member(&governance_members.get(1).unwrap()));
    assert!(client.is_governance_member(&governance_members.get(2).unwrap()));
    
    // Non-members should not be recognized
    let non_member = Address::generate(&e);
    assert!(!client.is_governance_member(&non_member));
}

/// Test rejection preserves request data
#[test]
fn test_integration_rejection_preserves_data() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "test_violation"),
    );
    
    let rejected_status = client.reject_slash_request();
    assert_eq!(rejected_status, SlashRequestStatus::Rejected);
    
    let request = client.get_slash_request();
    assert_eq!(request.status, SlashRequestStatus::Rejected);
    assert_eq!(request.amount, 500);
    assert_eq!(request.identity, identity);
}

/// Test approval on executed request fails
#[test]
#[should_panic(expected = "request not pending")]
fn test_integration_approval_on_executed_fails() {
    let (e, client, governance_members, _admin, identity) = create_env_with_bond();
    
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500,
        &Symbol::new(&e, "misconduct"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();
    
    // Try to approve again - should fail
    client.approve_slash_request(&governance_members.get(2).unwrap());
}
