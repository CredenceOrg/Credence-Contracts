extern crate std;
use crate::{
    upgrade_auth::{
        self, UpgradeAuthorization, UpgradeProposal, UpgradeRecord, UpgradeRole, UpgradeStatus,
    },
    CredenceBond, CredenceBondClient,
};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Bytes, Env, Vec};
use std::panic::AssertUnwindSafe;

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
    let env = Env::default();
    env.mock_all_auths();
    env
}

#[test]
fn test_upgrade_authorization_initialization() {
    let env = create_test_env();
    let (client, admin, _contract_id) = setup_with_contract(&env);

    // Verify admin is authorized
    assert!(client.is_authorized_upgrader(&admin));
    assert_eq!(client.get_upgrade_role(&admin), UpgradeRole::Upgrader);

    // Verify upgrade admin is set
    let auth_info = client.get_upgrade_auth(&admin).unwrap();
    assert_eq!(auth_info.authorized_address, admin);
    assert_eq!(auth_info.role, UpgradeRole::Upgrader);
    assert!(auth_info.active);
    assert_eq!(auth_info.granted_by, admin);
}

#[test]
fn test_grant_and_revoke_upgrade_authorization() {
    let env = create_test_env();
    let (client, admin, _contract_id) = setup_with_contract(&env);
    let user1 = create_test_address(&env);
    let user2 = create_test_address(&env);

    client.grant_upgrade_auth(&admin, &user1, &UpgradeRole::Upgrader, &0);
    assert!(client.is_authorized_upgrader(&user1));

    client.grant_upgrade_auth(&admin, &user2, &UpgradeRole::Proposer, &0);
    assert!(!client.is_authorized_upgrader(&user2));
    assert_eq!(client.get_upgrade_role(&user2), UpgradeRole::Proposer);

    client.revoke_upgrade_auth(&admin, &user2);

    // After revoke, should not be authorized
    assert!(!client.is_authorized_upgrader(&user2));
}

#[test]
fn test_upgrade_authorization_expiry() {
    let env = create_test_env();
    let (client, admin, _contract_id) = setup_with_contract(&env);
    let user = create_test_address(&env);

    env.ledger().with_mut(|li| li.timestamp = 10000); // Set a non-zero timestamp
    let now = env.ledger().timestamp();
    let expiry = now + 3600;
    client.grant_upgrade_auth(&admin, &user, &UpgradeRole::Upgrader, &expiry);

    assert!(client.is_authorized_upgrader(&user));

    // Advance time past expiry
    env.ledger().with_mut(|li| li.timestamp = expiry + 1);
    assert!(!client.is_authorized_upgrader(&user));
}

#[test]
fn test_upgrade_proposal_and_approval() {
    let env = create_test_env();
    let (client, admin, _contract_id) = setup_with_contract(&env);
    let proposer = create_test_address(&env);
    let approver1 = create_test_address(&env);
    let approver2 = create_test_address(&env);
    let new_impl = create_test_address(&env);

    client.grant_upgrade_auth(&admin, &proposer, &UpgradeRole::Proposer, &0);
    client.grant_upgrade_auth(&admin, &approver1, &UpgradeRole::Upgrader, &0);
    client.grant_upgrade_auth(&admin, &approver2, &UpgradeRole::Upgrader, &0);

    let proposal_id = client.propose_upgrade(&proposer, &new_impl, &Bytes::new(&env), &2);

    let proposal = client.get_upgrade_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, UpgradeStatus::Pending);
    assert_eq!(proposal.proposer, proposer);
    assert_eq!(proposal.new_implementation, new_impl);
    assert_eq!(proposal.required_approvals, 2);
    assert_eq!(proposal.approvals.len(), 0);

    client.approve_upgrade_proposal(&approver1, &proposal_id);

    let proposal_after_first = client.get_upgrade_proposal(&proposal_id).unwrap();
    assert_eq!(proposal_after_first.status, UpgradeStatus::Pending);
    assert_eq!(proposal_after_first.approvals.len(), 1);

    client.approve_upgrade_proposal(&approver2, &proposal_id);

    let proposal_after_second = client.get_upgrade_proposal(&proposal_id).unwrap();
    assert_eq!(proposal_after_second.status, UpgradeStatus::Approved);
    assert_eq!(proposal_after_second.approvals.len(), 2);
}

#[test]
fn test_upgrade_execution_with_proposal() {
    let env = create_test_env();
    let (client, admin, _contract_id) = setup_with_contract(&env);
    let proposer = create_test_address(&env);
    let approver = create_test_address(&env);
    let executor = create_test_address(&env);
    let _old_impl = create_test_address(&env);
    let new_impl = create_test_address(&env);

    client.grant_upgrade_auth(&admin, &proposer, &UpgradeRole::Proposer, &0);
    client.grant_upgrade_auth(&admin, &approver, &UpgradeRole::Upgrader, &0);
    client.grant_upgrade_auth(&admin, &executor, &UpgradeRole::Upgrader, &0);

    let proposal_id = client.propose_upgrade(&proposer, &new_impl, &Bytes::new(&env), &1);
    client.approve_upgrade_proposal(&approver, &proposal_id);

    client.execute_upgrade(&executor, &new_impl, &Some(proposal_id));

    assert_eq!(client.get_implementation(), new_impl);

    let executed_proposal = client.get_upgrade_proposal(&proposal_id).unwrap();
    assert_eq!(executed_proposal.status, UpgradeStatus::Executed);

    let history = client.get_upgrade_history();
    assert_eq!(history.len(), 1);
    let record = history.get(0).unwrap();
    assert_eq!(record.new_implementation, new_impl);
    assert_eq!(record.executed_by, executor);
    assert_eq!(record.proposal_id, Some(proposal_id));
}

#[test]
fn test_unauthorized_upgrade_attempts() {
    let env = create_test_env();
    let (client, admin, _contract_id) = setup_with_contract(&env);
    let unauthorized = create_test_address(&env);
    let new_impl = create_test_address(&env);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.execute_upgrade(&unauthorized, &new_impl, &None);
    }))
    .expect_err("Unauthorized upgrade should fail");

    client.grant_upgrade_auth(&admin, &unauthorized, &UpgradeRole::Proposer, &0);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.execute_upgrade(&unauthorized, &new_impl, &None);
    }))
    .expect_err("Proposer should not be able to upgrade");
}

#[test]
fn test_cannot_revoke_last_upgrade_admin() {
    let env = create_test_env();
    let (client, admin, _contract_id) = setup_with_contract(&env);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.revoke_upgrade_auth(&admin, &admin);
    }))
    .expect_err("Cannot revoke last upgrade admin");
}

#[test]
fn test_upgrade_history_tracking() {
    let env = create_test_env();
    let (client, admin, _contract_id) = setup_with_contract(&env);
    let executor = create_test_address(&env);
    let _impl1 = create_test_address(&env);
    let impl2 = create_test_address(&env);
    let impl3 = create_test_address(&env);

    client.grant_upgrade_auth(&admin, &executor, &UpgradeRole::Upgrader, &0);

    client.execute_upgrade(&executor, &impl2, &None);
    client.execute_upgrade(&executor, &impl3, &None);

    let history = client.get_upgrade_history();
    assert_eq!(history.len(), 2);

    let first_upgrade = history.get(0).unwrap();
    assert_eq!(first_upgrade.new_implementation, impl2);
    assert_eq!(first_upgrade.executed_by, executor);

    let second_upgrade = history.get(1).unwrap();
    assert_eq!(second_upgrade.new_implementation, impl3);
    assert_eq!(second_upgrade.executed_by, executor);
    assert_eq!(second_upgrade.old_implementation, impl2);
}

#[test]
fn test_proposal_expiry_handling() {
    let env = create_test_env();
    let (client, admin, _contract_id) = setup_with_contract(&env);
    let proposer = create_test_address(&env);
    let new_impl = create_test_address(&env);

    client.grant_upgrade_auth(&admin, &proposer, &UpgradeRole::Proposer, &0);

    let proposal_id = client.propose_upgrade(&proposer, &new_impl, &Bytes::new(&env), &1);

    let proposal = client.get_upgrade_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, UpgradeStatus::Pending);
    assert_eq!(proposal.proposer, proposer);
}
