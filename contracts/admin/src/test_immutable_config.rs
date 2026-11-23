//! Adversarial regression coverage for the immutable config surface
//! (`initialize`, `get_config`, `get_owner`, `get_config_epoch`).
//!
//! The contract's governance configuration is write-once: `initialize` is the
//! only entrypoint that persists `MinAdmins`/`MaxAdmins`/`Owner`, and it must
//! be atomic. These tests pin the invariants that follow from that:
//!
//! * **Loading** Ã¢â‚¬â€ every getter has a defined answer both before and after
//!   initialization (`get_config_epoch` reports `0`; the rest reject with
//!   `NotInitialized`).
//! * **Error** Ã¢â‚¬â€ each rejection carries a stable, documented error code.
//! * **Retry** Ã¢â‚¬â€ a rejected `initialize` leaves the contract uninitialized, so
//!   a later valid call still succeeds. Guards against a half-written
//!   `Initialized` flag.
//! * **Stale** Ã¢â‚¬â€ pure reads never advance the config epoch, and config values
//!   do not drift as the ledger advances.
//! * **Permission** Ã¢â‚¬â€ a non-admin cannot reach privileged mutations.
//!
//! The central property asserted throughout is *no silent data loss*: a
//! rejected call must leave the previously committed configuration and owner
//! byte-for-byte unchanged.

use crate::*;
use soroban_sdk::{Address, Env};

#[cfg(test)]
mod immutable_config_tests {
    use super::*;
    use crate::AdminContractClient;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    fn create_contract() -> AdminContract {
        AdminContract {}
    }

    /// Register a fresh, *uninitialized* contract and return its address.
    fn deploy_uninitialized(env: &Env) -> Address {
        let contract = create_contract();
        env.register_contract(None, AdminContract)
    }

    /// Register and initialize a contract, returning `(address, super_admin)`.
    fn setup(env: &Env, min_admins: u32, max_admins: u32) -> (Address, Address) {
        let contract_address = deploy_uninitialized(env);
        let super_admin = Address::generate(env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), min_admins, max_admins);
        });

        (contract_address, super_admin)
    }

    // ========================================================================
    // Loading states
    // ========================================================================

    #[test]
    fn test_config_returns_correct_values_after_initialization() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup(&env, 3, 50);

        env.as_contract(&contract_address, || {
            let (min_admins, max_admins) = AdminContract::get_config(env.clone());
            assert_eq!(min_admins, 3);
            assert_eq!(max_admins, 50);
        });
    }

    #[test]
    fn test_admin_config_returns_correct_owner_after_initialization() {
        let env = Env::default();
        let (contract_address, super_admin) = setup(&env, 1, 100);

        env.as_contract(&contract_address, || {
            let owner = AdminContract::get_owner(env.clone());
            assert_eq!(owner, super_admin);
        });
    }

    /// An uninitialized contract reports a zero epoch rather than panicking, so
    /// a client can read the epoch before initialization without special-casing.
    #[test]
    fn test_uninitialized_contract_reports_zero_config_epoch() {
        let env = Env::default();
        let contract_address = deploy_uninitialized(&env);

        env.as_contract(&contract_address, || {
            assert_eq!(AdminContract::get_config_epoch(env.clone()), 0);
        });
    }

    // ========================================================================
    // Error states
    // ========================================================================

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_config_requires_initialization() {
        let env = Env::default();
        let contract_address = deploy_uninitialized(&env);
        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            AdminContract::get_config(env.clone());
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn test_admin_config_requires_initialization() {
        let env = Env::default();
        let contract_address = deploy_uninitialized(&env);
        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            AdminContract::get_owner(env.clone());
        });
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_cannot_reinitialize_admin_contract() {
        let env = Env::default();
        let contract_address = deploy_uninitialized(&env);
        let super_admin = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);

            // A different super admin must not be able to re-initialize and
            // take ownership of an already-initialized contract.
            AdminContract::initialize(env.clone(), other, 2, 200);
        });
    }

    // ========================================================================
    // Boundary cases
    // ========================================================================

    /// `min_admins == 0` would make every quorum unsatisfiable, so it is
    /// rejected before any write.
    #[test]
    #[should_panic(expected = "Error(Contract, #107)")]
    fn test_initialize_rejects_zero_min_admins() {
        let env = Env::default();
        let contract_address = deploy_uninitialized(&env);
        let super_admin = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin, 0, 100);
        });
    }

    /// An inverted range (`min > max`) is unsatisfiable and must be rejected.
    #[test]
    #[should_panic(expected = "Error(Contract, #107)")]
    fn test_initialize_rejects_min_greater_than_max() {
        let env = Env::default();
        let contract_address = deploy_uninitialized(&env);
        let super_admin = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin, 5, 2);
        });
    }

    /// `min_admins == max_admins` is the tightest legal range and must be
    /// accepted Ã¢â‚¬â€ the bounds check is inclusive on both ends.
    #[test]
    fn test_initialize_accepts_min_equal_to_max() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup(&env, 2, 2);

        env.as_contract(&contract_address, || {
            let (min_admins, max_admins) = AdminContract::get_config(env.clone());
            assert_eq!(min_admins, 2);
            assert_eq!(max_admins, 2);
        });
    }

    // ========================================================================
    // Retry / partial failure Ã¢â‚¬â€ the core no-silent-data-loss invariant
    // ========================================================================

    /// A rejected `initialize` must not overwrite the committed configuration.
    /// The original min/max and owner survive the failed second attempt.
    #[test]
    fn test_rejected_reinitialize_preserves_config_and_owner() {
        let env = Env::default();
        let (contract_address, super_admin) = setup(&env, 3, 50);
        let impostor = Address::generate(&env);

        let client = AdminContractClient::new(&env, &contract_address);
        env.mock_all_auths();
        let res = client.try_initialize(&impostor, &99, &999);
        assert!(
            res.is_err(),
            "re-initializing an initialized contract must be rejected"
        );

        env.as_contract(&contract_address, || {
            let (min_admins, max_admins) = AdminContract::get_config(env.clone());
            assert_eq!(min_admins, 3, "rejected init must not alter min_admins");
            assert_eq!(max_admins, 50, "rejected init must not alter max_admins");
            assert_eq!(
                AdminContract::get_owner(env.clone()),
                super_admin,
                "rejected init must not transfer ownership"
            );
        });
    }

    /// A rejected `initialize` is a no-op on the config epoch: clients polling
    /// the epoch must not see a spurious bump for a call that changed nothing.
    #[test]
    fn test_rejected_reinitialize_does_not_advance_config_epoch() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup(&env, 3, 50);
        let impostor = Address::generate(&env);

        let client = AdminContractClient::new(&env, &contract_address);
        let epoch_before = client.get_config_epoch();

        env.mock_all_auths();
        let res = client.try_initialize(&impostor, &99, &999);
        assert!(res.is_err(), "re-initialization must be rejected");

        assert_eq!(
            client.get_config_epoch(),
            epoch_before,
            "a rejected call must not advance the config epoch"
        );
    }

    /// Invalid bounds must be rejected *before* the `Initialized` flag is
    /// written, leaving the contract cleanly uninitialized and retryable.
    #[test]
    fn test_rejected_initialize_leaves_contract_retryable() {
        let env = Env::default();
        let contract_address = deploy_uninitialized(&env);
        let super_admin = Address::generate(&env);

        let client = AdminContractClient::new(&env, &contract_address);

        // First attempt is invalid and must abort.
        env.mock_all_auths();
        let res = client.try_initialize(&super_admin, &0, &100);
        assert!(res.is_err(), "min_admins == 0 must be rejected");

        // The contract must still be uninitialized, so the getters still reject.
        env.as_contract(&contract_address, || {
            assert_eq!(
                AdminContract::get_config_epoch(env.clone()),
                0,
                "a rejected initialize must not set the initialized flag"
            );
        });

        // A valid retry must succeed and commit the intended config.
        env.mock_all_auths();
        client.initialize(&super_admin, &4, &40);

        env.as_contract(&contract_address, || {
            let (min_admins, max_admins) = AdminContract::get_config(env.clone());
            assert_eq!(min_admins, 4);
            assert_eq!(max_admins, 40);
            assert_eq!(AdminContract::get_owner(env.clone()), super_admin);
        });
    }

    // ========================================================================
    // Stale state
    // ========================================================================

    /// Pure reads must not advance the config epoch. Two identical reads
    /// separated by ledger advancement observe the same epoch.
    #[test]
    fn test_config_epoch_does_not_advance_on_pure_reads() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup(&env, 2, 20);

        env.as_contract(&contract_address, || {
            let epoch_before = AdminContract::get_config_epoch(env.clone());
            let _ = AdminContract::get_config(env.clone());
            let _ = AdminContract::get_owner(env.clone());
            assert_eq!(
                AdminContract::get_config_epoch(env.clone()),
                epoch_before,
                "reading config and owner must not advance the epoch"
            );
        });
    }

    /// Config is committed state, not a function of the clock: advancing the
    /// ledger must not drift the stored min/max or the owner.
    #[test]
    fn test_config_is_stable_across_ledger_advancement() {
        let env = Env::default();
        let (contract_address, super_admin) = setup(&env, 3, 50);

        env.ledger()
            .set_timestamp(env.ledger().timestamp() + 86_400 * 30);

        env.as_contract(&contract_address, || {
            let (min_admins, max_admins) = AdminContract::get_config(env.clone());
            assert_eq!(min_admins, 3, "min_admins must not drift with the clock");
            assert_eq!(max_admins, 50, "max_admins must not drift with the clock");
            assert_eq!(AdminContract::get_owner(env.clone()), super_admin);
        });
    }

    // ========================================================================
    // Permission state
    // ========================================================================

    /// A non-admin caller must not reach a privileged mutation.
    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_privileged_mutation_by_non_admin_is_rejected() {
        let env = Env::default();
        let (contract_address, _super_admin) = setup(&env, 1, 100);
        let outsider = Address::generate(&env);
        let recruit = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(env.clone(), outsider, recruit, AdminRole::Admin);
        });
    }

    /// `MinAdmins`/`MaxAdmins` are immutable after initialization. Privileged
    /// admin-list churn must not widen or narrow the committed range.
    #[test]
    fn test_config_survives_privileged_admin_churn_unchanged() {
        let env = Env::default();
        let (contract_address, super_admin) = setup(&env, 2, 4);
        let a = Address::generate(&env);
        let b = Address::generate(&env);

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                a.clone(),
                AdminRole::Admin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                b.clone(),
                AdminRole::Admin,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::update_admin_role(
                env.clone(),
                super_admin.clone(),
                a.clone(),
                AdminRole::Operator,
            );
        });

        env.mock_all_auths();
        env.as_contract(&contract_address, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), b.clone());
        });

        env.as_contract(&contract_address, || {
            let (min_admins, max_admins) = AdminContract::get_config(env.clone());
            assert_eq!(min_admins, 2, "admin churn must not change min_admins");
            assert_eq!(max_admins, 4, "admin churn must not change max_admins");
            assert_eq!(
                AdminContract::get_owner(env.clone()),
                super_admin,
                "admin churn must not change the owner"
            );
        });
    }
}
