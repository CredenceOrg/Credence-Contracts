use crate::*;
use soroban_sdk::{Address, Env};

#[cfg(test)]
mod ownership_transfer_tests {
    use super::*;
    use crate::AdminContractClient;
    use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};

    fn create_contract() -> AdminContract {
        AdminContract {}
    }

    fn setup_contract(env: &Env) -> (Address, Address) {
        let contract = create_contract();
        let super_admin = Address::generate(env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);
        });

        (contract_address, super_admin)
    }

    fn setup_multiple_super_admins(env: &Env) -> (Address, Address, Address) {
        let contract = create_contract();
        let super_admin_1 = Address::generate(env);
        let super_admin_2 = Address::generate(env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin_1.clone(), 1, 100);
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
                AdminRole::SuperAdmin,
            );
        });

        (contract_address, super_admin_1, super_admin_2)
    }

    #[test]
    fn test_get_owner_after_initialization() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        let owner = env.as_contract(&contract_address, || AdminContract::get_owner(env.clone()));

        assert_eq!(owner, super_admin);
    }

    #[test]
    fn test_get_pending_owner_returns_none_initially() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup_contract(&env);

        let pending_owner = env.as_contract(&contract_address, || {
            AdminContract::get_pending_owner(env.clone())
        });

        assert_eq!(pending_owner, None);
    }

    #[test]
    fn test_transfer_ownership_sets_pending_owner() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        let pending_owner = env.as_contract(&contract_address, || {
            AdminContract::get_pending_owner(env.clone())
        });

        assert_eq!(pending_owner, Some(super_admin_2));
    }

    #[test]
    fn test_ownership_remains_with_current_owner_before_accept() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        let owner = env.as_contract(&contract_address, || AdminContract::get_owner(env.clone()));

        // Owner should still be super_admin_1 until accept_ownership is called
        assert_eq!(owner, super_admin_1);
    }

    #[test]
    fn test_accept_ownership_transfers_control() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        // Advance past the timelock delay
        env.ledger().with_mut(|li| {
            li.timestamp += crate::OWNERSHIP_TRANSFER_TIMELOCK;
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::accept_ownership(env.clone(), super_admin_2.clone());
        });

        let owner = env.as_contract(&contract_address, || AdminContract::get_owner(env.clone()));

        assert_eq!(owner, super_admin_2);
    }

    #[test]
    fn test_pending_owner_cleared_after_acceptance() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        // Advance past the timelock delay
        env.ledger().with_mut(|li| {
            li.timestamp += crate::OWNERSHIP_TRANSFER_TIMELOCK;
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::accept_ownership(env.clone(), super_admin_2.clone());
        });

        let pending_owner = env.as_contract(&contract_address, || {
            AdminContract::get_pending_owner(env.clone())
        });

        assert_eq!(pending_owner, None);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_transfer_ownership_rejects_non_owner() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);

        let unauthorized_address = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                unauthorized_address.clone(),
                AdminRole::SuperAdmin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                unauthorized_address.clone(),
                super_admin_2.clone(),
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_accept_ownership_rejects_non_pending_owner() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin_1 = Address::generate(&env);
        let super_admin_2 = Address::generate(&env);
        let unauthorized_address = Address::generate(&env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin_1.clone(), 1, 100);
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
                AdminRole::SuperAdmin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                unauthorized_address.clone(),
                AdminRole::SuperAdmin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            // Try to accept as unauthorized address instead of pending owner
            AdminContract::accept_ownership(env.clone(), unauthorized_address.clone());
        });
    }

    #[test]
    // `AdminUnchanged` (111): proposing the current owner as their own
    // successor is rejected. The previous expectation of #107 no longer
    // matches the wire-stable error table.
    #[should_panic(expected = "Error(Contract, #111)")]
    fn test_transfer_ownership_rejects_same_owner() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin.clone(),
                super_admin.clone(),
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_transfer_ownership_rejects_non_admin() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let non_admin = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(env.clone(), super_admin.clone(), non_admin.clone());
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_transfer_ownership_rejects_non_super_admin() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin = Address::generate(&env);
        let regular_admin = Address::generate(&env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                regular_admin.clone(),
                AdminRole::Admin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin.clone(),
                regular_admin.clone(),
            );
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #404)")]
    fn test_transfer_ownership_rejects_inactive_admin() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);

        env.as_contract(&contract_address, || {
            // Ownership transfer requires the target to be a SuperAdmin, but peer
            // SuperAdmins cannot deactivate each other through the public API.
            // Set the target inactive directly so this test exercises the
            // transfer guard rather than the deactivation permission check.
            let mut admin_info: AdminInfo = env
                .storage()
                .instance()
                .get(&DataKey::AdminInfo(super_admin_2.clone()))
                .unwrap();
            admin_info.active = false;
            env.storage()
                .instance()
                .set(&DataKey::AdminInfo(super_admin_2.clone()), &admin_info);
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            // Try to transfer ownership to inactive admin
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });
    }

    #[test]
    // `NoPendingAdmin` (115): there is no proposal to consume. The previous
    // expectation of #109 no longer matches the wire-stable error table.
    #[should_panic(expected = "Error(Contract, #115)")]
    fn test_accept_ownership_rejects_when_no_pending_owner() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);

        env.as_contract(&contract_address, || {
            // Try to accept when no transfer was initiated
            AdminContract::accept_ownership(env.clone(), super_admin.clone());
        });
    }

    #[test]
    fn test_ownership_transfer_overwrite_behavior() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin_1 = Address::generate(&env);
        let super_admin_2 = Address::generate(&env);
        let super_admin_3 = Address::generate(&env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin_1.clone(), 1, 100);
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
                AdminRole::SuperAdmin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_3.clone(),
                AdminRole::SuperAdmin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            // Initiate first transfer to super_admin_2
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        let pending_owner = env.as_contract(&contract_address, || {
            AdminContract::get_pending_owner(env.clone())
        });
        assert_eq!(pending_owner, Some(super_admin_2.clone()));

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            // Overwrite with transfer to super_admin_3
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_3.clone(),
            );
        });

        let new_pending_owner = env.as_contract(&contract_address, || {
            AdminContract::get_pending_owner(env.clone())
        });
        assert_eq!(new_pending_owner, Some(super_admin_3.clone()));

        // Advance past the timelock delay
        env.ledger().with_mut(|li| {
            li.timestamp += crate::OWNERSHIP_TRANSFER_TIMELOCK;
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            // Accept the latest transfer
            AdminContract::accept_ownership(env.clone(), super_admin_3.clone());
        });

        let final_owner =
            env.as_contract(&contract_address, || AdminContract::get_owner(env.clone()));

        assert_eq!(final_owner, super_admin_3);
    }

    #[test]
    fn test_new_owner_can_initiate_next_transfer() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin_1 = Address::generate(&env);
        let super_admin_2 = Address::generate(&env);
        let super_admin_3 = Address::generate(&env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin_1.clone(), 1, 100);
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
                AdminRole::SuperAdmin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_3.clone(),
                AdminRole::SuperAdmin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            // Transfer from super_admin_1 to super_admin_2
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        // Advance past the timelock delay
        env.ledger().with_mut(|li| {
            li.timestamp += crate::OWNERSHIP_TRANSFER_TIMELOCK;
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::accept_ownership(env.clone(), super_admin_2.clone());
        });

        let owner = env.as_contract(&contract_address, || AdminContract::get_owner(env.clone()));
        assert_eq!(owner, super_admin_2);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            // super_admin_2 transfers to super_admin_3
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_2.clone(),
                super_admin_3.clone(),
            );
        });

        // Advance past the timelock delay again
        env.ledger().with_mut(|li| {
            li.timestamp += crate::OWNERSHIP_TRANSFER_TIMELOCK;
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::accept_ownership(env.clone(), super_admin_3.clone());
        });

        let final_owner =
            env.as_contract(&contract_address, || AdminContract::get_owner(env.clone()));

        assert_eq!(final_owner, super_admin_3);
    }

    /// Negative test: `accept_ownership` must reject before the timelock has elapsed.
    ///
    /// Threat modelled: If the current owner's key is compromised, an attacker can
    /// call `transfer_ownership` to propose themselves as the new owner. Without a
    /// timelock they can immediately call `accept_ownership` and seize control.
    /// With the timelock, the legitimate owner has a 24-hour window to detect the
    /// pending transfer and take corrective action (e.g. rotating credentials or
    /// proposing a different owner).
    #[test]
    #[should_panic(expected = "Error(Contract, #112)")]
    fn test_accept_ownership_rejects_before_timelock_elapses() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        // Advance only *partially* past the timelock — still not enough
        env.ledger().with_mut(|li| {
            li.timestamp += crate::OWNERSHIP_TRANSFER_TIMELOCK - 1;
        });

        env.mock_all_auths();
        // This must panic with TimelockNotReady (#112)
        env.as_contract(&contract_address, || {
            AdminContract::accept_ownership(env.clone(), super_admin_2.clone());
        });
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Adversarial regression: stale ownership proposals.
    //
    // A transfer proposal is only an *intent*. The timelock window exists
    // precisely so the candidate's authority can change before acceptance.
    // `accept_ownership` therefore revalidates the pending candidate against
    // current state (active, unsuspended, SuperAdmin) immediately before the
    // first ownership write. These cases pin that invariant, the timelock
    // boundary, replay rejection, and the atomic-rollback guarantee that
    // backs it: a rejected acceptance changes nothing observable.
    // ═══════════════════════════════════════════════════════════════════════

    fn advance_ledger(env: &Env, seconds: u64) {
        env.ledger().with_mut(|li| li.timestamp += seconds);
    }

    /// Fault-inject the `active` flag. Peer SuperAdmins cannot deactivate each
    /// other through the public API, so a future recovery path is modelled
    /// directly at the storage layer (mirrors `test_transfer_ownership_rejects_inactive_admin`).
    fn set_admin_active(env: &Env, contract_address: &Address, admin: &Address, active: bool) {
        env.as_contract(contract_address, || {
            let mut info: AdminInfo = env
                .storage()
                .instance()
                .get(&DataKey::AdminInfo(admin.clone()))
                .unwrap();
            info.active = active;
            env.storage()
                .instance()
                .set(&DataKey::AdminInfo(admin.clone()), &info);
        });
    }

    /// Remove an admin record entirely, modelling a candidate whose authority
    /// is revoked during the timelock by a future governance path.
    fn remove_admin_record(env: &Env, contract_address: &Address, admin: &Address) {
        env.as_contract(contract_address, || {
            env.storage()
                .instance()
                .remove(&DataKey::AdminInfo(admin.clone()));
        });
    }

    fn setup_three_super_admins(env: &Env) -> (Address, Address, Address, Address) {
        let super_admin_1 = Address::generate(env);
        let super_admin_2 = Address::generate(env);
        let super_admin_3 = Address::generate(env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin_1.clone(), 1, 100);
        });
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
                AdminRole::SuperAdmin,
            );
        });
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_3.clone(),
                AdminRole::SuperAdmin,
            );
        });

        (
            contract_address,
            super_admin_1,
            super_admin_2,
            super_admin_3,
        )
    }

    /// A candidate demoted from SuperAdmin during the timelock must not be able
    /// to accept ownership. Demotion is reachable through the public
    /// `update_admin_role` entrypoint, so no fault injection is required.
    #[test]
    fn test_accept_ownership_rejects_candidate_demoted_during_timelock() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);
        let client = AdminContractClient::new(&env, &contract_address);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::update_admin_role(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
                AdminRole::Admin,
            );
        });

        let epoch_before = client.get_config_epoch();
        let events_before = env.events().all().len();

        let res = client.try_accept_ownership(&super_admin_2);
        assert!(res.is_err(), "demoted candidate must not receive ownership");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(100) // NotAdmin
        );

        // Rejection is atomic: the proposal is intact and the epoch is unmoved.
        assert_eq!(client.get_owner(), super_admin_1);
        assert_eq!(client.get_pending_owner(), Some(super_admin_2.clone()));
        assert_eq!(client.get_config_epoch(), epoch_before);
        assert_eq!(env.events().all().len(), events_before);
    }

    /// A candidate deactivated during the timelock must not be able to accept.
    #[test]
    fn test_accept_ownership_rejects_candidate_deactivated_during_timelock() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);
        let client = AdminContractClient::new(&env, &contract_address);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);
        set_admin_active(&env, &contract_address, &super_admin_2, false);

        let epoch_before = client.get_config_epoch();
        let events_before = env.events().all().len();

        let res = client.try_accept_ownership(&super_admin_2);
        assert!(
            res.is_err(),
            "deactivated candidate must not receive ownership"
        );
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(404) // AlreadyDeactivated
        );

        assert_eq!(client.get_owner(), super_admin_1);
        assert_eq!(client.get_pending_owner(), Some(super_admin_2.clone()));
        assert_eq!(client.get_config_epoch(), epoch_before);
        assert_eq!(env.events().all().len(), events_before);
    }

    /// A candidate still suspended when the timelock elapses must not accept.
    #[test]
    fn test_accept_ownership_rejects_candidate_still_suspended_at_acceptance() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);
        let client = AdminContractClient::new(&env, &contract_address);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        // Suspension outlasts the timelock, so the candidate is still inactive
        // at the moment of acceptance.
        let suspended_until = env.ledger().timestamp() + crate::OWNERSHIP_TRANSFER_TIMELOCK + 3_600;
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
                suspended_until,
            );
        });
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);

        let res = client.try_accept_ownership(&super_admin_2);
        assert!(
            res.is_err(),
            "suspended candidate must not receive ownership"
        );
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(113) // AdminSuspended
        );

        assert_eq!(client.get_owner(), super_admin_1);
        assert_eq!(client.get_pending_owner(), Some(super_admin_2.clone()));
    }

    /// A candidate whose admin record is removed during the timelock must not be
    /// able to accept ownership.
    #[test]
    fn test_accept_ownership_rejects_candidate_removed_during_timelock() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);
        let client = AdminContractClient::new(&env, &contract_address);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);
        remove_admin_record(&env, &contract_address, &super_admin_2);

        let epoch_before = client.get_config_epoch();

        let res = client.try_accept_ownership(&super_admin_2);
        assert!(res.is_err(), "removed candidate must not receive ownership");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(100) // NotAdmin
        );

        assert_eq!(client.get_owner(), super_admin_1);
        assert_eq!(client.get_pending_owner(), Some(super_admin_2.clone()));
        assert_eq!(client.get_config_epoch(), epoch_before);
    }

    /// Symmetric boundary: a suspension that *expires* during the timelock does
    /// not permanently block the candidate. Suspension is a self-expiring clock,
    /// not a revocation, so acceptance succeeds once `suspended_until` passes.
    #[test]
    fn test_accept_ownership_allows_candidate_whose_suspension_expired_during_timelock() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);
        let client = AdminContractClient::new(&env, &contract_address);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });

        let suspended_until = env.ledger().timestamp() + 100;
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
                suspended_until,
            );
        });

        // Timelock is longer than the suspension, so it has expired by now.
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);

        let res = client.try_accept_ownership(&super_admin_2);
        assert!(res.is_ok(), "expired suspension must not block acceptance");
        assert_eq!(client.get_owner(), super_admin_2);
        assert_eq!(client.get_pending_owner(), None);
    }

    /// Overwriting a pending proposal must restart the timelock. If the elapsed
    /// clock from the superseded proposal carried over, an attacker who can
    /// re-propose could bypass the delay entirely.
    #[test]
    fn test_overwriting_pending_owner_restarts_timelock() {
        let env = Env::default();
        let (contract_address, s1, s2, s3) = setup_three_super_admins(&env);
        let client = AdminContractClient::new(&env, &contract_address);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(env.clone(), s1.clone(), s2.clone());
        });
        // Let the first proposal become eligible.
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);

        // Overwrite with a *new* proposal. It must get a fresh clock.
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(env.clone(), s1.clone(), s3.clone());
        });
        assert_eq!(client.get_pending_owner(), Some(s3.clone()));

        let res = client.try_accept_ownership(&s3);
        assert!(res.is_err(), "overwrite must restart the timelock");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(112) // TimelockNotReady
        );
        assert_eq!(client.get_owner(), s1);
        assert_eq!(client.get_pending_owner(), Some(s3.clone()));

        // After the full delay measured from the new proposal, acceptance works.
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::accept_ownership(env.clone(), s3.clone());
        });
        assert_eq!(client.get_owner(), s3);
        assert_eq!(client.get_pending_owner(), None);
    }

    /// After a successful acceptance the proposal is consumed; replaying the
    /// acceptance must be rejected and must not re-emit rotation events.
    #[test]
    fn test_accept_ownership_replay_after_success_is_rejected() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);
        let client = AdminContractClient::new(&env, &contract_address);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::accept_ownership(env.clone(), super_admin_2.clone());
        });
        assert_eq!(client.get_owner(), super_admin_2);

        let epoch_after = client.get_config_epoch();
        let events_after = env.events().all().len();

        let res = client.try_accept_ownership(&super_admin_2);
        assert!(res.is_err(), "a consumed proposal cannot be replayed");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(115) // NoPendingAdmin
        );
        assert_eq!(client.get_owner(), super_admin_2);
        assert_eq!(client.get_config_epoch(), epoch_after);
        assert_eq!(env.events().all().len(), events_after);
    }

    /// A transfer rejected because the target is not an effective SuperAdmin
    /// must leave no pending proposal and must not advance the config epoch.
    #[test]
    fn test_failed_transfer_leaves_no_pending_state_or_epoch_bump() {
        let env = Env::default();
        let (contract_address, super_admin) = setup_contract(&env);
        let client = AdminContractClient::new(&env, &contract_address);
        let operator = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                operator.clone(),
                AdminRole::Operator,
            );
        });

        let epoch_before = client.get_config_epoch();
        let res = client.try_transfer_ownership(&super_admin, &operator);
        assert!(res.is_err(), "non-SuperAdmin target must be rejected");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(100) // NotAdmin
        );

        assert_eq!(client.get_pending_owner(), None);
        assert_eq!(client.get_owner(), super_admin);
        assert_eq!(client.get_config_epoch(), epoch_before);
    }

    /// `transfer_ownership` before initialization is a validation failure, not
    /// a state transition.
    #[test]
    fn test_transfer_ownership_before_initialization_is_rejected() {
        let env = Env::default();
        let contract_address = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&env, &contract_address);
        let caller = Address::generate(&env);
        let new_owner = Address::generate(&env);

        env.mock_all_auths();
        let res = client.try_transfer_ownership(&caller, &new_owner);
        assert!(res.is_err(), "uninitialized contract must reject transfers");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(1) // NotInitialized
        );
        assert_eq!(client.get_pending_owner(), None);
    }

    /// `accept_ownership` before initialization has no proposal to consume.
    #[test]
    fn test_accept_ownership_before_initialization_is_rejected() {
        let env = Env::default();
        let contract_address = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&env, &contract_address);
        let caller = Address::generate(&env);

        env.mock_all_auths();
        let res = client.try_accept_ownership(&caller);
        assert!(
            res.is_err(),
            "uninitialized contract must reject acceptance"
        );
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(115) // NoPendingAdmin
        );
    }

    /// Failure recovery: when a candidate becomes ineligible during the
    /// timelock, the current owner keeps ownership and can replace the proposal
    /// with an eligible candidate. The failed attempt is not a dead end.
    #[test]
    fn test_owner_can_recover_by_replacing_ineligible_candidate() {
        let env = Env::default();
        let (contract_address, s1, s2, s3) = setup_three_super_admins(&env);
        let client = AdminContractClient::new(&env, &contract_address);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(env.clone(), s1.clone(), s2.clone());
        });
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);

        // The first candidate becomes ineligible; acceptance fails.
        set_admin_active(&env, &contract_address, &s2, false);
        assert!(client.try_accept_ownership(&s2).is_err());
        assert_eq!(client.get_owner(), s1);

        // Recovery: replace the stale proposal with an eligible candidate.
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(env.clone(), s1.clone(), s3.clone());
        });
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);
        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::accept_ownership(env.clone(), s3.clone());
        });

        assert_eq!(client.get_owner(), s3);
        assert_eq!(client.get_pending_owner(), None);
    }

    /// Observability: a successful acceptance emits exactly the rotation and
    /// acceptance events and clears the pending slot.
    #[test]
    fn test_successful_acceptance_emits_events_and_clears_pending() {
        let env = Env::default();
        let (contract_address, super_admin_1, super_admin_2) = setup_multiple_super_admins(&env);
        let client = AdminContractClient::new(&env, &contract_address);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::transfer_ownership(
                env.clone(),
                super_admin_1.clone(),
                super_admin_2.clone(),
            );
        });
        advance_ledger(&env, crate::OWNERSHIP_TRANSFER_TIMELOCK);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::accept_ownership(env.clone(), super_admin_2.clone());
        });
        // The event log is scoped to the most recent invocation, so this is
        // exactly the number of events the acceptance published.
        let acceptance_events = env.events().all().len();

        // `admin_rotated` + `ownership_transfer_accepted`
        assert_eq!(acceptance_events, 2);
        assert_eq!(client.get_owner(), super_admin_2);
        assert_eq!(client.get_pending_owner(), None);
    }
}
