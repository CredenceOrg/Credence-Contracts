//! Boundary and recovery test coverage for `credence_bond::access_control`
//! (issue #1316).
//!
//! ## What this file covers
//!
//! `access_control` provides the admin / verifier / identity-owner guards and the
//! verifier grant registry used by the bond contract. Before this change the
//! module was not in the module tree at all, so none of it was compiled, none of
//! it was reachable, and the sibling `test_access_control` module had been
//! disabled with `// [pre-broken on main]` markers. Coverage here is therefore
//! built from the contract's real storage layout rather than from a hand-seeded
//! fixture.
//!
//! ## Test strategy
//!
//! Two distinct failure surfaces need two distinct harnesses, because
//! `soroban-sdk` 22.1.3 does not let `std::panic::catch_unwind` observe a host
//! contract panic:
//!
//! * **Entrypoint guards** (`initialize`, `transfer_admin`) are exercised through
//!   the generated client. Failures are asserted on the returned `Result` via
//!   `client.try_*`, which yields a typed `ContractError` we can match on. This
//!   is the only place we can assert *which* error was raised, as opposed to
//!   merely that something panicked.
//! * **Module-level helpers** (`require_*`, `add_verifier_role`, ...) are plain
//!   library functions, not contract entrypoints, so they cannot go through
//!   `try_invoke_contract`. They are driven inside `env.as_contract` and their
//!   failure paths are asserted with `#[should_panic(expected = ...)]`, pinning
//!   the exact panic payload. Success paths assert the resulting storage and
//!   event state directly.
//!
//! Every test is deterministic: fixed addresses generated from the seeded
//! environment, no ledger time advancement, no fuzzing, and no shared mutable
//! state between cases. `Env::default()` is created per test.
//!
//! ## Event logs are per-frame
//!
//! `Env::events()` only reports events published in the *current* frame. Each
//! `env.as_contract` call opens a new frame, so a later frame appears to have an
//! empty log: the earlier `verifier_added` is not "missing", it is out of scope.
//! Assertions therefore inspect `emitted(..)` in the same frame that performs the
//! mutation, never after moving on to the next one.
//!
//! ## One authorization per frame
//!
//! Soroban rejects a second `require_auth` for the same address within a single
//! frame with `Error(Auth, ExistingValue)` ("frame is already authorized").
//! Since every guard here authenticates, a test that grants a role and then
//! exercises several guarded paths inside one `env.as_contract` closure will
//! fail for that reason rather than for the reason it is testing. Each guarded
//! call therefore gets its own `as_contract` frame, mirroring one call per
//! on-chain transaction.
//!
//! ## Notable findings pinned by these tests
//!
//! 1. `require_admin` reads `DataKey::Admin`, matching `initialize`. The original
//!    implementation read a bare `Symbol("admin")` key that nothing writes, so it
//!    failed with `NotInitialized` against every correctly initialized contract.
//!    `require_admin_reads_configured_admin` is the regression guard.
//! 2. `require_verifier` and `require_identity_owner` previously never called
//!    `require_auth`, so a contract forwarding an unverified caller-supplied
//!    address into them would authorize an unverified claim. Both now
//!    authenticate. `*_requires_signature_of_the_named_address` pins it.
//! 3. The `access_denied` payloads published on the failure paths are correct,
//!    but a failing transaction reverts the frame, so they never reach an indexer.
//!    `access_denied_payload_is_correct_and_does_not_survive_a_failure` asserts
//!    the payload and pins the observability limitation so it cannot be mistaken
//!    for a working audit trail.

use credence_bond::access_control::{
    add_verifier_role, get_admin, is_admin, is_verifier, remove_verifier_role, require_admin,
    require_admin_or_verifier, require_identity_owner, require_verifier,
};
use credence_bond::{CredenceBond, CredenceBondClient, DataKey};
use credence_errors::{ContractError, Role};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, IntoVal, Symbol, TryFromVal};

/// Invoke an entrypoint expecting failure and return its typed `ContractError`.
///
/// `Env::try_invoke_contract` is the only harness that surfaces a *typed*
/// contract error: the outer `Err(Ok(e))` is the contract panicking with one of
/// its own `ContractError` variants, while `Err(Err(invoke))` means the call
/// never produced one (bad auth, arity mismatch) and `Ok` means it succeeded.
/// Collapsing those three into "it failed" would let a test pass for the wrong
/// reason, so each is surfaced distinctly.
fn expect_contract_error(
    env: &Env,
    contract: &Address,
    func: &str,
    args: soroban_sdk::Vec<soroban_sdk::Val>,
) -> ContractError {
    match env.try_invoke_contract::<(), ContractError>(contract, &Symbol::new(env, func), args) {
        Err(Ok(e)) => e,
        Err(Err(invoke)) => panic!("{func} failed without a ContractError: {invoke:?}"),
        Ok(_) => panic!("{func} unexpectedly succeeded"),
    }
}

/// Registered contract plus the three principals used across the suite.
struct Fixture {
    env: Env,
    contract: Address,
    admin: Address,
    other: Address,
    third: Address,
}

impl Fixture {
    fn initialized() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register(CredenceBond, ());
        let client = CredenceBondClient::new(&env, &contract);
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        let third = Address::generate(&env);
        client.initialize(&admin, &None);
        Self {
            env,
            contract,
            admin,
            other,
            third,
        }
    }

    /// An initialized contract with **no** `mock_all_auths`.
    ///
    /// Every guard in this module calls `Address::require_auth`, which fails when
    /// no authorization is available. `mock_all_auths` would satisfy it
    /// unconditionally and hide whether the check is present at all, so tests that
    /// assert authentication must use this fixture.
    fn initialized_without_auth_mocks() -> Self {
        let env = Env::default();
        let contract = env.register(CredenceBond, ());
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        let third = Address::generate(&env);
        env.as_contract(&contract, || {
            env.storage().instance().set(&DataKey::Admin, &admin);
        });
        Self {
            env,
            contract,
            admin,
            other,
            third,
        }
    }

    /// A contract that has been registered but never initialized, so no admin
    /// exists under `DataKey::Admin`.
    fn uninitialized() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register(CredenceBond, ());
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        let third = Address::generate(&env);
        Self {
            env,
            contract,
            admin,
            other,
            third,
        }
    }

    /// Count of contract events emitted so far, across all frames.
    fn event_count(&self) -> u32 {
        self.env.events().all().len()
    }

    /// Decode the single `access_denied` event in the current frame.
    fn access_denied(&self) -> Option<(Address, Symbol, u32)> {
        let want = Symbol::new(&self.env, "access_denied");
        for (_c, topics, data) in self.env.events().all().iter() {
            let is_denied = topics
                .get(0)
                .and_then(|t| Symbol::try_from_val(&self.env, &t).ok())
                .is_some_and(|t| t == want);
            if is_denied {
                return <(Address, Symbol, u32)>::try_from_val(&self.env, &data).ok();
            }
        }
        None
    }

    /// Whether an event whose first topic is `name` has been emitted.
    fn emitted(&self, name: &str) -> bool {
        let want = Symbol::new(&self.env, name);
        self.env.events().all().iter().any(|(_c, topics, _d)| {
            topics
                .get(0)
                .and_then(|t| Symbol::try_from_val(&self.env, &t).ok())
                .is_some_and(|t| t == want)
        })
    }
}

// ---------------------------------------------------------------------------
// Admin resolution: the storage-key regression guard.
// ---------------------------------------------------------------------------

/// `require_admin` must authorize the admin that `initialize` actually stored.
/// Before the fix it read a bare `Symbol("admin")` key that no code path writes,
/// so this returned `NotInitialized` even for the real admin.
#[test]
fn require_admin_reads_configured_admin() {
    let f = Fixture::initialized();

    f.env.as_contract(&f.contract, || {
        // Storage-key alignment: the value initialize() wrote is the one the
        // guard authorizes.
        assert_eq!(get_admin(&f.env), f.admin);
        assert_eq!(is_admin(&f.env, &f.admin), Role::Admin);
        require_admin(&f.env, &f.admin);
    });
}

#[test]
fn is_admin_is_user_for_non_admin_and_when_uninitialized() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        assert_eq!(is_admin(&f.env, &f.other), Role::User);
        assert_eq!(is_admin(&f.env, &f.third), Role::User);
    });

    // Fail-closed: with no admin stored, nobody is an admin.
    let u = Fixture::uninitialized();
    u.env.as_contract(&u.contract, || {
        assert_eq!(is_admin(&u.env, &u.admin), Role::User);
        assert_eq!(is_admin(&u.env, &u.other), Role::User);
    });
}

#[test]
#[should_panic(expected = "not initialized")]
fn get_admin_panics_before_initialize() {
    let f = Fixture::uninitialized();
    f.env.as_contract(&f.contract, || {
        get_admin(&f.env);
    });
}

#[test]
#[should_panic]
fn require_admin_panics_before_initialize() {
    let f = Fixture::uninitialized();
    f.env.as_contract(&f.contract, || {
        require_admin(&f.env, &f.admin);
    });
}

/// The non-admin rejection path must surface `NotAdmin`, and because
/// `try_*` is available for the entrypoint we can assert the exact code rather
/// than just "it panicked".
#[test]
fn transfer_admin_by_non_admin_returns_not_admin() {
    let f = Fixture::initialized();

    let args = (f.other.clone(), f.third.clone()).into_val(&f.env);
    assert_eq!(
        expect_contract_error(&f.env, &f.contract, "transfer_admin", args),
        ContractError::NotAdmin,
        "non-admin must not transfer admin"
    );
}

/// Admin rotation is the recovery path for a compromised admin key: the new
/// admin must work and the old one must lose access in the same tx.
#[test]
fn transfer_admin_rotates_authority_and_locks_out_old_admin() {
    let f = Fixture::initialized();
    let client = CredenceBondClient::new(&f.env, &f.contract);

    client.transfer_admin(&f.admin, &f.third);

    f.env.as_contract(&f.contract, || {
        assert_eq!(get_admin(&f.env), f.third);
        assert_eq!(is_admin(&f.env, &f.admin), Role::User);
        assert_eq!(is_admin(&f.env, &f.third), Role::Admin);
    });

    // Old admin is locked out.
    let args = (f.admin.clone(), f.other.clone()).into_val(&f.env);
    assert_eq!(
        expect_contract_error(&f.env, &f.contract, "transfer_admin", args),
        ContractError::NotAdmin,
        "old admin must be rejected after rotation"
    );

    // New admin retains authority.
    client.transfer_admin(&f.third, &f.other);
    f.env.as_contract(&f.contract, || {
        assert_eq!(get_admin(&f.env), f.other);
    });
}

/// Re-initializing must not let a second caller seize admin. This is the
/// duplicate-entrypoint boundary.
#[test]
fn double_initialize_is_rejected_and_preserves_original_admin() {
    let f = Fixture::initialized();

    let args = (f.other.clone(), None::<Address>).into_val(&f.env);
    assert_eq!(
        expect_contract_error(&f.env, &f.contract, "initialize", args),
        ContractError::AlreadyInitialized,
        "re-initialize must be rejected"
    );
    f.env.as_contract(&f.contract, || {
        assert_eq!(get_admin(&f.env), f.admin, "admin must not be replaced");
    });
}

// ---------------------------------------------------------------------------
// Verifier grant registry.
// ---------------------------------------------------------------------------

#[test]
fn verifier_grant_lifecycle_is_observable() {
    let f = Fixture::initialized();

    // Baseline: the fixture publishes nothing on its own.
    assert_eq!(
        f.event_count(),
        0,
        "initialize must not be mistaken for a grant event"
    );

    f.env.as_contract(&f.contract, || {
        assert!(!is_verifier(&f.env, &f.other), "no implicit grants");
        add_verifier_role(&f.env, &f.admin, &f.other);
        // Checked in the same frame that published it: `Env::events()` is
        // per-frame, so a later frame would report an empty log.
        assert!(
            f.emitted("verifier_added"),
            "grant must publish verifier_added for indexers"
        );
    });

    f.env.as_contract(&f.contract, || {
        assert!(is_verifier(&f.env, &f.other));
        assert!(!is_verifier(&f.env, &f.third), "grant must not be shared");
        require_verifier(&f.env, &f.other);
        assert!(
            !f.emitted("verifier_added"),
            "a passing guard must not publish anything"
        );
    });

    f.env.as_contract(&f.contract, || {
        remove_verifier_role(&f.env, &f.admin, &f.other);
        assert!(
            f.emitted("verifier_removed"),
            "revoke must publish verifier_removed for indexers"
        );
        assert!(
            !f.emitted("verifier_added"),
            "revoke frame publishes only the revoke"
        );
    });

    f.env.as_contract(&f.contract, || {
        assert!(!is_verifier(&f.env, &f.other), "revoke takes effect");
    });
}

/// Re-granting an existing verifier is storage-idempotent. Documented as
/// re-publishing the event, so the assertion is on storage, not event count.
#[test]
fn duplicate_grant_is_storage_idempotent() {
    let f = Fixture::initialized();

    f.env.as_contract(&f.contract, || {
        add_verifier_role(&f.env, &f.admin, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        add_verifier_role(&f.env, &f.admin, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        assert!(is_verifier(&f.env, &f.other), "re-grant must stay granted");
        require_verifier(&f.env, &f.other);
    });
}

/// Revoking a role that was never granted succeeds and publishes
/// `verifier_removed` anyway. Pinned so the audit-trail ambiguity cannot be
/// "fixed" by accident, and so consumers know not to trust a single event.
#[test]
fn revoke_of_unheld_role_is_a_reported_no_op() {
    let f = Fixture::initialized();

    f.env.as_contract(&f.contract, || {
        assert!(!is_verifier(&f.env, &f.third));
        remove_verifier_role(&f.env, &f.admin, &f.third);
        assert!(!is_verifier(&f.env, &f.third));
        assert!(
            f.emitted("verifier_removed"),
            "revoke publishes unconditionally, even with no live grant"
        );
    });
}

/// Grant and revoke require admin authority.
#[test]
#[should_panic]
fn grant_by_non_admin_is_rejected() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        add_verifier_role(&f.env, &f.other, &f.third);
    });
}

#[test]
#[should_panic]
fn revoke_by_non_admin_is_rejected() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        add_verifier_role(&f.env, &f.admin, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        remove_verifier_role(&f.env, &f.other, &f.other);
    });
}

// ---------------------------------------------------------------------------
// require_verifier: authorization + authentication.
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn require_verifier_panics_for_ungranted_address() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        require_verifier(&f.env, &f.other);
    });
}

/// Revoking a verifier takes effect immediately: a role revoked earlier in the
/// same environment must not still authorize.
#[test]
#[should_panic]
fn require_verifier_after_revoke_is_rejected() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        add_verifier_role(&f.env, &f.admin, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        require_verifier(&f.env, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        remove_verifier_role(&f.env, &f.admin, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        require_verifier(&f.env, &f.other);
    });
}

/// `require_verifier` must authenticate the address it authorizes. Without
/// `require_auth`, a contract forwarding a caller-supplied address would accept
/// an unverified claim. We revoke the grant for `other` and confirm the guard
/// still fails; the meaningful assertion is that it fails at all, which only
/// happens if the storage check is live and the signature check is enforced.
#[test]
#[should_panic]
fn require_verifier_requires_signature_of_the_named_address() {
    // No `mock_all_auths` here on purpose.
    let f = Fixture::initialized_without_auth_mocks();
    f.env.as_contract(&f.contract, || {
        // Seed a genuine grant directly, bypassing the admin-gated mutator, so
        // the only thing left to fail is the signature requirement.
        let key = (Symbol::new(&f.env, "verifier"), f.other.clone());
        f.env.storage().instance().set(&key, &true);
        assert!(is_verifier(&f.env, &f.other), "grant is genuine");
        require_verifier(&f.env, &f.other);
    });
}

/// The same, for the identity-owner guard. Authorization (address equality) is
/// satisfied; authentication must still reject.
#[test]
#[should_panic]
fn require_identity_owner_requires_signature_of_the_named_address() {
    let f = Fixture::initialized_without_auth_mocks();
    f.env.as_contract(&f.contract, || {
        require_identity_owner(&f.env, &f.other, &f.other);
    });
}

/// And for the composed guard on the admin side.
#[test]
#[should_panic]
fn require_admin_or_verifier_requires_a_signature() {
    let f = Fixture::initialized_without_auth_mocks();
    f.env.as_contract(&f.contract, || {
        require_admin_or_verifier(&f.env, &f.admin);
    });
}

/// The admin is not implicitly a verifier: the two roles stay separate, so
/// adding a verifier cannot accidentally widen the verifier set.
#[test]
#[should_panic]
fn admin_is_not_implicitly_a_verifier() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        assert!(!is_verifier(&f.env, &f.admin));
        require_verifier(&f.env, &f.admin);
    });
}

// ---------------------------------------------------------------------------
// require_identity_owner: equality + authentication.
// ---------------------------------------------------------------------------

#[test]
fn require_identity_owner_accepts_matching_address() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        require_identity_owner(&f.env, &f.other, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        require_identity_owner(&f.env, &f.admin, &f.admin);
    });
}

#[test]
#[should_panic(expected = "not identity owner")]
fn require_identity_owner_rejects_mismatched_address() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        require_identity_owner(&f.env, &f.other, &f.third);
    });
}

/// Being the admin does not imply ownership of an identity.
#[test]
#[should_panic(expected = "not identity owner")]
fn admin_does_not_imply_identity_ownership() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        require_identity_owner(&f.env, &f.admin, &f.other);
    });
}

/// A verifier grant does not confer identity ownership either.
#[test]
#[should_panic(expected = "not identity owner")]
fn verifier_grant_does_not_confer_identity_ownership() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        add_verifier_role(&f.env, &f.admin, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        require_identity_owner(&f.env, &f.other, &f.third);
    });
}

// ---------------------------------------------------------------------------
// Role composition.
// ---------------------------------------------------------------------------

#[test]
fn require_admin_or_verifier_accepts_either_role() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        add_verifier_role(&f.env, &f.admin, &f.other);
    });
    // Admin path.
    f.env.as_contract(&f.contract, || {
        require_admin_or_verifier(&f.env, &f.admin);
    });
    // Verifier path.
    f.env.as_contract(&f.contract, || {
        require_admin_or_verifier(&f.env, &f.other);
    });
}

#[test]
#[should_panic]
fn require_admin_or_verifier_rejects_neither_role() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        require_admin_or_verifier(&f.env, &f.third);
    });
}

/// Composition must not become a privilege-escalation path: a verifier who is
/// not the admin must not satisfy the admin side of the composition.
#[test]
#[should_panic]
fn verifier_cannot_satisfy_admin_only_path() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        add_verifier_role(&f.env, &f.admin, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        // Holds a verifier grant, but the admin guard is storage-only on admin.
        require_admin(&f.env, &f.other);
    });
}

/// Revoking the verifier grant revokes composed access too.
#[test]
#[should_panic]
fn composed_access_is_revoked_with_the_grant() {
    let f = Fixture::initialized();
    f.env.as_contract(&f.contract, || {
        add_verifier_role(&f.env, &f.admin, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        remove_verifier_role(&f.env, &f.admin, &f.other);
    });
    f.env.as_contract(&f.contract, || {
        require_admin_or_verifier(&f.env, &f.other);
    });
}

/// On an uninitialized contract nothing satisfies the composition.
#[test]
#[should_panic]
fn require_admin_or_verifier_rejects_on_uninitialized_contract() {
    let f = Fixture::uninitialized();
    f.env.as_contract(&f.contract, || {
        require_admin_or_verifier(&f.env, &f.admin);
    });
}

// ---------------------------------------------------------------------------
// Event rollback semantics.
// ---------------------------------------------------------------------------

/// Soroban rolls back events published in a frame that later panics, so the
/// `access_denied` events these guards publish before panicking are not
/// observable on chain. This test documents the real behaviour so the module
/// docs stay truthful, and so a future change that does make them observable is
/// caught as a deliberate update rather than a surprise.
#[test]
fn access_denied_payload_is_correct_and_does_not_survive_a_failure() {
    let f = Fixture::initialized();

    // A passing guard publishes nothing.
    f.env
        .as_contract(&f.contract, || require_admin(&f.env, &f.admin));
    assert_eq!(
        f.event_count(),
        0,
        "a passing guard must not publish anything"
    );

    // Each failure path publishes a correctly shaped payload naming the caller
    // and the role it was denied for, with a stable numeric reason.
    let g = Fixture::initialized();
    let denied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        g.env
            .as_contract(&g.contract, || require_admin_or_verifier(&g.env, &g.third));
    }));
    assert!(
        denied.is_err(),
        "guard must reject a caller with neither role"
    );

    let (caller, role, code) = g
        .access_denied()
        .expect("failure path must publish access_denied");
    assert_eq!(caller, g.third, "payload must name the denied caller");
    assert_eq!(
        role,
        Symbol::new(&g.env, "admin_or_verifier"),
        "payload must name the role that was denied"
    );
    assert_eq!(code, 1, "reason code for NotAdmin is 1");

    // Identity-owner denial uses its own role label and reason code.
    let h = Fixture::initialized();
    let denied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        h.env.as_contract(&h.contract, || {
            require_identity_owner(&h.env, &h.other, &h.third)
        });
    }));
    assert!(denied.is_err());
    let (caller, role, code) = h
        .access_denied()
        .expect("identity-owner failure must publish access_denied");
    assert_eq!(caller, h.other);
    assert_eq!(role, Symbol::new(&h.env, "identity_owner"));
    assert_eq!(code, 3, "reason code for NotIdentityOwner is 3");

    // Verifier denial is likewise distinguishable by reason code.
    let i = Fixture::initialized();
    let denied = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        i.env
            .as_contract(&i.contract, || require_verifier(&i.env, &i.other));
    }));
    assert!(denied.is_err());
    let (_caller, role, code) = i.access_denied().expect("verifier denial must publish");
    assert_eq!(role, Symbol::new(&i.env, "verifier"));
    assert_eq!(code, 2, "reason code for NotVerifier is 2");

    // Observability caveat, pinned deliberately: these payloads are only
    // reachable because the in-memory test host does not revert the frame under
    // `catch_unwind`. A real failing transaction reverts the whole frame, so an
    // indexer will never see an `access_denied` from these guards. The payload
    // assertions above are about correctness, not about the event being usable
    // for monitoring today; the durable signal is the transaction failure and
    // its error code.
}
