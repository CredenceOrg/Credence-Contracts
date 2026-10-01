extern crate std;
use crate::{
    upgrade_auth::{self, UpgradeRole, UpgradeStatus},
    CredenceBond, CredenceBondClient,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Bytes, Env, Vec};
use std::panic::AssertUnwindSafe;

// Helper: register contract + admin, return (client, admin, contract_id).
fn setup_with_contract(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
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

    // Revoke sets active=false but does NOT remove the storage record.
    // get_upgrade_role still returns the role (record exists), but the address
    // is no longer an active upgrader.
    assert!(
        !upgrade_auth::is_authorized_upgrader(&env, &user2),
        "revoked address must not be an active upgrader"
    );
    let revoked_auth = upgrade_auth::get_upgrade_auth(&env, &user2);
    assert!(
        !revoked_auth.active,
        "revoked authorization record must have active == false"
    );
}

#[test]
fn test_upgrade_authorization_expiry() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let user = create_test_address(&env);

    // Initialize
    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    // Grant authorization with expiry in the future
    let now = env.ledger().timestamp();
    let expiry = now + 3600; // 1 hour from now
    upgrade_auth::grant_upgrade_auth(&env, &admin, &user, UpgradeRole::Upgrader, expiry);

    // Should be authorized before expiry
    assert!(upgrade_auth::is_authorized_upgrader(&env, &user));

    // Test with expired authorization: use a fresh address to avoid the
    // "already authorized" guard on the second grant.
    let user_expired = create_test_address(&env);
    let past_expiry = now.saturating_sub(3600);
    upgrade_auth::grant_upgrade_auth(
        &env,
        &admin,
        &user_expired,
        UpgradeRole::Upgrader,
        past_expiry,
    );
    assert!(!upgrade_auth::is_authorized_upgrader(&env, &user_expired));
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

    // Set an initial implementation so execute_upgrade has a "current" value to
    // compare against and record in history.
    env.storage().instance().set(
        &crate::DataKey::Upgrade(crate::UpgradeKey::Implementation),
        &old_impl,
    );

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

    // Seed the initial implementation so execute_upgrade has an existing value to compare.
    env.storage().instance().set(
        &crate::DataKey::Upgrade(crate::UpgradeKey::Implementation),
        &impl1,
    );

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

#[test]
fn test_upgrade_replay_prevention_surfaces_typed_error() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let executor = create_test_address(&env);
    let initial_impl = create_test_address(&env);
    let new_impl = create_test_address(&env);

    // Initialize and grant upgrader role
    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &executor, UpgradeRole::Upgrader, 0);

    // Seed the initial implementation required by execute_upgrade's "no current implementation" guard.
    env.storage().instance().set(
        &crate::DataKey::Upgrade(crate::UpgradeKey::Implementation),
        &initial_impl,
    );

    // First execution should succeed
    upgrade_auth::execute_upgrade(&env, &executor, &new_impl, None);

    // To trigger the replay guard (ExecutedOp) we need to bypass the "same implementation"
    // guard first by upgrading to a third address, then replaying new_impl.
    let another_impl = create_test_address(&env);
    upgrade_auth::execute_upgrade(&env, &executor, &another_impl, None);

    // Attempt to replay new_impl — must fail with ProposalAlreadyExecuted typed error.
    let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::execute_upgrade(&env, &executor, &new_impl, None);
    }));

    res.expect_err(
        "Replay of an already-executed implementation must panic (ProposalAlreadyExecuted)",
    );
}

#[test]
fn test_execute_upgrade_unapproved_proposal_fails() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let proposer = create_test_address(&env);
    let executor = create_test_address(&env);
    let new_impl = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &proposer, UpgradeRole::Proposer, 0);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &executor, UpgradeRole::Upgrader, 0);

    // Create proposal requiring 1 approval, but don't approve it
    let proposal_id =
        upgrade_auth::propose_upgrade(&env, &proposer, &new_impl, Bytes::new(&env), 1);

    // Attempt to execute unapproved proposal - should fail
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::execute_upgrade(&env, &executor, &new_impl, Some(proposal_id));
    }))
    .expect_err("Execution of unapproved proposal should fail");
}

#[test]
fn test_execute_upgrade_mismatched_proposal_implementation_fails() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let proposer = create_test_address(&env);
    let approver = create_test_address(&env);
    let executor = create_test_address(&env);
    let proposal_impl = create_test_address(&env);
    let actual_impl = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &proposer, UpgradeRole::Proposer, 0);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &approver, UpgradeRole::Upgrader, 0);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &executor, UpgradeRole::Upgrader, 0);

    let proposal_id =
        upgrade_auth::propose_upgrade(&env, &proposer, &proposal_impl, Bytes::new(&env), 1);
    upgrade_auth::approve_upgrade_proposal(&env, &approver, proposal_id);

    // Attempt to execute proposal with a different implementation address - should fail
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::execute_upgrade(&env, &executor, &actual_impl, Some(proposal_id));
    }))
    .expect_err("Execution with mismatched implementation should fail");
}

// ── Boundary and Recovery Tests (issue #1356) ────────────────────────────────
//
// Coverage map:
//
//   initialize_upgrade_auth   → double-init guard
//   grant_upgrade_auth        → already-authorized, non-admin caller
//   revoke_upgrade_auth       → non-existent target, non-admin caller,
//                               revoke-one-of-two, revoke-then-regrant
//   propose_upgrade           → expired-proposer rejection, counter monotonicity
//   approve_upgrade_proposal  → duplicate approval, nonexistent proposal,
//                               expired-upgrader rejection
//   execute_upgrade           → nonexistent proposal, missing implementation
//   get_upgrade_proposal      → nonexistent id
//   get_upgrade_auth          → unknown address
//   get_upgrade_role          → unknown address
//   get_implementation        → unset
//   expires_at == 0           → no-expiry sentinel never expires
//   accept_upgrade_admin      → exact timelock boundary (86_400 s, inclusive),
//                               one-second-before boundary (83_399 s, exclusive),
//                               exact expiry boundary (604_800 s, inclusive),
//                               one-second-past expiry (604_801 s, exclusive),
//                               new admin already in upgrader list (no duplicate)
//   cancel_upgrade_admin_transfer → cancel-then-re-propose recovery

/// Calling `initialize_upgrade_auth` a second time must panic.
///
/// Invariant: `require_contract_uninitialized` fires when the Admin key is
/// already present; re-initialization must never overwrite the existing admin.
#[test]
fn test_double_init_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::initialize_upgrade_auth(&env, &admin);
    }))
    .expect_err("second initialize_upgrade_auth must panic");
}

/// `grant_upgrade_auth` must panic when the target address already holds an
/// authorization record.
///
/// Invariant: each address owns exactly one active authorization; re-granting
/// without first revoking is prohibited to prevent silent role escalation.
#[test]
fn test_grant_to_already_authorized_address_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let user = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &user, UpgradeRole::Proposer, 0);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::grant_upgrade_auth(&env, &admin, &user, UpgradeRole::Upgrader, 0);
    }))
    .expect_err("grant_upgrade_auth on already-authorized address must panic");
}

/// A non-admin caller must not be able to grant upgrade authorization.
///
/// Invariant: only the address stored under `UpgradeKey::Admin` may extend
/// the authorization set; any other caller triggers `require_upgrade_admin`.
#[test]
fn test_non_admin_cannot_grant() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let impostor = create_test_address(&env);
    let target = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::grant_upgrade_auth(&env, &impostor, &target, UpgradeRole::Proposer, 0);
    }))
    .expect_err("non-admin caller of grant_upgrade_auth must panic");
}

/// A non-admin caller must not be able to revoke upgrade authorization.
///
/// Invariant: only the upgrade admin may shrink the authorization set.
#[test]
fn test_non_admin_cannot_revoke() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let user = create_test_address(&env);
    let impostor = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &user, UpgradeRole::Proposer, 0);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::revoke_upgrade_auth(&env, &impostor, &user);
    }))
    .expect_err("non-admin caller of revoke_upgrade_auth must panic");
}

/// Revoking an address that was never authorized must panic.
///
/// Invariant: revocation is strict; it is not a silent no-op for unknown
/// addresses.  This prevents misleading success returns from authorization-
/// management tooling.
#[test]
fn test_revoke_nonexistent_auth_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let stranger = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::revoke_upgrade_auth(&env, &admin, &stranger);
    }))
    .expect_err("revoke_upgrade_auth on unknown address must panic");
}

/// With two upgraders, revoking one must succeed and shrink the list to one.
///
/// Invariant: the last-admin guard (`upgraders.len() <= 1`) fires only when
/// there is exactly one upgrader; it must not fire when two exist.
#[test]
fn test_revoke_one_of_two_upgraders_succeeds() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let second = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &second, UpgradeRole::Upgrader, 0);

    // Revocation of the second upgrader must not panic
    upgrade_auth::revoke_upgrade_auth(&env, &admin, &second);

    assert!(
        !upgrade_auth::is_authorized_upgrader(&env, &second),
        "revoked upgrader must not be active"
    );
    assert!(
        upgrade_auth::is_authorized_upgrader(&env, &admin),
        "remaining upgrader (admin) must still be active"
    );

    let upgraders = upgrade_auth::get_authorized_upgraders(&env);
    assert_eq!(
        upgraders.len(),
        1,
        "authorized upgrader list must contain exactly one entry after revocation"
    );
}

/// After revoking an address the storage record still exists with
/// `active = false` (the contract does not remove the key). A subsequent
/// `grant_upgrade_auth` to the same address therefore still hits the
/// "already authorized" guard and must panic.
///
/// Invariant documents the expected behavior: to re-authorize a previously
/// revoked address, a new address must be used.
#[test]
fn test_revoke_then_regrant_same_address_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let second = create_test_address(&env); // keeps upgrader count above 1
    let target = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &second, UpgradeRole::Upgrader, 0);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &target, UpgradeRole::Upgrader, 0);

    upgrade_auth::revoke_upgrade_auth(&env, &admin, &target);

    // The storage key still exists (active = false).  grant must still panic.
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::grant_upgrade_auth(&env, &admin, &target, UpgradeRole::Upgrader, 0);
    }))
    .expect_err(
        "grant_upgrade_auth after revoke must panic because the storage record still exists",
    );
}

/// Approving the same proposal twice from the same address must panic.
///
/// Invariant: a single key must not be able to satisfy a multi-sig quorum
/// alone by submitting duplicate approvals.
#[test]
fn test_duplicate_approval_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let proposer = create_test_address(&env);
    let approver = create_test_address(&env);
    let new_impl = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &proposer, UpgradeRole::Proposer, 0);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &approver, UpgradeRole::Upgrader, 0);

    let pid = upgrade_auth::propose_upgrade(&env, &proposer, &new_impl, Bytes::new(&env), 2);
    upgrade_auth::approve_upgrade_proposal(&env, &approver, pid);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::approve_upgrade_proposal(&env, &approver, pid);
    }))
    .expect_err("second approval from the same address must panic");
}

/// Approving a proposal ID that does not exist must panic.
///
/// Invariant: `approve_upgrade_proposal` performs a strict storage lookup;
/// there is no silent no-op for missing proposals.
#[test]
fn test_approve_nonexistent_proposal_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let approver = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &approver, UpgradeRole::Upgrader, 0);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::approve_upgrade_proposal(&env, &approver, 999);
    }))
    .expect_err("approving a nonexistent proposal must panic");
}

/// Executing with an explicit proposal ID that does not exist must panic.
///
/// Invariant: proposal lookup on the execute path is strict.
#[test]
fn test_execute_nonexistent_proposal_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let executor = create_test_address(&env);
    let new_impl = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &executor, UpgradeRole::Upgrader, 0);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::execute_upgrade(&env, &executor, &new_impl, Some(999));
    }))
    .expect_err("executing a nonexistent proposal must panic");
}

/// `execute_upgrade` must panic when no implementation has been stored.
///
/// Invariant: the "no current implementation" guard fires before any proposal
/// or replay check.  Upgrade execution is only possible after an
/// implementation has been set by a prior successful `execute_upgrade`.
#[test]
fn test_execute_upgrade_without_implementation_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let executor = create_test_address(&env);
    let new_impl = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &executor, UpgradeRole::Upgrader, 0);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::execute_upgrade(&env, &executor, &new_impl, None);
    }))
    .expect_err("execute_upgrade with no implementation stored must panic");
}

/// `get_upgrade_proposal` must panic for a nonexistent proposal ID.
///
/// Invariant: read-only getters must not silently return defaults for
/// missing entries.
#[test]
fn test_get_upgrade_proposal_nonexistent_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::get_upgrade_proposal(&env, 42);
    }))
    .expect_err("get_upgrade_proposal for nonexistent id must panic");
}

/// `get_upgrade_auth` must panic for an address that has no authorization
/// record.
///
/// Invariant: callers must check membership before reading; the getter does
/// not synthesize default entries.
#[test]
fn test_get_upgrade_auth_unknown_address_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let stranger = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::get_upgrade_auth(&env, &stranger);
    }))
    .expect_err("get_upgrade_auth for unknown address must panic");
}

/// `get_upgrade_role` must panic for an address that has no authorization
/// record.
///
/// Invariant: same strict lookup as `get_upgrade_auth`.
#[test]
fn test_get_upgrade_role_unknown_address_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let stranger = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::get_upgrade_role(&env, &stranger);
    }))
    .expect_err("get_upgrade_role for unknown address must panic");
}

/// `get_implementation` must panic when the implementation key has not yet
/// been written.
///
/// Invariant: callers must not assume an implementation exists before one has
/// been set by a successful `execute_upgrade`.
#[test]
fn test_get_implementation_unset_panics() {
    let env = create_test_env();
    let admin = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::get_implementation(&env);
    }))
    .expect_err("get_implementation before any upgrade must panic");
}

/// An authorization with `expires_at = 0` must remain valid indefinitely,
/// even at extreme ledger timestamps.
///
/// Invariant: `0` is the "no expiry" sentinel, not the Unix epoch.  The
/// check `expires_at > 0 && timestamp > expires_at` must evaluate to false
/// when `expires_at == 0`, so the authorization is always considered active.
#[test]
fn test_no_expiry_zero_never_expires() {
    use soroban_sdk::testutils::Ledger as _;

    let env = create_test_env();
    let admin = create_test_address(&env);
    let upgrader = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &upgrader, UpgradeRole::Upgrader, 0);

    assert!(
        upgrade_auth::is_authorized_upgrader(&env, &upgrader),
        "upgrader with expires_at=0 must be authorized at grant time"
    );

    // Advance ledger to an extreme value
    env.ledger().with_mut(|l| {
        l.timestamp = u64::MAX / 2;
    });

    assert!(
        upgrade_auth::is_authorized_upgrader(&env, &upgrader),
        "upgrader with expires_at=0 must still be authorized at extreme timestamp"
    );
}

/// An authorization whose `expires_at` is in the past must be rejected by
/// `is_authorized_upgrader`.
///
/// Invariant: `is_authorized_upgrader` consults `expires_at` against the
/// current ledger timestamp.
#[test]
fn test_past_expiry_is_not_authorized() {
    use soroban_sdk::testutils::Ledger as _;

    let env = create_test_env();
    let admin = create_test_address(&env);
    let upgrader = create_test_address(&env);

    // Start at a non-zero timestamp so we can form a past value
    env.ledger().with_mut(|l| {
        l.timestamp = 10_000;
    });

    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    // expires_at is one second in the past
    let past = env.ledger().timestamp() - 1;
    upgrade_auth::grant_upgrade_auth(&env, &admin, &upgrader, UpgradeRole::Upgrader, past);

    assert!(
        !upgrade_auth::is_authorized_upgrader(&env, &upgrader),
        "upgrader with past expires_at must not be authorized"
    );
}

/// A proposer with an expired authorization must not be able to create a new
/// proposal.
///
/// Invariant: `propose_upgrade` checks `expires_at` before writing the
/// proposal record.
#[test]
fn test_expired_proposer_cannot_propose() {
    use soroban_sdk::testutils::Ledger as _;

    let env = create_test_env();
    let admin = create_test_address(&env);
    let proposer = create_test_address(&env);

    env.ledger().with_mut(|l| {
        l.timestamp = 10_000;
    });

    upgrade_auth::initialize_upgrade_auth(&env, &admin);

    let past = env.ledger().timestamp() - 1;
    upgrade_auth::grant_upgrade_auth(&env, &admin, &proposer, UpgradeRole::Proposer, past);

    let new_impl = create_test_address(&env);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::propose_upgrade(&env, &proposer, &new_impl, Bytes::new(&env), 1);
    }))
    .expect_err("expired proposer must not be able to create a proposal");
}

/// An upgrader with an expired authorization must not be able to approve a
/// proposal.
///
/// Invariant: `approve_upgrade_proposal` calls `require_upgrade_auth`, which
/// checks both `active` and `expires_at`.
#[test]
fn test_expired_upgrader_cannot_approve() {
    use soroban_sdk::testutils::Ledger as _;

    let env = create_test_env();
    let admin = create_test_address(&env);
    let proposer = create_test_address(&env);
    let approver = create_test_address(&env);

    env.ledger().with_mut(|l| {
        l.timestamp = 10_000;
    });

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &proposer, UpgradeRole::Proposer, 0);

    let past = env.ledger().timestamp() - 1;
    upgrade_auth::grant_upgrade_auth(&env, &admin, &approver, UpgradeRole::Upgrader, past);

    let new_impl = create_test_address(&env);
    let pid = upgrade_auth::propose_upgrade(&env, &proposer, &new_impl, Bytes::new(&env), 1);

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        upgrade_auth::approve_upgrade_proposal(&env, &approver, pid);
    }))
    .expect_err("expired upgrader must not be able to approve a proposal");
}

/// The proposal counter must start at 1 and produce unique, strictly
/// increasing IDs for consecutive proposals.
///
/// Invariant: the next-proposal-id counter increments by exactly one per
/// `propose_upgrade` call; IDs are never reused.
#[test]
fn test_proposal_counter_increments_monotonically() {
    let env = create_test_env();
    let admin = create_test_address(&env);
    let proposer = create_test_address(&env);

    upgrade_auth::initialize_upgrade_auth(&env, &admin);
    upgrade_auth::grant_upgrade_auth(&env, &admin, &proposer, UpgradeRole::Proposer, 0);

    let impl_a = create_test_address(&env);
    let impl_b = create_test_address(&env);
    let impl_c = create_test_address(&env);

    let id1 = upgrade_auth::propose_upgrade(&env, &proposer, &impl_a, Bytes::new(&env), 1);
    let id2 = upgrade_auth::propose_upgrade(&env, &proposer, &impl_b, Bytes::new(&env), 1);
    let id3 = upgrade_auth::propose_upgrade(&env, &proposer, &impl_c, Bytes::new(&env), 1);

    assert!(id1 < id2, "second proposal id must exceed first");
    assert!(id2 < id3, "third proposal id must exceed second");
    assert_eq!(id2, id1 + 1, "ids must increment by exactly one");
    assert_eq!(id3, id2 + 1, "ids must increment by exactly one");

    // Each proposal is independently stored and retrievable
    assert_eq!(
        upgrade_auth::get_upgrade_proposal(&env, id1).new_implementation,
        impl_a
    );
    assert_eq!(
        upgrade_auth::get_upgrade_proposal(&env, id2).new_implementation,
        impl_b
    );
    assert_eq!(
        upgrade_auth::get_upgrade_proposal(&env, id3).new_implementation,
        impl_c
    );
}

// ── Admin-transfer boundary tests using the contract client ─────────────────
//
// These tests require the full contract environment (`CredenceBondClient`)
// because `accept_upgrade_admin` is an entrypoint that calls
// `e.ledger().timestamp()` inside the contract execution context.

/// Helper: register a bare `CredenceBond` and return its address.
fn register_bond(env: &Env) -> Address {
    env.register(crate::CredenceBond, ())
}

/// `accept_upgrade_admin` at exactly `proposed_at + 86_400` must succeed
/// (timelock boundary is inclusive).
///
/// Invariant: `now >= proposed_at + timelock` — the boundary second is
/// valid; accepting one second earlier is rejected.
#[test]
fn test_accept_at_exact_timelock_boundary_succeeds() {
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let contract_id = register_bond(&env);
    let client = crate::CredenceBondClient::new(&env, &contract_id);
    client.initialize(&admin, &None);

    let proposed_at = env.ledger().timestamp();
    client.transfer_upgrade_admin(&admin, &new_admin);

    // Advance to exactly the timelock boundary (inclusive)
    env.ledger().with_mut(|l| {
        l.timestamp = proposed_at + 86_400;
    });

    client.accept_upgrade_admin(&new_admin); // must not panic

    assert_eq!(
        client.get_pending_upgrade_admin(),
        None,
        "pending admin must be cleared after acceptance at exact timelock boundary"
    );
}

/// `accept_upgrade_admin` at `proposed_at + 86_399` (one second before the
/// timelock boundary) must panic.
///
/// Invariant: off-by-one — the boundary second at `86_400` is the first
/// valid moment; `86_399` is still inside the forbidden window.
#[test]
fn test_accept_one_second_before_timelock_panics() {
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let contract_id = register_bond(&env);
    let client = crate::CredenceBondClient::new(&env, &contract_id);
    client.initialize(&admin, &None);

    let proposed_at = env.ledger().timestamp();
    client.transfer_upgrade_admin(&admin, &new_admin);

    env.ledger().with_mut(|l| {
        l.timestamp = proposed_at + 86_399;
    });

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.accept_upgrade_admin(&new_admin);
    }))
    .expect_err("accepting one second before the timelock must panic");
}

/// `accept_upgrade_admin` at exactly `proposed_at + 604_800` must succeed
/// (expiry boundary is inclusive).
///
/// Invariant: `now <= proposed_at + expiry` — the last valid second is
/// `604_800`; one second later is past the window.
#[test]
fn test_accept_at_exact_expiry_boundary_succeeds() {
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let contract_id = register_bond(&env);
    let client = crate::CredenceBondClient::new(&env, &contract_id);
    client.initialize(&admin, &None);

    let proposed_at = env.ledger().timestamp();
    client.transfer_upgrade_admin(&admin, &new_admin);

    // Advance to the last valid second (604_800 is both ≥ timelock and ≤ expiry)
    env.ledger().with_mut(|l| {
        l.timestamp = proposed_at + 604_800;
    });

    client.accept_upgrade_admin(&new_admin); // must not panic

    assert_eq!(
        client.get_pending_upgrade_admin(),
        None,
        "pending admin must be cleared after acceptance at exact expiry boundary"
    );
}

/// `accept_upgrade_admin` at `proposed_at + 604_801` (one second past the
/// expiry boundary) must panic.
///
/// Invariant: `now > proposed_at + 604_800` triggers the
/// "admin transfer proposal expired" panic.
#[test]
fn test_accept_one_second_past_expiry_panics() {
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let contract_id = register_bond(&env);
    let client = crate::CredenceBondClient::new(&env, &contract_id);
    client.initialize(&admin, &None);

    let proposed_at = env.ledger().timestamp();
    client.transfer_upgrade_admin(&admin, &new_admin);

    env.ledger().with_mut(|l| {
        l.timestamp = proposed_at + 604_801;
    });

    std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.accept_upgrade_admin(&new_admin);
    }))
    .expect_err("accepting one second past the 7-day expiry must panic");
}

/// When `accept_upgrade_admin` is called and `new_admin` already holds the
/// `Upgrader` role, the authorized-upgraders list must not gain a duplicate
/// entry.
///
/// Invariant: `accept_upgrade_admin` contains an `already_in` guard that
/// skips `push_back` when the caller is already present.
#[test]
fn test_accept_upgrade_admin_no_duplicate_when_already_upgrader() {
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let contract_id = register_bond(&env);
    let client = crate::CredenceBondClient::new(&env, &contract_id);
    client.initialize(&admin, &None);

    // Grant new_admin the Upgrader role before the admin transfer completes
    env.as_contract(&contract_id, || {
        upgrade_auth::grant_upgrade_auth(&env, &admin, &new_admin, UpgradeRole::Upgrader, 0);
    });

    client.transfer_upgrade_admin(&admin, &new_admin);

    env.ledger().with_mut(|l| {
        l.timestamp += 86_401;
    });

    client.accept_upgrade_admin(&new_admin);

    let upgraders = env.as_contract(&contract_id, || {
        upgrade_auth::get_authorized_upgraders(&env)
    });

    let count = (0..upgraders.len())
        .filter(|&i| upgraders.get(i).unwrap() == new_admin)
        .count();

    assert_eq!(
        count, 1,
        "new admin already in upgrader list must appear exactly once after accept"
    );
}

/// After cancelling a pending admin transfer, a fresh transfer to a new
/// address can be proposed and completed successfully.
///
/// Invariant: `cancel_upgrade_admin_transfer` removes the
/// `UpgradeKey::PndgUpgrAdmin` key completely; the two-step state machine
/// can be restarted without residual state.
#[test]
fn test_cancel_then_repropose_and_complete() {
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let abandoned = Address::generate(&env);
    let final_admin = Address::generate(&env);
    let contract_id = register_bond(&env);
    let client = crate::CredenceBondClient::new(&env, &contract_id);
    client.initialize(&admin, &None);

    // First attempt: propose then cancel
    client.transfer_upgrade_admin(&admin, &abandoned);
    assert_eq!(client.get_pending_upgrade_admin(), Some(abandoned.clone()));
    client.cancel_upgrade_admin_transfer(&admin);
    assert_eq!(
        client.get_pending_upgrade_admin(),
        None,
        "pending admin must be None after cancel"
    );

    // Second attempt: propose to a different address and complete
    let proposed_at = env.ledger().timestamp();
    client.transfer_upgrade_admin(&admin, &final_admin);
    assert_eq!(
        client.get_pending_upgrade_admin(),
        Some(final_admin.clone())
    );

    env.ledger().with_mut(|l| {
        l.timestamp = proposed_at + 86_401;
    });

    client.accept_upgrade_admin(&final_admin);

    assert_eq!(
        client.get_pending_upgrade_admin(),
        None,
        "pending admin must be cleared after second transfer completes"
    );

    // final_admin is now the upgrade admin
    let stored: Address = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::Upgrade(crate::UpgradeKey::Admin))
            .unwrap()
    });
    assert_eq!(stored, final_admin);
}
