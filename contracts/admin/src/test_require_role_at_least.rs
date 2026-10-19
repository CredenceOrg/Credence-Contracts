#![cfg(test)]

extern crate std;

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    Address, Env,
};
use testutils::user;

fn setup_env() -> (Env, Address, Address) {
    let env = Env::default();
    let contract_address = env.register_contract(None, AdminContract);
    let super_admin = user(&env);
    env.mock_all_auths();
    env.as_contract(&contract_address, || {
        AdminContract::initialize(env.clone(), super_admin.clone(), 1, 10);
    });
    (env, contract_address, super_admin)
}

fn advance(env: &Env, secs: u64) {
    env.ledger().set(LedgerInfo {
        timestamp: env.ledger().timestamp() + secs,
        protocol_version: 22,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 16,
        max_entry_ttl: 1000,
    });
}

fn as_admin(env: &Env, contract: &Address, caller: &Address, new_admin: &Address, role: AdminRole) {
    env.mock_all_auths();
    env.as_contract(contract, || {
        AdminContract::add_admin(env.clone(), caller.clone(), new_admin.clone(), role);
    });
}

// ---------------------------------------------------------------------------
// Grant + use before — verify a freshly-granted role satisfies the check
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_succeeds_for_operator_after_grant() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), operator.clone(), AdminRole::Operator)
    }));
}

#[test]
fn require_role_at_least_succeeds_for_admin_after_grant() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    }));
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Operator)
    }));
}

#[test]
fn require_role_at_least_succeeds_for_super_admin_after_grant() {
    let (env, contract, super_admin) = setup_env();
    let new_super = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &new_super,
        AdminRole::SuperAdmin,
    );

    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), new_super.clone(), AdminRole::SuperAdmin)
    }));
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), new_super.clone(), AdminRole::Admin)
    }));
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), new_super.clone(), AdminRole::Operator)
    }));
}

#[test]
fn require_role_at_least_rejects_unknown_address() {
    let (env, contract, _super_admin) = setup_env();
    let stranger = user(&env);

    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), stranger.clone(), AdminRole::Operator)
    }));
}

// ---------------------------------------------------------------------------
// Role upgrade + use after — verify a promoted admin satisfies the new level
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_succeeds_for_operator_promoted_to_admin() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Admin,
        );
    });

    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), operator.clone(), AdminRole::Admin)
    }));
}

#[test]
fn require_role_at_least_succeeds_for_admin_promoted_to_super_admin() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            AdminRole::SuperAdmin,
        );
    });

    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::SuperAdmin)
    }));
}

// ---------------------------------------------------------------------------
// Role downgrade + use after — after demotion the old level no longer passes
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_fails_for_admin_demoted_to_operator() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            AdminRole::Operator,
        );
    });

    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    }));
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Operator)
    }));
}

#[test]
fn require_role_at_least_fails_for_super_admin_demoted_to_operator() {
    let (env, contract, super_admin) = setup_env();
    let new_super = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &new_super,
        AdminRole::SuperAdmin,
    );

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            new_super.clone(),
            AdminRole::Operator,
        );
    });

    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), new_super.clone(), AdminRole::SuperAdmin)
    }));
    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), new_super.clone(), AdminRole::Admin)
    }));
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), new_super.clone(), AdminRole::Operator)
    }));
}

// ---------------------------------------------------------------------------
// Deactivation (revoke path 1) + use after — deactivated admin fails checks
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_fails_after_deactivation() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    }));
    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Operator)
    }));
}

#[test]
fn require_role_at_least_succeeds_after_reactivation() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });
    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::reactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    }));
}

// ---------------------------------------------------------------------------
// Removal (revoke path 2) + use after — removed admin panics on lookup
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_panics_after_removal() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::remove_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Operator)
    }));
}

// ---------------------------------------------------------------------------
// Suspension + use during / after expiry
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_fails_during_suspension() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let until_ts = env.ledger().timestamp() + 3600;
    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::suspend_admin(env.clone(), super_admin.clone(), admin.clone(), until_ts);
    });

    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Operator)
    }));
}

#[test]
fn require_role_at_least_succeeds_after_suspension_expiry() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let until_ts = env.ledger().timestamp() + 3600;
    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::suspend_admin(env.clone(), super_admin.clone(), admin.clone(), until_ts);
    });

    advance(&env, 7200);

    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    }));
}

// ---------------------------------------------------------------------------
// Entrypoint-level verification — require_role_at_least gates add_admin
// ---------------------------------------------------------------------------

#[test]
fn operator_cannot_add_admin_after_use_before() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    let target = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    env.mock_all_auths();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                operator.clone(),
                target.clone(),
                AdminRole::Operator,
            );
        });
    }));
    assert!(
        result.is_err(),
        "operator must not be allowed to add another admin"
    );
}

#[test]
fn operator_can_add_admin_after_promotion_to_admin() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    let target = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Admin,
        );
    });

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            operator.clone(),
            target.clone(),
            AdminRole::Operator,
        );
    });

    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), target.clone(), AdminRole::Operator)
    }));
}

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn operator_cannot_add_admin_after_role_revoked_by_deactivation() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    let target = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), operator.clone());
    });

    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            operator.clone(),
            target.clone(),
            AdminRole::Operator,
        );
    });
}

// ---------------------------------------------------------------------------
// Suspended_until = 0 — the sentinel for "not suspended" works
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_respects_suspended_until_zero() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let info = env.as_contract(&contract, || {
        AdminContract::get_admin_info(env.clone(), admin.clone())
    });
    assert_eq!(info.suspended_until, 0);

    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    }));
}


// ---------------------------------------------------------------------------
// Determinism — repeated calls with identical input never mutate state
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_is_deterministic_and_side_effect_free() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let epoch_before = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    let first = env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    });
    let second = env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    });

    let epoch_after = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    assert_eq!(first, second, "repeated checks must return the same result");
    assert!(first);
    assert_eq!(
        epoch_before, epoch_after,
        "a pure role check must never advance the config epoch"
    );
}

// ---------------------------------------------------------------------------
// Boundary — role exactly equal to required (not just strictly above)
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_exact_role_boundary_passes() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    // Operator checked against exactly Operator (the lowest tier) must pass.
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), operator.clone(), AdminRole::Operator)
    }));
    // Operator checked against the tier directly above must fail.
    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), operator.clone(), AdminRole::Admin)
    }));
}

// ---------------------------------------------------------------------------
// Boundary — suspension expiry at the exact timestamp (inclusive `>=`)
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_boundary_at_exact_suspension_expiry() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let until_ts = env.ledger().timestamp() + 100;
    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::suspend_admin(env.clone(), super_admin.clone(), admin.clone(), until_ts);
    });

    // One second before expiry: still suspended, check must fail.
    advance(&env, 99);
    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Operator)
    }));

    // Exactly at expiry: must pass (inclusive boundary).
    advance(&env, 1);
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Operator)
    }));
}

// ---------------------------------------------------------------------------
// Invalid input — unregistered / sentinel address never satisfies the check
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_never_satisfied_for_never_registered_address() {
    let (env, contract, _super_admin) = setup_env();
    let random_address = user(&env);

    // Never added as an admin at all — every tier must reject it.
    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(
            env.clone(),
            random_address.clone(),
            AdminRole::Operator,
        )
    }));
    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), random_address.clone(), AdminRole::Admin)
    }));
    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(
            env.clone(),
            random_address.clone(),
            AdminRole::SuperAdmin,
        )
    }));
}

// ---------------------------------------------------------------------------
// Duplicate / no-op — reassigning the same role twice does not corrupt state
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_duplicate_role_update_is_noop_and_check_still_passes() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    let epoch_after_grant = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    // Re-assign the identical role. update_admin_role treats this as a no-op.
    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            AdminRole::Admin,
        );
    });

    let epoch_after_duplicate = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    assert_eq!(
        epoch_after_grant, epoch_after_duplicate,
        "a duplicate role assignment must not advance the config epoch"
    );
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    }));
}

// ---------------------------------------------------------------------------
// Failure/retry — a rejected privileged call never advances the epoch,
// so a client's stale-state retry contract stays valid
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_rejection_does_not_advance_epoch() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    let target = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    let epoch_before = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    // Operator lacks the role required to add another admin; this must panic
    // and roll back without committing any partial state.
    env.mock_all_auths();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                operator.clone(),
                target.clone(),
                AdminRole::Operator,
            );
        });
    }));
    assert!(result.is_err());

    let epoch_after = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });

    assert_eq!(
        epoch_before, epoch_after,
        "a rejected role check must leave the config epoch untouched, \
         so a caller's retry snapshot is still valid"
    );
    // The target must not have been partially admitted.
    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), target.clone(), AdminRole::Operator)
    }));
}

// ---------------------------------------------------------------------------
// Stale-state retry — epoch advances exactly once per committed change,
// letting a client detect a concurrent mutation and retry against fresh state
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_epoch_signals_concurrent_change_for_retry() {
    let (env, contract, super_admin) = setup_env();
    let admin = user(&env);
    as_admin(&env, &contract, &super_admin, &admin, AdminRole::Admin);

    // Client reads state at epoch N.
    let epoch_snapshot = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });
    let snapshot_passed = env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    });
    assert!(snapshot_passed);

    // A concurrent transaction demotes the admin before the client retries.
    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            AdminRole::Operator,
        );
    });

    let epoch_after_concurrent_change = env.as_contract(&contract, || {
        AdminContract::get_config_epoch(env.clone())
    });
    assert!(
        epoch_after_concurrent_change > epoch_snapshot,
        "a committed mutation must advance the epoch so the client can detect it"
    );

    // The client's stale snapshot is now wrong; re-checking against current
    // state (the retry) correctly reflects the demotion.
    let fresh_check = env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), admin.clone(), AdminRole::Admin)
    });
    assert!(
        !fresh_check,
        "retrying against fresh state must reflect the concurrent demotion"
    );
}

// ---------------------------------------------------------------------------
// Diagnosability without leakage — a rejected entrypoint call fails with a
// generic contract error and exposes no role/identity data
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #100)")]
fn require_role_at_least_failure_is_generic_not_admin_error() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    let target = user(&env);
    as_admin(
        &env,
        &contract,
        &super_admin,
        &operator,
        AdminRole::Operator,
    );

    // The panic message here is the fixed `NotAdmin` error code — it carries
    // no caller role, no target identity, and no internal state details.
    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            operator.clone(),
            target.clone(),
            AdminRole::Admin,
        );
    });
}