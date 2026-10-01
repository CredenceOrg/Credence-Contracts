//! Boundary and recovery test coverage for `credence_errors::require_admin!`.
//!
//! # What is under test
//!
//! The `require_admin!` macro is a three-step access-control guard:
//!
//! 1. **Load** `admin` from `instance()` storage at `$admin_key`.  
//!    → Panics [`ContractError::NotInitialized`] (wire code 1) when the key is absent.
//! 2. **Compare** `stored_admin != *caller`.  
//!    → Panics [`ContractError::NotAdmin`] (wire code 100) on mismatch.
//! 3. **Require auth** via `$caller.require_auth()`.  
//!    → Panics with a Soroban host auth error when auth is not satisfied.
//!
//! # Test scenarios covered
//!
//! | # | Scenario | Expected outcome |
//! |---|----------|-----------------|
//! | 1 | Correct admin, auth mocked → success path | does not panic |
//! | 2 | Admin key absent (uninitialized contract) | panics `NotInitialized` (1) |
//! | 3 | Caller ≠ stored admin (identity mismatch) | panics `NotAdmin` (100) |
//! | 4 | `auth` not mocked (require_auth fails) | panics with host auth error |
//! | 5 | Boundary: storage key is a `Symbol` | does not panic |
//! | 6 | Boundary: storage key is a `u32` scalar | does not panic |
//! | 7 | Boundary: storage key is a tuple `(Symbol, u32)` | does not panic |
//! | 8 | Recovery: macro succeeds after admin is written post-construction | does not panic |
//! | 9 | Recovery: macro succeeds after admin is overwritten (key rotation) | does not panic |
//! | 10| Recovery: macro consistently rejects non-admin after admin changes | panics `NotAdmin` |
//! | 11| Idempotency: calling macro twice with the same caller succeeds both times | does not panic |
//! | 12| Boundary: caller IS stored admin but auth is explicitly not satisfied | panics host auth |
//! | 13| Two distinct keys in same env → each resolves to its own admin | per-key behavior |
//! | 14| Stale caller reference: original admin replaced; old caller is now rejected | panics `NotAdmin` |
//! | 15| Error code wire stability: NotInitialized == 1, NotAdmin == 100 | exact numeric assertions |
//!
//! # Infrastructure
//!
//! Each test registers a minimal [`AdminGuardContract`] and exercises the macro
//! from inside `e.as_contract(...)`, the only context in which
//! `e.storage().instance()` is valid.  `e.mock_all_auths()` is used wherever
//! `require_auth()` must not be the failure under test.

// Off-chain test binary, not deployed WASM (issue #713 exemption).
#![allow(clippy::disallowed_macros)]

use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::Address as _,
    Address, Env, Symbol,
};

// ---------------------------------------------------------------------------
// Minimal test contract
// ---------------------------------------------------------------------------

/// Storage keys used by the test contract.
///
/// We deliberately test three structural variants — `Address` (value),
/// `Symbol` (single atom), and a `(Symbol, u32)` tuple — because the macro
/// is generic over `$admin_key` and all three shapes must work correctly.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
enum AdminKey {
    /// Primary admin slot, mirrors the production `DataKey::Admin` shape.
    Admin,
    /// Secondary admin slot keyed by an index; tests tuple-key behaviour.
    Slot(u32),
}

#[contract]
struct AdminGuardContract;

#[contractimpl]
impl AdminGuardContract {
    // ── Initialisation helpers ───────────────────────────────────────────

    /// Write `admin` under `AdminKey::Admin` without any auth.
    /// Used by tests to set up pre-conditions without going through the guard.
    pub fn set_admin(e: Env, admin: Address) {
        e.storage().instance().set(&AdminKey::Admin, &admin);
    }

    /// Write `admin` under `AdminKey::Slot(index)`.
    pub fn set_admin_slot(e: Env, index: u32, admin: Address) {
        e.storage()
            .instance()
            .set(&AdminKey::Slot(index), &admin);
    }

    /// Remove `AdminKey::Admin` entirely — simulates an uninitialized state.
    pub fn clear_admin(e: Env) {
        e.storage().instance().remove(&AdminKey::Admin);
    }

    // ── Guard exercisers ─────────────────────────────────────────────────

    /// Invoke `require_admin!` against the primary admin slot.
    /// Returns `true` on success; panics (via the macro) on failure.
    pub fn check_admin(e: Env, caller: Address) -> bool {
        credence_errors::require_admin!(e, caller, AdminKey::Admin);
        true
    }

    /// Invoke `require_admin!` against an indexed slot.
    pub fn check_admin_slot(e: Env, index: u32, caller: Address) -> bool {
        credence_errors::require_admin!(e, caller, AdminKey::Slot(index));
        true
    }

    /// Invoke `require_admin!` with a `Symbol` key directly (tests raw-symbol storage shape).
    pub fn check_admin_symbol_key(e: Env, caller: Address) -> bool {
        let key = Symbol::new(&e, "admin");
        // Pre-store the caller as admin under this symbol key so we can test
        // success path; tests that need the uninitialized path skip this fn.
        credence_errors::require_admin!(e, caller, key);
        true
    }

    /// Write `caller` as admin under the `Symbol("admin")` key, then check.
    /// Convenience: stores-then-checks so we can exercise the symbol key path from outside.
    pub fn set_and_check_symbol_key(e: Env, caller: Address) -> bool {
        let key = Symbol::new(&e, "admin");
        e.storage().instance().set(&key, &caller);
        credence_errors::require_admin!(e, caller, key);
        true
    }

    /// Write `caller` as admin under a `u32` scalar key, then check.
    /// Exercises a primitive-value key shape.
    pub fn set_and_check_u32_key(e: Env, caller: Address) -> bool {
        let key: u32 = 42_u32;
        e.storage().instance().set(&key, &caller);
        credence_errors::require_admin!(e, caller, key);
        true
    }
}

// ---------------------------------------------------------------------------
// Helper: register the contract and return (Env, contract_id)
// ---------------------------------------------------------------------------

fn setup() -> (Env, Address) {
    let e = Env::default();
    let contract_id = e.register(AdminGuardContract, ());
    (e, contract_id)
}

// ---------------------------------------------------------------------------
// 1. Success path — correct admin, auth mocked
// ---------------------------------------------------------------------------

/// The macro must not panic when the caller equals the stored admin and all
/// auths are mocked.
///
/// Invariant: `stored_admin == *caller && auth_satisfied` ⟹ returns normally.
#[test]
fn require_admin_succeeds_for_correct_admin() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin = Address::generate(&e);

    let client = AdminGuardContractClient::new(&e, &cid);
    client.set_admin(&admin);

    let result = client.check_admin(&admin);
    assert!(result, "require_admin must return true for the correct admin");
}

// ---------------------------------------------------------------------------
// 2. Uninitialized contract — key absent → NotInitialized (wire code 1)
// ---------------------------------------------------------------------------

/// When no admin has been stored the macro must panic with `NotInitialized`.
///
/// Wire-code assertion: `ContractError::NotInitialized as u32 == 1`.
/// The error appears in the `HostError` message as `Error(Contract, #1)`.
#[test]
#[should_panic(expected = "HostError: Error(Contract, #1)")]
fn require_admin_panics_not_initialized_when_key_absent() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let caller = Address::generate(&e);

    // No set_admin call — admin key is absent.
    let client = AdminGuardContractClient::new(&e, &cid);
    client.check_admin(&caller); // must panic NotInitialized
}

// ---------------------------------------------------------------------------
// 3. Wrong caller — identity mismatch → NotAdmin (wire code 100)
// ---------------------------------------------------------------------------

/// When the caller does not equal the stored admin the macro must panic with
/// `NotAdmin`.
///
/// Wire-code assertion: `ContractError::NotAdmin as u32 == 100`.
/// The error appears in the `HostError` message as `Error(Contract, #100)`.
#[test]
#[should_panic(expected = "HostError: Error(Contract, #100)")]
fn require_admin_panics_not_admin_for_wrong_caller() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let impostor = Address::generate(&e);

    let client = AdminGuardContractClient::new(&e, &cid);
    client.set_admin(&admin);

    client.check_admin(&impostor); // must panic NotAdmin
}

// ---------------------------------------------------------------------------
// 4. Auth not satisfied — require_auth fails (host auth error)
// ---------------------------------------------------------------------------

/// When the caller matches the stored admin but `require_auth()` is not
/// satisfied the host must reject the call.
///
/// We intentionally do NOT call `e.mock_all_auths()` so that the SDK's
/// native auth check fires.  The exact panic message varies by SDK version but
/// always contains "HostError".
#[test]
#[should_panic(expected = "HostError")]
fn require_admin_panics_when_auth_not_satisfied() {
    let (e, cid) = setup();
    // No mock_all_auths — auth will not be satisfied.
    let admin = Address::generate(&e);

    let client = AdminGuardContractClient::new(&e, &cid);
    // Store admin while inside a mocked-auth scope so set_admin itself can run.
    e.mock_all_auths();
    client.set_admin(&admin);
    // Drop the mock scope by calling the real check without any mock.
    // We need a fresh env here: the mock persists for the env lifetime in SDK 22.
    // Instead we re-register and use as_contract to manually exercise step 3.
    let e2 = Env::default();
    let cid2 = e2.register(AdminGuardContract, ());
    e2.mock_all_auths();
    let admin2 = Address::generate(&e2);
    let client2 = AdminGuardContractClient::new(&e2, &cid2);
    client2.set_admin(&admin2);
    // Now test without auth mocked: check_admin requires the caller to auth.
    let e3 = Env::default();
    let cid3 = e3.register(AdminGuardContract, ());
    // Manually prime instance storage without going through a client call
    // (so we bypass auth for set_admin), then call check_admin without mocking.
    e3.as_contract(&cid3, || {
        e3.storage().instance().set(&AdminKey::Admin, &admin2);
    });
    // call check_admin WITHOUT mock_all_auths — require_auth must fail.
    let client3 = AdminGuardContractClient::new(&e3, &cid3);
    client3.check_admin(&admin2); // must panic with HostError (auth failure)
}

// ---------------------------------------------------------------------------
// 5. Boundary: Symbol storage key
// ---------------------------------------------------------------------------

/// The macro must work when `$admin_key` is a raw `Symbol` rather than a
/// `#[contracttype]` enum variant.
#[test]
fn require_admin_works_with_symbol_storage_key() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let caller = Address::generate(&e);

    let client = AdminGuardContractClient::new(&e, &cid);
    let result = client.set_and_check_symbol_key(&caller);
    assert!(result, "require_admin must succeed with a Symbol key");
}

// ---------------------------------------------------------------------------
// 6. Boundary: u32 scalar storage key
// ---------------------------------------------------------------------------

/// The macro must work when `$admin_key` is a bare `u32` value.
#[test]
fn require_admin_works_with_u32_scalar_storage_key() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let caller = Address::generate(&e);

    let client = AdminGuardContractClient::new(&e, &cid);
    let result = client.set_and_check_u32_key(&caller);
    assert!(result, "require_admin must succeed with a u32 key");
}

// ---------------------------------------------------------------------------
// 7. Boundary: tuple (Symbol, u32) composite key
// ---------------------------------------------------------------------------

/// The macro must work when `$admin_key` is a `#[contracttype]` tuple variant
/// `AdminKey::Slot(n)`, mirroring patterns like `(Symbol, Address)` used in
/// multi-role contracts.
#[test]
fn require_admin_works_with_indexed_slot_key() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin = Address::generate(&e);

    let client = AdminGuardContractClient::new(&e, &cid);
    client.set_admin_slot(&7_u32, &admin);

    let result = client.check_admin_slot(&7_u32, &admin);
    assert!(result, "require_admin must succeed with AdminKey::Slot(7)");
}

/// Absent slot key must panic NotInitialized.
#[test]
#[should_panic(expected = "HostError: Error(Contract, #1)")]
fn require_admin_panics_not_initialized_for_missing_slot_key() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let caller = Address::generate(&e);

    let client = AdminGuardContractClient::new(&e, &cid);
    // Slot 99 was never initialised.
    client.check_admin_slot(&99_u32, &caller);
}

// ---------------------------------------------------------------------------
// 8. Recovery: macro succeeds after admin is written post-construction
// ---------------------------------------------------------------------------

/// A contract that starts uninitialized must succeed once admin is stored.
///
/// This covers the scenario described in the issue as "retry / recovery
/// after state is corrected".
#[test]
fn require_admin_recovers_after_admin_is_set_post_construction() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let client = AdminGuardContractClient::new(&e, &cid);

    // First attempt — admin not yet stored — must panic.
    // We use std::panic::catch_unwind to continue the test.
    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.check_admin(&admin);
    }));
    assert!(first.is_err(), "first call must fail (NotInitialized)");

    // Now store the admin and retry.
    client.set_admin(&admin);
    let second = client.check_admin(&admin);
    assert!(second, "second call must succeed after admin is stored");
}

// ---------------------------------------------------------------------------
// 9. Recovery: macro succeeds after admin key is overwritten (rotation)
// ---------------------------------------------------------------------------

/// When the admin is rotated the new admin must succeed and the old one must
/// fail.
///
/// Invariant: the macro is purely data-driven — it reads whatever is in
/// storage at call time, so a key rotation is reflected immediately with no
/// stale caching.
#[test]
fn require_admin_reflects_admin_rotation_immediately() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let old_admin = Address::generate(&e);
    let new_admin = Address::generate(&e);
    let client = AdminGuardContractClient::new(&e, &cid);

    // Install old admin, verify success.
    client.set_admin(&old_admin);
    assert!(client.check_admin(&old_admin), "old admin must pass before rotation");

    // Rotate to new admin.
    client.set_admin(&new_admin);

    // New admin must succeed.
    assert!(
        client.check_admin(&new_admin),
        "new admin must pass after rotation"
    );

    // Old admin must now fail with NotAdmin.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.check_admin(&old_admin);
    }));
    assert!(result.is_err(), "old admin must be rejected after rotation");
}

// ---------------------------------------------------------------------------
// 10. Consistent rejection: non-admin is always rejected regardless of retries
// ---------------------------------------------------------------------------

/// An address that is never set as admin must be consistently rejected even
/// across multiple retries.
///
/// This guards against accidental state leakage between calls (e.g. if the
/// macro ever cached the caller).
#[test]
fn require_admin_consistently_rejects_non_admin() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let non_admin = Address::generate(&e);
    let client = AdminGuardContractClient::new(&e, &cid);

    client.set_admin(&admin);

    for attempt in 0..3 {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.check_admin(&non_admin);
        }));
        assert!(
            result.is_err(),
            "non-admin must be rejected on attempt {attempt}"
        );
    }
}

// ---------------------------------------------------------------------------
// 11. Idempotency: calling macro twice with the same caller succeeds both times
// ---------------------------------------------------------------------------

/// Calling the guard multiple times for the same (admin, key) pair is
/// idempotent: both calls succeed without mutating state or accumulating side
/// effects.
#[test]
fn require_admin_is_idempotent_for_correct_caller() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let client = AdminGuardContractClient::new(&e, &cid);

    client.set_admin(&admin);

    assert!(client.check_admin(&admin), "first call must succeed");
    assert!(client.check_admin(&admin), "second call must also succeed (idempotent)");
}

// ---------------------------------------------------------------------------
// 12. Auth failure: caller == stored admin but require_auth not mocked
// ---------------------------------------------------------------------------

/// Verifies that even when the address match (step 2) passes, the host rejects
/// the call if `require_auth()` (step 3) is not satisfied.
///
/// This is the critical security invariant: address equality alone is not
/// sufficient — the Soroban auth tree must confirm the signature.
#[test]
#[should_panic(expected = "HostError")]
fn require_admin_panics_when_address_matches_but_auth_missing() {
    let e = Env::default();
    let cid = e.register(AdminGuardContract, ());
    let admin = Address::generate(&e);

    // Prime instance storage directly (bypassing client auth checks).
    e.as_contract(&cid, || {
        e.storage().instance().set(&AdminKey::Admin, &admin);
    });

    // Call check_admin WITHOUT mock_all_auths — require_auth must fire.
    let client = AdminGuardContractClient::new(&e, &cid);
    client.check_admin(&admin);
}

// ---------------------------------------------------------------------------
// 13. Two distinct keys in the same env resolve to independent admins
// ---------------------------------------------------------------------------

/// A contract with two admin slots must resolve each independently.
/// Writing admin A to slot 1 and admin B to slot 2 must not cross-contaminate.
#[test]
fn require_admin_two_keys_are_independent() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin_a = Address::generate(&e);
    let admin_b = Address::generate(&e);
    let client = AdminGuardContractClient::new(&e, &cid);

    client.set_admin_slot(&1_u32, &admin_a);
    client.set_admin_slot(&2_u32, &admin_b);

    // Each admin passes against its own slot.
    assert!(
        client.check_admin_slot(&1_u32, &admin_a),
        "admin_a must pass slot 1"
    );
    assert!(
        client.check_admin_slot(&2_u32, &admin_b),
        "admin_b must pass slot 2"
    );

    // Cross-slot checks must fail (NotAdmin).
    let cross_a = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.check_admin_slot(&2_u32, &admin_a);
    }));
    assert!(cross_a.is_err(), "admin_a must be rejected against slot 2");

    let cross_b = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.check_admin_slot(&1_u32, &admin_b);
    }));
    assert!(cross_b.is_err(), "admin_b must be rejected against slot 1");
}

// ---------------------------------------------------------------------------
// 14. Stale caller: original admin replaced; old caller is now rejected
// ---------------------------------------------------------------------------

/// Confirms that the macro re-reads storage on every call and does not hold
/// a stale reference to an address that is no longer the admin.
///
/// This is the security dual of test 9: not just that the new admin is
/// accepted, but that the old one is definitely denied.
#[test]
fn require_admin_old_caller_rejected_after_key_overwrite() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let original = Address::generate(&e);
    let replacement = Address::generate(&e);
    let client = AdminGuardContractClient::new(&e, &cid);

    client.set_admin(&original);
    // Overwrite with replacement admin.
    client.set_admin(&replacement);

    // Original must fail.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.check_admin(&original);
    }));
    assert!(result.is_err(), "original admin must be rejected after overwrite");

    // Replacement must succeed.
    assert!(
        client.check_admin(&replacement),
        "replacement admin must be accepted"
    );
}

// ---------------------------------------------------------------------------
// 15. Wire-code stability assertions
// ---------------------------------------------------------------------------

/// Pin the wire codes for `NotInitialized` and `NotAdmin` to their
/// spec-locked values.  These are authoritative — any renumbering would be
/// a protocol-breaking change.
///
/// See `docs/error-codes-wire.md` for the full wire-stability contract.
#[test]
fn require_admin_error_wire_codes_are_stable() {
    // Initialization category sentinel
    assert_eq!(
        ContractError::NotInitialized as u32,
        1,
        "NotInitialized wire code must remain 1 (locked by protocol spec)"
    );
    // Authorization category sentinel
    assert_eq!(
        ContractError::NotAdmin as u32,
        100,
        "NotAdmin wire code must remain 100 (locked by protocol spec)"
    );
}

/// Confirm that `NotInitialized` and `NotAdmin` belong to the expected
/// `ErrorCategory` domains as consumed by off-chain monitoring clients.
#[test]
fn require_admin_errors_have_correct_categories() {
    use credence_errors::{ErrorCategory, ErrorExt};

    assert_eq!(
        ContractError::NotInitialized.category(),
        ErrorCategory::Initialization,
        "NotInitialized must be in the Initialization category"
    );
    assert_eq!(
        ContractError::NotAdmin.category(),
        ErrorCategory::Authorization,
        "NotAdmin must be in the Authorization category"
    );
}

// ---------------------------------------------------------------------------
// 16. Partial failure isolation: one failing call does not corrupt storage
// ---------------------------------------------------------------------------

/// After a rejected call (NotAdmin), storage must remain consistent.
/// The admin stored before the failed call must still be there and still
/// accepted in the next call.
#[test]
fn require_admin_failed_call_leaves_storage_intact() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let attacker = Address::generate(&e);
    let client = AdminGuardContractClient::new(&e, &cid);

    client.set_admin(&admin);

    // Attempt by a non-admin — must fail.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.check_admin(&attacker);
    }));

    // Admin slot must remain valid — original admin must still pass.
    assert!(
        client.check_admin(&admin),
        "admin must still be accepted after a failed non-admin attempt"
    );
}

// ---------------------------------------------------------------------------
// 17. Concurrent-call simulation: multiple callers checked sequentially
// ---------------------------------------------------------------------------

/// Soroban is single-threaded per ledger, but multiple operations within one
/// transaction can each invoke `require_admin!`.  Simulate this by checking
/// the same admin key repeatedly from different "callers" in sequence,
/// verifying that accepted calls are accepted and rejected calls are rejected
/// with no observable cross-call contamination.
#[test]
fn require_admin_sequential_callers_are_independent() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let others: Vec<Address> = (0..5).map(|_| Address::generate(&e)).collect();
    let client = AdminGuardContractClient::new(&e, &cid);

    client.set_admin(&admin);

    for other in &others {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.check_admin(other);
        }));
        assert!(result.is_err(), "non-admin address must always be rejected");
    }

    // Admin must still work after all the failed attempts.
    assert!(
        client.check_admin(&admin),
        "admin must succeed after sequential rejected calls"
    );
}

// ---------------------------------------------------------------------------
// 18. Boundary: empty slot cleared, then re-initialised
// ---------------------------------------------------------------------------

/// After `clear_admin` removes the admin key, the contract re-enters the
/// "uninitialized" state and must respond with `NotInitialized` until a new
/// admin is set.
#[test]
fn require_admin_cleared_key_behaves_like_uninitialized() {
    let (e, cid) = setup();
    e.mock_all_auths();
    let admin = Address::generate(&e);
    let client = AdminGuardContractClient::new(&e, &cid);

    client.set_admin(&admin);
    // Verify it works first.
    assert!(client.check_admin(&admin));

    // Clear the key.
    client.clear_admin();

    // Must now panic NotInitialized (code 1).
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.check_admin(&admin);
    }));
    assert!(result.is_err(), "cleared key must act as uninitialized");

    // Re-initialize with the same admin.
    client.set_admin(&admin);
    assert!(
        client.check_admin(&admin),
        "admin must succeed after re-initialization"
    );
}
