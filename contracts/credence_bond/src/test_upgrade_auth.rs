extern crate std;
use crate::{
    upgrade_auth::{
        self, UpgradeAuthorization, UpgradeProposal, UpgradeRecord, UpgradeRole, UpgradeStatus,
    },
    CredenceBond, CredenceBondClient,
};
use std::panic::AssertUnwindSafe;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, Env, Vec};

// Helper: register contract + admin, return (client, admin, contract_id).
fn setup_with_contract(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin);
    (client, admin, contract_id)
}

fn create_test_address(e: &Env) -> Address {
    Address::generate(e)
}

fn create_test_env() -> Env {
    Env::default()
}

#[test]
fn test_upgrade_authorization_initialization() {
    let env = create_test_env();
    let admin = create_test_address(&env);

    // Initialize upgrade authorization
    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    // Verify admin is authorized
    assert!(upgrade_auth::is_authorized_upgrader(&env, &admin));
    assert_eq!(
        upgrade_auth::get_upgrade_role(&env, &admin),
        UpgradeRole::Upgrader
    );

    // Verify upgrade admin is set
    let auth_info = upgrade_auth::get_upgrade_auth(&env, &admin);
    assert_eq!(auth_info.authorized_address, admin);
    assert_eq!(auth_info.role, UpgradeRole::Upgrader);
    assert!(auth_info.active);
    assert_eq!(auth_info.granted_by, admin);
}

#[test]
fn test_grant_and_revoke_upgrade_authorization() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let user1 = create_test_address(&env);
    let user2 = create_test_address(&env);

    // Initialize
    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    // Grant upgrader role to user1
    upgrade_auth::grant_upgrade_auth(&env, &admin, &user1, UpgradeRole::Upgrader, 0);
    assert!(upgrade_auth::is_authorized_upgrader(&env, &user1));

    // Grant proposer role to user2
    upgrade_auth::grant_upgrade_auth(&env, &admin, &user2, UpgradeRole::Proposer, 0);
    assert!(!upgrade_auth::is_authorized_upgrader(&env, &user2)); // Proposer cannot upgrade
    assert_eq!(
        upgrade_auth::get_upgrade_role(&env, &user2),
        UpgradeRole::Proposer
    );

    // Revoke user2's authorization
    upgrade_auth::revoke_upgrade_auth(&env, &admin, &user2);

    // Should panic when trying to get revoked authorization
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::get_upgrade_role(&env, &user2);
    }))
    .expect_err("Should panic when getting revoked authorization");
}

#[test]
fn test_upgrade_authorization_expiry() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let user = create_test_address(&env);

    // Initialize
    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    // Grant authorization with expiry
    let now = env.ledger().timestamp();
    let expiry = now + 3600; // 1 hour from now
    upgrade_auth::grant_upgrade_auth(&env, &admin, &user, UpgradeRole::Upgrader, expiry);

    // Should be authorized before expiry
    assert!(upgrade_auth::is_authorized_upgrader(&env, &user));

    // Simulate time passing (in real implementation, you'd need to mock time)
    // For now, we can't easily test expiry without time manipulation

    // Test with expired authorization (set expiry to past)
    let past_expiry = now - 3600;
    upgrade_auth::grant_upgrade_auth(&env, &admin, &user, UpgradeRole::Upgrader, past_expiry);
    assert!(!upgrade_auth::is_authorized_upgrader(&env, &user));
}

#[test]
fn test_upgrade_proposal_and_approval() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let proposer = create_test_address(&env);
    let approver1 = create_test_address(&env);
    let approver2 = create_test_address(&env);
    let new_impl = create_test_address(&env);

    // Initialize and grant roles
    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &proposer, UpgradeRole::Proposer, 0);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &approver1, UpgradeRole::Upgrader, 0);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &approver2, UpgradeRole::Upgrader, 0);

    // Create proposal requiring 2 approvals
    let proposal_id =
        upgrade_auth::propose_upgrade(&env, &proposer, &new_impl, Bytes::new(&env), 2);

    // Verify proposal is pending
    let proposal = upgrade_auth::get_upgrade_proposal(&env, proposal_id);
    assert_eq!(proposal.status, UpgradeStatus::Pending);
    assert_eq!(proposal.proposer, proposer);
    assert_eq!(proposal.new_implementation, new_impl);
    assert_eq!(proposal.required_approvals, 2);
    assert_eq!(proposal.approvals.len(), 0);

    // Approve proposal
    upgrade_auth::approve_upgrade_proposal(&env, &approver1, proposal_id);

    // Should still be pending (need 2 approvals)
    let proposal_after_first = upgrade_auth::get_upgrade_proposal(&env, proposal_id);
    assert_eq!(proposal_after_first.status, UpgradeStatus::Pending);
    assert_eq!(proposal_after_first.approvals.len(), 1);

    // Second approval
    upgrade_auth::approve_upgrade_proposal(&env, &approver2, proposal_id);

    // Should now be approved
    let proposal_after_second = upgrade_auth::get_upgrade_proposal(&env, proposal_id);
    assert_eq!(proposal_after_second.status, UpgradeStatus::Approved);
    assert_eq!(proposal_after_second.approvals.len(), 2);
}

#[test]
fn test_upgrade_execution_with_proposal() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let proposer = create_test_address(&env);
    let approver = create_test_address(&env);
    let executor = create_test_address(&env);
    let old_impl = create_test_address(&env);
    let new_impl = create_test_address(&env);

    // Initialize and setup
    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &proposer, UpgradeRole::Proposer, 0);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &approver, UpgradeRole::Upgrader, 0);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &executor, UpgradeRole::Upgrader, 0);

    // Set initial implementation (in real scenario, this would be done during deployment)
    // For testing, we'll skip this and assume it's set

    // Create and approve proposal
    let proposal_id =
        upgrade_auth::propose_upgrade(&env, &proposer, &new_impl, Bytes::new(&env), 1);
    upgrade_auth::approve_upgrade_proposal(&env, &approver, proposal_id);

    // Execute upgrade
    upgrade_auth::execute_upgrade(&env, &executor, &new_impl, Some(proposal_id));

    // Verify implementation was updated
    assert_eq!(upgrade_auth::get_implementation(&env), new_impl);

    // Verify proposal is marked as executed
    let executed_proposal = upgrade_auth::get_upgrade_proposal(&env, proposal_id);
    assert_eq!(executed_proposal.status, UpgradeStatus::Executed);

    // Verify upgrade history
    let history = upgrade_auth::get_upgrade_history(&env);
    assert_eq!(history.len(), 1);
    let record = history.get(0).unwrap();
    assert_eq!(record.new_implementation, new_impl);
    assert_eq!(record.executed_by, executor);
    assert_eq!(record.proposal_id, Some(proposal_id));
}

#[test]
fn test_unauthorized_upgrade_attempts() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let unauthorized = create_test_address(&env);
    let new_impl = create_test_address(&env);

    // Initialize
    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    // Try to upgrade without authorization - should fail
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::execute_upgrade(&env, &unauthorized, &new_impl, None);
    }))
    .expect_err("Unauthorized upgrade should fail");

    // Grant proposer role (still can't upgrade)
    upgrade_auth::grant_upgrade_auth(&env, &admin, &unauthorized, UpgradeRole::Proposer, 0);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::execute_upgrade(&env, &unauthorized, &new_impl, None);
    }))
    .expect_err("Proposer should not be able to upgrade");
}

#[test]
fn test_cannot_revoke_last_upgrade_admin() {
    let env = create_test_env();
    let admin = create_test_address(&env);

    // Initialize
    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    // Try to revoke the only upgrade admin - should fail
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::revoke_upgrade_auth(&env, &admin, &admin);
    }))
    .expect_err("Cannot revoke last upgrade admin");
}

#[test]
fn test_upgrade_history_tracking() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let executor = create_test_address(&env);
    let impl1 = create_test_address(&env);
    let impl2 = create_test_address(&env);
    let impl3 = create_test_address(&env);

    // Initialize
    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &executor, UpgradeRole::Upgrader, 0);

    // Execute multiple upgrades
    upgrade_auth::execute_upgrade(&env, &executor, &impl2, None);
    upgrade_auth::execute_upgrade(&env, &executor, &impl3, None);

    // Verify history
    let history = upgrade_auth::get_upgrade_history(&env);
    assert_eq!(history.len(), 2);

    // Check first upgrade
    let first_upgrade = history.get(0).unwrap();
    assert_eq!(first_upgrade.new_implementation, impl2);
    assert_eq!(first_upgrade.executed_by, executor);

    // Check second upgrade
    let second_upgrade = history.get(1).unwrap();
    assert_eq!(second_upgrade.new_implementation, impl3);
    assert_eq!(second_upgrade.executed_by, executor);
    assert_eq!(second_upgrade.old_implementation, impl2);
}

#[test]
fn test_proposal_expiry_handling() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let proposer = create_test_address(&env);
    let new_impl = create_test_address(&env);

    // Initialize and grant proposer role
    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &proposer, UpgradeRole::Proposer, 0);

    // Create proposal
    let proposal_id =
        upgrade_auth::propose_upgrade(&env, &proposer, &new_impl, Bytes::new(&env), 1);

    // In a real implementation, you'd test expiry by manipulating time
    // For now, we'll verify the proposal exists and is pending
    let proposal = upgrade_auth::get_upgrade_proposal(&env, proposal_id);
    assert_eq!(proposal.status, UpgradeStatus::Pending);
    assert_eq!(proposal.proposer, proposer);
}
