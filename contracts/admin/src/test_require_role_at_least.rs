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

fn as_admin(
    env: &Env,
    contract: &Address,
    caller: &Address,
    new_admin: &Address,
    role: AdminRole,
) {
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
    as_admin(&env, &contract, &super_admin, &operator, AdminRole::Operator);

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
        AdminContract::has_role_at_least(
            env.clone(),
            new_super.clone(),
            AdminRole::SuperAdmin,
        )
    }));
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), new_super.clone(), AdminRole::Admin)
    }));
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(
            env.clone(),
            new_super.clone(),
            AdminRole::Operator,
        )
    }));
}

#[test]
fn require_role_at_least_rejects_unknown_address() {
    let (env, contract, _super_admin) = setup_env();
    let stranger = user(&env);

    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(
            env.clone(),
            stranger.clone(),
            AdminRole::Operator,
        )
    }));
}

// ---------------------------------------------------------------------------
// Role upgrade + use after — verify a promoted admin satisfies the new level
// ---------------------------------------------------------------------------

#[test]
fn require_role_at_least_succeeds_for_operator_promoted_to_admin() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    as_admin(&env, &contract, &super_admin, &operator, AdminRole::Operator);

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
        AdminContract::has_role_at_least(
            env.clone(),
            admin.clone(),
            AdminRole::SuperAdmin,
        )
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
        AdminContract::has_role_at_least(
            env.clone(),
            admin.clone(),
            AdminRole::Operator,
        )
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
        AdminContract::has_role_at_least(
            env.clone(),
            new_super.clone(),
            AdminRole::SuperAdmin,
        )
    }));
    assert!(!env.as_contract(&contract, || {
        AdminContract::has_role_at_least(env.clone(), new_super.clone(), AdminRole::Admin)
    }));
    assert!(env.as_contract(&contract, || {
        AdminContract::has_role_at_least(
            env.clone(),
            new_super.clone(),
            AdminRole::Operator,
        )
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
    as_admin(&env, &contract, &super_admin, &operator, AdminRole::Operator);

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
    assert!(result.is_err(), "operator must not be allowed to add another admin");
}

#[test]
fn operator_can_add_admin_after_promotion_to_admin() {
    let (env, contract, super_admin) = setup_env();
    let operator = user(&env);
    let target = user(&env);
    as_admin(&env, &contract, &super_admin, &operator, AdminRole::Operator);

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
    as_admin(&env, &contract, &super_admin, &operator, AdminRole::Operator);

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
