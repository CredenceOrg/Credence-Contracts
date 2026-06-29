//! Issue #436: bond drift detection — panic + `bond_drift_detected` event coverage.

use crate::invariants::{assert_self_consistent, BondDriftKind};
use crate::{CredenceBond, CredenceBondClient, DataKey, IdentityBond};
use credence_errors::ContractError;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, Symbol, TryFromVal};

fn setup_contract(e: &Env) -> (Address, CredenceBondClient<'_>) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    let identity = Address::generate(e);
    client.initialize(&admin, &None);
    client.create_bond(&identity, &1_000_i128, &3_600_u64, &false, &0_u64);
    (contract_id, client)
}

fn inject_slashed_over_bonded(e: &Env, contract_id: &Address) {
    e.as_contract(contract_id, || {
        let key = DataKey::Bond;
        let mut bond: IdentityBond = e.storage().instance().get(&key).unwrap();
        bond.slashed_amount = bond.bonded_amount + 100;
        e.storage().instance().set(&key, &bond);
    });
}

fn inject_attestation_count_drift(e: &Env, contract_id: &Address, subject: &Address) {
    e.as_contract(contract_id, || {
        let count_key = DataKey::SubjectAttestationCount(subject.clone());
        e.storage().instance().set(&count_key, &99_u32);
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

#[test]
#[should_panic(expected = "HostError")]
fn bond_drift_slashed_over_bonded_panics_with_invariant_violation() {
    let e = Env::default();
    let (contract_id, _) = setup_contract(&e);
    inject_slashed_over_bonded(&e, &contract_id);
    e.as_contract(&contract_id, || {
        assert_self_consistent(&e);
    });
}

#[test]
fn bond_drift_slashed_over_bonded_emits_structured_event() {
    extern crate std;

    let e = Env::default();
    let (contract_id, _) = setup_contract(&e);
    inject_slashed_over_bonded(&e, &contract_id);

    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            assert_self_consistent(&e);
        });
    }));
    assert!(panic_result.is_err(), "expected InvariantViolation panic");

    let (kind, bonded, slashed, _count, _list_len) =
        last_bond_drift_event(&e).expect("bond_drift_detected event must be emitted before panic");
    assert_eq!(kind, BondDriftKind::SlashedExceedsBonded);
    assert_eq!(bonded, 1_000);
    assert_eq!(slashed, 1_100);
    assert_eq!(
        ContractError::InvariantViolation as u32,
        218,
        "InvariantViolation must live in bond error block 200-299"
    );
}

#[test]
#[should_panic(expected = "HostError")]
fn bond_drift_attestation_count_mismatch_panics() {
    let e = Env::default();
    let (contract_id, client) = setup_contract(&e);
    let bond = client.get_identity_state();
    inject_attestation_count_drift(&e, &contract_id, &bond.identity);
    e.as_contract(&contract_id, || {
        assert_self_consistent(&e);
    });
}

#[test]
fn bond_drift_attestation_count_mismatch_emits_event() {
    extern crate std;

    let e = Env::default();
    let (contract_id, client) = setup_contract(&e);
    let bond = client.get_identity_state();
    inject_attestation_count_drift(&e, &contract_id, &bond.identity);

    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        e.as_contract(&contract_id, || {
            assert_self_consistent(&e);
        });
    }));
    assert!(panic_result.is_err());

    let (kind, _, _, count, list_len) =
        last_bond_drift_event(&e).expect("bond_drift_detected event not found");
    assert_eq!(kind, BondDriftKind::AttestationCountMismatch);
    assert_eq!(count, 99);
    assert_eq!(list_len, 0);
}
