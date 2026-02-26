//! Comprehensive tests for the fixed_duration_bond contract.

#![cfg(test)]

use crate::test_helpers::*;
use crate::{FixedDurationBond, FixedDurationBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env};

// ═══════════════════════════════════════════════════════════════════
// 1. Initialization
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_initialize_success() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(FixedDurationBond, ());
    let client = FixedDurationBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let token = Address::generate(&e);
    client.initialize(&admin, &token);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_initialize_twice_panics() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(FixedDurationBond, ());
    let client = FixedDurationBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    let token = Address::generate(&e);
    client.initialize(&admin, &token);
    client.initialize(&admin, &token);
}

// ═══════════════════════════════════════════════════════════════════
// 2. Bond creation — happy path
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_create_bond_success() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);

    let bond = client.create_bond(&owner, &1_000_000_i128, &ONE_DAY);

    assert!(bond.active);
    assert_eq!(bond.amount, 1_000_000);
    assert_eq!(bond.bond_duration, ONE_DAY);
    assert_eq!(bond.owner, owner);
    assert_eq!(bond.bond_expiry, bond.bond_start + ONE_DAY);
}

#[test]
fn test_create_bond_stores_expiry_correctly() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let (client, _admin, owner, _token, _cid) = setup(&e);

    let bond = client.create_bond(&owner, &5_000_000_i128, &ONE_WEEK);

    assert_eq!(bond.bond_start, 1_000_000);
    assert_eq!(bond.bond_expiry, 1_000_000 + ONE_WEEK);
}

#[test]
fn test_create_bond_with_min_positive_amount() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    let bond = client.create_bond(&owner, &1_i128, &ONE_DAY);
    assert_eq!(bond.amount, 1);
    assert!(bond.active);
}

#[test]
fn test_create_bond_usdc_amount() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    let usdc = 100_000_000_i128; // 100 USDC (6 decimals)
    let bond = client.create_bond(&owner, &usdc, &ONE_DAY);
    assert_eq!(bond.amount, usdc);
}

// ═══════════════════════════════════════════════════════════════════
// 2b. Bond creation — error paths
// ═══════════════════════════════════════════════════════════════════

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_create_bond_zero_amount_panics() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &0_i128, &ONE_DAY);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_create_bond_negative_amount_panics() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &(-1_i128), &ONE_DAY);
}

#[test]
#[should_panic(expected = "duration must be positive")]
fn test_create_bond_zero_duration_panics() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &0_u64);
}

#[test]
#[should_panic(expected = "bond expiry timestamp would overflow")]
fn test_create_bond_overflow_panics() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = u64::MAX - 500);
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &1_000_u64);
}

#[test]
#[should_panic(expected = "bond already active for this owner")]
fn test_create_bond_duplicate_active_panics() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    client.create_bond(&owner, &2_000_i128, &ONE_DAY);
}

// ═══════════════════════════════════════════════════════════════════
// 3. Maturity checks
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_is_matured_false_before_expiry() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    assert!(!client.is_matured(&owner));
}

#[test]
fn test_is_matured_true_after_expiry() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    e.ledger().with_mut(|li| li.timestamp += ONE_DAY + 1);
    assert!(client.is_matured(&owner));
}

#[test]
fn test_is_matured_true_at_exact_expiry() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    e.ledger().with_mut(|li| li.timestamp = 1_000 + ONE_DAY);
    assert!(client.is_matured(&owner));
}

#[test]
fn test_get_time_remaining_before_expiry() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    e.ledger().with_mut(|li| li.timestamp = ONE_DAY / 2);
    let remaining = client.get_time_remaining(&owner);
    assert_eq!(remaining, ONE_DAY - ONE_DAY / 2);
}

#[test]
fn test_get_time_remaining_zero_after_maturity() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    e.ledger().with_mut(|li| li.timestamp += ONE_DAY + 100);
    assert_eq!(client.get_time_remaining(&owner), 0_u64);
}

// ═══════════════════════════════════════════════════════════════════
// 4. Normal withdrawal (after lock)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_withdraw_success_after_maturity() {
    let e = Env::default();
    let (client, _admin, owner, token_addr, contract_id) = setup(&e);

    let amount = 5_000_000_i128;
    client.create_bond(&owner, &amount, &ONE_DAY);

    e.ledger().with_mut(|li| li.timestamp += ONE_DAY + 1);
    let bond = client.withdraw(&owner);

    assert!(!bond.active);
    let tok = TokenClient::new(&e, &token_addr);
    assert_eq!(tok.balance(&owner), DEFAULT_MINT);
    assert_eq!(tok.balance(&contract_id), 0);
}

#[test]
#[should_panic(expected = "lock period has not elapsed yet")]
fn test_withdraw_before_maturity_panics() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    client.withdraw(&owner);
}

#[test]
#[should_panic(expected = "no active bond found")]
fn test_withdraw_no_bond_panics() {
    let e = Env::default();
    let (client, _admin, _owner, _token, _cid) = setup(&e);
    let other = Address::generate(&e);
    client.withdraw(&other);
}

#[test]
#[should_panic(expected = "no active bond found")]
fn test_withdraw_already_withdrawn_panics() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    e.ledger().with_mut(|li| li.timestamp += ONE_DAY + 1);
    client.withdraw(&owner);
    client.withdraw(&owner); // second call should panic
}

#[test]
fn test_withdraw_deactivates_bond() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    e.ledger().with_mut(|li| li.timestamp += ONE_DAY + 1);
    let bond = client.withdraw(&owner);
    assert!(!bond.active);
}

// ═══════════════════════════════════════════════════════════════════
// 5. Early withdrawal
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_withdraw_early_deducts_penalty() {
    let e = Env::default();
    let (client, admin, owner, token_addr, _cid) = setup(&e);

    // 10% penalty
    client.set_penalty_config(&admin, &1_000_u32);

    let amount = 10_000_i128;
    client.create_bond(&owner, &amount, &ONE_DAY);
    client.withdraw_early(&owner);

    let tok = TokenClient::new(&e, &token_addr);
    let expected_net = 9_000_i128; // 10000 - 10%
    assert_eq!(tok.balance(&owner), DEFAULT_MINT - amount + expected_net);
}

#[test]
fn test_withdraw_early_sends_penalty_to_treasury() {
    let e = Env::default();
    let (client, admin, owner, token_addr, _cid) = setup(&e);

    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &0_u32); // treasury set, no creation fee
    client.set_penalty_config(&admin, &500_u32); // 5% penalty

    let amount = 10_000_i128;
    client.create_bond(&owner, &amount, &ONE_DAY);
    client.withdraw_early(&owner);

    let tok = TokenClient::new(&e, &token_addr);
    assert_eq!(tok.balance(&treasury), 500); // 5% of 10000
}

#[test]
#[should_panic(expected = "early-exit penalty not configured")]
fn test_withdraw_early_no_penalty_panics() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    client.withdraw_early(&owner);
}

#[test]
#[should_panic(expected = "bond has matured; use withdraw instead")]
fn test_withdraw_early_after_maturity_panics() {
    let e = Env::default();
    let (client, admin, owner, _token, _cid) = setup(&e);
    client.set_penalty_config(&admin, &500_u32);
    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    e.ledger().with_mut(|li| li.timestamp += ONE_DAY + 1);
    client.withdraw_early(&owner);
}

#[test]
#[should_panic(expected = "no active bond found")]
fn test_withdraw_early_no_bond_panics() {
    let e = Env::default();
    let (client, admin, _owner, _token, _cid) = setup(&e);
    client.set_penalty_config(&admin, &500_u32);
    let other = Address::generate(&e);
    client.withdraw_early(&other);
}

// ═══════════════════════════════════════════════════════════════════
// 6. Fee config / collection
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_fee_deducted_from_bond_amount() {
    let e = Env::default();
    let (client, admin, owner, _token, _cid) = setup(&e);

    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32); // 1% fee

    let gross = 10_000_i128;
    let bond = client.create_bond(&owner, &gross, &ONE_DAY);
    assert_eq!(bond.amount, 9_900); // net after 1%
}

#[test]
fn test_collect_fees() {
    let e = Env::default();
    let (client, admin, owner, token_addr, _cid) = setup(&e);

    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32); // 1% fee

    client.create_bond(&owner, &10_000_i128, &ONE_DAY);

    let tok = TokenClient::new(&e, &token_addr);
    let before = tok.balance(&treasury);
    client.collect_fees(&admin, &treasury);
    assert_eq!(tok.balance(&treasury) - before, 100); // 1% of 10000
}

#[test]
#[should_panic(expected = "no fees to collect")]
fn test_collect_fees_when_none_panics() {
    let e = Env::default();
    let (client, admin, _owner, _token, _cid) = setup(&e);
    let recipient = Address::generate(&e);
    client.collect_fees(&admin, &recipient);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_set_fee_config_unauthorized_panics() {
    let e = Env::default();
    let (client, _admin, _owner, _token, _cid) = setup(&e);
    let impostor = Address::generate(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&impostor, &treasury, &100_u32);
}

// ═══════════════════════════════════════════════════════════════════
// 7. Re-bond after withdrawal
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_rebond_after_withdraw() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);

    client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    e.ledger().with_mut(|li| li.timestamp += ONE_DAY + 1);
    client.withdraw(&owner);

    // Should be able to create a new bond after the first is withdrawn.
    let bond2 = client.create_bond(&owner, &2_000_i128, &ONE_WEEK);
    assert!(bond2.active);
    assert_eq!(bond2.amount, 2_000);
}

// ═══════════════════════════════════════════════════════════════════
// 8. Penalty config
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_penalty_stored_on_bond() {
    let e = Env::default();
    let (client, admin, owner, _token, _cid) = setup(&e);
    client.set_penalty_config(&admin, &250_u32); // 2.5%
    let bond = client.create_bond(&owner, &1_000_i128, &ONE_DAY);
    assert_eq!(bond.penalty_bps, 250);
}

#[test]
#[should_panic(expected = "unauthorized")]
fn test_set_penalty_config_unauthorized_panics() {
    let e = Env::default();
    let (client, _admin, _owner, _token, _cid) = setup(&e);
    let impostor = Address::generate(&e);
    client.set_penalty_config(&impostor, &500_u32);
}

// ═══════════════════════════════════════════════════════════════════
// 9. Query functions
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_get_bond_returns_correct_state() {
    let e = Env::default();
    let (client, _admin, owner, _token, _cid) = setup(&e);
    client.create_bond(&owner, &3_333_i128, &ONE_WEEK);
    let b = client.get_bond(&owner);
    assert_eq!(b.amount, 3_333);
    assert_eq!(b.bond_duration, ONE_WEEK);
    assert!(b.active);
}

#[test]
#[should_panic(expected = "no active bond found")]
fn test_get_bond_nonexistent_panics() {
    let e = Env::default();
    let (client, _admin, _owner, _token, _cid) = setup(&e);
    let stranger = Address::generate(&e);
    client.get_bond(&stranger);
}
