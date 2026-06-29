//! Per-entrypoint resource-budget ceiling tests for `CredenceBond`.
//!
//! Each test exercises one write-path entrypoint, resets the Soroban budget
//! snapshot immediately before the call, then asserts that the recorded CPU
//! and memory costs fall within the pre-defined ceiling from
//! [`test_budget_helper::budget_ceilings`].
//!
//! See `docs/budget-ceilings.md` for the full ceiling table and the
//! methodology used to derive the values.
//!
//! # Running
//!
//! ```sh
//! cargo test -p credence_bond budget
//! ```
//!
//! # How ceilings are enforced
//!
//! Every test calls [`assert_budget_under`] after the target entrypoint. If a
//! future change (new storage key, extra event, heavier cryptography) inflates
//! the cost beyond the ceiling, the test panics with a message that shows the
//! actual cost, the ceiling, and the excess, making the regression immediately
//! actionable in CI.

#![cfg(test)]
extern crate std;

use crate::{
    test_budget_helper::{assert_budget_under, budget_ceilings},
    test_helpers,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

// ---------------------------------------------------------------------------
// Internal setup helpers
// ---------------------------------------------------------------------------

/// Advance the ledger timestamp by `seconds` so lockup/notice checks pass.
fn advance_time(e: &Env, seconds: u64) {
    e.ledger().with_mut(|li| {
        li.timestamp = li.timestamp.saturating_add(seconds);
    });
}

// ---------------------------------------------------------------------------
// create_bond – non-rolling (3-arg shorthand)
// ---------------------------------------------------------------------------

/// Budget ceiling: `create_bond` – single standard non-rolling bond.
#[test]
fn budget_create_bond_non_rolling() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);

    e.cost_estimate().budget().reset_default();
    client.create_bond(&identity, &10_000_i128, &86_400_u64);
    assert_budget_under(&e, budget_ceilings::CREATE_BOND);
}

// ---------------------------------------------------------------------------
// create_bond – rolling (5-arg form via create_bond_with_rolling)
// ---------------------------------------------------------------------------

/// Budget ceiling: `create_bond` – rolling bond (with notice-period
/// validation branch).
#[test]
fn budget_create_bond_rolling() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);

    e.cost_estimate().budget().reset_default();
    client.create_bond_with_rolling(
        &identity,
        &10_000_i128,
        &86_400_u64,
        &true,
        &3_600_u64,
    );
    assert_budget_under(&e, budget_ceilings::CREATE_BOND);
}

// ---------------------------------------------------------------------------
// top_up
// ---------------------------------------------------------------------------

/// Budget ceiling: `top_up` – bond exists, add tokens.
#[test]
fn budget_top_up() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond(&identity, &10_000_i128, &86_400_u64);

    e.cost_estimate().budget().reset_default();
    client.top_up(&500_i128);
    assert_budget_under(&e, budget_ceilings::TOP_UP);
}

// ---------------------------------------------------------------------------
// extend_duration
// ---------------------------------------------------------------------------

/// Budget ceiling: `extend_duration` – extend by one hour.
#[test]
fn budget_extend_duration() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond(&identity, &10_000_i128, &86_400_u64);

    e.cost_estimate().budget().reset_default();
    client.extend_duration(&3_600_u64);
    assert_budget_under(&e, budget_ceilings::EXTEND_DURATION);
}

// ---------------------------------------------------------------------------
// request_withdrawal
// ---------------------------------------------------------------------------

/// Budget ceiling: `request_withdrawal` – rolling bond, first request.
#[test]
fn budget_request_withdrawal() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    // Rolling bond required for request_withdrawal to succeed.
    client.create_bond_with_rolling(
        &identity,
        &10_000_i128,
        &86_400_u64,
        &true,
        &3_600_u64,
    );

    e.cost_estimate().budget().reset_default();
    client.request_withdrawal();
    assert_budget_under(&e, budget_ceilings::REQUEST_WITHDRAWAL);
}

// ---------------------------------------------------------------------------
// withdraw (after lockup expires)
// ---------------------------------------------------------------------------

/// Budget ceiling: `withdraw` – non-rolling bond, lockup has expired.
#[test]
fn budget_withdraw_after_lockup() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    // Advance past lockup so withdraw() does not reject.
    advance_time(&e, 86_401);

    e.cost_estimate().budget().reset_default();
    client.withdraw(&5_000_i128);
    assert_budget_under(&e, budget_ceilings::WITHDRAW);
}

// ---------------------------------------------------------------------------
// withdraw_early
// ---------------------------------------------------------------------------

/// Budget ceiling: `withdraw_early` – standard penalty path.
///
/// Uses a zero penalty rate to avoid token-transfer side effects while still
/// exercising the full penalty-calculation branch (the cost path is the same
/// regardless of the computed penalty value).
#[test]
fn budget_withdraw_early() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    let treasury = Address::generate(&e);
    let (client, admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.set_early_exit_config(&admin, &treasury, &0_u32);
    // Long duration so "now < bond_end" is satisfied.
    client.create_bond_with_rolling(
        &identity,
        &50_000_i128,
        &3_153_600_u64, // ~1 year
        &false,
        &0_u64,
    );
    // Advance only one day – still well before lockup end.
    advance_time(&e, 86_400);

    e.cost_estimate().budget().reset_default();
    client.withdraw_early(&1_000_i128);
    assert_budget_under(&e, budget_ceilings::WITHDRAW_EARLY);
}

// ---------------------------------------------------------------------------
// slash (simple, no reentrancy guard)
// ---------------------------------------------------------------------------

/// Budget ceiling: `slash` – admin slashes a portion of the bond.
#[test]
fn budget_slash() {
    let e = Env::default();
    let (client, admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);

    e.cost_estimate().budget().reset_default();
    client.slash(&admin, &1_000_i128);
    assert_budget_under(&e, budget_ceilings::SLASH_BOND);
}

// ---------------------------------------------------------------------------
// slash_bond (reentrancy-guarded)
// ---------------------------------------------------------------------------

/// Budget ceiling: `slash_bond` – admin slashes via reentrancy-guarded path.
#[test]
fn budget_slash_bond() {
    let e = Env::default();
    let (client, admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond(&identity, &10_000_i128, &86_400_u64);

    e.cost_estimate().budget().reset_default();
    client.slash_bond(&admin, &1_000_i128);
    assert_budget_under(&e, budget_ceilings::SLASH_BOND);
}

// ---------------------------------------------------------------------------
// add_attestation (normal and max-size)
// ---------------------------------------------------------------------------

/// Budget ceiling: `add_attestation` – typical short payload.
#[test]
fn budget_add_attestation_normal() {
    let e = Env::default();
    let (client, _admin, _identity, _token_id, contract_id) =
        test_helpers::setup_with_token(&e);
    let attester = Address::generate(&e);
    client.register_attester(&attester);

    let subject = Address::generate(&e);
    let data = String::from_str(&e, "credence:verified:kyc-tier-1");
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(&attester);

    e.cost_estimate().budget().reset_default();
    client.add_attestation(&attester, &subject, &data, &contract_id, &deadline, &nonce);
    assert_budget_under(&e, budget_ceilings::ADD_ATTESTATION);
}

/// Budget ceiling: `add_attestation` – maximum-length data string (64 ASCII
/// characters, the documented hard limit for attestation_data).
#[test]
fn budget_add_attestation_max_size_payload() {
    let e = Env::default();
    let (client, _admin, _identity, _token_id, contract_id) =
        test_helpers::setup_with_token(&e);
    let attester = Address::generate(&e);
    client.register_attester(&attester);

    let subject = Address::generate(&e);
    // Build a 64-character ASCII string (maximum payload size per spec).
    let max_payload: std::string::String = "A".repeat(64);
    let data = String::from_str(&e, &max_payload);
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(&attester);

    e.cost_estimate().budget().reset_default();
    client.add_attestation(&attester, &subject, &data, &contract_id, &deadline, &nonce);
    assert_budget_under(&e, budget_ceilings::ADD_ATTESTATION_MAX_SIZE);
}

/// Budget ceiling: `add_attestation` – subject already has many prior
/// attestations (max-list edge case). Each prior attestation grows the
/// `SubjectAttestations` Vec that must be deserialized and re-serialized on
/// every subsequent call.
#[test]
fn budget_add_attestation_max_prior_attestations() {
    let e = Env::default();
    let (client, _admin, _identity, _token_id, contract_id) =
        test_helpers::setup_with_token(&e);
    let attester = Address::generate(&e);
    client.register_attester(&attester);
    let subject = Address::generate(&e);

    // Pre-populate 20 prior attestations.
    // The dedup key is (attester, subject, data), so we vary `data` each time.
    for i in 0_u32..20_u32 {
        let prior_data = String::from_str(&e, &std::format!("prior:{i}"));
        let deadline = e.ledger().timestamp() + 100_000;
        let nonce = client.get_nonce(&attester);
        client.add_attestation(
            &attester,
            &subject,
            &prior_data,
            &contract_id,
            &deadline,
            &nonce,
        );
    }

    // Measure the 21st call – largest Vec to deserialize/serialize.
    let data = String::from_str(&e, "new-attestation-at-max-list");
    let deadline = e.ledger().timestamp() + 100_000;
    let nonce = client.get_nonce(&attester);

    e.cost_estimate().budget().reset_default();
    client.add_attestation(&attester, &subject, &data, &contract_id, &deadline, &nonce);
    assert_budget_under(&e, budget_ceilings::ADD_ATTESTATION_MAX_SIZE);
}

// ---------------------------------------------------------------------------
// withdraw_bond (partial, reentrancy-guarded)
// ---------------------------------------------------------------------------

/// Budget ceiling: `withdraw_bond` – partial amount after lockup expires.
#[test]
fn budget_withdraw_bond_partial() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);
    advance_time(&e, 86_401);

    e.cost_estimate().budget().reset_default();
    client.withdraw_bond(&5_000_i128);
    assert_budget_under(&e, budget_ceilings::WITHDRAW_BOND);
}

/// Budget ceiling: `withdraw_bond` – rolling bond, partial withdrawal after
/// notice period elapses.
#[test]
fn budget_withdraw_bond_rolling_after_notice() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(
        &identity,
        &10_000_i128,
        &86_400_u64,
        &true,
        &3_600_u64,
    );
    client.request_withdrawal();
    // Advance past the notice period.
    advance_time(&e, 3_601);

    e.cost_estimate().budget().reset_default();
    client.withdraw_bond(&5_000_i128);
    assert_budget_under(&e, budget_ceilings::WITHDRAW_BOND);
}

// ---------------------------------------------------------------------------
// collect_fees (reentrancy-guarded)
// ---------------------------------------------------------------------------

/// Budget ceiling: `collect_fees` – non-zero fees have been deposited.
#[test]
fn budget_collect_fees() {
    let e = Env::default();
    let (client, admin, _identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    // Deposit fees so the storage key is populated (non-zero value path).
    client.deposit_fees(&1_000_i128);

    e.cost_estimate().budget().reset_default();
    client.collect_fees(&admin);
    assert_budget_under(&e, budget_ceilings::COLLECT_FEES);
}

// ---------------------------------------------------------------------------
// renew_if_rolling
// ---------------------------------------------------------------------------

/// Budget ceiling: `renew_if_rolling` – period has ended, bond renews.
#[test]
fn budget_renew_if_rolling() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(
        &identity,
        &10_000_i128,
        &86_400_u64,
        &true,
        &3_600_u64,
    );
    // Advance past the bond period so renewal fires.
    advance_time(&e, 86_401);

    e.cost_estimate().budget().reset_default();
    client.renew_if_rolling();
    assert_budget_under(&e, budget_ceilings::RENEW_IF_ROLLING);
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

/// Edge case: `slash_bond` with the maximum slash amount (entire bond).
/// Verifies the ceiling holds when slashed_amount reaches the bond total.
#[test]
fn budget_slash_bond_max_slash() {
    let e = Env::default();
    let (client, admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond(&identity, &10_000_i128, &86_400_u64);

    e.cost_estimate().budget().reset_default();
    // Slash the full bond amount.
    client.slash_bond(&admin, &10_000_i128);
    assert_budget_under(&e, budget_ceilings::SLASH_BOND);
}

/// Edge case: `top_up` with a very large amount to stress checked arithmetic.
#[test]
fn budget_top_up_large_amount() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_max_mint(&e);
    client.create_bond(&identity, &1_000_i128, &86_400_u64);

    e.cost_estimate().budget().reset_default();
    // Top up with a value just below overflow boundary.
    client.top_up(&(i128::MAX / 2 - 1_000));
    assert_budget_under(&e, budget_ceilings::TOP_UP);
}

/// Edge case: `slash` and then `withdraw` – tests the interaction path where
/// available = bonded - slashed before partial withdrawal.
#[test]
fn budget_slash_then_withdraw_interaction() {
    let e = Env::default();
    e.ledger().with_mut(|li| li.timestamp = 0);
    let (client, admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);
    client.create_bond_with_rolling(&identity, &10_000_i128, &86_400_u64, &false, &0_u64);
    test_helpers::advance_ledger_sequence(&e);
    client.slash(&admin, &2_000_i128);
    advance_time(&e, 86_401);

    e.cost_estimate().budget().reset_default();
    // Withdraw only the available (non-slashed) portion.
    client.withdraw(&8_000_i128);
    assert_budget_under(&e, budget_ceilings::WITHDRAW);
}

// ---------------------------------------------------------------------------
// Regression guard (deliberately exceeds ceiling — run with --ignored)
// ---------------------------------------------------------------------------

/// Regression guard: a deliberately tight ceiling (1 CPU unit, 1 mem byte)
/// must be exceeded by any real entrypoint call, proving that
/// `assert_budget_under` actually catches violations.
///
/// This test is **expected to panic** and is therefore marked `#[ignore]`.
/// Run it manually to verify the assertion mechanism:
///
/// ```sh
/// cargo test -p credence_bond budget_regression_guard_fails -- --ignored
/// ```
///
/// CI output should include a "budget ceiling EXCEEDED" message, confirming
/// the guard would fire on a real regression.
#[test]
#[ignore = "intentionally exceeds budget ceiling – run with --ignored to verify guard"]
fn budget_regression_guard_fails() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) =
        test_helpers::setup_with_token(&e);

    // Use an absurdly tight ceiling so any real contract call exceeds it.
    let absurdly_tight = crate::test_budget_helper::BudgetCeiling {
        cpu_max: 1,
        mem_max: 1,
    };

    e.cost_estimate().budget().reset_default();
    client.create_bond(&identity, &1_000_i128, &86_400_u64);

    // This MUST panic with "budget ceiling EXCEEDED" – the test is correct
    // only if it panics.
    assert_budget_under(&e, absurdly_tight);
}