use crate::*;
use soroban_sdk::{Address, Env};

#[cfg(test)]
mod ownership_transfer_tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

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
    #[should_panic(expected = "Error(Contract, #107)")]
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
    #[should_panic(expected = "Error(Contract, #109)")]
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
}
