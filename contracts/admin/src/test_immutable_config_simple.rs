//! Adversarial regression suite for the Admin contract's *immutable*
//! configuration (`MinAdmins` / `MaxAdmins`) and its initialization gate.
//!
//! The configuration is written exactly once by [`AdminContract::initialize`]
//! and has no setter. These tests pin the invariants that make that one-shot
//! write safe under adverse conditions:
//!
//! * **Validation** — `min_admins == 0` and `min_admins > max_admins` are
//!   rejected before any state is written; the `min == max` boundary and the
//!   `u32::MAX` ceiling are accepted.
//! * **Immutability** — once initialized, `get_config` returns the same values
//!   forever: privileged mutations, a competing re-`initialize`, and an
//!   ownership rotation all leave `(min_admins, max_admins)` untouched.
//! * **Duplicate / concurrent initialization** — the first writer wins; a
//!   second `initialize` is rejected with `AlreadyInitialized` and changes
//!   nothing observable (config, owner, epoch, or events).
//! * **Atomic rejection / recovery** — a failed `initialize` rolls back
//!   completely, so reads still report `NotInitialized` and a later valid call
//!   can succeed.
//! * **Authorization** — `initialize` requires the super admin's signature.
//! * **Coupling** — `min_admins` is actually enforced by the suspension floor,
//!   so the immutable value governs a real state-transition invariant.
//!
//! The baseline success cases are retained verbatim; adversarial cases use the
//! generated [`AdminContractClient`] so error codes and atomicity are asserted
//! at the real contract boundary rather than through direct Rust calls.

use crate::*;
use soroban_sdk::{Address, Env};

#[cfg(test)]
mod immutable_config_tests {
    use super::*;
    use crate::AdminContractClient;
    use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};

    fn create_contract() -> AdminContract {
        AdminContract {}
    }

    /// Deploy a fresh contract and initialize it with the supplied bounds.
    /// Mirrors the proven `setup()` shape used across the admin test suite.
    fn setup(
        min_admins: u32,
        max_admins: u32,
    ) -> (Env, Address, AdminContractClient<'static>, Address) {
        let env = Env::default();
        let contract_address = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&env, &contract_address);
        let super_admin = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&super_admin, &min_admins, &max_admins);

        (env, contract_address, client, super_admin)
    }

    /// Deploy a fresh, *uninitialized* contract with auths mocked, so tests can
    /// drive the `initialize` validation matrix directly.
    fn setup_uninitialized() -> (Env, Address, AdminContractClient<'static>) {
        let env = Env::default();
        let contract_address = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&env, &contract_address);
        env.mock_all_auths();
        (env, contract_address, client)
    }

    // ---------------------------------------------------------------------
    // Baseline success paths.
    // ---------------------------------------------------------------------

    #[test]
    fn test_config_returns_correct_values_after_initialization() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin = Address::generate(&env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 3, 50);

            let (min_admins, max_admins) = AdminContract::get_config(env.clone());
            assert_eq!(min_admins, 3);
            assert_eq!(max_admins, 50);
        });
    }

    #[test]
    fn test_admin_config_returns_correct_owner_after_initialization() {
        let env = Env::default();
        let contract = create_contract();
        let super_admin = Address::generate(&env);
        let contract_address = env.register_contract(None, AdminContract);

        env.mock_all_auths();

        env.as_contract(&contract_address, || {
            AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);

            let owner = AdminContract::get_owner(env.clone());
            assert_eq!(owner, super_admin);
        });
    }

    // ---------------------------------------------------------------------
    // Pre-initialization reads fail deterministically (no default config).
    // ---------------------------------------------------------------------

    #[test]
    fn test_get_config_before_initialization_is_rejected() {
        let (_env, _contract_address, client) = setup_uninitialized();

        let res = client.try_get_config();
        assert!(res.is_err(), "config must not be readable before init");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(1) // NotInitialized
        );
    }

    #[test]
    fn test_get_owner_before_initialization_is_rejected() {
        let (_env, _contract_address, client) = setup_uninitialized();

        let res = client.try_get_owner();
        assert!(res.is_err(), "owner must not be readable before init");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(1) // NotInitialized
        );
    }

    // ---------------------------------------------------------------------
    // Initialization validation matrix + atomic rollback / recovery.
    // ---------------------------------------------------------------------

    #[test]
    fn test_initialize_rejects_zero_min_admins_and_is_recoverable() {
        let (env, _contract_address, client) = setup_uninitialized();
        let super_admin = Address::generate(&env);

        let res = client.try_initialize(&super_admin, &0u32, &100u32);
        assert!(res.is_err(), "min_admins == 0 must be rejected");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(107) // InvalidPauseAction
        );

        // Atomic rollback: the rejected call wrote nothing, so reads still
        // report NotInitialized and a later valid call recovers cleanly.
        assert!(client.try_get_config().is_err());
        client.initialize(&super_admin, &1u32, &100u32);
        assert_eq!(client.get_config(), (1u32, 100u32));
    }

    #[test]
    fn test_initialize_rejects_min_admins_greater_than_max_admins() {
        let (env, _contract_address, client) = setup_uninitialized();
        let super_admin = Address::generate(&env);

        let res = client.try_initialize(&super_admin, &51u32, &50u32);
        assert!(res.is_err(), "min_admins > max_admins must be rejected");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(107) // InvalidPauseAction
        );

        // Nothing was committed.
        assert!(client.try_get_config().is_err());
    }

    #[test]
    fn test_initialize_accepts_equal_min_and_max_boundary() {
        let (env, _contract_address, client) = setup_uninitialized();
        let super_admin = Address::generate(&env);

        // `min == max` is a legitimate fixed-size configuration.
        client.initialize(&super_admin, &5u32, &5u32);

        assert_eq!(client.get_config(), (5u32, 5u32));
    }

    #[test]
    fn test_initialize_accepts_max_u32_boundary() {
        let (env, _contract_address, client) = setup_uninitialized();
        let super_admin = Address::generate(&env);

        // Upper bound of the u32 domain must round-trip without truncation.
        client.initialize(&super_admin, &u32::MAX, &u32::MAX);

        assert_eq!(client.get_config(), (u32::MAX, u32::MAX));
    }

    // ---------------------------------------------------------------------
    // Duplicate / concurrent initialization: first writer wins.
    // ---------------------------------------------------------------------

    #[test]
    fn test_reinitialization_is_rejected_and_preserves_original_config() {
        let (env, _contract_address, client, original_admin) = setup(3, 50);
        let attacker = Address::generate(&env);

        let epoch_before = client.get_config_epoch();
        let events_before = env.events().all().len();

        let res = client.try_initialize(&attacker, &1u32, &1u32);
        assert!(res.is_err(), "a second initialize must be rejected");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(2) // AlreadyInitialized
        );

        // The rejected duplicate is a no-op: config, owner, epoch, and the
        // event stream are all untouched.
        assert_eq!(client.get_config(), (3u32, 50u32));
        assert_eq!(client.get_owner(), original_admin);
        assert_eq!(client.get_config_epoch(), epoch_before);
        assert_eq!(env.events().all().len(), events_before);
    }

    // ---------------------------------------------------------------------
    // Immutability across the privileged-mutation surface.
    // ---------------------------------------------------------------------

    #[test]
    fn test_config_is_immutable_across_privileged_mutations() {
        let (env, _contract_address, client, admin) = setup(1, 4);

        let a1 = Address::generate(&env);
        let a2 = Address::generate(&env);
        let a3 = Address::generate(&env);
        client.add_admin(&admin, &a1, &AdminRole::Admin);
        client.add_admin(&admin, &a2, &AdminRole::Operator);
        client.add_admin(&admin, &a3, &AdminRole::Operator);
        assert_eq!(client.get_admin_count(), 4);
        assert_eq!(client.get_config(), (1u32, 4u32));

        // Boundary: filling to max_admins succeeds exactly; one more is
        // rejected and the immutable config is unchanged.
        let a4 = Address::generate(&env);
        let res = client.try_add_admin(&admin, &a4, &AdminRole::Operator);
        assert!(res.is_err(), "max_admins must be enforced at the boundary");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(601) // ThresholdExceedsSigners
        );
        assert_eq!(client.get_config(), (1u32, 4u32));

        // Role changes, deactivation, and suspension cannot rewrite config.
        client.update_admin_role(&admin, &a1, &AdminRole::Operator);
        client.deactivate_admin(&admin, &a2);
        client.suspend_admin(&admin, &a3, &(env.ledger().timestamp() + 100));
        assert_eq!(client.get_config(), (1u32, 4u32));

        // Removal also leaves min/max untouched.
        client.remove_admin(&admin, &a1);
        assert_eq!(client.get_config(), (1u32, 4u32));
    }

    #[test]
    fn test_config_survives_ownership_rotation() {
        let (env, _contract_address, client, admin) = setup(2, 7);
        let new_owner = Address::generate(&env);
        client.add_admin(&admin, &new_owner, &AdminRole::SuperAdmin);

        client.transfer_ownership(&admin, &new_owner);
        env.ledger()
            .with_mut(|ledger| ledger.timestamp += crate::OWNERSHIP_TRANSFER_TIMELOCK);
        client.accept_ownership(&new_owner);

        assert_eq!(client.get_owner(), new_owner);
        assert_eq!(client.get_config(), (2u32, 7u32));
    }

    // ---------------------------------------------------------------------
    // Coupling: the immutable `min_admins` value governs a real invariant.
    // ---------------------------------------------------------------------

    #[test]
    fn test_min_admins_config_blocks_suspension_below_floor() {
        let (env, _contract_address, client, admin) = setup(2, 10);
        let second = Address::generate(&env);
        client.add_admin(&admin, &second, &AdminRole::Admin);

        let until = env.ledger().timestamp() + 100;

        // Suspending `second` would leave only 1 effective admin, below the
        // configured floor of 2 — rejected without touching any state.
        let res = client.try_suspend_admin(&admin, &second, &until);
        assert!(res.is_err(), "suspension below min_admins must be rejected");
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(107) // InvalidPauseAction
        );
        assert_eq!(client.get_config(), (2u32, 10u32));
        assert_eq!(client.is_admin(&second), Role::Admin);

        // Boundary: once a third effective admin exists the floor is met, so
        // the same suspension is allowed.
        let third = Address::generate(&env);
        client.add_admin(&admin, &third, &AdminRole::Admin);
        client.suspend_admin(&admin, &second, &until);
        assert_eq!(client.is_admin(&second), Role::User);
        assert_eq!(client.get_config(), (2u32, 10u32));
    }

    // ---------------------------------------------------------------------
    // Stale reads / retries: the config snapshot is always reusable.
    // ---------------------------------------------------------------------

    #[test]
    fn test_config_snapshot_is_stable_across_concurrent_mutations() {
        let (env, _contract_address, client, admin) = setup(1, 10);
        let (min_before, max_before) = client.get_config();
        let epoch_before = client.get_config_epoch();

        // A concurrent privileged mutation moves the epoch...
        let other = Address::generate(&env);
        client.add_admin(&admin, &other, &AdminRole::Admin);
        assert_ne!(client.get_config_epoch(), epoch_before);

        // ...but never the configuration, so a retried read stays valid.
        assert_eq!(client.get_config(), (min_before, max_before));
    }

    #[test]
    fn test_config_reads_do_not_mutate_state() {
        let (env, _contract_address, client, _admin) = setup(2, 9);
        let epoch_before = client.get_config_epoch();
        let events_before = env.events().all().len();

        for _ in 0..3 {
            assert_eq!(client.get_config(), (2u32, 9u32));
            let _ = client.get_owner();
            let _ = client.get_config_epoch();
        }

        assert_eq!(client.get_config_epoch(), epoch_before);
        assert_eq!(env.events().all().len(), events_before);
    }

    // ---------------------------------------------------------------------
    // Authorization + observability.
    // ---------------------------------------------------------------------

    #[test]
    fn test_initialize_requires_super_admin_authorization() {
        let env = Env::default();
        let contract_address = env.register_contract(None, AdminContract);
        let client = AdminContractClient::new(&env, &contract_address);
        let stranger = Address::generate(&env);

        // No auths mocked: the caller has not authorised `initialize`.
        let res = client.try_initialize(&stranger, &1u32, &100u32);
        assert!(
            res.is_err(),
            "unauthenticated initialization must be rejected"
        );

        // The unauthorized attempt wrote nothing.
        env.mock_all_auths();
        let res = client.try_get_config();
        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().unwrap(),
            soroban_sdk::Error::from_contract_error(1) // NotInitialized
        );
    }

    #[test]
    fn test_successful_initialization_emits_single_event() {
        let (env, _contract_address, client) = setup_uninitialized();
        let super_admin = Address::generate(&env);

        assert_eq!(env.events().all().len(), 0);
        client.initialize(&super_admin, &2u32, &5u32);

        // Exactly the `admin_initialized` event — no hidden config writes.
        assert_eq!(env.events().all().len(), 1);
        assert_eq!(client.get_config(), (2u32, 5u32));
    }
}
