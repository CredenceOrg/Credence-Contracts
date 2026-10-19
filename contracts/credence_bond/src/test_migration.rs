#![cfg(test)]

//! Boundary, idempotency, and recovery coverage for `migration.rs` (issue #1340).
//!
//! `migrate_v1_to_v2` is invoked on the read path of `get_identity_state`, so a
//! regression here silently loses (or fabricates) bond state on every read.
//! These tests pin both branches of the write guard, the "no write" empty-result
//! path, field-preservation, identity scoping, and the interaction between the
//! `InProgress` guard and the migration itself.

// The crate is `#![no_std]`; tests link `std`, needed here for `catch_unwind`
// in the recovery test (same pattern as `test_batch_transfer.rs`).
extern crate std;

use crate::migration::{migrate_v1_to_v2, require_no_ongoing_migration, MigrationStatus};
use crate::{CredenceBond, DataKey, IdentityBond};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

/// Registers a bare contract so instance storage can be exercised without
/// running the full `initialize` flow.
fn register(e: &Env) -> Address {
    e.register(CredenceBond, ())
}

/// A bond whose every field is non-default, including the three fields the v2
/// schema added (`is_rolling`, `withdrawal_requested_at`,
/// `notice_period_duration`). A migration that rebuilt the entry from defaults
/// — the classic silent-data-loss bug — would be caught by these values.
fn populated_bond(identity: &Address) -> IdentityBond {
    IdentityBond {
        identity: identity.clone(),
        bonded_amount: 7_654_321,
        bond_start: 1_700_000_000,
        bond_duration: 86_400,
        slashed_amount: 123_456,
        active: true,
        is_rolling: true,
        withdrawal_requested_at: 1_700_050_000,
        notice_period_duration: 3_600,
    }
}

/// Mirrors the caller-side gating contract: no state mutation may run while a
/// migration is marked `InProgress`.
fn guarded_migrate(e: &Env, identity: &Address, status: MigrationStatus) {
    require_no_ongoing_migration(e, status);
    migrate_v1_to_v2(e, identity);
}

// ── write-guard boundaries ──────────────────────────────────────────────────

/// Covers the `None` arm of `if let Some(old_bond) = ... get(...)`: with no
/// bond stored, the migration must be a pure read that leaves the key absent.
/// No prior test called `migrate_v1_to_v2`, so this branch had zero coverage.
#[test]
fn migrate_v1_to_v2_does_not_create_a_missing_bond() {
    let e = Env::default();
    let cid = register(&e);
    let identity = Address::generate(&e);
    let unrelated = Address::generate(&e);

    e.as_contract(&cid, || {
        let key = DataKey::Bond(identity.clone());
        assert!(!e.storage().instance().has(&key));

        migrate_v1_to_v2(&e, &identity);

        assert!(
            !e.storage().instance().has(&key),
            "empty-result path must not materialise a bond entry"
        );
        assert!(
            !e.storage().instance().has(&DataKey::Bond(unrelated.clone())),
            "migration must not touch any other identity's key"
        );
    });
}

/// Covers the `Some` arm of the write guard and asserts byte-for-byte field
/// preservation, including the v2 fields (`is_rolling`,
/// `withdrawal_requested_at`, `notice_period_duration`).
#[test]
fn migrate_v1_to_v2_preserves_every_field_of_an_existing_bond() {
    let e = Env::default();
    let cid = register(&e);
    let identity = Address::generate(&e);
    let original = populated_bond(&identity);

    e.as_contract(&cid, || {
        let key = DataKey::Bond(identity.clone());
        e.storage().instance().set(&key, &original);

        migrate_v1_to_v2(&e, &identity);

        let stored: IdentityBond = e
            .storage()
            .instance()
            .get(&key)
            .expect("bond must still be present after migration");
        assert_eq!(
            stored, original,
            "migration must not drop or rewrite any bond field"
        );
        assert!(stored.is_rolling);
        assert_eq!(stored.withdrawal_requested_at, 1_700_050_000);
        assert_eq!(stored.notice_period_duration, 3_600);
    });
}

/// The documented "idempotent" contract: a second call yields an identical
/// stored value and keeps the entry present.
#[test]
fn migrate_v1_to_v2_is_idempotent() {
    let e = Env::default();
    let cid = register(&e);
    let identity = Address::generate(&e);
    let original = populated_bond(&identity);

    e.as_contract(&cid, || {
        let key = DataKey::Bond(identity.clone());
        e.storage().instance().set(&key, &original);

        migrate_v1_to_v2(&e, &identity);
        let after_first: Option<IdentityBond> = e.storage().instance().get(&key);

        migrate_v1_to_v2(&e, &identity);
        let after_second: Option<IdentityBond> = e.storage().instance().get(&key);

        assert_eq!(
            after_first, after_second,
            "repeat migration must be a no-op on the stored value"
        );
        assert_eq!(after_second, Some(original));
        assert!(e.storage().instance().has(&key));
    });
}

/// The key is `DataKey::Bond(identity)`; migrating one identity must never
/// create or mutate another identity's entry.
#[test]
fn migrate_v1_to_v2_is_scoped_to_the_requested_identity() {
    let e = Env::default();
    let cid = register(&e);
    let stored_identity = Address::generate(&e);
    let untouched_identity = Address::generate(&e);
    let original = populated_bond(&stored_identity);

    e.as_contract(&cid, || {
        let stored_key = DataKey::Bond(stored_identity.clone());
        let untouched_key = DataKey::Bond(untouched_identity.clone());
        e.storage().instance().set(&stored_key, &original);

        migrate_v1_to_v2(&e, &untouched_identity);

        assert!(
            !e.storage().instance().has(&untouched_key),
            "migrating a bondless identity must not create its entry"
        );
        let kept: Option<IdentityBond> = e.storage().instance().get(&stored_key);
        assert_eq!(kept, Some(original));
    });
}

// ── guard ⇄ migration interaction (recovery gating) ─────────────────────────

/// `InProgress` must abort the guarded mutation with the typed contract error.
/// The pre-existing `test_migration_guard` only panics inside the guard; this
/// exercises the guard in front of an actual storage mutation.
#[test]
#[should_panic(expected = "Error(Contract, #125)")]
fn in_progress_status_blocks_the_migration() {
    let e = Env::default();
    let cid = register(&e);
    let identity = Address::generate(&e);
    let original = populated_bond(&identity);

    e.as_contract(&cid, || {
        e.storage()
            .instance()
            .set(&DataKey::Bond(identity.clone()), &original);
        guarded_migrate(&e, &identity, MigrationStatus::InProgress);
    });
}

/// The two non-`InProgress` states must let the mutation through untouched.
#[test]
fn none_status_lets_the_migration_run() {
    let e = Env::default();
    let cid = register(&e);
    let identity = Address::generate(&e);
    let original = populated_bond(&identity);

    e.as_contract(&cid, || {
        let key = DataKey::Bond(identity.clone());
        e.storage().instance().set(&key, &original);

        guarded_migrate(&e, &identity, MigrationStatus::None);

        let stored: Option<IdentityBond> = e.storage().instance().get(&key);
        assert_eq!(stored, Some(original));
    });
}

/// See `none_status_lets_the_migration_run` — `Completed` is the terminal state
/// the guard explicitly permits.
#[test]
fn completed_status_lets_the_migration_run() {
    let e = Env::default();
    let cid = register(&e);
    let identity = Address::generate(&e);
    let original = populated_bond(&identity);

    e.as_contract(&cid, || {
        let key = DataKey::Bond(identity.clone());
        e.storage().instance().set(&key, &original);

        guarded_migrate(&e, &identity, MigrationStatus::Completed);

        let stored: Option<IdentityBond> = e.storage().instance().get(&key);
        assert_eq!(stored, Some(original));
    });
}

/// Recovery path: a blocked attempt must not have written a partial entry, and
/// once the migration is marked complete the retry succeeds losslessly.
#[test]
fn blocked_migration_leaves_storage_untouched_and_recovers_after_clear() {
    let e = Env::default();
    let cid = register(&e);
    let identity = Address::generate(&e);
    let original = populated_bond(&identity);

    e.as_contract(&cid, || {
        e.storage()
            .instance()
            .set(&DataKey::Bond(identity.clone()), &original);
    });

    let blocked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        e.as_contract(&cid, || {
            guarded_migrate(&e, &identity, MigrationStatus::InProgress);
        });
    }));
    assert!(blocked.is_err(), "InProgress must abort the guarded migration");

    // The aborted attempt must leave the pre-existing bond exactly as it was.
    let after_block: Option<IdentityBond> = e.as_contract(&cid, || {
        e.storage().instance().get(&DataKey::Bond(identity.clone()))
    });
    assert_eq!(after_block, Some(original.clone()));

    // Once the guard clears, the retry migrates without data loss.
    e.as_contract(&cid, || {
        guarded_migrate(&e, &identity, MigrationStatus::Completed);
    });
    let after_retry: Option<IdentityBond> = e.as_contract(&cid, || {
        e.storage().instance().get(&DataKey::Bond(identity.clone()))
    });
    assert_eq!(after_retry, Some(original));
}

// ── status wire boundary ────────────────────────────────────────────────────

/// Pins the numeric encoding of the status enum: `None` must stay the zero
/// "unset" value so a caller that defaults the status cannot accidentally
/// select `InProgress`.
#[test]
fn migration_status_discriminants_are_wire_stable() {
    assert_eq!(MigrationStatus::None as u32, 0);
    assert_eq!(MigrationStatus::InProgress as u32, 1);
    assert_eq!(MigrationStatus::Completed as u32, 2);
}
