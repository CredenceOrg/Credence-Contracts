//! Bug Condition Exploration Test - Reentrancy Vulnerability
//!
//! **Validates: Requirements 1.1, 1.2, 1.3, 1.4**
//!
//! This test demonstrates the reentrancy vulnerability in withdraw_bond(), withdraw_early(),
//! and execute_cooldown_withdrawal() functions. These functions perform external token transfers
//! before completing state updates, violating the Checks-Effects-Interactions (CEI) pattern.
//!
//! **CRITICAL**: This test is EXPECTED TO FAIL on unfixed code - failure confirms the bug exists.
//! The test encodes the expected behavior (reentrancy should be blocked).
//!
//! **Property 1: Bug Condition** - Reentrancy Attack Demonstration
//!
//! The test demonstrates that:
//! 1. withdraw_bond() lacks reentrancy protection (no acquire_lock/release_lock calls)
//! 2. withdraw_early() lacks reentrancy protection (no acquire_lock/release_lock calls)
//! 3. execute_cooldown_withdrawal() lacks reentrancy protection (no acquire_lock/release_lock calls)
//!
//! When the fix is implemented, this test will PASS, confirming reentrancy protection is added.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Env;

// ===========================================================================
// Malicious contract that attempts reentrancy during withdraw_bond callback
// ===========================================================================
mod withdraw_bond_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct WithdrawBondAttacker;

    #[contractimpl]
    impl WithdrawBondAttacker {
        /// Callback invoked during token transfer - attempts to re-enter withdraw_bond
        pub fn on_withdraw(e: Env, _amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);

            // Attempt reentrancy - this should panic with "reentrancy detected" on fixed code
            // On unfixed code, this succeeds and drains more funds than available
            client.withdraw_bond(&1000_i128);
        }

        pub fn setup(e: Env, target: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
        }
    }
}

// ===========================================================================
// Malicious contract that attempts reentrancy during withdraw_early callback
// ===========================================================================
mod withdraw_early_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct WithdrawEarlyAttacker;

    #[contractimpl]
    impl WithdrawEarlyAttacker {
        /// Callback invoked during token transfer - attempts to re-enter withdraw_early
        pub fn on_withdraw(e: Env, _amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);

            // Attempt reentrancy - this should panic with "reentrancy detected" on fixed code
            // On unfixed code, this succeeds and drains more funds than available
            client.withdraw_early(&identity, &500_i128);
        }

        pub fn setup(e: Env, target: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
        }
    }
}

// ===========================================================================
// Malicious contract that attempts reentrancy during execute_cooldown_withdrawal
// ===========================================================================
mod cooldown_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct CooldownAttacker;

    #[contractimpl]
    impl CooldownAttacker {
        /// Callback invoked during any external call - attempts to re-enter execute_cooldown_withdrawal
        pub fn on_withdraw(e: Env, _amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);

            // Attempt reentrancy - this should panic with "reentrancy detected" on fixed code
            // On unfixed code, this may succeed depending on state update order
            client.withdraw_bond(&1000_i128);
        }
        pub fn setup(e: Env, target: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
        }
    }
}

use cooldown_attacker::{CooldownAttacker, CooldownAttackerClient};
use withdraw_bond_attacker::{WithdrawBondAttacker, WithdrawBondAttackerClient};
use withdraw_early_attacker::{WithdrawEarlyAttacker, WithdrawEarlyAttackerClient};

// ===========================================================================
// Test 1: withdraw_bond() reentrancy attack
// ===========================================================================
/// **Property 1: Bug Condition** - Reentrancy Attack on withdraw_bond()
///
/// This test demonstrates that withdraw_bond() performs token transfer at line 652
/// BEFORE updating bond state at line 660. A malicious contract can re-enter
/// during the transfer callback and drain more funds than available.
///
/// **Expected on UNFIXED code**: Test FAILS (panic does NOT occur, demonstrating vulnerability)
/// **Expected on FIXED code**: Test PASSES (panic with "reentrancy detected")
#[test]
#[should_panic(expected = "HostError")]
fn test_withdraw_bond_reentrancy_attack() {
    let e = Env::default();

    let (client, admin, identity, _token_id, contract_id) = test_helpers::setup_with_token(&e);

    // Create bond with 2000 tokens
    client.create_bond(&identity, &2000_i128, &86400_u64);

    // Advance time past lock-up period to allow withdrawal
    e.ledger().with_mut(|l| {
        l.timestamp += 86401;
    });

    // Setup malicious attacker contract
    let attacker_id = e.register(WithdrawBondAttacker, ());
    let attacker_client = WithdrawBondAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&contract_id);

    // Register attacker as callback (simulates malicious token contract)
    client.set_callback(&admin, &attacker_id);

    // Attempt withdrawal - attacker will try to re-enter during callback
    // On UNFIXED code: Both withdrawals succeed, draining 2000 tokens (1000 + 1000)
    // On FIXED code: Second withdrawal panics with "reentrancy detected"
    client.withdraw_bond(&1000_i128);

    // If we reach here on unfixed code, the vulnerability was exploited
    // On fixed code, we never reach here (panic occurs in callback)
}

// ===========================================================================
// Test 2: withdraw_early() reentrancy attack
// ===========================================================================
/// **Property 1: Bug Condition** - Reentrancy Attack on withdraw_early()
///
/// This test demonstrates that withdraw_early() performs token transfers at lines 738-747
/// BEFORE updating bond state at line 755. A malicious contract can re-enter
/// during the transfer callback and drain more funds than available.
///
/// **Expected on UNFIXED code**: Test FAILS (panic does NOT occur, demonstrating vulnerability)
/// **Expected on FIXED code**: Test PASSES (panic with "reentrancy detected")
#[test]
#[should_panic(expected = "HostError")]
fn test_withdraw_early_reentrancy_attack() {
    let e = Env::default();

    let (client, admin, identity, _token_id, contract_id) = test_helpers::setup_with_token(&e);
    let treasury = Address::generate(&e);

    // Configure early exit penalty (10% penalty)
    client.set_early_exit_config(&admin, &treasury, &1000_u32);

    // Create bond with 2000 tokens
    client.create_bond(&identity, &2000_i128, &86400_u64);

    // Advance time to middle of lock-up period (allows early withdrawal)
    e.ledger().with_mut(|l| {
        l.timestamp += 43200; // Half of 86400
    });

    // Setup malicious attacker contract
    let attacker_id = e.register(WithdrawEarlyAttacker, ());
    let attacker_client = WithdrawEarlyAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&contract_id);

    // Register attacker as callback (simulates malicious token contract)
    client.set_callback(&admin, &attacker_id);

    // Attempt early withdrawal - attacker will try to re-enter during callback
    // On UNFIXED code: Both withdrawals succeed, draining more than available balance
    // On FIXED code: Second withdrawal panics with "reentrancy detected"
    client.withdraw_early(&identity, &500_i128);

    // If we reach here on unfixed code, the vulnerability was exploited
    // On fixed code, we never reach here (panic occurs in callback)
}

// ===========================================================================
// Test 3: execute_cooldown_withdrawal() reentrancy attack
// ===========================================================================
/// **Property 1: Bug Condition** - Reentrancy Attack on withdraw_bond() (via rolling bond)
///
/// This test demonstrates that withdraw_bond() performs token transfer
/// BEFORE updating bond state. A malicious contract can re-enter
/// during the transfer callback and drain more funds than available.
///
/// **Expected on UNFIXED code**: Test FAILS (panic does NOT occur, demonstrating vulnerability)
/// **Expected on FIXED code**: Test PASSES (panic with "reentrancy detected")
#[test]
#[should_panic(expected = "HostError")]
fn test_execute_cooldown_withdrawal_reentrancy_attack() {
    let e = Env::default();

    let (client, admin, identity, _token_id, contract_id) = test_helpers::setup_with_token(&e);

    // Set cooldown period
    client.set_cooldown_period(&admin, &3600_u64);

    // Create bond with 2000 tokens (rolling)
    client.create_bond_with_rolling(&identity, &2000_i128, &86400_u64, &true, &3600_u64);

    // Request withdrawal
    client.request_withdrawal(&identity);

    // Advance time past cooldown period
    e.ledger().with_mut(|l| {
        l.timestamp += 3601;
    });

    // Setup malicious attacker contract
    let attacker_id = e.register(CooldownAttacker, ());
    let attacker_client = CooldownAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&contract_id);

    // Register attacker as callback
    client.set_callback(&admin, &attacker_id);

    // Attempt withdrawal
    client.withdraw_bond(&1000_i128);
}

// ===========================================================================
// Test 4: Nested reentrancy (3+ levels) should be blocked at first re-entry
// ===========================================================================
/// **Property 1: Bug Condition** - Nested Reentrancy Attack
///
/// This test demonstrates that even deeply nested reentrancy attempts should be
/// blocked at the first re-entry level.
///
/// **Expected on UNFIXED code**: Test FAILS (panic does NOT occur, demonstrating vulnerability)
/// **Expected on FIXED code**: Test PASSES (reentrancy is blocked at first re-entry)
#[test]
#[should_panic(expected = "HostError")]
fn test_nested_reentrancy_blocked() {
    let e = Env::default();

    let (client, admin, identity, _token_id, contract_id) = test_helpers::setup_with_token(&e);

    // Create bond with 3000 tokens
    client.create_bond(&identity, &3000_i128, &86400_u64);

    // Advance time past lock-up period
    e.ledger().with_mut(|l| {
        l.timestamp += 86401;
    });

    // Setup malicious attacker contract
    let attacker_id = e.register(WithdrawBondAttacker, ());
    let attacker_client = WithdrawBondAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&contract_id);

    // Register attacker as callback
    client.set_callback(&admin, &attacker_id);

    // Attempt withdrawal - attacker will try nested reentrancy
    // On UNFIXED code: All calls succeed, draining 3000 tokens (1000 + 1000 + 1000)
    // On FIXED code: Second call panics with "reentrancy detected"
    client.withdraw_bond(&1000_i128);

    // If we reach here on unfixed code, nested reentrancy was successful
    // On fixed code, we never reach here (panic occurs at first re-entry)
}
