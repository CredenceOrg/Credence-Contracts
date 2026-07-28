//! Treasury withdrawal guardrails for pause and recovery paths (issue #1048).
//!
//! Authorization and boundary tests for withdrawal flows during paused,
//! recovering, and resumed states. Verifies:
//! - Withdrawals are blocked during paused state
//! - Authorization is enforced during pause/recovery transitions
//! - Balance and guardrail invariants hold after unpause recovery

#![cfg(test)]

use crate::{CredenceTreasury, CredenceTreasuryClient, FundSource};
use soroban_sdk::testutils::{Address as _, Ledger as _};
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

// ── Authorization during paused state ──────────────────────────────────────

#[test]
fn test_admin_can_still_manage_pause_while_paused() {
    let e = Env::default();
    let (client, _s1, _s2, _recipient, admin) = setup_funded_with_signers(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    // Admin can still unpause
    client.unpause(&admin);
    assert!(!client.is_paused());

    // Admin can re-pause
    client.pause(&admin);
    assert!(client.is_paused());
}

#[test]
fn test_non_admin_cannot_unpause() {
    let e = Env::default();
    let (client, s1, _s2, _recipient, admin) = setup_funded_with_signers(&e);

    client.pause(&admin);
    assert!(client.is_paused());

    // Non-admin signer cannot unpause directly (threshold = 1, but pause management is admin-only when threshold=0)
    let result = client.try_unpause(&s1);
    assert!(result.is_err(), "non-admin cannot unpause");
}

#[test]
fn test_withdrawal_guardrails_preserved_across_pause_unpause_cycle() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    // Set a min liquidity floor
    client.set_min_liquidity(&admin, &5_000);

    // Pause
    client.pause(&admin);

    // Attempt withdrawal while paused
    let r = client.try_propose_withdrawal(&s1, &recipient, &1000);
    assert!(r.is_err(), "propose must fail while paused");

    // Unpause
    client.unpause(&admin);

    // Now withdrawal should work (as long as floor is respected)
    let id = client.propose_withdrawal(&s1, &recipient, &4_000);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);

    // remaining = 10_000 - 4_000 = 6_000 >= 5_000 floor
    assert_eq!(client.get_balance(), 6_000);
}

#[test]
fn test_floor_guardrail_still_enforced_after_unpause() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    client.set_min_liquidity(&admin, &9_000);

    // Pause and unpause
    client.pause(&admin);
    client.unpause(&admin);

    // Withdrawal that would breach the floor (10_000 - 2_000 = 8_000 < 9_000 floor)
    let id = client.propose_withdrawal(&s1, &recipient, &2_000);
    client.approve_withdrawal(&s1, &id);
    let result = client.try_execute_withdrawal(&id, &0);
    assert!(result.is_err(), "floor guardrail must still be enforced after unpause");
}

// ── Balance invariants across pause/recovery ───────────────────────────────

#[test]
fn test_balance_unchanged_by_pause_cycle() {
    let e = Env::default();
    let (client, _s1, _s2, _recipient, admin) = setup_funded_with_signers(&e);

    let balance_before = client.get_balance();

    client.pause(&admin);
    assert_eq!(client.get_balance(), balance_before);

    client.unpause(&admin);
    assert_eq!(client.get_balance(), balance_before);
}

#[test]
fn test_multiple_pause_unpause_cycles_preserve_state() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    // Do a withdrawal first
    let id = client.propose_withdrawal(&s1, &recipient, &1_000);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 9_000);

    // Multiple pause/unpause cycles
    for _ in 0..3 {
        client.pause(&admin);
        assert!(client.is_paused());
        assert_eq!(client.get_balance(), 9_000);
        client.unpause(&admin);
        assert!(!client.is_paused());
        assert_eq!(client.get_balance(), 9_000);
    }

    // State is still intact - can still withdraw
    let id2 = client.propose_withdrawal(&s1, &recipient, &1_000);
    client.approve_withdrawal(&s1, &id2);
    client.execute_withdrawal(&id2, &0);
    assert_eq!(client.get_balance(), 8_000);
}

// ── Slippage guardrail preservation across pause ────────────────────────────

#[test]
fn test_slippage_guardrail_preserved_across_pause() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    // Propose before pause
    let id = client.propose_withdrawal(&s1, &recipient, &3_000);
    client.approve_withdrawal(&s1, &id);

    // Pause and unpause
    client.pause(&admin);
    client.unpause(&admin);

    // Slippage guard must still work
    let result = client.try_execute_withdrawal(&id, &5_000);
    assert!(result.is_err(), "slippage guard must be preserved across pause");
}

// ── Rapid pause/unpause boundary ────────────────────────────────────────────

#[test]
fn test_rapid_pause_unpause_no_state_corruption() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    // Rapid toggle
    client.pause(&admin);
    client.unpause(&admin);
    client.pause(&admin);
    client.unpause(&admin);

    // Verify state is clean
    let id = client.propose_withdrawal(&s1, &recipient, &5_000);
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 5_000);
}

// ── Recovering state: withdrawal right after unpause ───────────────────────

#[test]
fn test_withdrawal_immediately_after_unpause_succeeds() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    let id = client.propose_withdrawal(&s1, &recipient, &2_000);
    client.approve_withdrawal(&s1, &id);

    client.pause(&admin);

    // Execute should be blocked while paused
    let r = client.try_execute_withdrawal(&id, &0);
    assert!(r.is_err());

    client.unpause(&admin);

    // Execute immediately after unpause must succeed (no cooldown)
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 8_000);
}

#[test]
fn test_withdrawal_proposal_state_never_corrupted_by_pause() {
    let e = Env::default();
    let (client, s1, _s2, recipient, admin) = setup_funded_with_signers(&e);

    // Create a proposal
    let id = client.propose_withdrawal(&s1, &recipient, &4_000);
    let proposal_before = client.get_proposal(&id);

    // Pause and unpause
    client.pause(&admin);
    client.unpause(&admin);

    // Proposal must be identical
    let proposal_after = client.get_proposal(&id);
    assert_eq!(proposal_before.recipient, proposal_after.recipient);
    assert_eq!(proposal_before.amount, proposal_after.amount);
    assert_eq!(proposal_before.executed, proposal_after.executed);

    // Can still approve and execute
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 6_000);
}

// ── Multisig pause + recovery path ──────────────────────────────────────────

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

#[test]
fn test_multisig_pause_recovery_full_withdrawal_lifecycle() {
    let e = Env::default();
    let (client, s1, s2, recipient, _admin) = setup_multisig_pause(&e);

    // Phase 1: Normal operation - propose
    let id = client.propose_withdrawal(&s1, &recipient, &3_000);

    // Phase 2: Multisig pause
    let pause_id = client.pause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &pause_id);
    client.execute_pause_proposal(&pause_id);
    assert!(client.is_paused());

    // Phase 3: Withdrawal blocked while paused
    let r = client.try_approve_withdrawal(&s1, &id);
    assert!(r.is_err());

    // Phase 4: Multisig unpause (recovery)
    let unpause_id = client.unpause(&s1).unwrap();
    client.approve_pause_proposal(&s2, &unpause_id);
    client.execute_pause_proposal(&unpause_id);
    assert!(!client.is_paused());

    // Phase 5: Resume withdrawal
    client.approve_withdrawal(&s1, &id);
    client.execute_withdrawal(&id, &0);
    assert_eq!(client.get_balance(), 7_000);
}

#[test]
fn test_deposits_allowed_during_paused_state() {
    let e = Env::default();
    let (client, _s1, _s2, _recipient, admin) = setup_funded_with_signers(&e);

    client.pause(&admin);

    // Fee deposits should still work during pause
    client.receive_fee(&admin, &5_000, &FundSource::ProtocolFee);
    assert_eq!(client.get_balance(), 15_000);

    client.unpause(&admin);
    assert_eq!(client.get_balance(), 15_000);
}
