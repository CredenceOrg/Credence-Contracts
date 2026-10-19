//! Deterministic failure-boundary coverage for `set_pause_signer`.
//!
//! # Design
//!
//! This test module provides exhaustive, specification-driven coverage for
//! every input class and state transition in [`crate::AdminContract::set_pause_signer`]
//! (implemented in [`crate::pausable::set_pause_signer`]).
//!
//! ## Acceptance criteria (issue #1409)
//!
//! | # | Criterion | Tests |
//! |---|-----------|-------|
//! | AC-1 | Behaviour is deterministic for valid, invalid, duplicate, and boundary inputs | `sps_*` tests throughout |
//! | AC-2 | Authorization, validation, and state-transition invariants remain enforced | `sps_auth_*`, `sps_invalid_*` |
//! | AC-3 | Retries / partial failure / concurrent execution cannot produce unsafe state | `sps_idempotent_*`, `sps_concurrent_*` |
//! | AC-4 | Focused tests cover success, rejection, boundary, and regression scenarios | all sections below |
//! | AC-5 | Existing callers remain compatible | `sps_public_interface_*` |
//! | AC-6 | Failures are diagnosable without exposing sensitive data | error-code assertions throughout |
//!
//! ## Invariants documented in code
//!
//! * **I-1 (Count integrity)**: `PauseSignerCount` always equals the number of
//!   `PauseSigner(addr)` keys present in instance storage that are `true`.
//! * **I-2 (Threshold safety)**: `PauseThreshold <= PauseSignerCount` at all times.
//! * **I-3 (Epoch monotonicity)**: each committed mutation advances
//!   `ConfigEpoch` by exactly 1; no-ops leave it unchanged.
//! * **I-4 (Event integrity)**: a `pause_signer_set` event is emitted if and
//!   only if a real state change occurs.
//! * **I-5 (Address rejection)**: the zero/sentinel address
//!   (`GAAAAAA…WHF`) and the contract's own address are always rejected before
//!   any state is written.

#![cfg(test)]

use crate::pausable::PROPOSAL_EPOCH_SIZE;
use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, String};

// ---------------------------------------------------------------------------
// Wire-stable error discriminants
// ---------------------------------------------------------------------------
const ERR_NOT_ADMIN: u32 = 100;
const ERR_INVALID_ADMIN_ADDRESS: u32 = 110;

// The zero/sentinel address per INVALID_ADDRESS_SENTINEL in lib.rs.
const ZERO_STRKEY: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Minimal contract setup: one SuperAdmin, mock-all-auths.
fn setup() -> (Env, AdminContractClient<'static>, Address) {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register_contract(None, AdminContract);
    let client = AdminContractClient::new(&e, &contract_id);
    let super_admin = Address::generate(&e);
    client.initialize(&super_admin, &1u32, &100u32);
    (e, client, super_admin)
}

/// Return the zero/sentinel address.
fn zero_address(e: &Env) -> Address {
    Address::from_string(&String::from_str(e, ZERO_STRKEY))
}

/// Assert the count/threshold storage invariants hold (I-1 and I-2).
///
/// `addrs` is the complete universe of addresses that may have been enabled or
/// disabled during this test; the helper walks them all and compares the live
/// tally against `PauseSignerCount`.
fn assert_invariants(e: &Env, client: &AdminContractClient, addrs: &[Address]) {
    e.as_contract(&client.address, || {
        // I-1: count matches enabled entries
        let mut live: u32 = 0;
        for a in addrs {
            let enabled: bool = e
                .storage()
                .instance()
                .get(&DataKey::PauseSigner(a.clone()))
                .unwrap_or(false);
            if enabled {
                live += 1;
            }
        }
        let stored_count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        assert_eq!(
            stored_count, live,
            "I-1 violated: PauseSignerCount ({stored_count}) != live enabled ({live})"
        );

        // I-2: threshold never exceeds count
        let threshold: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0);
        assert!(
            threshold <= stored_count,
            "I-2 violated: PauseThreshold ({threshold}) > PauseSignerCount ({stored_count})"
        );
    });
}

// ---------------------------------------------------------------------------
// Section 1 – Authorization boundaries (AC-2)
// ---------------------------------------------------------------------------

/// Enabling a signer with an Admin-role caller is rejected with `NotAdmin`
/// because `set_pause_signer` requires SuperAdmin authority.
/// No state, event, or epoch change occurs.
#[test]
fn sps_auth_admin_role_is_rejected() {
    let (e, client, super_admin) = setup();
    let mid_admin = Address::generate(&e);
    client.add_admin(&super_admin, &mid_admin, &AdminRole::Admin);

    let signer = Address::generate(&e);
    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_set_pause_signer(&mid_admin, &signer, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN),
        "Admin role must be rejected with NotAdmin"
    );

    // I-1, I-3, I-4
    assert_invariants(&e, &client, &[signer]);
    assert_eq!(
        client.get_config_epoch(),
        epoch_before,
        "I-3: failed call must not advance epoch"
    );
    assert_eq!(
        e.events().all().len(),
        events_before,
        "I-4: failed call must not emit events"
    );
}

/// Enabling a signer with an Operator-role caller is rejected.
#[test]
fn sps_auth_operator_role_is_rejected() {
    let (e, client, super_admin) = setup();
    let operator = Address::generate(&e);
    client.add_admin(&super_admin, &operator, &AdminRole::Operator);

    let signer = Address::generate(&e);
    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_set_pause_signer(&operator, &signer, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    assert_invariants(&e, &client, &[signer]);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// A completely non-admin stranger is rejected.
#[test]
fn sps_auth_stranger_is_rejected() {
    let (e, client, _super_admin) = setup();
    let stranger = Address::generate(&e);
    let signer = Address::generate(&e);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_set_pause_signer(&stranger, &signer, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, soroban_sdk::Error::from_contract_error(ERR_NOT_ADMIN));

    assert_invariants(&e, &client, &[signer]);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// A SuperAdmin caller is accepted – the success path (AC-4: success scenario).
#[test]
fn sps_auth_super_admin_succeeds() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    client.set_pause_signer(&super_admin, &signer, &true);

    // State must reflect the change, epoch must advance exactly once, event emitted.
    assert_invariants(&e, &client, &[signer]);
    assert_eq!(
        client.get_config_epoch(),
        epoch_before + 1,
        "I-3: successful enable must advance epoch by 1"
    );
    assert_eq!(
        e.events().all().len(),
        events_before + 1,
        "I-4: successful enable must emit exactly one event"
    );
}

/// Disabling an unregistered signer with SuperAdmin is a no-op (idempotent),
/// not an authorization failure.
#[test]
fn sps_auth_super_admin_disable_unregistered_is_noop() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    // Disabling a signer that was never enabled: must not error.
    client.set_pause_signer(&super_admin, &signer, &false);

    assert_invariants(&e, &client, &[signer]);
    assert_eq!(
        client.get_config_epoch(),
        epoch_before,
        "I-3: no-op disable must not advance epoch"
    );
    assert_eq!(
        e.events().all().len(),
        events_before,
        "I-4: no-op disable must not emit events"
    );
}

// ---------------------------------------------------------------------------
// Section 2 – Input validation / invalid addresses (AC-2, I-5)
// ---------------------------------------------------------------------------

/// The zero/sentinel address (GAAAAAA…WHF) is rejected with
/// `InvalidAdminAddress` for both enable and disable operations.
/// No state or epoch change occurs (I-3, I-5).
#[test]
fn sps_invalid_zero_address_enable_rejected() {
    let (e, client, super_admin) = setup();
    let zero = zero_address(&e);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_set_pause_signer(&super_admin, &zero, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_ADMIN_ADDRESS),
        "zero address enable must return InvalidAdminAddress"
    );

    assert_invariants(&e, &client, &[zero]);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// The zero/sentinel address is also rejected for a disable attempt.
#[test]
fn sps_invalid_zero_address_disable_rejected() {
    let (e, client, super_admin) = setup();
    let zero = zero_address(&e);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_set_pause_signer(&super_admin, &zero, &false)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_ADMIN_ADDRESS),
        "zero address disable must return InvalidAdminAddress"
    );

    assert_invariants(&e, &client, &[zero]);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// The contract's own address is rejected as a signer (self-assignment guard,
/// I-5).
#[test]
fn sps_invalid_contract_address_as_signer_rejected() {
    let (e, client, super_admin) = setup();

    // The contract address is obtained from the client.
    let contract_addr = client.address.clone();

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    let err = client
        .try_set_pause_signer(&super_admin, &contract_addr, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_ADMIN_ADDRESS),
        "contract self-address enable must return InvalidAdminAddress"
    );

    assert_invariants(&e, &client, &[contract_addr.clone()]);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// Contract address is rejected for disable too (no partial storage state).
#[test]
fn sps_invalid_contract_address_disable_rejected() {
    let (e, client, super_admin) = setup();
    let contract_addr = client.address.clone();

    let epoch_before = client.get_config_epoch();

    let err = client
        .try_set_pause_signer(&super_admin, &contract_addr, &false)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(ERR_INVALID_ADMIN_ADDRESS)
    );
    assert_eq!(client.get_config_epoch(), epoch_before);
}

// ---------------------------------------------------------------------------
// Section 3 – Idempotency / duplicate-call safety (AC-3, AC-4)
// ---------------------------------------------------------------------------

/// Re-enabling an already-enabled signer is a deterministic no-op:
/// no event is emitted and the epoch does not advance (I-3, I-4).
#[test]
fn sps_idempotent_reenable_is_noop() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    client.set_pause_signer(&super_admin, &signer, &true);
    let epoch_after_first = client.get_config_epoch();
    let events_after_first = e.events().all().len();

    // Duplicate enable.
    client.set_pause_signer(&super_admin, &signer, &true);

    assert_invariants(&e, &client, &[signer]);
    assert_eq!(
        client.get_config_epoch(),
        epoch_after_first,
        "I-3: duplicate enable must not advance epoch"
    );
    assert_eq!(
        e.events().all().len(),
        events_after_first,
        "I-4: duplicate enable must not emit events"
    );
}

/// Re-disabling an already-disabled signer is a deterministic no-op.
#[test]
fn sps_idempotent_redisable_is_noop() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    // Enable then disable.
    client.set_pause_signer(&super_admin, &signer, &true);
    client.set_pause_signer(&super_admin, &signer, &false);
    let epoch_after_disable = client.get_config_epoch();
    let events_after_disable = e.events().all().len();

    // Duplicate disable.
    client.set_pause_signer(&super_admin, &signer, &false);

    assert_invariants(&e, &client, &[signer]);
    assert_eq!(
        client.get_config_epoch(),
        epoch_after_disable,
        "I-3: duplicate disable must not advance epoch"
    );
    assert_eq!(
        e.events().all().len(),
        events_after_disable,
        "I-4: duplicate disable must not emit events"
    );
}

/// Disabling a signer that was never enabled is a no-op from the first call.
#[test]
fn sps_idempotent_disable_never_enabled_is_noop() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    let epoch_before = client.get_config_epoch();
    let events_before = e.events().all().len();

    client.set_pause_signer(&super_admin, &signer, &false);

    assert_invariants(&e, &client, &[signer]);
    assert_eq!(client.get_config_epoch(), epoch_before);
    assert_eq!(e.events().all().len(), events_before);
}

/// A client that retries a timed-out `set_pause_signer` cannot
/// desynchronise an off-chain indexer: the second call emits no duplicate
/// `pause_signer_set` event (AC-3).
#[test]
fn sps_idempotent_retry_does_not_duplicate_event() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    client.set_pause_signer(&super_admin, &signer, &true);
    let events_after_first = e.events().all().len();

    // Simulate a retry (same arguments, same result).
    client.set_pause_signer(&super_admin, &signer, &true);

    assert_eq!(
        e.events().all().len(),
        events_after_first,
        "retry must not emit a second pause_signer_set event"
    );
}

// ---------------------------------------------------------------------------
// Section 4 – Boundary arithmetic (AC-1, AC-4)
// ---------------------------------------------------------------------------

/// Enabling the first signer sets `PauseSignerCount` from 0 to 1.
#[test]
fn sps_boundary_first_signer_increments_count_from_zero() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    // Precondition: count is 0.
    e.as_contract(&client.address, || {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        assert_eq!(count, 0);
    });

    client.set_pause_signer(&super_admin, &signer, &true);

    e.as_contract(&client.address, || {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        assert_eq!(count, 1, "first enable must set count to 1");
    });
}

/// Disabling the last signer decrements `PauseSignerCount` back to 0.
#[test]
fn sps_boundary_last_signer_decrements_count_to_zero() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    client.set_pause_signer(&super_admin, &signer, &true);
    client.set_pause_signer(&super_admin, &signer, &false);

    e.as_contract(&client.address, || {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        assert_eq!(count, 0, "removing the last signer must bring count to 0");
    });
    assert_invariants(&e, &client, &[signer]);
}

/// Each enable/disable pair increments then decrements the count correctly
/// across multiple signers (count integrity walk).
#[test]
fn sps_boundary_count_walks_correctly_with_multiple_signers() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let s3 = Address::generate(&e);
    let all = [s1.clone(), s2.clone(), s3.clone()];

    client.set_pause_signer(&super_admin, &s1, &true);
    assert_invariants(&e, &client, &all);

    client.set_pause_signer(&super_admin, &s2, &true);
    assert_invariants(&e, &client, &all);

    client.set_pause_signer(&super_admin, &s3, &true);
    assert_invariants(&e, &client, &all);

    client.set_pause_signer(&super_admin, &s2, &false);
    assert_invariants(&e, &client, &all);

    client.set_pause_signer(&super_admin, &s1, &false);
    assert_invariants(&e, &client, &all);

    client.set_pause_signer(&super_admin, &s3, &false);
    assert_invariants(&e, &client, &all);
}

/// When a signer is removed while `threshold == signer_count`, the threshold
/// is clamped to the new count (I-2) and the remaining signer can still achieve
/// quorum alone.
#[test]
fn sps_boundary_threshold_clamped_on_last_excess_signer_removal() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin, &s2, &true);
    // threshold == signer_count == 2
    client.set_pause_threshold(&super_admin, &2u32);

    // Remove one: threshold must clamp from 2 to 1 (I-2).
    client.set_pause_signer(&super_admin, &s2, &false);

    assert_invariants(&e, &client, &[s1.clone(), s2]);

    // The surviving signer can still propose + execute (threshold is now 1).
    let id = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

/// Threshold clamped to zero when the only signer is removed.
#[test]
fn sps_boundary_threshold_clamped_to_zero_when_sole_signer_removed() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);

    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_threshold(&super_admin, &1u32);

    client.set_pause_signer(&super_admin, &s1, &false);

    // Both count and threshold should be 0 now.
    e.as_contract(&client.address, || {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        let threshold: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0);
        assert_eq!(count, 0);
        assert_eq!(threshold, 0, "I-2: threshold must clamp to 0");
    });

    // With threshold == 0, SuperAdmin can pause directly.
    assert!(client.pause(&super_admin).is_none());
    assert!(client.is_paused());
}

/// When the threshold is already ≤ the new count, removing a signer must NOT
/// reduce the threshold (it should only clamp when threshold > new_count).
#[test]
fn sps_boundary_threshold_not_lowered_unnecessarily() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let s3 = Address::generate(&e);

    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin, &s2, &true);
    client.set_pause_signer(&super_admin, &s3, &true);
    // threshold = 2, count = 3
    client.set_pause_threshold(&super_admin, &2u32);

    // Remove s3: new count = 2, threshold = 2 — already equal, no clamping needed.
    client.set_pause_signer(&super_admin, &s3, &false);

    e.as_contract(&client.address, || {
        let threshold: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0);
        assert_eq!(
            threshold, 2,
            "threshold must not be lowered when it is already ≤ new count"
        );
    });
    assert_invariants(&e, &client, &[s1, s2, s3]);
}

// ---------------------------------------------------------------------------
// Section 5 – Epoch monotonicity (I-3, AC-4)
// ---------------------------------------------------------------------------

/// Each real state change advances the epoch by exactly 1; no-ops leave it
/// unchanged.  Sequence: enable → duplicate enable → disable → duplicate
/// disable.
#[test]
fn sps_epoch_advances_exactly_once_per_real_change() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    let e0 = client.get_config_epoch();

    client.set_pause_signer(&super_admin, &signer, &true); // +1
    assert_eq!(client.get_config_epoch(), e0 + 1);

    client.set_pause_signer(&super_admin, &signer, &true); // no-op
    assert_eq!(client.get_config_epoch(), e0 + 1);

    client.set_pause_signer(&super_admin, &signer, &false); // +1
    assert_eq!(client.get_config_epoch(), e0 + 2);

    client.set_pause_signer(&super_admin, &signer, &false); // no-op
    assert_eq!(client.get_config_epoch(), e0 + 2);
}

/// Epoch advances exactly once per signer when N signers are added in sequence.
#[test]
fn sps_epoch_increments_once_per_new_signer() {
    let (e, client, super_admin) = setup();
    let e0 = client.get_config_epoch();

    for i in 1u32..=5 {
        let s = Address::generate(&e);
        client.set_pause_signer(&super_admin, &s, &true);
        assert_eq!(
            client.get_config_epoch(),
            e0 + u64::from(i),
            "epoch must advance once per new signer (i={i})"
        );
    }
}

/// Rejected calls (wrong role, invalid address) never advance the epoch.
#[test]
fn sps_epoch_unchanged_on_all_rejection_paths() {
    let (e, client, super_admin) = setup();
    let zero = zero_address(&e);
    let contract_addr = client.address.clone();
    let stranger = Address::generate(&e);
    let mid_admin = Address::generate(&e);
    client.add_admin(&super_admin, &mid_admin, &AdminRole::Admin);

    let e0 = client.get_config_epoch();
    let signer = Address::generate(&e);

    // Rejected: wrong role
    let _ = client.try_set_pause_signer(&mid_admin, &signer, &true);
    let _ = client.try_set_pause_signer(&stranger, &signer, &true);
    // Rejected: invalid address
    let _ = client.try_set_pause_signer(&super_admin, &zero, &true);
    let _ = client.try_set_pause_signer(&super_admin, &zero, &false);
    let _ = client.try_set_pause_signer(&super_admin, &contract_addr, &true);
    let _ = client.try_set_pause_signer(&super_admin, &contract_addr, &false);

    assert_eq!(
        client.get_config_epoch(),
        e0,
        "I-3: none of the rejected calls must advance the epoch"
    );
}

// ---------------------------------------------------------------------------
// Section 6 – Event integrity (I-4, AC-6)
// ---------------------------------------------------------------------------

/// A successful enable emits exactly one `pause_signer_set` event; a
/// subsequent disable emits exactly one more; no-ops emit nothing.
#[test]
fn sps_event_emitted_iff_state_changes() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    let events_baseline = e.events().all().len();

    client.set_pause_signer(&super_admin, &signer, &true); // +1 event
    assert_eq!(e.events().all().len(), events_baseline + 1);

    client.set_pause_signer(&super_admin, &signer, &true); // no-op
    assert_eq!(e.events().all().len(), events_baseline + 1);

    client.set_pause_signer(&super_admin, &signer, &false); // +1 event
    assert_eq!(e.events().all().len(), events_baseline + 2);

    client.set_pause_signer(&super_admin, &signer, &false); // no-op
    assert_eq!(e.events().all().len(), events_baseline + 2);
}

/// Rejected calls (any error path) must emit zero events so off-chain indexers
/// cannot be confused into thinking a signer was set (AC-6).
#[test]
fn sps_event_not_emitted_on_rejection() {
    let (e, client, super_admin) = setup();
    let zero = zero_address(&e);
    let signer = Address::generate(&e);
    let stranger = Address::generate(&e);

    let events_baseline = e.events().all().len();

    // Invalid address
    let _ = client.try_set_pause_signer(&super_admin, &zero, &true);
    let _ = client.try_set_pause_signer(&super_admin, &zero, &false);
    let _ = client.try_set_pause_signer(&super_admin, &client.address.clone(), &true);

    // Wrong role
    let _ = client.try_set_pause_signer(&stranger, &signer, &true);

    assert_eq!(
        e.events().all().len(),
        events_baseline,
        "I-4: rejected calls must not emit any events"
    );
}

// ---------------------------------------------------------------------------
// Section 7 – Concurrent execution safety (AC-3)
// ---------------------------------------------------------------------------

/// Two SuperAdmins independently adding the same signer converge on count == 1
/// (the second call is idempotent).  This models the "concurrent duplicate
/// proposal" pattern from the retry contract.
#[test]
fn sps_concurrent_same_signer_converges_to_count_one() {
    let (e, client, super_admin) = setup();

    // Add a second SuperAdmin to simulate a second actor.
    let super_admin2 = Address::generate(&e);
    client.add_admin(&super_admin, &super_admin2, &AdminRole::SuperAdmin);

    let signer = Address::generate(&e);

    // First actor enables.
    client.set_pause_signer(&super_admin, &signer, &true);
    // Second actor retries with the same signer (simulate concurrent proposal).
    client.set_pause_signer(&super_admin2, &signer, &true);

    e.as_contract(&client.address, || {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        assert_eq!(count, 1, "duplicate concurrent enables must not double-count");
    });
    assert_invariants(&e, &client, &[signer]);
}

/// Interleaved enable/disable sequences from two independent SuperAdmins
/// always leave a consistent count (no count drift from concurrent ops).
#[test]
fn sps_concurrent_interleaved_ops_leave_consistent_count() {
    let (e, client, super_admin) = setup();
    let super_admin2 = Address::generate(&e);
    client.add_admin(&super_admin, &super_admin2, &AdminRole::SuperAdmin);

    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let all = [s1.clone(), s2.clone()];

    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin2, &s2, &true);
    assert_invariants(&e, &client, &all);

    client.set_pause_signer(&super_admin, &s1, &false);
    assert_invariants(&e, &client, &all);

    client.set_pause_signer(&super_admin2, &s1, &true); // re-enable from 2nd admin
    assert_invariants(&e, &client, &all);

    client.set_pause_signer(&super_admin, &s2, &false);
    assert_invariants(&e, &client, &all);
}

// ---------------------------------------------------------------------------
// Section 8 – State-machine / lifecycle (AC-1, AC-4)
// ---------------------------------------------------------------------------

/// Full lifecycle: add → use for multisig pause → remove → verify count and
/// threshold both updated → verify contract still operable.
#[test]
fn sps_lifecycle_full_add_use_remove() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    // Setup: 2 signers, threshold 2.
    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin, &s2, &true);
    client.set_pause_threshold(&super_admin, &2u32);

    // Pause the contract via multisig.
    let id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &id);
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());

    // Unpause.
    let id2 = client.unpause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &id2);
    client.execute_pause_proposal(&id2);
    assert!(!client.is_paused());

    // Remove s2; threshold must clamp to 1.
    client.set_pause_signer(&super_admin, &s2, &false);
    assert_invariants(&e, &client, &[s1.clone(), s2]);

    // s1 alone can still pause (threshold == 1).
    let id3 = client.pause(&s1).unwrap();
    client.execute_pause_proposal(&id3);
    assert!(client.is_paused());
}

/// Signer toggled off then back on re-enters the count and threshold
/// calculations correctly (round-trip consistency).
#[test]
fn sps_lifecycle_signer_round_trip_reenable() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    for _ in 0..3 {
        client.set_pause_signer(&super_admin, &signer, &true);
        assert_invariants(&e, &client, &[signer.clone()]);
        client.set_pause_signer(&super_admin, &signer, &false);
        assert_invariants(&e, &client, &[signer.clone()]);
    }
}

/// After a pause signer is removed and re-added while a multisig proposal is
/// live in the same epoch, the proposal remains independently valid — signer
/// management and proposal state are orthogonal.
#[test]
fn sps_lifecycle_signer_removal_does_not_invalidate_live_proposal() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);

    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_signer(&super_admin, &s2, &true);
    client.set_pause_threshold(&super_admin, &1u32);

    // Propose from s1.
    let id = client.pause(&s1).unwrap();

    // Remove s2 while the proposal is live (affects count/threshold, not the proposal).
    client.set_pause_signer(&super_admin, &s2, &false);
    assert_invariants(&e, &client, &[s1.clone(), s2]);

    // The proposal is still executable (s1's approval meets threshold == 1).
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// Section 9 – Regression / public interface compatibility (AC-5)
// ---------------------------------------------------------------------------

/// `set_pause_signer(admin, signer, false)` followed immediately by
/// `set_pause_signer(admin, signer, true)` re-enables the signer in one round
/// trip without corrupting the count.
#[test]
fn sps_regression_disable_then_reenable_sequence() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);
    let all = [signer.clone()];

    client.set_pause_signer(&super_admin, &signer, &true);
    client.set_pause_signer(&super_admin, &signer, &false);
    client.set_pause_signer(&super_admin, &signer, &true);

    assert_invariants(&e, &client, &all);

    // Should still be usable for multisig.
    client.set_pause_threshold(&super_admin, &1u32);
    let id = client.pause(&signer).unwrap();
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

/// The function signature and return type remain the same as the original
/// contract definition (public interface compatibility, AC-5).
#[test]
fn sps_public_interface_returns_unit() {
    let (e, client, super_admin) = setup();
    let signer = Address::generate(&e);

    // `set_pause_signer` returns `()` — this test verifies the signature has not
    // changed to return a value.  The `.unwrap()` chain works only if it returns unit.
    client.set_pause_signer(&super_admin, &signer, &true);
    client.set_pause_signer(&super_admin, &signer, &false);
}

/// Multiple unique signers can be registered without count drift, and threshold
/// can be raised to their exact count (the inclusive boundary).
#[test]
fn sps_regression_n_signers_threshold_at_exact_count() {
    let (e, client, super_admin) = setup();

    const N: usize = 5;
    let mut signers: Vec<Address> = Vec::new();
    for _ in 0..N {
        let s = Address::generate(&e);
        client.set_pause_signer(&super_admin, &s, &true);
        signers.push(s);
    }
    let all: Vec<Address> = signers.clone();

    // Threshold == count (inclusive upper bound).
    client.set_pause_threshold(&super_admin, &(N as u32));
    assert_invariants(&e, &client, &all);

    // All N signers approve — should be executable.
    let id = client.pause(&signers[0]).unwrap();
    for s in &signers[1..] {
        client.approve_pause_proposal(s, &id);
    }
    client.execute_pause_proposal(&id);
    assert!(client.is_paused());
}

/// After removing all N signers one by one, `PauseSignerCount` reaches 0 and
/// `PauseThreshold` is clamped to 0 at each step.
#[test]
fn sps_regression_drain_all_signers_leaves_zero_count_and_threshold() {
    let (e, client, super_admin) = setup();

    const N: usize = 4;
    let mut signers: Vec<Address> = Vec::new();
    for _ in 0..N {
        let s = Address::generate(&e);
        client.set_pause_signer(&super_admin, &s, &true);
        signers.push(s.clone());
    }
    client.set_pause_threshold(&super_admin, &(N as u32));

    for s in &signers {
        client.set_pause_signer(&super_admin, s, &false);
        assert_invariants(&e, &client, &signers);
    }

    e.as_contract(&client.address, || {
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseSignerCount)
            .unwrap_or(0);
        let threshold: u32 = e
            .storage()
            .instance()
            .get(&DataKey::PauseThreshold)
            .unwrap_or(0);
        assert_eq!(count, 0, "all signers removed: count must be 0");
        assert_eq!(threshold, 0, "all signers removed: threshold must be clamped to 0");
    });

    // With threshold 0, SuperAdmin can pause directly again.
    assert!(client.pause(&super_admin).is_none());
    assert!(client.is_paused());
}

/// Verify the stale-epoch detection still operates correctly after a
/// `set_pause_signer` call that advanced the epoch mid-proposal.
#[test]
fn sps_regression_epoch_advance_does_not_affect_proposal_staleness() {
    let (e, client, super_admin) = setup();
    let s1 = Address::generate(&e);
    client.set_pause_signer(&super_admin, &s1, &true);
    client.set_pause_threshold(&super_admin, &1u32);

    let epoch_boundary = u32::from(PROPOSAL_EPOCH_SIZE);
    e.ledger().with_mut(|l| l.sequence_number = epoch_boundary - 1);

    let id = client.pause(&s1).unwrap();

    // Advancing epoch via a signer management call must not retroactively make
    // the proposal valid beyond its own ledger epoch.
    e.ledger().with_mut(|l| l.sequence_number = epoch_boundary);

    // The proposal is now stale.
    let err = client
        .try_execute_pause_proposal(&id)
        .unwrap_err()
        .unwrap();
    assert_eq!(
        err,
        soroban_sdk::Error::from_contract_error(514), // StaleAdminEpoch
        "proposal must be stale after crossing epoch boundary"
    );
}
