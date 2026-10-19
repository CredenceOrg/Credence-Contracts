//! Concurrency / race-safety regression tests for the Admin contract
//! (issue #1297).
//!
//! These tests lock the serialization contract at the actual integration
//! boundary (the generated contract client):
//!
//! * every committed privileged mutation advances the monotonic
//!   [`AdminContract::get_config_epoch`] exactly once, so concurrent clients
//!   can detect conflicts and retry;
//! * rejected, stale, repeated, and failed operations advance nothing and
//!   leave no partial state behind;
//! * repeated no-op operations (same-role update, duplicate pause/unpause,
//!   duplicate proposal approval, stale proposal execution) are idempotent;
//! * conflicting multi-step flows (e.g. two ownership transfers) serialize by
//!   last-writer-wins and the loser can never complete the flow.

#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

fn setup() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    e.mock_all_auths();
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, super_admin)
}

/// Every committed privileged mutation advances the epoch exactly once.
#[test]
fn config_epoch_advances_exactly_once_per_committed_mutation() {
    let (e, client, admin) = setup();
    assert_eq!(client.get_config_epoch(), 0);

    let new_admin = Address::generate(&e);
    client.add_admin(&admin, &new_admin, &AdminRole::Admin);
    assert_eq!(client.get_config_epoch(), 1);

    client.update_admin_role(&admin, &new_admin, &AdminRole::Operator);
    assert_eq!(client.get_config_epoch(), 2);

    client.deactivate_admin(&admin, &new_admin);
    assert_eq!(client.get_config_epoch(), 3);

    client.reactivate_admin(&admin, &new_admin);
    assert_eq!(client.get_config_epoch(), 4);

    let now = e.ledger().timestamp();
    client.suspend_admin(&admin, &new_admin, &(now + 1_000));
    assert_eq!(client.get_config_epoch(), 5);

    client.remove_admin(&admin, &new_admin);
    assert_eq!(client.get_config_epoch(), 6);

    let super2 = Address::generate(&e);
    client.add_admin(&admin, &super2, &AdminRole::SuperAdmin);
    assert_eq!(client.get_config_epoch(), 7);

    client.transfer_ownership(&admin, &super2);
    assert_eq!(client.get_config_epoch(), 8);

    e.ledger()
        .with_mut(|l| l.timestamp += crate::OWNERSHIP_TRANSFER_TIMELOCK + 1);
    client.accept_ownership(&super2);
    assert_eq!(client.get_config_epoch(), 9);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&admin, &s1, &true);
    assert_eq!(client.get_config_epoch(), 10);
    client.set_pause_signer(&admin, &s2, &true);
    assert_eq!(client.get_config_epoch(), 11);
    client.set_pause_threshold(&admin, &2u32);
    assert_eq!(client.get_config_epoch(), 12);

    let id = client.pause(&s1).unwrap();
    assert_eq!(client.get_config_epoch(), 13);
    client.approve_pause_proposal(&s2, &id);
    assert_eq!(client.get_config_epoch(), 14);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), 15);

    let unpause_id = client.unpause(&s1).unwrap();
    assert_eq!(client.get_config_epoch(), 16);
    client.approve_pause_proposal(&s2, &unpause_id);
    assert_eq!(client.get_config_epoch(), 17);
    client.execute_pause_proposal(&unpause_id);
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), 18);
}

/// A mutation rejected at the authorization boundary leaves no state and no
/// epoch advance.
#[test]
fn unauthorized_mutation_leaves_no_state_and_no_epoch_bump() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    use soroban_sdk::IntoVal;

    let e = Env::default();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    let authorized_target = Address::generate(&e);
    let attempted_target = Address::generate(&e);

    e.mock_auths(&[MockAuth {
        address: &super_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (super_admin.clone(), 1u32, 100u32).into_val(&e),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&super_admin, &1u32, &100u32);
    assert_eq!(client.get_config_epoch(), 0);

    // Authorize `add_admin` for a *different* target than the one attempted.
    e.mock_auths(&[MockAuth {
        address: &super_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "add_admin",
            args: (
                super_admin.clone(),
                authorized_target.clone(),
                AdminRole::Admin,
            )
                .into_val(&e),
            sub_invokes: &[],
        },
    }]);

    let res = client.try_add_admin(&super_admin, &attempted_target, &AdminRole::Admin);
    assert!(res.is_err(), "mismatched auth must reject the mutation");
    assert_eq!(client.get_config_epoch(), 0);
    assert_eq!(client.get_admin_count(), 1);
    assert_eq!(client.get_all_admins().len(), 1);
}

/// A failed pause execution leaves the proposal live, usable, and the epoch
/// unchanged — no partial state is committed.
#[test]
fn insufficient_approvals_leave_proposal_intact() {
    let (e, client, admin) = setup();
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    let id = client.pause(&s1).unwrap();
    let epoch_after_propose = client.get_config_epoch();

    // Not enough approvals yet: the execute must fail without touching state.
    assert!(client.try_execute_pause_proposal(&id).is_err());
    assert!(!client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_after_propose);

    // The proposal is still live and can complete normally afterwards.
    client.approve_pause_proposal(&s2, &id);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), epoch_after_propose + 2);

    // A second execution is rejected (proposal consumed) without a bump.
    assert!(client.try_execute_pause_proposal(&id).is_err());
    assert_eq!(client.get_config_epoch(), epoch_after_propose + 2);
}

/// Repeated operations that would not change state are idempotent no-ops.
#[test]
fn repeated_operations_are_idempotent() {
    let (e, client, admin) = setup();

    // Direct pause/unpause toggles.
    client.pause(&admin);
    assert_eq!(client.get_config_epoch(), 1);
    client.pause(&admin); // already paused — no-op
    assert_eq!(client.get_config_epoch(), 1);
    client.unpause(&admin);
    assert_eq!(client.get_config_epoch(), 2);
    client.unpause(&admin); // already unpaused — no-op
    assert_eq!(client.get_config_epoch(), 2);

    // Same-role update is a no-op: no epoch bump, no role-list churn.
    let new_admin = Address::generate(&e);
    client.add_admin(&admin, &new_admin, &AdminRole::Admin);
    assert_eq!(client.get_config_epoch(), 3);
    let info = client.update_admin_role(&admin, &new_admin, &AdminRole::Admin);
    assert_eq!(info.role, AdminRole::Admin);
    assert_eq!(client.get_config_epoch(), 3);
    let role_admins = client.get_admins_by_role(&AdminRole::Admin);
    assert_eq!(role_admins.len(), 1);
    let only = role_admins.get(0).unwrap();
    assert_eq!(only, new_admin);

    // Duplicate proposal approval and repeated propose are no-ops.
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);
    let id = client.pause(&s1).unwrap();
    assert_eq!(client.get_config_epoch(), 7);
    client.approve_pause_proposal(&s2, &id);
    assert_eq!(client.get_config_epoch(), 8);
    client.approve_pause_proposal(&s2, &id); // duplicate approval — no-op
    client.pause(&s1); // already proposed and approved by s1 — no-op
    assert_eq!(client.get_config_epoch(), 8);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
    assert_eq!(client.get_config_epoch(), 9);
}

/// Two conflicting mutations are serialized by the ledger; a stale client can
/// detect the conflict through the epoch and retry against fresh state.
#[test]
fn concurrent_conflicts_are_detectable_and_retryable() {
    let (e, client, admin) = setup();

    // Client A snapshots governance state.
    let epoch_before = client.get_config_epoch();
    let admins_before = client.get_all_admins();
    assert_eq!(epoch_before, 0);
    assert_eq!(admins_before.len(), 1);

    // Client B commits a conflicting privileged mutation.
    let new_admin = Address::generate(&e);
    client.add_admin(&admin, &new_admin, &AdminRole::Admin);

    // Client A's snapshot is stale: the epoch moved and the state changed.
    assert_ne!(client.get_config_epoch(), epoch_before);
    assert_eq!(client.get_config_epoch(), 1);

    // Retry contract: re-read the state and proceed against the latest view.
    let admins_after = client.get_all_admins();
    assert_eq!(admins_after.len(), 2);
    assert_eq!(client.get_admin_count(), 2);
}

/// Ownership transfers serialize last-writer-wins: the superseded pending
/// owner can never accept, and the winning proposal completes normally.
#[test]
fn stale_ownership_transfer_is_last_writer_wins() {
    let (e, client, admin) = setup();
    let super2 = Address::generate(&e);
    let super3 = Address::generate(&e);
    client.add_admin(&admin, &super2, &AdminRole::SuperAdmin);
    client.add_admin(&admin, &super3, &AdminRole::SuperAdmin);

    client.transfer_ownership(&admin, &super2);
    client.transfer_ownership(&admin, &super3); // supersedes the first
    assert_eq!(client.get_pending_owner(), Some(super3.clone()));
    assert_eq!(client.get_config_epoch(), 4);

    // The superseded pending owner can never complete the takeover.
    assert!(client.try_accept_ownership(&super2).is_err());
    assert_eq!(client.get_owner(), admin);
    assert_eq!(client.get_config_epoch(), 4);

    e.ledger()
        .with_mut(|l| l.timestamp += crate::OWNERSHIP_TRANSFER_TIMELOCK + 1);
    client.accept_ownership(&super3);
    assert_eq!(client.get_owner(), super3);
    assert_eq!(client.get_pending_owner(), None);
    assert_eq!(client.get_config_epoch(), 5);
}

/// Paused-contract rejections leave state and epoch untouched.
#[test]
fn paused_rejections_leave_no_partial_state() {
    let (e, client, admin) = setup();
    client.pause(&admin);
    let epoch_paused = client.get_config_epoch();

    let new_admin = Address::generate(&e);
    assert!(client
        .try_add_admin(&admin, &new_admin, &AdminRole::Admin)
        .is_err());
    assert_eq!(client.get_config_epoch(), epoch_paused);
    assert_eq!(client.get_admin_count(), 1);
}
