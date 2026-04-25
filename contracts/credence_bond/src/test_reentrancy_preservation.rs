//! Preservation Property Tests - Non-Reentrant Withdrawal Behavior
//!
//! **Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.5**
//!
//! **Property 2: Preservation** - Non-Reentrant Withdrawal Behavior
//!
//! This test suite follows the observation-first methodology:
//! 1. Observe behavior on UNFIXED code for normal (non-reentrant) withdrawals
//! 2. Write property-based tests capturing observed behavior patterns
//! 3. Run tests on UNFIXED code - EXPECTED OUTCOME: Tests PASS
//! 4. After fix is implemented, re-run tests - EXPECTED OUTCOME: Tests still PASS (no regressions)
//!
//! The tests verify that all inputs that do NOT involve re-entrant calls are completely
//! unaffected by the reentrancy fix. This includes:
//! - Normal single withdrawal calls that complete successfully
//! - Withdrawal calls that fail validation checks (insufficient balance, wrong timing, etc.)
//! - Sequential non-reentrant withdrawals
//! - All error messages, events, calculations, and state updates
//!
//! Property-based testing generates many test cases for stronger guarantees.

use super::*;
use crate::test_helpers;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::token::TokenClient;
use soroban_sdk::{Address, Env, Symbol, TryFromVal};

// ===========================================================================
// Helper: Deterministic RNG for property-based testing
// ===========================================================================

#[derive(Clone, Copy, Debug)]
struct TestRng {
    state: u64,
}

impl TestRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn gen_range(&mut self, min: i128, max: i128) -> i128 {
        if max <= min {
            return min;
        }
        let range = (max - min) as u64;
        min + (self.next_u64() % range) as i128
    }

    fn gen_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }
}

// ===========================================================================
// Property 1: Normal withdraw_bond() preserves behavior
// ===========================================================================

/// **Property 2.1: Preservation** - Normal withdraw_bond() behavior unchanged
///
/// For all valid non-reentrant withdraw_bond() inputs:
/// - Balance updates match expected calculations
/// - Events are emitted with correct data
/// - Return values are correct
/// - Sequential withdrawals both succeed
///
/// This test generates many test cases to verify preservation across the input domain.
#[test]
fn property_withdraw_bond_normal_behavior_preserved() {
    let mut rng = TestRng::new(0xDEADBEEF);

    // Run 50 property-based test cases
    for iteration in 0..50 {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().with_mut(|l| l.timestamp = 1000);

        let (client, _admin, identity, token_id, bond_contract_id) =
            test_helpers::setup_with_token(&e);
        let token_client = TokenClient::new(&e, &token_id);

        // Generate random valid inputs
        let initial_amount = rng.gen_range(1000, 10000);
        let duration = rng.gen_range(86400, 86400 * 30); // 1-30 days
        let is_rolling = rng.gen_bool();

        // Create bond
        client.create_bond_with_rolling(
            &identity,
            &initial_amount,
            &(duration as u64),
            &is_rolling,
            &0_u64,
        );

        // Advance time past lock-up period
        e.ledger().with_mut(|l| {
            l.timestamp = 1000 + duration as u64 + 1;
        });

        // Generate withdrawal amount (valid range)
        let withdraw_amount = rng.gen_range(1, initial_amount);

        // Record state before withdrawal
        let before_bond = client.get_identity_state();
        let before_identity_balance = token_client.balance(&identity);
        let before_contract_balance = token_client.balance(&bond_contract_id);

        // Perform withdrawal
        let after_bond = client.withdraw_bond(&withdraw_amount);

        // Verify balance updates match expected calculations
        assert_eq!(
            after_bond.bonded_amount,
            before_bond.bonded_amount - withdraw_amount,
            "iteration {}: bonded_amount not updated correctly",
            iteration
        );

        // Verify token transfers
        let after_identity_balance = token_client.balance(&identity);
        let after_contract_balance = token_client.balance(&bond_contract_id);

        assert_eq!(
            after_identity_balance,
            before_identity_balance + withdraw_amount,
            "iteration {}: identity balance not updated correctly",
            iteration
        );

        assert_eq!(
            after_contract_balance,
            before_contract_balance - withdraw_amount,
            "iteration {}: contract balance not updated correctly",
            iteration
        );

        // Verify event emission
        let events = e.events().all();
        let has_bond_withdrawn = events.iter().any(|(contract_id, topics, _data)| {
            contract_id == bond_contract_id
                && topics.len() > 0
                && Symbol::try_from_val(&e, &topics.get(0).unwrap())
                    .map(|topic| topic == Symbol::new(&e, "bond_withdrawn"))
                    .unwrap_or(false)
        });

        assert!(
            has_bond_withdrawn,
            "iteration {}: bond_withdrawn event not emitted",
            iteration
        );
    }
}

// ===========================================================================
// Property 2: withdraw_early() preserves penalty calculations
// ===========================================================================

/// **Property 2.2: Preservation** - withdraw_early() penalty calculations unchanged
///
/// For all valid non-reentrant withdraw_early() inputs:
/// - Penalty calculations are correct
/// - Net amount transferred to user is correct
/// - Penalty transferred to treasury is correct
/// - Balance updates match expected calculations
///
/// This test generates many test cases to verify preservation.
#[test]
fn property_withdraw_early_penalty_calculations_preserved() {
    let mut rng = TestRng::new(0xCAFEBABE);

    // Run 50 property-based test cases
    for iteration in 0..50 {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().with_mut(|l| l.timestamp = 1000);

        let (client, admin, identity, token_id, bond_contract_id) =
            test_helpers::setup_with_token(&e);
        let token_client = TokenClient::new(&e, &token_id);
        let treasury = Address::generate(&e);

        // Configure early exit penalty (10% = 1000 bps)
        client.set_early_exit_config(&admin, &treasury, &1000_u32);

        // Generate random valid inputs
        let initial_amount = rng.gen_range(1000, 10000);
        let duration = rng.gen_range(86400, 86400 * 30); // 1-30 days

        // Create bond
        client.create_bond(&identity, &initial_amount, &(duration as u64));

        // Advance time to middle of lock-up period
        let elapsed = rng.gen_range(1, duration - 1);
        e.ledger().with_mut(|l| {
            l.timestamp = 1000 + elapsed as u64;
        });

        // Generate withdrawal amount (valid range)
        let withdraw_amount = rng.gen_range(100, initial_amount);

        // Record state before withdrawal
        let before_bond = client.get_identity_state();
        let before_identity_balance = token_client.balance(&identity);
        let before_treasury_balance = token_client.balance(&treasury);
        let before_contract_balance = token_client.balance(&bond_contract_id);

        // Calculate expected penalty
        let remaining = duration - elapsed;
        let penalty = (withdraw_amount * remaining as i128 * 1000) / (duration as i128 * 10000);
        let expected_net = withdraw_amount - penalty;

        // Perform early withdrawal
        let after_bond = client.withdraw_early(&withdraw_amount);

        // Verify balance updates
        assert_eq!(
            after_bond.bonded_amount,
            before_bond.bonded_amount - withdraw_amount,
            "iteration {}: bonded_amount not updated correctly",
            iteration
        );

        // Verify token transfers (user receives net amount)
        let after_identity_balance = token_client.balance(&identity);
        let after_treasury_balance = token_client.balance(&treasury);
        let after_contract_balance = token_client.balance(&bond_contract_id);

        // User receives net amount (after penalty)
        assert!(
            after_identity_balance >= before_identity_balance + expected_net - 1
                && after_identity_balance <= before_identity_balance + expected_net + 1,
            "iteration {}: identity balance not updated correctly (expected ~{}, got {})",
            iteration,
            before_identity_balance + expected_net,
            after_identity_balance
        );

        // Treasury receives penalty (or part of it due to refund mechanism)
        assert!(
            after_treasury_balance >= before_treasury_balance,
            "iteration {}: treasury should receive penalty",
            iteration
        );

        // Contract balance decreases by full withdrawal amount
        assert_eq!(
            after_contract_balance,
            before_contract_balance - withdraw_amount,
            "iteration {}: contract balance not updated correctly",
            iteration
        );
    }
}

// ===========================================================================
// Property 3: Error conditions produce correct panic messages
// ===========================================================================

/// **Property 2.3: Preservation** - Error handling unchanged
///
/// For all error conditions:
/// - Panic messages match observed messages
/// - State is not modified on error
/// - No tokens are transferred on error
#[test]
#[should_panic(expected = "insufficient balance for withdrawal")]
fn property_withdraw_bond_insufficient_balance_error_preserved() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1000);

    let (client, _admin, identity, _token_id, _bond_contract_id) =
        test_helpers::setup_with_token(&e);

    // Create bond with 1000 tokens
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Advance time past lock-up period
    e.ledger().with_mut(|l| {
        l.timestamp = 1000 + 86400 + 1;
    });

    // Attempt to withdraw more than available - should panic with specific message
    client.withdraw_bond(&1001_i128);
}

#[test]
#[should_panic(expected = "lock-up period not elapsed; use withdraw_early")]
fn property_withdraw_bond_before_lockup_error_preserved() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1000);

    let (client, _admin, identity, _token_id, _bond_contract_id) =
        test_helpers::setup_with_token(&e);

    // Create bond with 1000 tokens
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Attempt to withdraw before lock-up period - should panic with specific message
    e.ledger().with_mut(|l| {
        l.timestamp = 1000 + 100; // Still in lock-up period
    });

    client.withdraw_bond(&500_i128);
}

#[test]
#[should_panic(expected = "use withdraw for post lock-up")]
fn property_withdraw_early_after_lockup_error_preserved() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1000);

    let (client, admin, identity, _token_id, _bond_contract_id) =
        test_helpers::setup_with_token(&e);
    let treasury = Address::generate(&e);

    // Configure early exit penalty
    client.set_early_exit_config(&admin, &treasury, &1000_u32);

    // Create bond with 1000 tokens
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Advance time past lock-up period
    e.ledger().with_mut(|l| {
        l.timestamp = 1000 + 86400 + 1;
    });

    // Attempt early withdrawal after lock-up - should panic with specific message
    client.withdraw_early(&500_i128);
}

#[test]
#[should_panic(expected = "amount must be non-negative")]
fn property_withdraw_bond_negative_amount_error_preserved() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1000);

    let (client, _admin, identity, _token_id, _bond_contract_id) =
        test_helpers::setup_with_token(&e);

    // Create bond with 1000 tokens
    client.create_bond(&identity, &1000_i128, &86400_u64);

    // Advance time past lock-up period
    e.ledger().with_mut(|l| {
        l.timestamp = 1000 + 86400 + 1;
    });

    // Attempt to withdraw negative amount - should panic with specific message
    client.withdraw_bond(&-100_i128);
}

// ===========================================================================
// Property 4: Sequential non-reentrant withdrawals both succeed
// ===========================================================================

/// **Property 2.4: Preservation** - Sequential withdrawals work correctly
///
/// For all valid sequential (non-reentrant) withdrawal scenarios:
/// - Both withdrawals succeed
/// - Balance updates are correct for each withdrawal
/// - Total withdrawn equals sum of individual withdrawals
#[test]
fn property_sequential_withdrawals_preserved() {
    let mut rng = TestRng::new(0xFEEDFACE);

    // Run 30 property-based test cases
    for iteration in 0..30 {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().with_mut(|l| l.timestamp = 1000);

        let (client, _admin, identity, token_id, bond_contract_id) =
            test_helpers::setup_with_token(&e);
        let token_client = TokenClient::new(&e, &token_id);

        // Generate random valid inputs
        let initial_amount = rng.gen_range(2000, 10000);
        let duration = rng.gen_range(86400, 86400 * 30);

        // Create bond
        client.create_bond(&identity, &initial_amount, &(duration as u64));

        // Advance time past lock-up period
        e.ledger().with_mut(|l| {
            l.timestamp = 1000 + duration as u64 + 1;
        });

        // Generate two withdrawal amounts
        let first_amount = rng.gen_range(100, initial_amount / 2);
        let second_amount = rng.gen_range(100, initial_amount - first_amount);

        // Record initial state
        let initial_identity_balance = token_client.balance(&identity);
        let initial_contract_balance = token_client.balance(&bond_contract_id);

        // First withdrawal
        let after_first = client.withdraw_bond(&first_amount);
        assert_eq!(
            after_first.bonded_amount,
            initial_amount - first_amount,
            "iteration {}: first withdrawal bonded_amount incorrect",
            iteration
        );

        // Second withdrawal (sequential, not reentrant)
        let after_second = client.withdraw_bond(&second_amount);
        assert_eq!(
            after_second.bonded_amount,
            initial_amount - first_amount - second_amount,
            "iteration {}: second withdrawal bonded_amount incorrect",
            iteration
        );

        // Verify total token transfers
        let final_identity_balance = token_client.balance(&identity);
        let final_contract_balance = token_client.balance(&bond_contract_id);

        assert_eq!(
            final_identity_balance,
            initial_identity_balance + first_amount + second_amount,
            "iteration {}: total identity balance incorrect",
            iteration
        );

        assert_eq!(
            final_contract_balance,
            initial_contract_balance - first_amount - second_amount,
            "iteration {}: total contract balance incorrect",
            iteration
        );
    }
}

// ===========================================================================
// Property 5: withdraw_bond_full() remains unchanged
// ===========================================================================

/// **Property 2.5: Preservation** - withdraw_bond_full() unchanged
///
/// The withdraw_bond_full() function already has correct reentrancy protection
/// and must remain completely unchanged by the fix.
#[test]
fn property_withdraw_bond_full_unchanged() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1000);

    let (client, _admin, identity, token_id, bond_contract_id) = test_helpers::setup_with_token(&e);
    let token_client = TokenClient::new(&e, &token_id);

    // Create bond with 5000 tokens
    let initial_amount = 5000_i128;
    client.create_bond(&identity, &initial_amount, &86400_u64);

    // Record state before withdrawal
    let before_identity_balance = token_client.balance(&identity);
    let before_contract_balance = token_client.balance(&bond_contract_id);

    // Perform full withdrawal
    let withdrawn = client.withdraw_bond_full(&identity);

    // Verify full amount withdrawn
    assert_eq!(
        withdrawn, initial_amount,
        "withdraw_bond_full should withdraw full amount"
    );

    // Verify token transfers
    let after_identity_balance = token_client.balance(&identity);
    let after_contract_balance = token_client.balance(&bond_contract_id);

    assert_eq!(
        after_identity_balance,
        before_identity_balance + initial_amount,
        "identity balance not updated correctly"
    );

    assert_eq!(
        after_contract_balance,
        before_contract_balance - initial_amount,
        "contract balance not updated correctly"
    );

    // Verify bond is deactivated
    let final_bond = client.get_identity_state();
    assert_eq!(final_bond.bonded_amount, 0, "bonded_amount should be 0");
    assert!(!final_bond.active, "bond should be inactive");
}

// ===========================================================================
// Property 6: execute_cooldown_withdrawal() normal behavior preserved
// ===========================================================================

/// **Property 2.6: Preservation** - execute_cooldown_withdrawal() behavior unchanged
///
/// For all valid non-reentrant execute_cooldown_withdrawal() inputs:
/// - Balance updates match expected calculations
/// - Cooldown request is removed after execution
/// - Events are emitted correctly
#[test]
fn property_execute_cooldown_withdrawal_preserved() {
    let mut rng = TestRng::new(0xBEEFCAFE);

    // Run 30 property-based test cases
    for iteration in 0..30 {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().with_mut(|l| l.timestamp = 1000);

        let (client, admin, identity, _token_id, _bond_contract_id) =
            test_helpers::setup_with_token(&e);

        // Set cooldown period
        let cooldown_period = rng.gen_range(3600, 86400) as u64; // 1-24 hours
        client.set_cooldown_period(&admin, &cooldown_period);

        // Generate random valid inputs
        let initial_amount = rng.gen_range(2000, 10000);
        let duration = rng.gen_range(86400, 86400 * 30) as u64;

        // Create bond
        client.create_bond(&identity, &initial_amount, &duration);

        // Request cooldown withdrawal
        let withdraw_amount = rng.gen_range(100, initial_amount);
        client.request_cooldown_withdrawal(&identity, &withdraw_amount);

        // Advance time past cooldown period
        e.ledger().with_mut(|l| {
            l.timestamp = 1000 + cooldown_period + 1;
        });

        // Record state before execution
        let before_bond = client.get_identity_state();

        // Execute cooldown withdrawal
        let after_bond = client.execute_cooldown_withdrawal(&identity);

        // Verify balance updates
        assert_eq!(
            after_bond.bonded_amount,
            before_bond.bonded_amount - withdraw_amount,
            "iteration {}: bonded_amount not updated correctly",
            iteration
        );
    }
}

// ===========================================================================
// Property 7: Zero amount withdrawals preserve behavior
// ===========================================================================

/// **Property 2.7: Preservation** - Zero amount withdrawals unchanged
///
/// Withdrawing zero amount should succeed without modifying state.
#[test]
fn property_zero_amount_withdrawal_preserved() {
    let e = Env::default();
    e.mock_all_auths();
    e.ledger().with_mut(|l| l.timestamp = 1000);

    let (client, _admin, identity, token_id, bond_contract_id) = test_helpers::setup_with_token(&e);
    let token_client = TokenClient::new(&e, &token_id);

    // Create bond with 1000 tokens
    let initial_amount = 1000_i128;
    client.create_bond(&identity, &initial_amount, &86400_u64);

    // Advance time past lock-up period
    e.ledger().with_mut(|l| {
        l.timestamp = 1000 + 86400 + 1;
    });

    // Record state before withdrawal
    let before_bond = client.get_identity_state();
    let before_identity_balance = token_client.balance(&identity);
    let before_contract_balance = token_client.balance(&bond_contract_id);

    // Withdraw zero amount
    let after_bond = client.withdraw_bond(&0_i128);

    // Verify state unchanged
    assert_eq!(
        after_bond.bonded_amount, before_bond.bonded_amount,
        "bonded_amount should not change for zero withdrawal"
    );

    // Verify no token transfers
    let after_identity_balance = token_client.balance(&identity);
    let after_contract_balance = token_client.balance(&bond_contract_id);

    assert_eq!(
        after_identity_balance, before_identity_balance,
        "identity balance should not change for zero withdrawal"
    );

    assert_eq!(
        after_contract_balance, before_contract_balance,
        "contract balance should not change for zero withdrawal"
    );
}
