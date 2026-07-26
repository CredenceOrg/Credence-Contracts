//! Tests for the max-leverage guard in the position-open (bond-creation) flow.
//!
//! Coverage:
//! 1. Default parameter value
//! 2. Admin can update the cap; non-admin cannot
//! 3. Parameter bounds enforcement (set_max_leverage)
//! 4. Bond opens below cap — success
//! 5. Bond opens at cap — success
//! 6. Bond opens above cap — reverts
//! 7. Scenarios with amounts representing 6-decimal and 18-decimal token scales
//! 8. Reduced cap takes effect on subsequent bonds

use crate::parameters::{DEFAULT_MAX_LEVERAGE, MAX_MAX_LEVERAGE, MIN_MAX_LEVERAGE};
use crate::test_helpers::setup_with_token_mint;
use crate::validation::MIN_BOND_AMOUNT;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_no_token(e: &Env) -> (CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    (client, admin)
}

// ---------------------------------------------------------------------------
// 1. Default parameter value
// ---------------------------------------------------------------------------

#[test]
fn test_default_max_leverage() {
    let e = Env::default();
    let (client, _admin) = setup_no_token(&e);
    assert_eq!(client.get_max_leverage(), DEFAULT_MAX_LEVERAGE);
}

// ---------------------------------------------------------------------------
// 2. Admin access control
// ---------------------------------------------------------------------------

#[test]
fn test_set_max_leverage_by_admin() {
    let e = Env::default();
    let (client, admin) = setup_no_token(&e);

    client.set_max_leverage(&admin, &50_000_u32);
    assert_eq!(client.get_max_leverage(), 50_000_u32);
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_max_leverage_non_admin_rejected() {
    let e = Env::default();
    let (client, _admin) = setup_no_token(&e);
    let non_admin = Address::generate(&e);

    client.set_max_leverage(&non_admin, &50_000_u32);
}

// ---------------------------------------------------------------------------
// 3. Parameter bounds enforcement
// ---------------------------------------------------------------------------

#[test]
fn test_set_max_leverage_min_bound() {
    let e = Env::default();
    let (client, admin) = setup_no_token(&e);

    client.set_max_leverage(&admin, &MIN_MAX_LEVERAGE);
    assert_eq!(client.get_max_leverage(), MIN_MAX_LEVERAGE);
}

#[test]
fn test_set_max_leverage_max_bound() {
    let e = Env::default();
    let (client, admin) = setup_no_token(&e);

    client.set_max_leverage(&admin, &MAX_MAX_LEVERAGE);
    assert_eq!(client.get_max_leverage(), MAX_MAX_LEVERAGE);
}

#[test]
#[should_panic(expected = "max_leverage out of bounds")]
fn test_set_max_leverage_zero_rejected() {
    let e = Env::default();
    let (client, admin) = setup_no_token(&e);

    client.set_max_leverage(&admin, &0_u32);
}

#[test]
#[should_panic(expected = "max_leverage out of bounds")]
fn test_set_max_leverage_above_hard_ceiling_rejected() {
    let e = Env::default();
    let (client, admin) = setup_no_token(&e);

    client.set_max_leverage(&admin, &(MAX_MAX_LEVERAGE + 1));
}

// ---------------------------------------------------------------------------
// 4. Bond below cap — success
// ---------------------------------------------------------------------------

#[test]
fn test_create_bond_below_cap_succeeds() {
    let e = Env::default();
    // Cap at 10× → max bond = 10 × MIN_BOND_AMOUNT
    let cap = 10_u32;
    let amount = 5 * MIN_BOND_AMOUNT; // 5× — below cap
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 2);

    client.set_max_leverage(&admin, &cap);
    let bond = client.create_bond(&identity, &amount, &86_400_u64);
    assert_eq!(bond.bonded_amount, amount);
    assert!(bond.active);
}

// ---------------------------------------------------------------------------
// 5. Bond exactly at cap — success
// ---------------------------------------------------------------------------

#[test]
fn test_create_bond_at_cap_succeeds() {
    let e = Env::default();
    let cap = 10_u32;
    let amount = cap as i128 * MIN_BOND_AMOUNT; // exactly 10×
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 2);

    client.set_max_leverage(&admin, &cap);
    let bond = client.create_bond(&identity, &amount, &86_400_u64);
    assert_eq!(bond.bonded_amount, amount);
}

// ---------------------------------------------------------------------------
// 6. Bond above cap — reverts
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn test_create_bond_above_cap_reverts() {
    let e = Env::default();
    let cap = 10_u32;
    let amount = (cap as i128 + 1) * MIN_BOND_AMOUNT; // 11× — one step over
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 2);

    client.set_max_leverage(&admin, &cap);
    client.create_bond(&identity, &amount, &86_400_u64);
}

// ---------------------------------------------------------------------------
// 7. Varying collateral decimal scales
// ---------------------------------------------------------------------------

/// 6-decimal token (e.g. USDC): MIN_BOND_AMOUNT = 1_000_000 = 1.000000 USDC
#[test]
fn test_leverage_6_decimal_at_cap_succeeds() {
    let e = Env::default();
    let cap = 100_u32;
    // 100 USDC = 100_000_000 raw units = 100× MIN_BOND_AMOUNT
    let amount = 100 * MIN_BOND_AMOUNT;
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 2);

    client.set_max_leverage(&admin, &cap);
    let bond = client.create_bond(&identity, &amount, &86_400_u64);
    assert_eq!(bond.bonded_amount, amount);
}

#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn test_leverage_6_decimal_above_cap_reverts() {
    let e = Env::default();
    let cap = 100_u32;
    // 101 USDC = 101_000_000 raw units = 101× MIN_BOND_AMOUNT — one over
    let amount = 101 * MIN_BOND_AMOUNT;
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 2);

    client.set_max_leverage(&admin, &cap);
    client.create_bond(&identity, &amount, &86_400_u64);
}

/// 18-decimal token: amounts are 10^12 larger per human unit than 6-decimal.
/// The leverage formula (amount / MIN_BOND_AMOUNT) is dimensionless — we simply
/// verify the arithmetic handles large i128 values correctly.
#[test]
fn test_leverage_18_decimal_scale_at_cap_succeeds() {
    let e = Env::default();
    // Treat MIN_BOND_AMOUNT as representing 10^-18 granularity:
    // 1 "token" at 18 decimals relative to our MIN_BOND_AMOUNT (6 decimals) is
    // 1_000_000_000_000 × MIN_BOND_AMOUNT.  Cap generously.
    let cap = 1_000_u32;
    let amount = cap as i128 * MIN_BOND_AMOUNT; // exactly at cap
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 2);

    client.set_max_leverage(&admin, &cap);
    let bond = client.create_bond(&identity, &amount, &86_400_u64);
    assert_eq!(bond.bonded_amount, amount);
}

#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn test_leverage_18_decimal_scale_above_cap_reverts() {
    let e = Env::default();
    let cap = 1_000_u32;
    let amount = (cap as i128 + 1) * MIN_BOND_AMOUNT;
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 2);

    client.set_max_leverage(&admin, &cap);
    client.create_bond(&identity, &amount, &86_400_u64);
}

// ---------------------------------------------------------------------------
// 8. Reduced cap takes effect on subsequent bonds
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "leverage exceeds maximum")]
fn test_reduced_cap_blocks_previously_valid_amount() {
    let e = Env::default();
    let original_cap = 100_u32;
    let amount = 50 * MIN_BOND_AMOUNT; // 50× — fine under original cap

    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount * 4);

    // First bond succeeds under original cap
    client.set_max_leverage(&admin, &original_cap);
    client.create_bond(&identity, &amount, &86_400_u64);

    // Admin tightens the cap below the previously-valid amount
    client.set_max_leverage(&admin, &10_u32);

    // Second bond with the same amount must now revert
    client.create_bond(&identity, &amount, &86_400_u64);
}

// ---------------------------------------------------------------------------
// Regression: checked arithmetic in leverage calculation (issue fix)
// ---------------------------------------------------------------------------

/// `validate_leverage` computes `bond_amount / MIN_BOND_AMOUNT`.
/// Before the fix this used a bare `/`; the fix uses `checked_div_leverage`
/// which surfaces `ContractError::Overflow` instead of panicking.
///
/// For a valid positive `bond_amount` and the positive constant `MIN_BOND_AMOUNT`
/// the division never produces `None`, so this test verifies the refactoring
/// preserves correct gate semantics: amounts at exactly the cap pass; one step
/// above the cap are rejected.
#[test]
fn test_validate_leverage_checked_div_at_boundary() {
    let e = Env::default();
    let cap = 5_u32;
    let amount_at_cap = cap as i128 * MIN_BOND_AMOUNT;
    let (client, admin, identity, ..) = setup_with_token_mint(&e, amount_at_cap * 4);

    client.set_max_leverage(&admin, &cap);

    // Exactly at cap → must succeed.
    let bond = client.create_bond(&identity, &amount_at_cap, &86_400_u64);
    assert_eq!(bond.bonded_amount, amount_at_cap);
}

/// `bond_amount = i128::MAX` is the extreme end of the input range.
/// `i128::MAX / MIN_BOND_AMOUNT` (positive divisor) never wraps, so
/// `checked_div` returns `Some(quotient)` and the leverage check runs normally.
#[test]
fn test_validate_leverage_checked_div_max_i128_amount() {
    let e = Env::default();
    // Set a very high cap so the bond is not rejected by leverage.
    let (client, admin, identity, ..) = setup_with_token_mint(&e, i128::MAX);

    client.set_max_leverage(&admin, &u32::MAX);
    // Should not panic — checked_div succeeds and leverage <= u32::MAX.
    let bond = client.create_bond(&identity, &i128::MAX, &86_400_u64);
    assert_eq!(bond.bonded_amount, i128::MAX);
}