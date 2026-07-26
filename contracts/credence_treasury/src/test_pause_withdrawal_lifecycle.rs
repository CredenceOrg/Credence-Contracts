//! Pause-blocks-withdrawal lifecycle integration tests.
//!
//! Asserts that pausing halts each stage of propose/approve/execute and
//! unpause resumes cleanly with no partial state mutation.

use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceTreasuryClient<'_>, Address, Address) {
    let contract_id = e.register(CredenceTreasury, ());
    let client = CredenceTreasuryClient::new(e, &contract_id);
    let admin = Address::generate(e);

    let token_admin = Address::generate(e);
    let token_id = e.register_stellar_asset_contract(token_admin.clone());

    e.mock_all_auths();
    client.initialize(&admin, &token_id);

    let stellar_client = soroban_sdk::token::StellarAssetClient::new(e, &token_id);
    stellar_client.mint(&admin, &(i128::MAX / 2));

    (client, admin, token_id)
}

/// Fully fund the treasury and set up two signers, threshold=1.
fn setup_funded_with_signers(
    e: &Env,
) -> (
    CredenceTreasuryClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    let (client, admin, _token) = setup(e);

    client.receive_fee(&admin, &10_000, &FundSource::ProtocolFee);

    let s1 = Address::generate(e);
    let s2 = Address::generate(e);
    let recipient = Address::generate(e);

    client.add_signer(&s1);
    client.add_signer(&s2);
    client.set_threshold(&1);

    (client, s1, s2, recipient, admin)
}

// ─── Pause before propose ─────────────────────────────────────────────────

#[test]
fn test_pause_before_propose_blocks_propose() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_propose_withdrawal(&s1, &recipient, &1000);
    assert!(result.is_err());

    client.unpause(&admin);
    assert!(!client.is_paused());

    let id = client.propose_withdrawal(&s1, &recipient, &1000);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 9000);
}

// ─── Pause between propose and approve ─────────────────────────────────────

#[test]
fn test_pause_between_propose_and_approve_blocks_approve() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    let id = client.propose_withdrawal(&s1, &recipient, &3000);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_approve_withdrawal(&s1, &id);
    assert!(result.is_err());

    assert_eq!(client.get_approval_count(&id), 0);
    assert_eq!(client.get_balance(), 10_000);

    client.unpause(&admin);

    client.approve_withdrawal(&s1, &id);
    assert_eq!(client.get_approval_count(&id), 1);

    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 7000);
}

// ─── Pause between approve and execute ──────────────────────────────────────

#[test]
fn test_pause_between_approve_and_execute_blocks_execute() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    let id = client.propose_withdrawal(&s1, &recipient, &2000);
    client.approve_withdrawal(&s1, &id);
    assert_eq!(client.get_approval_count(&id), 1);

    assert!(!client.get_proposal(&id).executed);

    client.pause(&admin);
    assert!(client.is_paused());

    let result = client.try_execute_withdrawal(&id, &0);
    assert!(result.is_err());

    assert!(!client.get_proposal(&id).executed);
    assert_eq!(client.get_balance(), 10_000);

    client.unpause(&admin);
    assert!(!client.is_paused());

    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 8000);
    assert!(client.get_proposal(&id).executed);
}

// ─── No partial state mutation on reverted calls ────────────────────────────

#[test]
fn test_no_partial_state_mutation_on_paused_propose() {
    let e = Env::default();
    let (client, s1, _s2, _recipient, admin) = setup_funded_with_signers(&e);

    let balance_before = client.get_balance();

    client.pause(&admin);

    let r = client.try_propose_withdrawal(&s1, &Address::generate(&e), &500);
    assert!(r.is_err());

    assert_eq!(client.get_balance(), balance_before);
}

#[test]
fn test_no_partial_state_mutation_on_paused_approve() {
    let e = Env::default();
    let (client, s1, s2, recipient, admin) = setup_funded_with_signers(&e);

    let id = client.propose_withdrawal(&s1, &recipient, &4000);
    assert_eq!(client.get_approval_count(&id), 0);

    client.approve_withdrawal(&s1, &id);
    assert_eq!(client.get_approval_count(&id), 1);

    client.pause(&admin);

    let r = client.try_approve_withdrawal(&s2, &id);
    assert!(r.is_err());

    assert_eq!(client.get_approval_count(&id), 1);
    assert_eq!(client.get_balance(), 10_000);

    client.unpause(&admin);

    client.approve_withdrawal(&s2, &id);
    assert_eq!(client.get_approval_count(&id), 2);
}

#[test]
fn test_no_partial_state_mutation_on_paused_execute() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    let id = client.propose_withdrawal(&s1, &recipient, &5000);
    client.approve_withdrawal(&s1, &id);

    let balance_before = client.get_balance();
    let count_before = client.get_approval_count(&id);

    client.pause(&admin);

    let r = client.try_execute_withdrawal(&id, &0);
    assert!(r.is_err());

    assert!(!client.get_proposal(&id).executed);
    assert_eq!(client.get_balance(), balance_before);
    assert_eq!(client.get_approval_count(&id), count_before);

    client.unpause(&admin);

    client.execute_withdrawal(&id, &0);
    assert!(client.get_proposal(&id).executed);
    assert_eq!(client.get_balance(), 5000);
}

// ─── Execute attempt while paused at threshold ──────────────────────────────

#[test]
fn test_execute_paused_at_threshold_reverts() {
    let e = Env::default();
    let (client, s1, s2, recipient, admin) = setup_funded_with_signers(&e);

    client.set_threshold(&2);

    let id = client.propose_withdrawal(&s1, &recipient, &1000);
    client.approve_withdrawal(&s1, &id);
    client.approve_withdrawal(&s2, &id);
    assert_eq!(client.get_approval_count(&id), 2);

    client.pause(&admin);
    assert!(client.is_paused());

    let r = client.try_execute_withdrawal(&id, &0);
    assert!(r.is_err());

    assert!(!client.get_proposal(&id).executed);

    client.unpause(&admin);

    client.execute_withdrawal(&id, &0);
    assert!(client.get_proposal(&id).executed);
    assert_eq!(client.get_balance(), 9000);
}

// ─── Double-pause: pausing while already paused ────────────────────────────

#[test]
fn test_double_pause_stays_paused() {
    let e = Env::default();
    let (client, _s1, _s2, _recipient, admin) = setup_funded_with_signers(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    client.pause(&admin);
    assert!(client.is_paused());

    client.unpause(&admin);
    assert!(!client.is_paused());
}

// ─── Unpause restores exact pre-pause state ─────────────────────────────────

#[test]
fn test_unpause_restores_exact_pre_pause_state() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    let balance_before = client.get_balance();

    client.pause(&admin);

    let r = client.try_propose_withdrawal(&s1, &recipient, &500);
    assert!(r.is_err());

    client.unpause(&admin);

    assert_eq!(client.get_balance(), balance_before);
    assert!(!client.is_paused());
}

// ─── Pause via multisig (pause-signer flow) then try lifecycle steps ────────

fn setup_multisig_pause(
    e: &Env,
) -> (
    CredenceTreasuryClient<'_>,
    Address,
    Address,
    Address,
    Address,
) {
    let (client, s1, s2, recipient, admin) = setup_funded_with_signers(e);

    client.set_pause_signer(&admin, &s1, &true);
    client.set_pause_signer(&admin, &s2, &true);
    client.set_pause_threshold(&admin, &2u32);

    (client, s1, s2, recipient, admin)
}

fn pause_via_multisig(client: &CredenceTreasuryClient<'_>, s1: &Address, s2: &Address) {
    let pid = client.pause(s1).unwrap();
    assert!(!client.is_paused());
    client.approve_pause_proposal(s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(client.is_paused());
}

fn unpause_via_multisig(client: &CredenceTreasuryClient<'_>, s1: &Address, s2: &Address) {
    let pid = client.unpause(s1).unwrap();
    client.approve_pause_proposal(s2, &pid);
    client.execute_pause_proposal(&pid);
    assert!(!client.is_paused());
}

#[test]
fn test_pause_multisig_between_propose_and_approve_blocks_approve() {
    let e = Env::default();
    let (client, s1, s2, recipient, _admin) = setup_multisig_pause(&e);

    let id = client.propose_withdrawal(&s1, &recipient, &3000);

    pause_via_multisig(&client, &s1, &s2);

    let r = client.try_approve_withdrawal(&s1, &id);
    assert!(r.is_err());
    assert_eq!(client.get_approval_count(&id), 0);

    unpause_via_multisig(&client, &s1, &s2);

    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 7000);
}

#[test]
fn test_pause_multisig_between_approve_and_execute_blocks_execute() {
    let e = Env::default();
    let (client, s1, s2, recipient, _admin) = setup_multisig_pause(&e);

    let id = client.propose_withdrawal(&s1, &recipient, &2000);
    client.approve_withdrawal(&s1, &id);

    pause_via_multisig(&client, &s1, &s2);

    let r = client.try_execute_withdrawal(&id, &0);
    assert!(r.is_err());
    assert!(!client.get_proposal(&id).executed);
    assert_eq!(client.get_balance(), 10_000);

    unpause_via_multisig(&client, &s1, &s2);

    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 8000);
}

#[test]
fn test_pause_multisig_before_propose_blocks_propose() {
    let e = Env::default();
    let (client, s1, s2, recipient, _admin) = setup_multisig_pause(&e);

    pause_via_multisig(&client, &s1, &s2);

    let r = client.try_propose_withdrawal(&s1, &recipient, &1000);
    assert!(r.is_err());

    unpause_via_multisig(&client, &s1, &s2);

    let id = client.propose_withdrawal(&s1, &recipient, &1000);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 9000);
}
