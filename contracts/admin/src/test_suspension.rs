use crate::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (Address, Address) {
    let contract_address = env.register_contract(None, AdminContract);
    let super_admin = Address::generate(env);
    env.mock_all_auths();
    env.as_contract(&contract_address, || {
        AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);
    });
    (contract_address, super_admin)
}

#[cfg(test)]
mod suspension_tests {
    use super::*;
    use credence_errors::Role;
    use soroban_sdk::testutils::Ledger;

    // ── 1. suspend_admin succeeds ─────────────────────────────────────────────

    #[test]
    fn test_suspend_admin_succeeds() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let target = Address::generate(&env);

        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                AdminRole::Admin,
            );
        });

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                now + 100,
            );
        });

        let info = env.as_contract(&contract, || {
            AdminContract::get_admin_info(env.clone(), target.clone())
        });
        assert_eq!(info.suspended_until, now + 100);
        assert!(info.active); // permanent flag unchanged
    }

    // ── 2. Suspended admin is inactive during suspension ─────────────────────

    #[test]
    fn test_suspended_admin_is_inactive() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let target = Address::generate(&env);

        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                AdminRole::Admin,
            );
        });

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                now + 100,
            );
        });

        assert_eq!(
            env.as_contract(&contract, || {
                AdminContract::is_admin(env.clone(), target.clone())
            }),
            Role::User
        );
        assert!(!env.as_contract(&contract, || {
            AdminContract::has_role_at_least(env.clone(), target.clone(), AdminRole::Operator)
        }));
    }

    // ── 3. Auto-reactivation after expiry — no second tx ────────────────────

    #[test]
    fn test_auto_reactivation_after_expiry() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let target = Address::generate(&env);

        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                AdminRole::Admin,
            );
        });

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                now + 100,
            );
        });

        // Advance ledger past expiry — no reactivate_admin call needed
        env.ledger().with_mut(|li| li.timestamp = now + 101);

        assert_eq!(
            env.as_contract(&contract, || {
                AdminContract::is_admin(env.clone(), target.clone())
            }),
            Role::Admin
        );
        assert!(env.as_contract(&contract, || {
            AdminContract::has_role_at_least(env.clone(), target.clone(), AdminRole::Admin)
        }));
    }

    // ── 4. until_ts in the past → AdminSuspended (109) ───────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #113)")]
    fn test_suspend_past_timestamp_rejected() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let target = Address::generate(&env);

        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                AdminRole::Admin,
            );
        });

        env.as_contract(&contract, || {
            AdminContract::suspend_admin(env.clone(), super_admin.clone(), target.clone(), 0);
        });
    }

    // ── 5. until_ts == now → AdminSuspended (109) ────────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #113)")]
    fn test_suspend_current_timestamp_rejected() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let target = Address::generate(&env);

        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                AdminRole::Admin,
            );
        });

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(env.clone(), super_admin.clone(), target.clone(), now);
        });
    }

    // ── 6. Suspending last active admin → InvalidPauseAction (107) ───────────

    #[test]
    #[should_panic(expected = "Error(Contract, #107)")]
    fn test_suspend_below_min_admins_rejected() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin.clone(),
                super_admin.clone(),
                now + 100,
            );
        });
    }

    // ── 7. Suspending a deactivated admin → AlreadyDeactivated (404) ─────────

    #[test]
    #[should_panic(expected = "Error(Contract, #404)")]
    fn test_suspend_deactivated_admin_rejected() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let target = Address::generate(&env);

        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                AdminRole::Admin,
            );
        });

        env.as_contract(&contract, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), target.clone());
        });

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                now + 100,
            );
        });
    }

    // ── 8. Lower-role caller cannot suspend higher-role target → NotAdmin ─────

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_operator_cannot_suspend_admin() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let admin = Address::generate(&env);
        let operator = Address::generate(&env);

        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                admin.clone(),
                AdminRole::Admin,
            );
            AdminContract::add_admin(
                env.clone(),
                admin.clone(),
                operator.clone(),
                AdminRole::Operator,
            );
        });

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(env.clone(), operator.clone(), admin.clone(), now + 100);
        });
    }

    // ── 9. Suspending a non-admin → NotAdmin (100) ───────────────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_suspend_non_admin_rejected() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let stranger = Address::generate(&env);

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin.clone(),
                stranger.clone(),
                now + 100,
            );
        });
    }

    // ── 10. Re-suspending extends the suspension deadline ────────────────────

    #[test]
    fn test_re_suspend_extends_suspension() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let target = Address::generate(&env);

        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                AdminRole::Admin,
            );
        });

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                now + 50,
            );
        });

        // Extend the suspension in a separate call
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin.clone(),
                target.clone(),
                now + 200,
            );
        });

        let info = env.as_contract(&contract, || {
            AdminContract::get_admin_info(env.clone(), target.clone())
        });
        assert_eq!(info.suspended_until, now + 200);

        // Still inactive within the extended window
        env.ledger().with_mut(|li| li.timestamp = now + 100);
        assert_eq!(
            env.as_contract(&contract, || {
                AdminContract::is_admin(env.clone(), target.clone())
            }),
            Role::User
        );

        // Active after the extended deadline
        env.ledger().with_mut(|li| li.timestamp = now + 201);
        assert_eq!(
            env.as_contract(&contract, || {
                AdminContract::is_admin(env.clone(), target.clone())
            }),
            Role::Admin
        );
    }

    // ── 11. Suspended admin cannot call privileged operations ─────────────────

    #[test]
    #[should_panic(expected = "Error(Contract, #100)")]
    fn test_suspended_admin_cannot_add_operator() {
        let env = Env::default();
        let (contract, super_admin) = setup(&env);
        let admin = Address::generate(&env);
        let new_op = Address::generate(&env);

        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                admin.clone(),
                AdminRole::Admin,
            );
        });

        let now = env.ledger().timestamp();
        env.as_contract(&contract, || {
            AdminContract::suspend_admin(
                env.clone(),
                super_admin.clone(),
                admin.clone(),
                now + 100,
            );
        });

        // Suspended admin tries to add an operator — must be rejected
        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                admin.clone(),
                new_op.clone(),
                AdminRole::Operator,
            );
        });
    }
}
