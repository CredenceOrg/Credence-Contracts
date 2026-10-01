//! Boundary and recovery coverage for the `emergency` module (#1322).
//!
//! `test_emergency.rs` drives the public `CredenceBondClient` entrypoints. This
//! suite complements it by exercising `crate::emergency` directly, pinning the
//! boundaries that are awkward to reach through the contract surface:
//!
//! * fee-bps validation edges (0, the 10_000 cap, one past it),
//! * fee rounding at sub-unit and full-bps amounts,
//! * the idempotent `set_enabled` no-op and monotonic audit sequences,
//! * missing-record / missing-transition recovery, and
//! * determinism of repeated reads across a ledger advance.

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

fn setup(e: &Env) -> CredenceBondClient<'_> {
    let (client, _admin, _identity, ..) = test_helpers::setup_with_token(e);
    client
}

// ───────────────────────────── calculate_fee ─────────────────────────────

#[test]
fn calculate_fee_zero_bps_is_zero_for_any_amount() {
    // The zero-bps short-circuit must not touch the multiply/divide path, so even
    // an extreme amount yields zero without an overflow panic.
    assert_eq!(crate::emergency::calculate_fee(0, 0), 0);
    assert_eq!(crate::emergency::calculate_fee(1, 0), 0);
    assert_eq!(crate::emergency::calculate_fee(i128::MAX, 0), 0);
}

#[test]
fn calculate_fee_full_bps_returns_whole_amount() {
    assert_eq!(crate::emergency::calculate_fee(1_000, 10_000), 1_000);
    assert_eq!(crate::emergency::calculate_fee(9, 10_000), 9);
    assert_eq!(crate::emergency::calculate_fee(0, 10_000), 0);
}

#[test]
fn calculate_fee_truncates_sub_unit_remainders_deterministically() {
    // 1 * 1 / 10_000 truncates to 0; 10_000 * 1 / 10_000 == 1.
    assert_eq!(crate::emergency::calculate_fee(1, 1), 0);
    assert_eq!(crate::emergency::calculate_fee(9_999, 1), 0);
    assert_eq!(crate::emergency::calculate_fee(10_000, 1), 1);
    // Matches the value asserted by the integration suite.
    assert_eq!(crate::emergency::calculate_fee(200, 500), 10);
}

// ───────────────────────── set_config / get_config ─────────────────────────

#[test]
#[should_panic(expected = "emergency config not set")]
fn get_config_panics_before_configuration() {
    let e = Env::default();
    let client = setup(&e);

    e.as_contract(&client.address, || {
        let _ = crate::emergency::get_config(&e);
    });
}

#[test]
fn set_config_accepts_exact_fee_boundaries() {
    let e = Env::default();
    let client = setup(&e);
    let governance = Address::generate(&e);
    let treasury = Address::generate(&e);

    e.as_contract(&client.address, || {
        crate::emergency::set_config(&e, governance.clone(), treasury.clone(), 0, false);
        let zero = crate::emergency::get_config(&e);
        assert_eq!(zero.emergency_fee_bps, 0);
        assert!(!zero.enabled);

        // Exactly at the cap is allowed and overwrites the previous config.
        crate::emergency::set_config(&e, governance.clone(), treasury.clone(), 10_000, true);
        let full = crate::emergency::get_config(&e);
        assert_eq!(full.emergency_fee_bps, 10_000);
        assert!(full.enabled);
        assert_eq!(full.governance, governance);
        assert_eq!(full.treasury, treasury);
    });
}

#[test]
#[should_panic(expected = "emergency fee bps must be <= 10000")]
fn set_config_rejects_one_bps_above_boundary() {
    let e = Env::default();
    let client = setup(&e);
    let governance = Address::generate(&e);
    let treasury = Address::generate(&e);

    e.as_contract(&client.address, || {
        crate::emergency::set_config(&e, governance, treasury, 10_001, false);
    });
}

// ─────────────────── set_enabled: idempotency + recovery ───────────────────

#[test]
fn set_enabled_is_a_noop_when_state_is_unchanged() {
    let e = Env::default();
    let client = setup(&e);
    let admin = Address::generate(&e);
    let governance = Address::generate(&e);
    let treasury = Address::generate(&e);

    e.as_contract(&client.address, || {
        crate::emergency::set_config(&e, governance.clone(), treasury, 250, false);

        // Already disabled → no transition is written, sequence stays at 0.
        crate::emergency::set_enabled(&e, false, &admin, &governance, Symbol::new(&e, "noop"));
        assert_eq!(crate::emergency::latest_transition_id(&e), 0);
        assert!(!crate::emergency::get_config(&e).enabled);
    });
}

#[test]
fn set_enabled_records_only_real_transitions_and_recovers_sequence() {
    let e = Env::default();
    let client = setup(&e);
    let admin = Address::generate(&e);
    let governance = Address::generate(&e);
    let treasury = Address::generate(&e);

    e.as_contract(&client.address, || {
        crate::emergency::set_config(&e, governance.clone(), treasury, 250, false);

        // on, on (no-op), off, off (no-op), on → exactly three real transitions.
        crate::emergency::set_enabled(&e, true, &admin, &governance, Symbol::new(&e, "on1"));
        crate::emergency::set_enabled(&e, true, &admin, &governance, Symbol::new(&e, "on1dup"));
        crate::emergency::set_enabled(&e, false, &admin, &governance, Symbol::new(&e, "off1"));
        crate::emergency::set_enabled(&e, false, &admin, &governance, Symbol::new(&e, "off1dup"));
        crate::emergency::set_enabled(&e, true, &admin, &governance, Symbol::new(&e, "on2"));

        assert_eq!(crate::emergency::latest_transition_id(&e), 3);
        assert!(crate::emergency::get_transition(&e, 1).enabled);
        assert!(!crate::emergency::get_transition(&e, 2).enabled);
        assert!(crate::emergency::get_transition(&e, 3).enabled);
        assert!(crate::emergency::get_config(&e).enabled);
    });
}

#[test]
#[should_panic(expected = "transition not found")]
fn get_transition_missing_id_panics() {
    let e = Env::default();
    let client = setup(&e);

    e.as_contract(&client.address, || {
        let _ = crate::emergency::get_transition(&e, 1);
    });
}

// ───────────────────────── store_record / get_record ─────────────────────────

#[test]
fn record_sequence_starts_at_zero_and_increments_per_store() {
    let e = Env::default();
    let client = setup(&e);
    let identity = Address::generate(&e);
    let treasury = Address::generate(&e);
    let admin = Address::generate(&e);
    let governance = Address::generate(&e);

    e.as_contract(&client.address, || {
        assert_eq!(crate::emergency::latest_record_id(&e), 0);

        let id1 = crate::emergency::store_record(
            &e,
            identity.clone(),
            200,
            10,
            190,
            treasury.clone(),
            admin.clone(),
            governance.clone(),
            Symbol::new(&e, "r1"),
        );
        let id2 = crate::emergency::store_record(
            &e,
            identity.clone(),
            500,
            0,
            500,
            treasury.clone(),
            admin.clone(),
            governance.clone(),
            Symbol::new(&e, "r2"),
        );

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(crate::emergency::latest_record_id(&e), 2);

        let first = crate::emergency::get_record(&e, id1);
        assert_eq!(first.identity, identity);
        assert_eq!(first.gross_amount, 200);
        assert_eq!(first.fee_amount, 10);
        assert_eq!(first.net_amount, 190);

        // Record and transition sequences are independent.
        assert_eq!(crate::emergency::latest_transition_id(&e), 0);
    });
}

#[test]
#[should_panic(expected = "record not found")]
fn get_record_missing_id_panics() {
    let e = Env::default();
    let client = setup(&e);

    e.as_contract(&client.address, || {
        let _ = crate::emergency::get_record(&e, 1);
    });
}

#[test]
fn stored_records_survive_ledger_advance_and_stay_recoverable() {
    let e = Env::default();
    let client = setup(&e);
    let identity = Address::generate(&e);
    let treasury = Address::generate(&e);
    let admin = Address::generate(&e);
    let governance = Address::generate(&e);

    e.as_contract(&client.address, || {
        crate::emergency::store_record(
            &e,
            identity.clone(),
            100,
            5,
            95,
            treasury.clone(),
            admin.clone(),
            governance.clone(),
            Symbol::new(&e, "recover"),
        );
    });

    // Advance the ledger: the persistent entry must remain readable and intact.
    test_helpers::advance_ledger_sequence(&e);
    e.ledger().with_mut(|l| l.timestamp += 86_400);

    e.as_contract(&client.address, || {
        let record = crate::emergency::get_record(&e, 1);
        assert_eq!(record.id, 1);
        assert_eq!(record.net_amount, 95);
        assert_eq!(record.reason, Symbol::new(&e, "recover"));
        assert_eq!(crate::emergency::latest_record_id(&e), 1);
    });
}
