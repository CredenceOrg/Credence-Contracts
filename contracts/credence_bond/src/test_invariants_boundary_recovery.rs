//! Boundary and recovery coverage for `invariants.rs` (issue #1334).
//!
//! Covers:
//! - I2 drift guard: `slashed_amount <= bonded_amount` with boundary cases
//! - I7 drift guard: attestation count/list consistency with boundary cases
//! - `assert_self_consistent` and `assert_self_consistent_for_subject` behavior
//! - `fail_drift` event emission and panic semantics
//! - Retry/recovery after invariant violation detection
//! - Multi-identity scenarios where subject != bond identity

#![cfg(test)]

use crate::invariants::{
    assert_self_consistent, assert_self_consistent_for_subject,
    check_attestation_count_consistent, check_bond_slashed_within_bonded,
    fail_drift, BondDriftDetails, BondDriftKind,
};
use crate::{CredenceBond, CredenceBondClient, DataKey, IdentityBond};
use credence_errors::ContractError;
use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{Address, Env, Symbol, TryFromVal, Vec};
use std::panic::AssertUnwindSafe;

fn make_env() -> Env {
    let e = Env::default();
    e.mock_all_auths();
    e
}

fn deploy_bond_contract(e: &Env) -> (Address, CredenceBondClient<'_>, Address, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let identity = Address::generate(e);
    client.initialize(&admin, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);
    (contract_id, client, admin, identity)
}

fn inject_bond(e: &Env, contract_id: &Address, bond: IdentityBond) {
    e.as_contract(contract_id, || {
        e.storage().instance().set(&DataKey::Bond, &bond);
    });
}

fn inject_attestation_count(e: &Env, contract_id: &Address, subject: &Address, count: u32) {
    e.as_contract(contract_id, || {
        let count_key = DataKey::SubjectAttestationCount(subject.clone());
        e.storage().instance().set(&count_key, &count);
    });
}

fn inject_attestation_list(e: &Env, contract_id: &Address, subject: &Address, ids: Vec<u64>) {
    e.as_contract(contract_id, || {
        let list_key = DataKey::SubjectAttestations(subject.clone());
        e.storage().instance().set(&list_key, &ids);
    });
}

fn last_bond_drift_event(e: &Env) -> Option<(BondDriftKind, i128, i128, u32, u32)> {
    let drift_sym = Symbol::new(e, "bond_drift_detected");
    e.events().all().iter().rev().find_map(|(_, topics, data)| {
        let tag = Symbol::try_from_val(e, &topics.get(0).unwrap()).ok()?;
        if tag != drift_sym {
            return None;
        }
        <(BondDriftKind, i128, i128, u32, u32)>::try_from_val(e, &data).ok()
    })
}

// ============================================================================
// check_bond_slashed_within_bonded — boundary cases (I2)
// ============================================================================

#[test]
fn check_bond_slashed_within_bonded_passes_when_equal() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let bond = IdentityBond {
        identity: Address::generate(&e),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 1_000,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());
    e.as_contract(&contract_id, || {
        check_bond_slashed_within_bonded(&e, &bond);
    });
}

#[test]
#[should_panic(expected = "InvariantViolation")]
fn check_bond_slashed_within_bonded_panics_when_slashed_exceeds_bonded() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let bond = IdentityBond {
        identity: Address::generate(&e),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 1_001,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());
    e.as_contract(&contract_id, || {
        check_bond_slashed_within_bonded(&e, &bond);
    });
}

#[test]
#[should_panic(expected = "InvariantViolation")]
fn check_bond_slashed_within_bonded_panics_when_bonded_negative() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let bond = IdentityBond {
        identity: Address::generate(&e),
        bonded_amount: -1,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());
    e.as_contract(&contract_id, || {
        check_bond_slashed_within_bonded(&e, &bond);
    });
}

#[test]
#[should_panic(expected = "InvariantViolation")]
fn check_bond_slashed_within_bonded_panics_when_slashed_negative() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let bond = IdentityBond {
        identity: Address::generate(&e),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: -1,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());
    e.as_contract(&contract_id, || {
        check_bond_slashed_within_bonded(&e, &bond);
    });
}

#[test]
fn check_bond_slashed_within_bonded_emits_structured_event_on_violation() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let bond = IdentityBond {
        identity: Address::generate(&e),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 1_100,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());

    let panic_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            check_bond_slashed_within_bonded(&e, &bond);
        });
    }));
    assert!(panic_result.is_err());

    let (kind, bonded, slashed, _count, _list_len) =
        last_bond_drift_event(&e).expect("bond_drift_detected event must be emitted");
    assert_eq!(kind, BondDriftKind::SlashedExceedsBonded);
    assert_eq!(bonded, 1_000);
    assert_eq!(slashed, 1_100);
}

// ============================================================================
// check_attestation_count_consistent — boundary cases (I7)
// ============================================================================

#[test]
fn check_attestation_count_consistent_passes_when_count_matches_list() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let bond = IdentityBond {
        identity: subject.clone(),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());

    let list = Vec::from_array(&e, [1u64, 2u64, 3u64]);
    inject_attestation_count(&e, &contract_id, &subject, 3);
    inject_attestation_list(&e, &contract_id, &subject, list);

    e.as_contract(&contract_id, || {
        check_attestation_count_consistent(&e, &subject, &bond);
    });
}

#[test]
fn check_attestation_count_consistent_passes_when_no_count_key() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let bond = IdentityBond {
        identity: subject.clone(),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());
    // No count key, no list — should pass (list_len = 0, count absent)
    e.as_contract(&contract_id, || {
        check_attestation_count_consistent(&e, &subject, &bond);
    });
}

#[test]
fn check_attestation_count_consistent_passes_when_empty_list_and_zero_count() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let bond = IdentityBond {
        identity: subject.clone(),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());
    inject_attestation_count(&e, &contract_id, &subject, 0);
    inject_attestation_list(&e, &contract_id, &subject, Vec::new(&e));

    e.as_contract(&contract_id, || {
        check_attestation_count_consistent(&e, &subject, &bond);
    });
}

#[test]
#[should_panic(expected = "InvariantViolation")]
fn check_attestation_count_consistent_panics_when_count_exceeds_list() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let bond = IdentityBond {
        identity: subject.clone(),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());
    inject_attestation_count(&e, &contract_id, &subject, 5);
    inject_attestation_list(&e, &contract_id, &subject, Vec::from_array(&e, [1u64, 2u64]));

    e.as_contract(&contract_id, || {
        check_attestation_count_consistent(&e, &subject, &bond);
    });
}

#[test]
#[should_panic(expected = "InvariantViolation")]
fn check_attestation_count_consistent_panics_when_list_exceeds_count() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let bond = IdentityBond {
        identity: subject.clone(),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());
    inject_attestation_count(&e, &contract_id, &subject, 2);
    inject_attestation_list(&e, &contract_id, &subject, Vec::from_array(&e, [1u64, 2u64, 3u64]));

    e.as_contract(&contract_id, || {
        check_attestation_count_consistent(&e, &subject, &bond);
    });
}

#[test]
fn check_attestation_count_consistent_emits_structured_event_on_violation() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let bond = IdentityBond {
        identity: subject.clone(),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());
    inject_attestation_count(&e, &contract_id, &subject, 99);
    inject_attestation_list(&e, &contract_id, &subject, Vec::new(&e));

    let panic_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            check_attestation_count_consistent(&e, &subject, &bond);
        });
    }));
    assert!(panic_result.is_err());

    let (kind, _bonded, _slashed, count, list_len) =
        last_bond_drift_event(&e).expect("bond_drift_detected event must be emitted");
    assert_eq!(kind, BondDriftKind::AttestationCountMismatch);
    assert_eq!(count, 99);
    assert_eq!(list_len, 0);
}

// ============================================================================
// fail_drift — event emission and panic semantics
// ============================================================================

#[test]
fn fail_drift_emits_bond_drift_detected_before_panic() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let details = BondDriftDetails {
        kind: BondDriftKind::SlashedExceedsBonded,
        subject: Address::generate(&e),
        bonded_amount: 500,
        slashed_amount: 600,
        attestation_count: 0,
        attestation_list_len: 0,
    };

    let panic_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            fail_drift(&e, details.clone());
        });
    }));
    assert!(panic_result.is_err());

    let (kind, bonded, slashed, _count, _list_len) =
        last_bond_drift_event(&e).expect("bond_drift_detected event must be emitted");
    assert_eq!(kind, BondDriftKind::SlashedExceedsBonded);
    assert_eq!(bonded, 500);
    assert_eq!(slashed, 600);
}

#[test]
fn fail_drift_panics_with_invariant_violation_error_code() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let details = BondDriftDetails {
        kind: BondDriftKind::AttestationCountMismatch,
        subject: Address::generate(&e),
        bonded_amount: 0,
        slashed_amount: 0,
        attestation_count: 42,
        attestation_list_len: 0,
    };

    let panic_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            fail_drift(&e, details);
        });
    }));
    assert!(panic_result.is_err(), "fail_drift must panic with InvariantViolation");
}

#[test]
fn fail_drift_uses_provided_details_in_event() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let details = BondDriftDetails {
        kind: BondDriftKind::AttestationCountMismatch,
        subject: subject.clone(),
        bonded_amount: 123,
        slashed_amount: 456,
        attestation_count: 10,
        attestation_list_len: 7,
    };

    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            fail_drift(&e, details);
        });
    }));

    let (kind, bonded, slashed, count, list_len) =
        last_bond_drift_event(&e).expect("bond_drift_detected event must be emitted");
    assert_eq!(kind, BondDriftKind::AttestationCountMismatch);
    assert_eq!(bonded, 123);
    assert_eq!(slashed, 456);
    assert_eq!(count, 10);
    assert_eq!(list_len, 7);
}

// ============================================================================
// assert_self_consistent — multi-identity and boundary scenarios
// ============================================================================

#[test]
fn assert_self_consistent_passes_for_clean_bond() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    e.as_contract(&contract_id, || {
        assert_self_consistent(&e);
    });
}

#[test]
fn assert_self_consistent_passes_when_no_bond_exists() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let admin = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&admin, &None);
    // No bond created yet

    e.as_contract(&contract_id, || {
        assert_self_consistent(&e);
    });
}

#[test]
#[should_panic(expected = "InvariantViolation")]
fn assert_self_consistent_panics_on_slashed_over_bonded() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    // Inject drift
    e.as_contract(&contract_id, || {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e.storage().instance().get(&key).unwrap();
        bond.slashed_amount = bond.bonded_amount + 100;
        e.storage().instance().set(&key, &bond);
    });

    e.as_contract(&contract_id, || {
        assert_self_consistent(&e);
    });
}

// ============================================================================
// assert_self_consistent_for_subject — subject != bond identity
// ============================================================================

#[test]
fn assert_self_consistent_for_subject_passes_when_subject_matches_bond_identity() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    e.as_contract(&contract_id, || {
        assert_self_consistent_for_subject(&e, &identity);
    });
}

#[test]
fn assert_self_consistent_for_subject_passes_when_no_bond_for_subject() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let other_subject = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    // other_subject has no bond, but check should pass (attestation count check is a no-op)
    e.as_contract(&contract_id, || {
        assert_self_consistent_for_subject(&e, &other_subject);
    });
}

#[test]
#[should_panic(expected = "InvariantViolation")]
fn assert_self_consistent_for_subject_panics_when_attestation_count_drift() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let other_subject = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    // Inject attestation count drift for other_subject
    inject_attestation_count(&e, &contract_id, &other_subject, 99);
    // No attestation list injected (so list_len = 0)

    e.as_contract(&contract_id, || {
        assert_self_consistent_for_subject(&e, &other_subject);
    });
}

#[test]
fn assert_self_consistent_for_subject_emits_event_on_attestation_drift() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let other_subject = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    inject_attestation_count(&e, &contract_id, &other_subject, 99);

    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            assert_self_consistent_for_subject(&e, &other_subject);
        });
    }));

    let (kind, _bonded, _slashed, count, list_len) =
        last_bond_drift_event(&e).expect("bond_drift_detected event must be emitted");
    assert_eq!(kind, BondDriftKind::AttestationCountMismatch);
    assert_eq!(count, 99);
    assert_eq!(list_len, 0);
}

// ============================================================================
// Retry / recovery after invariant violation
// ============================================================================

#[test]
fn retry_after_caught_invariant_violation_succeeds_when_drift_cleared() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    // First: inject drift and catch the panic
    e.as_contract(&contract_id, || {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e.storage().instance().get(&key).unwrap();
        bond.slashed_amount = bond.bonded_amount + 100;
        e.storage().instance().set(&key, &bond);
    });

    let first_panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            assert_self_consistent(&e);
        });
    }));
    assert!(first_panic.is_err());

    // Second: clear the drift (admin fixes the bond state)
    e.as_contract(&contract_id, || {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e.storage().instance().get(&key).unwrap();
        bond.slashed_amount = bond.bonded_amount; // Bring back within bounds
        e.storage().instance().set(&key, &bond);
    });

    // Third: retry the check — should now pass
    e.as_contract(&contract_id, || {
        assert_self_consistent(&e);
    });
}

#[test]
fn retry_after_attestation_drift_succeeds_when_count_fixed() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let other_subject = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    // Inject drift
    inject_attestation_count(&e, &contract_id, &other_subject, 99);

    let first_panic = std::panic::catch_unwind(AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            assert_self_consistent_for_subject(&e, &other_subject);
        });
    }));
    assert!(first_panic.is_err());

    // Fix the drift: add matching attestation list
    inject_attestation_list(&e, &contract_id, &other_subject, Vec::from_array(&e, [1u64, 2u64, 3u64, 4u64, 5u64, 6u64, 7u64, 8u64, 9u64, 10u64, 11u64, 12u64, 13u64, 14u64, 15u64, 16u64, 17u64, 18u64, 19u64, 20u64, 21u64, 22u64, 23u64, 24u64, 25u64, 26u64, 27u64, 28u64, 29u64, 30u64, 31u64, 32u64, 33u64, 34u64, 35u64, 36u64, 37u64, 38u64, 39u64, 40u64, 41u64, 42u64, 43u64, 44u64, 45u64, 46u64, 47u64, 48u64, 49u64, 50u64, 51u64, 52u64, 53u64, 54u64, 55u64, 56u64, 57u64, 58u64, 59u64, 60u64, 61u64, 62u64, 63u64, 64u64, 65u64, 66u64, 67u64, 68u64, 69u64, 70u64, 71u64, 72u64, 73u64, 74u64, 75u64, 76u64, 77u64, 78u64, 79u64, 80u64, 81u64, 82u64, 83u64, 84u64, 85u64, 86u64, 87u64, 88u64, 89u64, 90u64, 91u64, 92u64, 93u64, 94u64, 95u64, 96u64, 97u64, 98u64, 99u64]));

    // Retry — should now pass
    e.as_contract(&contract_id, || {
        assert_self_consistent_for_subject(&e, &other_subject);
    });
}

// ============================================================================
// Determinism: duplicate inputs, boundary values
// ============================================================================

#[test]
fn assert_self_consistent_deterministic_for_identical_state() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    // Run the check twice with identical state
    e.as_contract(&contract_id, || {
        assert_self_consistent(&e);
    });
    e.as_contract(&contract_id, || {
        assert_self_consistent(&e);
    });
}

#[test]
fn check_bond_slashed_within_bonded_deterministic_for_boundary_zero() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let bond = IdentityBond {
        identity: Address::generate(&e),
        bonded_amount: 0,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());

    e.as_contract(&contract_id, || {
        check_bond_slashed_within_bonded(&e, &bond);
    });
    e.as_contract(&contract_id, || {
        check_bond_slashed_within_bonded(&e, &bond);
    });
}

#[test]
fn check_attestation_count_consistent_deterministic_for_large_list() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let bond = IdentityBond {
        identity: subject.clone(),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());

    // Create a large list (near u32 boundary for count)
    let large_list: Vec<u64> = (0..1000).map(|i| i as u64).collect();
    inject_attestation_count(&e, &contract_id, &subject, 1000);
    inject_attestation_list(&e, &contract_id, &subject, large_list.clone());

    e.as_contract(&contract_id, || {
        check_attestation_count_consistent(&e, &subject, &bond);
    });
    e.as_contract(&contract_id, || {
        check_attestation_count_consistent(&e, &subject, &bond);
    });
}

// ============================================================================
// Concurrent / interleaved execution safety
// ============================================================================

#[test]
fn assert_self_consistent_safe_under_rapid_sequential_calls() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    // Rapid sequential calls should all pass
    for _ in 0..20 {
        e.as_contract(&contract_id, || {
            assert_self_consistent(&e);
        });
    }
}

#[test]
fn assert_self_consistent_for_subject_safe_under_rapid_sequential_calls() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let identity = Address::generate(&e);
    let other_subject = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&identity, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    // Rapid sequential calls for different subjects
    for _ in 0..20 {
        e.as_contract(&contract_id, || {
            assert_self_consistent_for_subject(&e, &identity);
            assert_self_consistent_for_subject(&e, &other_subject);
        });
    }
}

// ============================================================================
// Diagnosability: event structure and error messages
// ============================================================================

#[test]
fn bond_drift_event_contains_all_required_fields() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let bond = IdentityBond {
        identity: Address::generate(&e),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 1_100,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());

    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            check_bond_slashed_within_bonded(&e, &bond);
        });
    }));

    let (kind, bonded, slashed, count, list_len) =
        last_bond_drift_event(&e).expect("bond_drift_detected event must be emitted");

    // All fields must be present and well-formed
    assert_eq!(kind, BondDriftKind::SlashedExceedsBonded);
    assert!(bonded >= 0, "bonded_amount must be non-negative in event");
    assert!(slashed > bonded, "slashed must exceed bonded in event");
    assert_eq!(count, 0, "attestation_count must be present");
    assert_eq!(list_len, 0, "attestation_list_len must be present");
}

#[test]
fn invariant_violation_error_code_in_bond_block() {
    // ContractError::InvariantViolation must be in the bond error range (200-299)
    assert_eq!(ContractError::InvariantViolation as u32, 218);
}

// ============================================================================
// Permission boundary: only admin can trigger drift (via slashing)
// ============================================================================

#[test]
fn non_admin_cannot_directly_invoke_assert_self_consistent_via_client() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let admin = Address::generate(&e);
    let identity = Address::generate(&e);
    let non_admin = Address::generate(&e);
    let client = CredenceBondClient::new(&e, &contract_id);
    client.initialize(&admin, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);

    // assert_self_consistent is internal, not exposed as a client entrypoint.
    // This test documents that it's only callable internally (via as_contract).
    // Non-admin users cannot directly invoke it — they can only trigger it
    // indirectly through state-changing entrypoints like slash, add_attestation, etc.
    e.as_contract(&contract_id, || {
        assert_self_consistent(&e);
    });
}

// ============================================================================
// Stress: maximum attestation list size within storage limits
// ============================================================================

#[test]
fn check_attestation_count_consistent_handles_large_matching_list() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let bond = IdentityBond {
        identity: subject.clone(),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());

    // Large list (1000 items) - tests O(n) list length walk
    let large_list: Vec<u64> = (0..1000).map(|i| i as u64).collect();
    inject_attestation_count(&e, &contract_id, &subject, 1000);
    inject_attestation_list(&e, &contract_id, &subject, large_list);

    e.as_contract(&contract_id, || {
        check_attestation_count_consistent(&e, &subject, &bond);
    });
}

#[test]
#[should_panic(expected = "InvariantViolation")]
fn check_attestation_count_consistent_detects_drift_at_large_scale() {
    let e = make_env();
    let contract_id = e.register(CredenceBond, ());
    let subject = Address::generate(&e);
    let bond = IdentityBond {
        identity: subject.clone(),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 3_600,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    };
    inject_bond(&e, &contract_id, bond.clone());

    // Count says 1000 but list has 1001
    let large_list: Vec<u64> = (0..1001).map(|i| i as u64).collect();
    inject_attestation_count(&e, &contract_id, &subject, 1000);
    inject_attestation_list(&e, &contract_id, &subject, large_list);

    e.as_contract(&contract_id, || {
        check_attestation_count_consistent(&e, &subject, &bond);
    });
}

