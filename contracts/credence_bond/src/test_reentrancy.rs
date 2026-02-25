#![cfg(test)]
//! Security tests for reentrancy protection in the Credence Bond contract.
//!
//! These tests verify that:
//! - Reentrancy in `withdraw_bond` is blocked
//! - Reentrancy in `slash_bond` is blocked
//! - Reentrancy in `collect_fees` is blocked
//! - State locks are correctly acquired and released
//! - Normal (non-reentrant) operations succeed
//! - Sequential operations work after lock release

use super::*;
use crate::test_helpers;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Env;

// ---------------------------------------------------------------------------
// Each attacker contract lives in its own submodule to avoid Soroban macro
// name collisions (the #[contractimpl] macro generates module-level symbols
// for each function name).
// ---------------------------------------------------------------------------

mod withdraw_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct WithdrawAttacker;

    #[contractimpl]
    impl WithdrawAttacker {
        pub fn on_withdraw(e: Env, _amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let victim_identity: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "identity"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);
            client.withdraw_bond_full(&victim_identity);
        }

        pub fn setup(e: Env, target: Address, identity: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "identity"), &identity);
        }
    }
}

mod slash_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct SlashAttacker;

    #[contractimpl]
    impl SlashAttacker {
        pub fn on_slash(e: Env, _amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let admin: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "admin"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);
            client.slash_bond(&admin, &100_i128);
        }

        pub fn setup(e: Env, target: Address, admin: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "admin"), &admin);
        }
    }
}

mod fee_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct FeeAttacker;

    #[contractimpl]
    impl FeeAttacker {
        pub fn on_collect(e: Env, _amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let admin: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "admin"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);
            client.collect_fees(&admin);
        }

        pub fn setup(e: Env, target: Address, admin: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "admin"), &admin);
        }
    }
}

mod benign_callback {
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    pub struct BenignCallback;

    #[contractimpl]
    impl BenignCallback {
        pub fn on_withdraw(_e: Env, _amount: i128) {}
        pub fn on_slash(_e: Env, _amount: i128) {}
        pub fn on_collect(_e: Env, _amount: i128) {}
    }
}

mod cross_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    /// Attacker that tries to call `slash_bond` from inside `on_withdraw`.
    #[contract]
    pub struct CrossAttacker;

    #[contractimpl]
    impl CrossAttacker {
        pub fn on_withdraw(e: Env, _amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let admin: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "admin"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);
            client.slash_bond(&admin, &100_i128);
        }

        pub fn setup(e: Env, target: Address, admin: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "admin"), &admin);
        }
    }
}

use benign_callback::BenignCallback;
use cross_attacker::{CrossAttacker, CrossAttackerClient};
use fee_attacker::{FeeAttacker, FeeAttackerClient};
use slash_attacker::{SlashAttacker, SlashAttackerClient};
use withdraw_attacker::{WithdrawAttacker, WithdrawAttackerClient};

// ---------------------------------------------------------------------------
// Helper: set up a bond contract with admin, identity, and a bond.
// ---------------------------------------------------------------------------
fn setup_bond(e: &Env) -> (Address, Address, Address) {
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);

    let admin = Address::generate(e);
    let identity = Address::generate(e);

    client.initialize(&admin);
    client.create_bond(&identity, &10_000_i128, &86400_u64);

    (contract_id, admin, identity)
    let (client, admin, identity, _token_id, bond_id) = test_helpers::setup_with_token(e);
    client.create_bond(&identity, &10_000_i128, &86400_u64, &false, &0_u64);
    (bond_id, admin, identity)
}

// ===========================================================================
// 1. Reentrancy in withdrawal — MUST be blocked
// ===========================================================================
#[test]
#[should_panic(expected = "HostError")]
fn test_withdraw_reentrancy_blocked() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, _admin, identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let attacker_id = e.register(WithdrawAttacker, ());
    let attacker_client = WithdrawAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&bond_id, &identity);
    client.set_callback(&attacker_id);

    client.withdraw_bond_full(&identity);
}

// ===========================================================================
// 2. Reentrancy in slashing — SHOULD be blocked
// ===========================================================================
#[test]
#[should_panic(expected = "HostError")]
fn test_slash_reentrancy_blocked() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let attacker_id = e.register(SlashAttacker, ());
    let attacker_client = SlashAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&bond_id, &admin);
    client.set_callback(&attacker_id);

    client.slash_bond(&admin, &500_i128);
}

// ===========================================================================
// 3. Reentrancy in fee collection — MUST be blocked
// ===========================================================================
#[test]
#[should_panic(expected = "HostError")]
fn test_fee_collection_reentrancy_blocked() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    client.deposit_fees(&500_i128);

    let attacker_id = e.register(FeeAttacker, ());
    let attacker_client = FeeAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&bond_id, &admin);
    client.set_callback(&attacker_id);

    client.collect_fees(&admin);
}

// ===========================================================================
// 4. State lock is NOT held before any guarded call
// ===========================================================================
#[test]
fn test_lock_not_held_initially() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, _admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    assert!(!client.is_locked());
}

// ===========================================================================
// 5. State lock is released after successful withdrawal
// ===========================================================================
#[test]
fn test_lock_released_after_withdraw() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, _admin, identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let benign_id = e.register(BenignCallback, ());
    client.set_callback(&benign_id);

    client.withdraw_bond_full(&identity);
    assert!(!client.is_locked());
}

// ===========================================================================
// 6. State lock is released after successful slash
// ===========================================================================
#[test]
fn test_lock_released_after_slash() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let benign_id = e.register(BenignCallback, ());
    client.set_callback(&benign_id);

    client.slash_bond(&admin, &100_i128);
    assert!(!client.is_locked());
}

// ===========================================================================
// 7. State lock is released after successful fee collection
// ===========================================================================
#[test]
fn test_lock_released_after_fee_collection() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    client.deposit_fees(&200_i128);

    let benign_id = e.register(BenignCallback, ());
    client.set_callback(&benign_id);

    let collected = client.collect_fees(&admin);
    assert_eq!(collected, 200_i128);
    assert!(!client.is_locked());
}

// ===========================================================================
// 8. Normal withdrawal succeeds (happy path)
// ===========================================================================
#[test]
fn test_normal_withdraw_succeeds() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, _admin, identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let amount = client.withdraw_bond_full(&identity);
    assert_eq!(amount, 10_000_i128);

    let state = client.get_identity_state();
    assert!(!state.active);
    assert_eq!(state.bonded_amount, 0);
}

// ===========================================================================
// 9. Normal slash succeeds (happy path)
// ===========================================================================
#[test]
fn test_normal_slash_succeeds() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let slashed = client.slash_bond(&admin, &3_000_i128);
    assert_eq!(slashed, 3_000_i128);

    let state = client.get_identity_state();
    assert_eq!(state.slashed_amount, 3_000_i128);
    assert!(state.active);
}

// ===========================================================================
// 10. Normal fee collection succeeds (happy path)
// ===========================================================================
#[test]
fn test_normal_fee_collection_succeeds() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    client.deposit_fees(&750_i128);
    let collected = client.collect_fees(&admin);
    assert_eq!(collected, 750_i128);
}

// ===========================================================================
// 11. Sequential operations succeed (lock is properly released between calls)
// ===========================================================================
#[test]
fn test_sequential_operations_succeed() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    client.slash_bond(&admin, &1_000_i128);
    assert!(!client.is_locked());

    client.deposit_fees(&100_i128);
    let fees = client.collect_fees(&admin);
    assert_eq!(fees, 100_i128);
    assert!(!client.is_locked());

    let withdrawn = client.withdraw_bond_full(&identity);
    assert_eq!(withdrawn, 9_000_i128);
    assert!(!client.is_locked());
}

// ===========================================================================
// 12. Slash exceeding bond is rejected
// ===========================================================================
#[test]
#[should_panic(expected = "slash exceeds bond")]
fn test_slash_exceeds_bond_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    client.slash_bond(&admin, &20_000_i128);
}

// ===========================================================================
// 13. Withdraw by non-owner is rejected
// ===========================================================================
#[test]
#[should_panic(expected = "not bond owner")]
fn test_withdraw_non_owner_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, _admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let stranger = Address::generate(&e);
    client.withdraw_bond_full(&stranger);
}

// ===========================================================================
// 14. Double withdrawal is rejected (bond inactive after first)
// ===========================================================================
#[test]
#[should_panic(expected = "bond not active")]
fn test_double_withdraw_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, _admin, identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    client.withdraw_bond_full(&identity);
    client.withdraw_bond_full(&identity);
}

// ===========================================================================
// 15. Cross-function reentrancy: attacker tries slash during withdraw
// ===========================================================================
#[test]
#[should_panic(expected = "HostError")]
fn test_cross_function_reentrancy_blocked() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let attacker_id = e.register(CrossAttacker, ());
    let attacker_client = CrossAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&bond_id, &admin);
    client.set_callback(&attacker_id);

    client.withdraw_bond_full(&identity);
}
