//! Tests asserting that the bond lifecycle entrypoints move USDC tokens on-
//! chain via the configured token contract, and that the custody invariant
//! `token.balance(bond_contract) == bonded_amount` (with treasury sweeps
//! tracked separately via `set_slash_treasury`) holds at every step.
//!
//! These tests are the contract-level proof that bond storage is the source
//! of truth for staked amounts — no phantom balances.

#![cfg(test)]

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::TokenClient;
use soroban_sdk::Address;
use soroban_sdk::Env;

const DAY: u64 = credence_math::Timestamp::SECONDS_PER_DAY;

/// Token moves from `identity` to the bond contract on `create_bond`.
#[test]
fn test_create_bond_pulls_usdc_from_identity() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, token_id, bond_contract) = test_helpers::setup_with_token(&e);

    let token = TokenClient::new(&e, &token_id);
    let identity_before = token.balance(&identity);
    let contract_before = token.balance(&bond_contract);
    assert_eq!(contract_before, 0, "freshly deployed contract holds zero");

    let amount: i128 = 5_000;
    client.create_bond(&identity, &amount, &DAY, &false, &0_u64);

    assert_eq!(
        token.balance(&identity),
        identity_before - amount,
        "create_bond must debit identity for the full bonded amount"
    );
    assert_eq!(
        token.balance(&bond_contract),
        contract_before + amount,
        "create_bond must credit the bond contract for the full bonded amount"
    );
}

/// Token moves from `identity` to the bond contract on `top_up`.
#[test]
fn test_top_up_pulls_usdc_from_identity() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, token_id, bond_contract) = test_helpers::setup_with_token(&e);

    client.create_bond(&identity, &1_000, &DAY, &false, &0_u64);

    let token = TokenClient::new(&e, &token_id);
    let initial_contract = token.balance(&bond_contract);
    let initial_identity = token.balance(&identity);

    let extra: i128 = 500;
    client.top_up(&identity, &extra);

    assert_eq!(
        token.balance(&bond_contract),
        initial_contract + extra,
        "top_up must credit the bond contract for the top-up amount"
    );
    assert_eq!(
        token.balance(&identity),
        initial_identity - extra,
        "top_up must debit the identity for the top-up amount"
    );

    // Storage mirrors token movement: bonded_amount increased by the same delta.
    let bond_after = client.get_identity_state(&identity);
    assert_eq!(bond_after.bonded_amount, 1_500);
}

/// Token moves from the bond contract to `identity` on `withdraw`.
#[test]
fn test_withdraw_pushes_usdc_to_identity() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, token_id, bond_contract) = test_helpers::setup_with_token(&e);

    client.create_bond(&identity, &2_000, &DAY, &false, &0_u64);

    // Advance time past lock-up.
    e.ledger().with_mut(|li| li.timestamp = 1_000 + DAY + 1);

    let token = TokenClient::new(&e, &token_id);
    let before_identity = token.balance(&identity);
    let before_contract = token.balance(&bond_contract);
    assert_eq!(before_contract, 2_000);

    let amount: i128 = 700;
    client.withdraw(&identity, &amount);

    assert_eq!(
        token.balance(&identity),
        before_identity + amount,
        "withdraw must credit the identity for the withdrawn amount"
    );
    assert_eq!(
        token.balance(&bond_contract),
        before_contract - amount,
        "withdraw must debit the bond contract for the withdrawn amount"
    );

    // Storage mirrors token movement: bonded_amount decreased by the same delta.
    let bond_after = client.get_identity_state(&identity);
    assert_eq!(bond_after.bonded_amount, 2_000 - amount);
}

/// `withdraw_early` splits the gross withdrawal: penalty to treasury, net
/// to identity. Token movements match the on-paper accounting exactly.
#[test]
fn test_withdraw_early_splits_usdc_between_treasury_and_identity() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin, identity, token_id, bond_contract) = test_helpers::setup_with_token(&e);

    let treasury = Address::generate(&e);
    // 10% penalty — easy to compute by hand.
    client.set_early_exit_config(&admin, &treasury, &1_000_u32);
    client.create_bond(&identity, &10_000, &DAY, &false, &0_u64);

    // Withdraw at the very start of the bond — full penalty applies.
    e.ledger().with_mut(|li| li.timestamp = 1_001);

    let token = TokenClient::new(&e, &token_id);
    let gross: i128 = 1_000;
    let penalty: i128 = gross * 1_000 / 10_000; // 100
    let payout: i128 = gross - penalty; // 900

    let before_identity = token.balance(&identity);
    let before_treasury = token.balance(&treasury);
    let before_contract = token.balance(&bond_contract);

    client.withdraw_early(&identity, &gross);

    assert_eq!(
        token.balance(&bond_contract),
        before_contract - gross,
        "withdraw_early must move the gross amount out of the bond contract"
    );
    assert_eq!(
        token.balance(&identity),
        before_identity + payout,
        "withdraw_early must credit the identity with net_amount = amount - penalty"
    );
    assert_eq!(
        token.balance(&treasury),
        before_treasury + penalty,
        "withdraw_early must credit the early-exit treasury with the penalty"
    );

    // Storage mirrors token movement: bonded_amount decreased by the gross.
    let bond_after = client.get_identity_state(&identity);
    assert_eq!(bond_after.bonded_amount, 10_000 - gross);
}

/// `withdraw_early` is well-defined when treasury == identity: both the
/// penalty leg and the net-amount leg route tokens to the same address.
/// The identity therefore ends up holding both transfers, and its balance
/// changes by the gross withdrawal amount. The contract invariant still
/// holds (`penalty + net_amount == amount` and `bond_contract -= gross`).
#[test]
fn test_withdraw_early_treasury_equals_caller_identity_gets_gross() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin, identity, token_id, bond_contract) = test_helpers::setup_with_token(&e);

    // Treasury == identity — the penalty leg and the net-amount leg both
    // route tokens back to the bond owner, so identity's net balance change
    // is `+gross` rather than `+net_amount`.
    client.set_early_exit_config(&admin, &identity, &1_000_u32);
    client.create_bond(&identity, &10_000, &DAY, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 1_001);

    let token = TokenClient::new(&e, &token_id);
    let gross: i128 = 1_000;

    let before_identity = token.balance(&identity);
    let before_contract = token.balance(&bond_contract);

    client.withdraw_early(&identity, &gross);

    // Both transfers land on the same address, so the identity sees +gross.
    assert_eq!(
        token.balance(&identity),
        before_identity + gross,
        "treasury == identity: penalty leg + net leg both credit the identity, net = +gross"
    );
    // The bond contract loses the gross regardless of where the penalty goes.
    assert_eq!(
        token.balance(&bond_contract),
        before_contract - gross,
        "bond contract must lose the gross amount"
    );

    let bond_after = client.get_identity_state(&identity);
    assert_eq!(bond_after.bonded_amount, 10_000 - gross);
}

/// Custody invariant (without slash): at every step of the create → top_up →
/// withdraw flow, the bond contract's token balance equals `bonded_amount`.
///
/// Slashing is tracked separately via `set_slash_treasury`; it adjusts
/// `slashed_amount` in storage but does not move tokens until the slash
/// treasury sweeps funds.
#[test]
fn test_full_lifecycle_token_balance_matches_storage() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, token_id, bond_contract) = test_helpers::setup_with_token(&e);

    let token = TokenClient::new(&e, &token_id);

    // Step 1: create_bond(5_000) — pull 5_000 from identity into contract.
    client.create_bond(&identity, &5_000, &DAY, &false, &0_u64);
    assert_eq!(token.balance(&bond_contract), 5_000);
    assert_eq!(client.get_identity_state(&identity).bonded_amount, 5_000);

    // Step 2: top_up(2_000) — pull 2_000 more from identity into contract.
    client.top_up(&identity, &2_000);
    assert_eq!(token.balance(&bond_contract), 7_000);
    assert_eq!(client.get_identity_state(&identity).bonded_amount, 7_000);

    // Step 3: withdraw(1_000) — push 1_000 from contract to identity.
    e.ledger().with_mut(|li| li.timestamp = 1_000 + DAY + 1);
    client.withdraw(&identity, &1_000);
    assert_eq!(token.balance(&bond_contract), 6_000);
    assert_eq!(client.get_identity_state(&identity).bonded_amount, 6_000);

    // Custody invariant: the bond contract holds exactly `bonded_amount`.
    assert_eq!(
        token.balance(&bond_contract),
        client.get_identity_state(&identity).bonded_amount,
        "token.balance(bond_contract) must equal bonded_amount (no slash)"
    );
}

/// Slash accounting parity: after `create_bond → top_up → slash`, the bond
/// contract still holds the full principal (the canonical `slash`
/// entrypoint only marks state and does not move tokens on its own). The
/// actual token routing is performed by `slash_bond` (a separate
/// entrypoint at lib.rs:2097) and is verified by the dedicated slashing
/// tests under `test_slashing.rs`. Here we assert the invariants that
/// apply on the canonical `slash` path: storage reads
/// `bonded_amount = principal` and `slashed_amount = principal - bonded_after_slash`
/// while the token balance remains untouched.
#[test]
fn test_slash_only_marks_state_does_not_move_tokens() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, admin, identity, token_id, bond_contract) = test_helpers::setup_with_token(&e);

    let slash_treasury = Address::generate(&e);
    client.set_slash_treasury(&admin, &slash_treasury);

    let token = TokenClient::new(&e, &token_id);

    // Step 1: create_bond(5_000).
    client.create_bond(&identity, &5_000, &DAY, &false, &0_u64);
    // Step 2: top_up(2_000).
    client.top_up(&identity, &2_000);
    assert_eq!(token.balance(&bond_contract), 7_000);

    // Move to a subsequent ledger so the same-ledger slash guard allows it.
    test_helpers::advance_ledger_sequence(&e);

    // Step 3: canonical `slash` only marks `slashed_amount` in storage;
    // tokens are NOT moved out of the bond contract at this step.
    client.slash(&admin, &2_000);
    let after_slash = client.get_identity_state(&identity);
    assert_eq!(after_slash.bonded_amount, 7_000);
    assert_eq!(after_slash.slashed_amount, 2_000);

    assert_eq!(
        token.balance(&bond_contract),
        7_000,
        "canonical slash() only marks state; tokens remain in the contract"
    );
    assert_eq!(
        token.balance(&slash_treasury),
        0,
        "no tokens routed to slash treasury until slash_bond is called"
    );
}

/// `withdraw` with `amount == 0` is rejected at validation. The pre-transfer
/// state must be unchanged.
#[test]
fn test_withdraw_zero_amount_rejected_at_validation() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, identity, token_id, bond_contract) = test_helpers::setup_with_token(&e);

    client.create_bond(&identity, &1_000, &DAY, &false, &0_u64);
    e.ledger().with_mut(|li| li.timestamp = 1_000 + DAY + 1);

    let token = TokenClient::new(&e, &token_id);
    let before_identity = token.balance(&identity);
    let before_contract = token.balance(&bond_contract);
    let before_state = client.get_identity_state(&identity);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw(&identity, &0_i128);
    }));
    assert!(
        result.is_err(),
        "withdraw(amount = 0) must panic at validation"
    );

    let after_state = client.get_identity_state(&identity);
    assert_eq!(
        token.balance(&identity),
        before_identity,
        "balance must be unchanged after rejected zero-amount withdraw"
    );
    assert_eq!(token.balance(&bond_contract), before_contract);
    assert_eq!(
        after_state.bonded_amount, before_state.bonded_amount,
        "storage must be unchanged after rejected zero-amount withdraw"
    );
}

/// `withdraw` reverts atomically when the underlying token transfer fails.
/// We wire a `ChaosToken` (from `crate::chaos_token`) as the bond token and
/// arm it to make every outbound `transfer` revert. The bond's storage
/// mirrors the canonical "all or nothing" transactional guarantee: a
/// failing outbound transfer must NOT decrement `bonded_amount`.
#[test]
fn test_withdraw_reverts_atomically_when_token_transfer_fails() {
    use crate::chaos_token::{ChaosToken, ChaosTokenClient};

    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    e.mock_all_auths();

    let contract_id = e.register(crate::CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    client.initialize(&admin, &None);

    let token_id = e.register(ChaosToken, ());
    let chaos = ChaosTokenClient::new(&e, &token_id);
    chaos.mint(&identity, &10_000_i128);

    let token = soroban_sdk::token::TokenClient::new(&e, &token_id);
    let expiration = e.ledger().sequence().saturating_add(10_000);
    token.approve(&identity, &contract_id, &10_000, &expiration);

    let mut accepted = soroban_sdk::Vec::new(&e);
    accepted.push_back(token_id.clone());
    client.set_accepted_tokens(&admin, &accepted);
    client.set_token(&admin, &token_id);

    client.create_bond(&identity, &5_000, &DAY, &false, &0_u64);
    assert_eq!(client.get_identity_state(&identity).bonded_amount, 5_000);

    // Arm ChaosToken to fail any outbound token transfer.
    chaos.set_fail_transfer(&true);

    e.ledger().with_mut(|li| li.timestamp = 1_000 + DAY + 1);

    let snapshot = client.get_identity_state(&identity);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw(&identity, &1_000);
    }));
    assert!(
        result.is_err(),
        "withdraw must panic when underlying token transfer fails"
    );

    let after = client.get_identity_state(&identity);
    assert_eq!(
        after.bonded_amount, snapshot.bonded_amount,
        "transaction failure must NOT have mutated bonded_amount"
    );
    assert_eq!(
        after.slashed_amount, snapshot.slashed_amount,
        "transaction failure must NOT have mutated slashed_amount"
    );
    let _ = contract_id; // silence unused when only client is referenced
}

/// Phantom-balance mode: when no token has been configured via
/// `set_token`, the canonical `create_bond` skips the external token
/// transfer and only mutates `IdentityBond` storage. This proves that
/// legacy / non-token deployments can still operate the on-paper ledger
/// without performing on-token moves.
#[test]
fn test_phantom_balance_mode_succeeds_without_token_configuration() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    e.mock_all_auths();

    let contract_id = e.register(crate::CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    client.initialize(&admin, &None);
    // No `set_token` call — phantom-balance deployment.

    // Canonical `create_bond` is gated by `if token_integration::has_token(&e)`;
    // with no token configured it succeeds as a pure storage write.
    let bond = client.create_bond(&identity, &1_000, &DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 1_000, "phantom-mode create_bond must succeed");
    assert_eq!(
        client.get_identity_state(&identity).bonded_amount,
        1_000,
        "phantom-mode create_bond must persist the new bond"
    );
    let _ = admin;
}
