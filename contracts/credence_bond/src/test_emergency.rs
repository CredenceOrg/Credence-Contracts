//! Comprehensive tests for emergency withdrawal system.
//! Covers governance approvals, emergency mode gating, fee application,
//! immutable audit trail, and crisis-only behavior.

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    let (client, admin, identity, ..) = test_helpers::setup_with_token(e);
    let governance = Address::generate(e);
    let treasury = Address::generate(e);
    (client, admin, governance, treasury, identity)
}

#[test]
fn test_emergency_withdraw_success_records_audit_trail() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 10_000);
    let (client, admin, governance, treasury, identity) = setup(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &500, &true);
    client.create_bond(&identity, &1000_i128, &86_400_u64, &false, &0_u64);

    let reason = Symbol::new(&e, "crisis");
    let bond = client.emergency_withdraw(&admin, &governance, &200_i128, &reason);
    assert_eq!(bond.bonded_amount, 800);

    let latest_id = client.get_latest_emergency_record_id();
    assert_eq!(latest_id, 1);

    let record = client.get_emergency_record(&latest_id);
    assert_eq!(record.id, 1);
    assert_eq!(record.identity, identity);
    assert_eq!(record.gross_amount, 200);
    assert_eq!(record.fee_amount, 10); // 5% of 200
    assert_eq!(record.net_amount, 190);
    assert_eq!(record.treasury, treasury);
    assert_eq!(record.approved_admin, admin);
    assert_eq!(record.approved_governance, governance);
    assert_eq!(record.reason, reason);
    assert_eq!(record.timestamp, 10_000);
}

#[test]
fn test_emergency_withdraw_multiple_records_increment_ids() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 100);
    let (client, admin, governance, treasury, identity) = setup(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &100, &true);
    client.create_bond(&identity, &1000_i128, &86_400_u64, &false, &0_u64);

    client.emergency_withdraw(&admin, &governance, &100_i128, &Symbol::new(&e, "ops1"));
    e.ledger().with_mut(|li| li.timestamp = 101);
    client.emergency_withdraw(&admin, &governance, &100_i128, &Symbol::new(&e, "ops2"));

    let first = client.get_emergency_record(&1_u64);
    let second = client.get_emergency_record(&2_u64);

    assert_eq!(first.id, 1);
    assert_eq!(second.id, 2);
    assert_eq!(client.get_latest_emergency_record_id(), 2);
}

#[test]
fn test_set_emergency_mode_requires_elevated_approval_and_updates_state() {
    let e = Env::default();
    let (client, admin, governance, treasury, _identity) = setup(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &250, &false);
    let cfg = client.get_emergency_config();
    assert!(!cfg.enabled);

    client.set_emergency_mode(&admin, &governance, &true);
    let cfg = client.get_emergency_config();
    assert!(cfg.enabled);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_emergency_config_rejects_non_admin() {
    let e = Env::default();
    let (client, admin, governance, treasury, _identity) = setup(&e);
    let other = Address::generate(&e);

    client.set_emergency_config(&other, &governance, &treasury, &250, &true);
    let _ = admin;
}

#[test]
#[should_panic(expected = "not governance")]
fn test_set_emergency_mode_rejects_wrong_governance() {
    let e = Env::default();
    let (client, admin, governance, treasury, _identity) = setup(&e);
    let wrong_governance = Address::generate(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &250, &false);
    client.set_emergency_mode(&admin, &wrong_governance, &true);
}

#[test]
#[should_panic(expected = "emergency mode disabled")]
fn test_emergency_withdraw_rejected_when_disabled() {
    let e = Env::default();
    let (client, admin, governance, treasury, identity) = setup(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &500, &false);
    client.create_bond(&identity, &1000_i128, &86_400_u64, &false, &0_u64);

    client.emergency_withdraw(&admin, &governance, &100_i128, &Symbol::new(&e, "crisis"));
}

#[test]
#[should_panic(expected = "not governance")]
fn test_emergency_withdraw_requires_governance_approver() {
    let e = Env::default();
    let (client, admin, governance, treasury, identity) = setup(&e);
    let wrong_governance = Address::generate(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &500, &true);
    client.create_bond(&identity, &1000_i128, &86_400_u64, &false, &0_u64);

    client.emergency_withdraw(
        &admin,
        &wrong_governance,
        &100_i128,
        &Symbol::new(&e, "crisis"),
    );
}

#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn test_emergency_withdraw_respects_slashed_available_balance() {
    let e = Env::default();
    let (client, admin, governance, treasury, identity) = setup(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &500, &true);
    client.create_bond(&identity, &1000_i128, &86_400_u64, &false, &0_u64);
    client.slash(&admin, &900_i128);

    client.emergency_withdraw(&admin, &governance, &101_i128, &Symbol::new(&e, "crisis"));
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_emergency_withdraw_rejects_non_positive_amount() {
    let e = Env::default();
    let (client, admin, governance, treasury, identity) = setup(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &500, &true);
    client.create_bond(&identity, &1000_i128, &86_400_u64, &false, &0_u64);
    client.emergency_withdraw(&admin, &governance, &0_i128, &Symbol::new(&e, "crisis"));
}

#[test]
#[should_panic(expected = "emergency fee bps must be <= 10000")]
fn test_set_emergency_config_rejects_invalid_fee_bps() {
    let e = Env::default();
    let (client, admin, governance, treasury, _identity) = setup(&e);

    client.set_emergency_config(&admin, &governance, &treasury, &10_001_u32, &true);
}
