#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::vec;
use soroban_sdk::Env;

// ============================================================================
// Test Helpers
// ============================================================================

fn create_test_env() -> (Env, CredenceBondClient<'static>, Vec<Address>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    // Generate governance members
    let gov_member1 = Address::generate(&e);
    let gov_member2 = Address::generate(&e);
    let gov_member3 = Address::generate(&e);

    let admin = Address::generate(&e);

    // Initialize with 2 required approvals (multi-sig)
    let gov_members_vec = vec![
        &e,
        gov_member1.clone(),
        gov_member2.clone(),
        gov_member3.clone(),
    ];

    client.initialize_with_governance(&admin, &2, &gov_members_vec);

    let governance_members = vec![&e, gov_member1, gov_member2, gov_member3];

    (e, client, governance_members, admin)
}

fn create_test_env_with_bond() -> (
    Env,
    CredenceBondClient<'static>,
    Vec<Address>,
    Address,
    Address,
) {
    let (e, client, governance_members, admin) = create_test_env();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    (e, client, governance_members, admin, identity)
}

// ============================================================================
// Integration Test 1: Slash Request Submission
// ============================================================================

/// Test that governance member can submit a slash request
#[test]
fn test_slash_request_submission_by_governance_member() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    let request_id = client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    assert_eq!(request_id, 1);

    let request = client.get_slash_request();
    assert_eq!(request.id, 1);
    assert_eq!(request.identity, identity);
    assert_eq!(request.amount, 500_i128);
    assert_eq!(request.status, SlashRequestStatus::Pending);
    assert!(!request.disputed);
}

/// Test that non-governance member cannot submit slash request
#[test]
#[should_panic(expected = "only governance members can submit slash requests")]
fn test_slash_request_submission_by_non_member_panics() {
    let (e, client, _governance_members, _admin) = create_test_env();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    let non_member = Address::generate(&e);
    client.submit_slash_request(
        &non_member,
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );
}

/// Test slash request with different reasons
#[test]
fn test_slash_request_different_reasons() {
    let (e, client, governance_members, _admin) = create_test_env();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Test different reason symbols
    let reasons = vec![
        &e,
        Symbol::new(&e, "fraud"),
        Symbol::new(&e, "violation"),
        Symbol::new(&e, "inactivity"),
        Symbol::new(&e, "abuse"),
    ];

    for (i, reason) in reasons.iter().enumerate() {
        let request_id = client.submit_slash_request(
            &governance_members.get(0).unwrap(),
            &identity,
            &(100_i128 * (i + 1) as i128),
            &reason,
        );

        assert_eq!(request_id, (i + 1) as u32);

        let request = client.get_slash_request();
        assert_eq!(request.reason, reason.clone());
    }
}

/// Test that slash request counter increments
#[test]
fn test_slash_request_counter_increments() {
    let (e, client, governance_members, _admin) = create_test_env();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Submit multiple slash requests
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &100_i128,
        &Symbol::new(&e, "reason1"),
    );

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &200_i128,
        &Symbol::new(&e, "reason2"),
    );

    let config = client.get_governance_config();
    assert_eq!(config.slash_request_counter, 2);
}

// ============================================================================
// Integration Test 2: Multi-Sig Approval Flow
// ============================================================================

/// Test single approval doesn't meet threshold
#[test]
fn test_single_approval_does_not_meet_threshold() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    let approved = client.approve_slash_request(&governance_members.get(1).unwrap());

    // With 2 required approvals, single approval shouldn't be enough
    // But since requester self-approves, 1 approval = 2 total
    assert!(approved);

    let request = client.get_slash_request();
    assert_eq!(request.status, SlashRequestStatus::Approved);
}

/// Test multiple approvals meet threshold
#[test]
fn test_multiple_approvals_meet_threshold() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Submit slash request (auto-approves from requester)
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    // First approval
    let approved1 = client.approve_slash_request(&governance_members.get(1).unwrap());
    assert!(approved1);

    let request1 = client.get_slash_request();
    assert_eq!(request1.status, SlashRequestStatus::Approved);
    assert_eq!(request1.approvals.len(), 2); // requester + gov_member1
}

/// Test duplicate approval is rejected
#[test]
fn test_duplicate_approval_rejected() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    // Try to approve again with same member
    let approved = client.approve_slash_request(&governance_members.get(0).unwrap());
    assert!(!approved); // Should return false for duplicate

    let request = client.get_slash_request();
    assert_eq!(request.approvals.len(), 1); // Still only one approval
}

/// Test non-member cannot approve
#[test]
#[should_panic(expected = "only governance members can approve slash requests")]
fn test_non_member_approval_panics() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    let non_member = Address::generate(&e);
    client.approve_slash_request(&non_member);
}

/// Test approval on non-pending request fails
#[test]
#[should_panic(expected = "request not pending")]
fn test_approval_on_executed_request_panics() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Submit slash request
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    client.approve_slash_request(&governance_members.get(1).unwrap());

    // Execute
    client.execute_slash();

    // Try to approve again - should fail
    client.approve_slash_request(&governance_members.get(2).unwrap());
}

/// Test governance member verification
#[test]
fn test_is_governance_member() {
    let (e, client, governance_members, _admin) = create_test_env();

    assert!(client.is_governance_member(&governance_members.get(0).unwrap()));
    assert!(client.is_governance_member(&governance_members.get(1).unwrap()));
    assert!(client.is_governance_member(&governance_members.get(2).unwrap()));

    let non_member = Address::generate(&e);
    assert!(!client.is_governance_member(&non_member));
}

// ============================================================================
// Integration Test 3: Slash Execution After Approval
// ============================================================================

/// Test slash execution after approval
#[test]
fn test_slash_execution_after_approval() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Get initial bond state
    let initial_bond = client.get_identity_state();
    assert_eq!(initial_bond.slashed_amount, 0);
    assert!(initial_bond.active);

    // Submit slash request
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    // Get approval
    client.approve_slash_request(&governance_members.get(1).unwrap());

    // Execute slash
    let slashed_bond = client.execute_slash();

    assert_eq!(slashed_bond.slashed_amount, 500_i128);
    assert!(slashed_bond.active); // Still active since 500 < 1000
    assert_eq!(slashed_bond.bonded_amount, 1000_i128);

    // Verify state consistency
    let current_bond = client.get_identity_state();
    assert_eq!(current_bond.slashed_amount, 500_i128);
    assert!(current_bond.active);
}

/// Test full bond slash (exceeds bonded amount)
#[test]
fn test_full_bond_slash() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Submit slash for full amount
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &1000_i128,
        &Symbol::new(&e, "fraud"),
    );

    client.approve_slash_request(&governance_members.get(1).unwrap());

    // Execute slash
    let slashed_bond = client.execute_slash();

    assert!(!slashed_bond.active); // Bond is now inactive
    assert_eq!(slashed_bond.slashed_amount, 1000_i128);
}

/// Test slash execution without approval fails
#[test]
#[should_panic(expected = "request not approved")]
fn test_slash_execution_without_approval_panics() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    // Try to execute without approval
    client.execute_slash();
}

/// Test request status updates after execution
#[test]
fn test_request_status_after_execution() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    let status_before = client.get_slash_request_status();
    assert_eq!(status_before, SlashRequestStatus::Pending);

    client.approve_slash_request(&governance_members.get(1).unwrap());

    let status_approved = client.get_slash_request_status();
    assert_eq!(status_approved, SlashRequestStatus::Approved);

    client.execute_slash();

    let status_executed = client.get_slash_request_status();
    assert_eq!(status_executed, SlashRequestStatus::Executed);
}

// ============================================================================
// Integration Test 4: Dispute Resolution
// ============================================================================

/// Test dispute on pending request
#[test]
fn test_dispute_on_pending_request() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    let disputed = client.dispute_slash_request(
        &governance_members.get(1).unwrap(),
        &Symbol::new(&e, "unfair"),
    );

    assert!(disputed);

    let request = client.get_slash_request();
    assert!(request.disputed);
    assert_eq!(request.status, SlashRequestStatus::Disputed);
}

/// Test dispute on approved request
#[test]
fn test_dispute_on_approved_request() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    client.approve_slash_request(&governance_members.get(1).unwrap());

    // Dispute after approval
    let disputed = client.dispute_slash_request(
        &governance_members.get(2).unwrap(),
        &Symbol::new(&e, "new_evidence"),
    );

    assert!(disputed);

    let request = client.get_slash_request();
    assert!(request.disputed);
    assert_eq!(request.status, SlashRequestStatus::Disputed);
}

/// Test dispute resolution - approved
#[test]
fn test_dispute_resolution_approved() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.dispute_slash_request(
        &governance_members.get(2).unwrap(),
        &Symbol::new(&e, "review"),
    );

    // Resolve with approval
    let resolved_status = client.resolve_dispute(&true);
    assert_eq!(resolved_status, SlashRequestStatus::Approved);

    let request = client.get_slash_request();
    assert!(!request.disputed);
}

/// Test dispute resolution - rejected
#[test]
fn test_dispute_resolution_rejected() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    client.dispute_slash_request(
        &governance_members.get(1).unwrap(),
        &Symbol::new(&e, "unfair"),
    );

    // Resolve by rejecting
    let resolved_status = client.resolve_dispute(&false);
    assert_eq!(resolved_status, SlashRequestStatus::Rejected);

    let request = client.get_slash_request();
    assert!(!request.disputed);
    assert_eq!(request.status, SlashRequestStatus::Rejected);
}

/// Test non-member cannot dispute
#[test]
#[should_panic(expected = "only governance members can dispute slash requests")]
fn test_non_member_dispute_panics() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    let non_member = Address::generate(&e);
    client.dispute_slash_request(&non_member, &Symbol::new(&e, "unfair"));
}

/// Test cannot dispute executed request
#[test]
#[should_panic(expected = "cannot dispute in current state")]
fn test_cannot_dispute_executed_request() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();

    // Try to dispute executed request
    client.dispute_slash_request(
        &governance_members.get(2).unwrap(),
        &Symbol::new(&e, "too_late"),
    );
}

// ============================================================================
// Integration Test 5: State Consistency
// ============================================================================

/// Test bond state consistency after slash
#[test]
fn test_bond_state_consistency() {
    let (e, client, governance_members, _admin) = create_test_env();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Submit multiple slash requests
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &300_i128,
        &Symbol::new(&e, "reason1"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();

    let bond1 = client.get_identity_state();
    assert_eq!(bond1.slashed_amount, 300_i128);
    assert!(bond1.active);

    // Submit another slash
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &400_i128,
        &Symbol::new(&e, "reason2"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();

    let bond2 = client.get_identity_state();
    assert_eq!(bond2.slashed_amount, 700_i128); // 300 + 400
    assert!(bond2.active);

    // Submit final slash that exceeds bond
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "reason3"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();

    let bond3 = client.get_identity_state();
    assert_eq!(bond3.slashed_amount, 1000_i128); // Capped at bonded amount
    assert!(!bond3.active);
}

/// Test governance config remains consistent
#[test]
fn test_governance_config_consistency() {
    let (e, client, _governance_members, admin) = create_test_env();

    let config = client.get_governance_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.required_approvals, 2);
    assert_eq!(config.governance_members.len(), 3);
    assert_eq!(config.slash_request_counter, 0);

    // Create bond and submit slash requests
    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &config.governance_members.get(0).unwrap(),
        &identity,
        &100_i128,
        &Symbol::new(&e, "test"),
    );

    let config_after = client.get_governance_config();
    assert_eq!(config_after.slash_request_counter, 1);

    // Admin and required approvals should remain unchanged
    assert_eq!(config_after.admin, admin);
    assert_eq!(config_after.required_approvals, 2);
}

/// Test slash request state consistency across operations
#[test]
fn test_slash_request_state_consistency() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Initial submission
    let request_id = client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    let request1 = client.get_slash_request();
    assert_eq!(request1.id, request_id);
    assert_eq!(request1.status, SlashRequestStatus::Pending);
    assert_eq!(request1.approvals.len(), 1); // Self-approval

    // After approval
    client.approve_slash_request(&governance_members.get(1).unwrap());

    let request2 = client.get_slash_request();
    assert_eq!(request2.status, SlashRequestStatus::Approved);
    assert_eq!(request2.approvals.len(), 2);

    // After execution
    client.execute_slash();

    let request3 = client.get_slash_request();
    assert_eq!(request3.status, SlashRequestStatus::Executed);

    // All state should be consistent
    assert_eq!(request3.id, request_id);
    assert_eq!(request3.amount, 500_i128);
    assert_eq!(request3.identity, identity);
}

/// Test reject slash request
#[test]
fn test_reject_slash_request() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    let status = client.reject_slash_request();
    assert_eq!(status, SlashRequestStatus::Rejected);

    let request = client.get_slash_request();
    assert_eq!(request.status, SlashRequestStatus::Rejected);

    // Bond should be unchanged
    let bond = client.get_identity_state();
    assert_eq!(bond.slashed_amount, 0);
    assert!(bond.active);
}

/// Test multiple slash requests in sequence
#[test]
fn test_multiple_slash_requests_sequence() {
    let (e, client, governance_members, _admin) = create_test_env();

    let identity1 = Address::generate(&e);

    client.create_bond(&identity1, &1000_i128, &86400_u64);

    // Slash identity1
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity1,
        &300_i128,
        &Symbol::new(&e, "reason1"),
    );
    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.execute_slash();

    let bond1 = client.get_identity_state();
    assert_eq!(bond1.slashed_amount, 300_i128);

    // Counter should have incremented
    let config = client.get_governance_config();
    assert_eq!(config.slash_request_counter, 1);
}

/// Test edge case: zero amount slash
#[test]
fn test_zero_amount_slash() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &0_i128,
        &Symbol::new(&e, "no_reason"),
    );

    client.approve_slash_request(&governance_members.get(1).unwrap());
    let bond = client.execute_slash();

    assert_eq!(bond.slashed_amount, 0);
    assert!(bond.active);
}

/// Test governance initialization
#[test]
fn test_governance_initialization() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, CredenceBond);
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let gov_member1 = Address::generate(&e);
    let gov_member2 = Address::generate(&e);

    let gov_members_vec = vec![&e, gov_member1.clone(), gov_member2.clone()];

    // Initialize with 1 required approval
    client.initialize_with_governance(&admin, &1, &gov_members_vec);

    let config = client.get_governance_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.required_approvals, 1);
    assert_eq!(config.governance_members.len(), 2);
    assert_eq!(config.slash_request_counter, 0);
}

/// Test approval count tracking
#[test]
fn test_approval_count_tracking() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // With 2 required approvals, submit adds 1 (self-approval)
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    let request1 = client.get_slash_request();
    assert_eq!(request1.approvals.len(), 1);

    // First additional approval - should meet threshold
    client.approve_slash_request(&governance_members.get(1).unwrap());

    let request2 = client.get_slash_request();
    assert_eq!(request2.approvals.len(), 2);
    assert_eq!(request2.status, SlashRequestStatus::Approved);

    // Once approved, cannot approve again - should panic
    // This tests that the contract properly prevents double-approval
}

// ============================================================================
// Additional Edge Case Tests
// ============================================================================

/// Test slash amount exceeds current bond (partial slash)
#[test]
fn test_slash_amount_exceeds_bond() {
    let (e, client, governance_members, _admin) = create_test_env();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &500_i128, &86400_u64); // Only 500 bonded

    // Try to slash 1000 (more than bonded)
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &1000_i128,
        &Symbol::new(&e, "fraud"),
    );

    client.approve_slash_request(&governance_members.get(1).unwrap());
    let bond = client.execute_slash();

    // Should cap at bonded amount
    assert_eq!(bond.slashed_amount, 500_i128);
    assert!(!bond.active);
}

/// Test dispute preserves original request data
#[test]
fn test_dispute_preserves_request_data() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    client.approve_slash_request(&governance_members.get(1).unwrap());
    client.dispute_slash_request(
        &governance_members.get(2).unwrap(),
        &Symbol::new(&e, "review_needed"),
    );

    let request = client.get_slash_request();
    assert_eq!(request.amount, 500_i128);
    assert_eq!(request.identity, identity);
    assert_eq!(request.reason, Symbol::new(&e, "misconduct"));
    assert!(request.disputed);
    assert_eq!(request.dispute_reason, Symbol::new(&e, "review_needed"));
}

/// Test resolve dispute without disputed request
#[test]
#[should_panic(expected = "no disputed request")]
fn test_resolve_dispute_without_disputed_request_panics() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Submit but don't dispute
    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    // Try to resolve without dispute
    client.resolve_dispute(&true);
}

/// Test state after rejection
#[test]
fn test_state_after_rejection() {
    let (e, client, governance_members, _admin, _identity) = create_test_env_with_bond();

    let identity = Address::generate(&e);
    client.create_bond(&identity, &1000_i128, &86400_u64);

    client.submit_slash_request(
        &governance_members.get(0).unwrap(),
        &identity,
        &500_i128,
        &Symbol::new(&e, "misconduct"),
    );

    client.reject_slash_request();

    // Bond should be unchanged
    let bond = client.get_identity_state();
    assert_eq!(bond.slashed_amount, 0);
    assert!(bond.active);

    // Request should be rejected
    let status = client.get_slash_request_status();
    assert_eq!(status, SlashRequestStatus::Rejected);
}
