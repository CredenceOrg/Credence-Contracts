//! Comprehensive tests for the Credence Treasury contract.
//! Covers: initialization, fees, depositors, multi-sig (signers, threshold,
//! propose/approve/execute), fund source tracking, events, and security.

#![cfg(test)]

use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceTreasuryClient<'_>, Address) {
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);
    e.mock_all_auths();
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_initialize() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    assert_eq!(client.get_admin(), _admin);
    assert_eq!(client.get_balance(), 0);
    assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 0);
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 0);
    assert_eq!(client.get_threshold(), 0);
}

#[test]
fn test_receive_fee_as_admin() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 1000);
    assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 1000);
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 0);
    client.receive_fee(&admin, &500, &FundSource::SlashedFunds);
    assert_eq!(client.get_balance(), 1500);
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 500);
}

#[test]
fn test_receive_fee_as_depositor() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let depositor = Address::generate(&e);
    client.add_depositor(&depositor);
    client.receive_fee(&depositor, &2000, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 2000);
    assert!(client.is_depositor(&depositor));
    client.remove_depositor(&depositor);
    assert!(!client.is_depositor(&depositor));
}

#[test]
#[should_panic(expected = "only admin or authorized depositor can receive_fee")]
fn test_receive_fee_unauthorized() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let other = Address::generate(&e);
    client.receive_fee(&other, &100, &FundSource::ProtocolFee);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_receive_fee_zero_amount() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &0, &FundSource::ProtocolFee);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_receive_fee_negative_amount() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &-100, &FundSource::ProtocolFee);
}

#[test]
fn test_add_remove_signer_and_threshold() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    assert!(client.is_signer(&s1));
    assert!(client.is_signer(&s2));
    client.set_threshold(&2);
    assert_eq!(client.get_threshold(), 2);
    client.remove_signer(&s1);
    assert!(!client.is_signer(&s1));
    assert_eq!(client.get_threshold(), 1);
}

#[test]
#[should_panic(expected = "threshold cannot exceed signer count")]
fn test_set_threshold_exceeds_signers() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let s1 = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&3);
}

#[test]
fn test_propose_approve_execute_withdrawal() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &10_000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&2);
    let id = client.propose_withdrawal(&s1, &recipient, &3000);
    let prop = client.get_proposal(&id);
    assert_eq!(prop.recipient, recipient);
    assert_eq!(prop.amount, 3000);
    assert!(!prop.executed);
    assert_eq!(client.get_approval_count(&id), 0);
    client.approve_withdrawal(&s1, &id);
    assert!(client.has_approved(&id, &s1));
    assert_eq!(client.get_approval_count(&id), 1);
    client.approve_withdrawal(&s2, &id);
    assert_eq!(client.get_approval_count(&id), 2);
    client.execute_withdrawal(&id);
    assert_eq!(client.get_balance(), 7000);
    let prop2 = client.get_proposal(&id);
    assert!(prop2.executed);
}

#[test]
#[should_panic(expected = "only signer can propose withdrawal")]
fn test_propose_withdrawal_non_signer() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let other = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.propose_withdrawal(&other, &recipient, &500);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_propose_withdrawal_zero_amount() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    client.propose_withdrawal(&s1, &recipient, &0);
}

#[test]
#[should_panic(expected = "insufficient treasury balance")]
fn test_propose_withdrawal_exceeds_balance() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &100, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    client.propose_withdrawal(&s1, &recipient, &200);
}

#[test]
#[should_panic(expected = "only signer can approve")]
fn test_approve_withdrawal_non_signer() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let other = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&other, &id);
}

#[test]
fn test_double_approve_is_noop() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&s1, &id);
    client.approve_withdrawal(&s1, &id);
    assert_eq!(client.get_approval_count(&id), 1);
    client.execute_withdrawal(&id);
}

#[test]
#[should_panic(expected = "insufficient approvals to execute")]
fn test_execute_without_threshold() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&2);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id);
}

#[test]
#[should_panic(expected = "proposal already executed")]
fn test_execute_twice_fails() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id);
    client.execute_withdrawal(&id);
}

#[test]
#[should_panic(expected = "proposal not found")]
fn test_get_proposal_invalid_id() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let _ = client.get_proposal(&999);
}

#[test]
#[should_panic(expected = "proposal already executed")]
fn test_approve_after_execute_fails() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &1000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let recipient = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&1);
    let id = client.propose_withdrawal(&s1, &recipient, &100);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id);
    client.approve_withdrawal(&s2, &id);
}

#[test]
fn test_fund_source_tracking() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &100, &FundSource::ProtocolFee);
    client.receive_fee(&admin, &200, &FundSource::SlashedFunds);
    client.receive_fee(&admin, &50, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 350);
    assert_eq!(client.get_balance_by_source(&FundSource::ProtocolFee), 150);
    assert_eq!(client.get_balance_by_source(&FundSource::SlashedFunds), 200);
}

#[test]
fn test_multiple_proposals() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    client.receive_fee(&admin, &5000, &FundSource::ProtocolFee);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&2);
    let id1 = client.propose_withdrawal(&s1, &r1, &1000);
    let id2 = client.propose_withdrawal(&s2, &r2, &2000);
    assert_ne!(id1, id2);
    client.approve_withdrawal(&s1, &id1);
    client.approve_withdrawal(&s2, &id1);
    client.execute_withdrawal(&id1);
    assert_eq!(client.get_balance(), 4000);
    client.approve_withdrawal(&s1, &id2);
    client.approve_withdrawal(&s2, &id2);
    client.execute_withdrawal(&id2);
    assert_eq!(client.get_balance(), 2000);
}

#[test]
fn test_remove_signer_caps_threshold() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let s1 = Address::generate(&e);
    let s2 = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&2);
    client.remove_signer(&s2);
    assert_eq!(client.get_threshold(), 1);
}

#[test]
fn test_add_signer_idempotent() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let s1 = Address::generate(&e);
    client.add_signer(&s1);
    client.add_signer(&s1);
    assert!(client.is_signer(&s1));
}

#[test]
#[should_panic(expected = "not initialized")]
fn test_get_admin_uninitialized() {
    let e = Env::default();
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(&e, &contract_id);
    let _ = client.get_admin();
}

#[test]
fn test_get_approval_count_nonexistent_proposal() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    assert_eq!(client.get_approval_count(&99), 0);
}
