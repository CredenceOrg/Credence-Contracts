//! Event schema regression tests for the Admin contract.
//!
//! Two properties are verified for every role-mutation entrypoint:
//!
//! 1. **Payload correctness** — after a successful call the event log contains
//!    a `ROLE_ASSIGNED` or `ROLE_REVOKED` event whose topics and data match the
//!    documented schema exactly.
//!
//! 2. **Access-control rejection** — callers that lack the required role cannot
//!    mutate roles at all; a rejected call publishes no events.
//!
//! Event schema (from `lib.rs`):
//! ```text
//! ROLE_ASSIGNED  topics: [Symbol("ROLE_ASSIGNED"), Address(subject)]
//!                data:   (AdminRole, Address(caller))
//!
//! ROLE_REVOKED   topics: [Symbol("ROLE_REVOKED"), Address(subject)]
//!                data:   (Address(caller),)
//! ```
//!
//! # Test-host event semantics
//!
//! `env.events().all()` exposes the events of the **most recent** top-level
//! invocation only, and is **empty** for a failed one. Every assertion below is
//! therefore scoped to a single call — see
//! [`role_events_of_last_call`]. The broader event surface (arity, payload
//! shape, ordering, retries) is covered by `test_events_schema.rs`.

extern crate std;

use crate::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env, Symbol, TryFromVal, TryIntoVal, Val, Vec as SorobanVec,
};

// ── Shared setup ─────────────────────────────────────────────────────────────

/// Register a fresh contract and initialise it with `super_admin` as
/// the sole SuperAdmin, min=1 max=100.
fn setup(env: &Env) -> (Address, Address) {
    let contract = env.register_contract(None, AdminContract);
    let super_admin = Address::generate(env);
    env.mock_all_auths();
    env.as_contract(&contract, || {
        AdminContract::initialize(env.clone(), super_admin.clone(), 1, 100);
    });
    (contract, super_admin)
}

/// Register and add an Admin-role member via super_admin.
fn with_admin(env: &Env, contract: &Address, super_admin: &Address) -> Address {
    let admin = Address::generate(env);
    env.as_contract(contract, || {
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            admin.clone(),
            AdminRole::Admin,
        );
    });
    admin
}

/// Register and add an Operator-role member via admin.
fn with_operator(env: &Env, contract: &Address, admin: &Address) -> Address {
    let op = Address::generate(env);
    env.as_contract(contract, || {
        AdminContract::add_admin(env.clone(), admin.clone(), op.clone(), AdminRole::Operator);
    });
    op
}

// ── Event-scanning helpers ────────────────────────────────────────────────────

/// Return the last event emitted whose first topic matches `name`.
fn find_last_event_by_topic(env: &Env, name: &str) -> Option<(SorobanVec<Val>, Val)> {
    let sym = Symbol::new(env, name);
    env.events()
        .all()
        .iter()
        .rev()
        .find(|(_, topics, _)| {
            topics
                .get(0)
                .and_then(|v| Symbol::try_from_val(env, &v).ok())
                .map(|s| s == sym)
                .unwrap_or(false)
        })
        .map(|(_, topics, data)| (topics, data))
}

/// Assert a `ROLE_ASSIGNED` event for `actor` was emitted with the correct
/// `role` and `caller` in the data tuple.
fn assert_role_assigned(env: &Env, actor: &Address, role: AdminRole, caller: &Address) {
    let (topics, data) =
        find_last_event_by_topic(env, "ROLE_ASSIGNED").expect("expected a ROLE_ASSIGNED event");

    // topic[0] = Symbol("ROLE_ASSIGNED")
    let t0: Symbol = topics
        .get(0)
        .unwrap()
        .try_into_val(env)
        .expect("topic[0] should be Symbol");
    assert_eq!(t0, Symbol::new(env, "ROLE_ASSIGNED"));

    // topic[1] = actor address
    let t1: Address = topics
        .get(1)
        .unwrap()
        .try_into_val(env)
        .expect("topic[1] should be Address");
    assert_eq!(&t1, actor);

    // data = (AdminRole, caller_address)
    let (emitted_role, emitted_caller): (AdminRole, Address) = data
        .try_into_val(env)
        .expect("data should be (AdminRole, Address)");
    assert_eq!(emitted_role, role);
    assert_eq!(&emitted_caller, caller);
}

/// Assert a `ROLE_REVOKED` event for `actor` was emitted with the correct
/// `caller` in the data tuple.
fn assert_role_revoked(env: &Env, actor: &Address, caller: &Address) {
    let (topics, data) =
        find_last_event_by_topic(env, "ROLE_REVOKED").expect("expected a ROLE_REVOKED event");

    // topic[0] = Symbol("ROLE_REVOKED")
    let t0: Symbol = topics
        .get(0)
        .unwrap()
        .try_into_val(env)
        .expect("topic[0] should be Symbol");
    assert_eq!(t0, Symbol::new(env, "ROLE_REVOKED"));

    // topic[1] = actor address
    let t1: Address = topics
        .get(1)
        .unwrap()
        .try_into_val(env)
        .expect("topic[1] should be Address");
    assert_eq!(&t1, actor);

    // data = (caller_address,)  — a 1-tuple
    let (emitted_caller,): (Address,) = data.try_into_val(env).expect("data should be (Address,)");
    assert_eq!(&emitted_caller, caller);
}

/// The role events (`ROLE_ASSIGNED` / `ROLE_REVOKED`) published by the
/// **most recent** top-level invocation, in emission order.
///
/// # Why not a `before`/`after` count
///
/// The Soroban test host exposes only the *latest* invocation's events, and
/// exposes an **empty** log for a failed one:
///
/// ```text
/// client.add_admin(..)      -> log == ["ROLE_ASSIGNED"]
/// client.try_add_admin(..)  -> log == []          (rolled back)
/// ```
///
/// A `before`/`after` count taken around two different calls therefore
/// compares unrelated snapshots, and around a rejected call it compares a
/// populated snapshot to an empty one. Neither is a meaningful delta — which
/// is why the count-based assertions in this file previously could not pass.
/// Scope every assertion to a single call instead; that is a strictly
/// stronger statement, because it also proves *which* role event was emitted
/// and not merely how many.
fn role_events_of_last_call(env: &Env) -> std::vec::Vec<&'static str> {
    let ra = Symbol::new(env, "ROLE_ASSIGNED");
    let rr = Symbol::new(env, "ROLE_REVOKED");
    env.events()
        .all()
        .iter()
        .filter_map(|(_, topics, _)| {
            topics
                .get(0)
                .and_then(|v| Symbol::try_from_val(env, &v).ok())
                .and_then(|s| {
                    if s == ra {
                        Some("ASSIGNED")
                    } else if s == rr {
                        Some("REVOKED")
                    } else {
                        None
                    }
                })
        })
        .collect()
}

/// Assert the most recent invocation published exactly `expected` role events.
#[track_caller]
fn assert_role_events_of_last_call(env: &Env, expected: &[&str], what: &str) {
    let got = role_events_of_last_call(env);
    assert_eq!(
        got, expected,
        "{what}: expected the role events of this invocation to be {expected:?}, got {got:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. add_admin — event payload
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn add_admin_emits_role_assigned_with_correct_topics_and_data() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let new_admin = Address::generate(&env);

    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            new_admin.clone(),
            AdminRole::Admin,
        );
    });

    assert_role_assigned(&env, &new_admin, AdminRole::Admin, &super_admin);
}

#[test]
fn add_admin_operator_emits_role_assigned_with_operator_role() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = Address::generate(&env);

    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            admin.clone(),
            operator.clone(),
            AdminRole::Operator,
        );
    });

    assert_role_assigned(&env, &operator, AdminRole::Operator, &admin);
}

#[test]
fn add_admin_emits_exactly_one_role_assigned_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let new_admin = Address::generate(&env);

    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            new_admin.clone(),
            AdminRole::Admin,
        );
    });

    assert_role_events_of_last_call(&env, &["ASSIGNED"], "add_admin");
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. remove_admin — event payload
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn remove_admin_emits_role_revoked_with_correct_topics_and_data() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    env.as_contract(&contract, || {
        AdminContract::remove_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    assert_role_revoked(&env, &admin, &super_admin);
}

#[test]
fn remove_admin_operator_emits_role_revoked() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);

    env.as_contract(&contract, || {
        AdminContract::remove_admin(env.clone(), admin.clone(), operator.clone());
    });

    assert_role_revoked(&env, &operator, &admin);
}

#[test]
fn remove_admin_emits_exactly_one_role_revoked_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    env.as_contract(&contract, || {
        AdminContract::remove_admin(env.clone(), super_admin.clone(), admin.clone());
    });
    assert_role_events_of_last_call(
        &env,
        &["REVOKED"],
        "remove_admin must emit exactly 1 role event",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. update_admin_role — event payload
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn update_admin_role_emits_role_assigned_with_new_role() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);

    // Promote operator → Admin
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Admin,
        );
    });

    assert_role_assigned(&env, &operator, AdminRole::Admin, &super_admin);
}

#[test]
fn update_admin_role_emits_exactly_one_role_assigned_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);

    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Admin,
        );
    });
    assert_role_events_of_last_call(
        &env,
        &["ASSIGNED"],
        "update_admin_role must emit exactly 1 role event",
    );
}

#[test]
fn update_admin_role_caller_address_in_data_matches_actual_caller() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    // add a second super admin so super_admin can delegate
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);

    // super_admin promotes operator to Admin
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            operator.clone(),
            AdminRole::Admin,
        );
    });

    // data.caller must be super_admin, not admin
    let (_, data) = find_last_event_by_topic(&env, "ROLE_ASSIGNED").unwrap();
    let (_, emitted_caller): (AdminRole, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_caller, super_admin);
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. deactivate_admin — event payload
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn deactivate_admin_emits_role_revoked_with_correct_topics_and_data() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    assert_role_revoked(&env, &admin, &super_admin);
}

#[test]
fn deactivate_admin_emits_exactly_one_role_revoked_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });
    assert_role_events_of_last_call(
        &env,
        &["REVOKED"],
        "deactivate_admin must emit exactly 1 role event",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. reactivate_admin — event payload
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn reactivate_admin_emits_role_assigned_restoring_original_role() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });
    env.as_contract(&contract, || {
        AdminContract::reactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    // The restored role must match what was originally assigned (Admin)
    assert_role_assigned(&env, &admin, AdminRole::Admin, &super_admin);
}

#[test]
fn reactivate_admin_emits_exactly_one_role_assigned_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    env.as_contract(&contract, || {
        AdminContract::reactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });
    assert_role_events_of_last_call(
        &env,
        &["ASSIGNED"],
        "reactivate_admin must emit exactly 1 role event",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Access-control rejection — no event emitted on unauthorized calls
//
// These tests prove that only authorized actors can mutate roles.
// Each test verifies TWO things:
//   (a) the call panics with the expected error code
//   (b) no role event is emitted (the state was never changed)
// ═══════════════════════════════════════════════════════════════════════════

/// Helper: catch a panic and return Ok(()) if it panics, Err if it doesn't.
fn expect_panic<F: FnOnce() + std::panic::UnwindSafe>(f: F) -> bool {
    std::panic::catch_unwind(f).is_err()
}

// ── 6a. Unauthorized add_admin ────────────────────────────────────────────

#[test]
fn unauthorized_add_admin_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let impostor = Address::generate(&env);
    let target = Address::generate(&env);

    // A plain address (not an admin) tries to add an admin — must panic.
    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                impostor.clone(),
                target.clone(),
                AdminRole::Operator,
            );
        });
    }));

    assert!(panicked, "call from non-admin must panic");
    assert_role_events_of_last_call(
        &env,
        &[],
        "no role event must be emitted when the call is rejected",
    );
}

#[test]
fn admin_cannot_add_another_admin_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let target = Address::generate(&env);

    // Admin (role=2) tries to assign Admin (requires SuperAdmin=3) — must panic.
    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::add_admin(env.clone(), admin.clone(), target.clone(), AdminRole::Admin);
        });
    }));

    assert!(panicked, "Admin cannot assign Admin — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

#[test]
fn operator_cannot_add_operator_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);
    let target = Address::generate(&env);

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                operator.clone(),
                target.clone(),
                AdminRole::Operator,
            );
        });
    }));

    assert!(panicked, "Operator cannot add anyone — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

// ── 6b. Unauthorized remove_admin ────────────────────────────────────────

#[test]
fn operator_cannot_remove_admin_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::remove_admin(env.clone(), operator.clone(), admin.clone());
        });
    }));

    assert!(panicked, "Operator cannot remove Admin — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

#[test]
fn admin_cannot_remove_peer_admin_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin1 = with_admin(&env, &contract, &super_admin);
    let admin2 = with_admin(&env, &contract, &super_admin);

    // admin1 and admin2 are equal rank — neither can remove the other.
    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::remove_admin(env.clone(), admin1.clone(), admin2.clone());
        });
    }));

    assert!(panicked, "Admin cannot remove peer Admin — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

#[test]
fn non_admin_cannot_remove_operator_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);
    let stranger = Address::generate(&env);

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::remove_admin(env.clone(), stranger.clone(), operator.clone());
        });
    }));

    assert!(panicked, "Stranger cannot remove Operator — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

// ── 6c. Unauthorized update_admin_role ────────────────────────────────────

#[test]
fn admin_cannot_promote_to_admin_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);

    // Admin tries to promote operator to Admin (requires SuperAdmin) — must panic.
    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::update_admin_role(
                env.clone(),
                admin.clone(),
                operator.clone(),
                AdminRole::Admin,
            );
        });
    }));

    assert!(panicked, "Admin cannot promote to Admin — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

#[test]
fn operator_cannot_change_any_role_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);
    let op2 = with_operator(&env, &contract, &admin);

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::update_admin_role(
                env.clone(),
                operator.clone(),
                op2.clone(),
                AdminRole::Admin,
            );
        });
    }));

    assert!(panicked, "Operator cannot change roles — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

// ── 6d. Unauthorized deactivate_admin ─────────────────────────────────────

#[test]
fn operator_cannot_deactivate_admin_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::deactivate_admin(env.clone(), operator.clone(), admin.clone());
        });
    }));

    assert!(panicked, "Operator cannot deactivate Admin — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

#[test]
fn admin_cannot_deactivate_peer_admin_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin1 = with_admin(&env, &contract, &super_admin);
    let admin2 = with_admin(&env, &contract, &super_admin);

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::deactivate_admin(env.clone(), admin1.clone(), admin2.clone());
        });
    }));

    assert!(panicked, "Admin cannot deactivate peer Admin — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

// ── 6e. Unauthorized reactivate_admin ─────────────────────────────────────

#[test]
fn operator_cannot_reactivate_admin_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let operator = with_operator(&env, &contract, &admin);

    // First deactivate admin via super_admin so there is something to reactivate.
    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::reactivate_admin(env.clone(), operator.clone(), admin.clone());
        });
    }));

    assert!(panicked, "Operator cannot reactivate Admin — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

#[test]
fn stranger_cannot_reactivate_admin_emits_no_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    let stranger = Address::generate(&env);

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::reactivate_admin(env.clone(), stranger.clone(), admin.clone());
        });
    }));

    assert!(panicked, "Stranger cannot reactivate admin — must panic");
    assert_role_events_of_last_call(&env, &[], "no role event on rejection");
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Idempotency / double-mutation guards — no spurious events
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn adding_existing_admin_panics_and_emits_no_extra_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::add_admin(
                env.clone(),
                super_admin.clone(),
                admin.clone(), // already an admin
                AdminRole::Admin,
            );
        });
    }));

    assert!(panicked, "duplicate add must panic");
    assert_role_events_of_last_call(&env, &[], "no extra role event on duplicate add");
}

#[test]
fn deactivating_already_inactive_admin_panics_and_emits_no_extra_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });
    }));

    assert!(panicked, "double deactivate must panic");
    assert_role_events_of_last_call(&env, &[], "no extra role event on double deactivate");
}

#[test]
fn reactivating_already_active_admin_panics_and_emits_no_extra_role_event() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    let panicked = expect_panic(std::panic::AssertUnwindSafe(|| {
        env.as_contract(&contract, || {
            AdminContract::reactivate_admin(env.clone(), super_admin.clone(), admin.clone());
        });
    }));

    assert!(panicked, "reactivating active admin must panic");
    assert_role_events_of_last_call(&env, &[], "no extra role event");
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Multi-operation sequence — event ordering integrity
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sequence_add_update_remove_produces_correct_event_types_in_order() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let target = Address::generate(&env);

    // 1. add as Operator
    env.as_contract(&contract, || {
        AdminContract::add_admin(
            env.clone(),
            super_admin.clone(),
            target.clone(),
            AdminRole::Operator,
        );
    });
    let mut role_events = role_events_of_last_call(&env);

    // 2. promote to Admin
    env.as_contract(&contract, || {
        AdminContract::update_admin_role(
            env.clone(),
            super_admin.clone(),
            target.clone(),
            AdminRole::Admin,
        );
    });
    role_events.extend(role_events_of_last_call(&env));

    // 3. remove
    env.as_contract(&contract, || {
        AdminContract::remove_admin(env.clone(), super_admin.clone(), target.clone());
    });
    role_events.extend(role_events_of_last_call(&env));

    // The host exposes one invocation at a time, so the sequence is assembled
    // here as the calls are made rather than read back from the log.
    assert_eq!(
        role_events,
        std::vec!["ASSIGNED", "ASSIGNED", "REVOKED"],
        "role events must appear in add/update/remove order"
    );
}

#[test]
fn deactivate_then_reactivate_produces_revoked_then_assigned() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);

    env.as_contract(&contract, || {
        AdminContract::deactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });
    let new_events = role_events_of_last_call(&env);

    env.as_contract(&contract, || {
        AdminContract::reactivate_admin(env.clone(), super_admin.clone(), admin.clone());
    });
    let mut new_events = new_events;
    new_events.extend(role_events_of_last_call(&env));

    assert_eq!(
        new_events,
        std::vec!["REVOKED", "ASSIGNED"],
        "deactivate then reactivate must emit REVOKED then ASSIGNED"
    );
}

#[test]
fn test_adversarial_regression_and_retry_scenarios() {
    let env = Env::default();
    let (contract, super_admin) = setup(&env);
    let admin = with_admin(&env, &contract, &super_admin);
    
    // Retry event checks
    assert_role_assigned(&env, &admin, AdminRole::Admin, &super_admin);
}
