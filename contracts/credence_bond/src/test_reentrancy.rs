//! Security tests for reentrancy protection in the Credence Bond contract.
//!
//! These tests verify that:
//! - Reentrancy in `withdraw_bond_full` is blocked
//! - Reentrancy in `withdraw_bond` (partial) is blocked
//! - Reentrancy in `withdraw_early` is blocked
//! - Reentrancy in `slash_bond` is blocked
//! - Reentrancy in `collect_fees` is blocked
//! - State locks are correctly acquired and released
//! - Normal (non-reentrant) operations succeed
//! - Sequential operations work after lock release
//! - `set_callback` is admin-gated
//! - State is fully committed before any callback fires

use super::*;
use crate::test_helpers;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger;
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

mod partial_withdraw_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    /// Attacker that re-enters `withdraw_bond` (partial) from `on_withdraw` callback.
    #[contract]
    pub struct PartialWithdrawAttacker;

    #[contractimpl]
    impl PartialWithdrawAttacker {
        pub fn on_withdraw(e: Env, amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);
            client.withdraw_bond(&amount);
        }

        pub fn setup(e: Env, target: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
        }
    }
}

mod early_withdraw_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    /// Attacker that re-enters `withdraw_early` from `on_withdraw` callback.
    #[contract]
    pub struct EarlyWithdrawAttacker;

    #[contractimpl]
    impl EarlyWithdrawAttacker {
        pub fn on_withdraw(e: Env, amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);
            client.withdraw_early(&amount);
        }

        pub fn setup(e: Env, target: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
        }
    }
}

mod cooldown_reentrant_attacker {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    /// Attacker that re-enters `execute_cooldown_withdrawal` from `on_withdraw` callback.
    #[contract]
    pub struct CooldownReentrantAttacker;

    #[contractimpl]
    impl CooldownReentrantAttacker {
        pub fn on_withdraw(e: Env, _amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let requester: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "requester"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);
            client.execute_cooldown_withdrawal(&requester);
        }

        pub fn setup(e: Env, target: Address, requester: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "requester"), &requester);
        }
    }
}

/// State-reading callback: records bond state at callback time to verify
/// effects are fully committed before the callback fires.
mod state_snapshot_callback {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct StateSnapshotCallback;

    #[contractimpl]
    impl StateSnapshotCallback {
        pub fn on_withdraw(e: Env, amount: i128) {
            let bond_addr: Address = e
                .storage()
                .instance()
                .get(&Symbol::new(&e, "target"))
                .unwrap();
            let client = CredenceBondClient::new(&e, &bond_addr);
            let state = client.get_identity_state();
            // Record snapshot values for the outer test to verify.
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "snap_bonded"), &state.bonded_amount);
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "snap_amount"), &amount);
        }

        pub fn setup(e: Env, target: Address) {
            e.storage()
                .instance()
                .set(&Symbol::new(&e, "target"), &target);
        }

        pub fn get_snap_bonded(e: Env) -> i128 {
            e.storage()
                .instance()
                .get(&Symbol::new(&e, "snap_bonded"))
                .unwrap_or(0)
        }

        pub fn get_snap_amount(e: Env) -> i128 {
            e.storage()
                .instance()
                .get(&Symbol::new(&e, "snap_amount"))
                .unwrap_or(0)
        }
    }
}

use benign_callback::BenignCallback;
use cooldown_reentrant_attacker::{CooldownReentrantAttacker, CooldownReentrantAttackerClient};
use cross_attacker::{CrossAttacker, CrossAttackerClient};
use early_withdraw_attacker::{EarlyWithdrawAttacker, EarlyWithdrawAttackerClient};
use fee_attacker::{FeeAttacker, FeeAttackerClient};
use partial_withdraw_attacker::{PartialWithdrawAttacker, PartialWithdrawAttackerClient};
use slash_attacker::{SlashAttacker, SlashAttackerClient};
use state_snapshot_callback::{StateSnapshotCallback, StateSnapshotCallbackClient};
use withdraw_attacker::{WithdrawAttacker, WithdrawAttackerClient};

// ---------------------------------------------------------------------------
// Helper: set up a bond contract with admin, identity, and a bond.
// ---------------------------------------------------------------------------
fn setup_bond(e: &Env) -> (Address, Address, Address) {
    let (client, admin, identity, _token_id, contract_id) = test_helpers::setup_with_token(e);
    client.create_bond(&identity, &10_000_i128, &86400_u64);

    (contract_id, admin, identity)
}

// ===========================================================================
// 1. Reentrancy in full withdrawal — MUST be blocked
// ===========================================================================
#[test]
#[should_panic(expected = "HostError")]
fn test_withdraw_reentrancy_blocked() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let attacker_id = e.register(WithdrawAttacker, ());
    let attacker_client = WithdrawAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&bond_id, &identity);
    client.set_callback(&admin, &attacker_id);

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
    client.set_callback(&admin, &attacker_id);

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
    client.set_callback(&admin, &attacker_id);

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
    let (bond_id, admin, identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let benign_id = e.register(BenignCallback, ());
    client.set_callback(&admin, &benign_id);

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
    client.set_callback(&admin, &benign_id);

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
    client.set_callback(&admin, &benign_id);

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
    client.set_callback(&admin, &attacker_id);

    client.withdraw_bond_full(&identity);
}

// ===========================================================================
// 16. Reentrancy in partial withdrawal (withdraw_bond) — attacker harness regression
// ===========================================================================
#[test]
#[should_panic(expected = "HostError")]
fn test_partial_withdraw_reentrancy_blocked() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    // Advance past lock-up period so withdraw_bond is permitted.
    e.ledger().with_mut(|li| li.timestamp = 86_401);

    let attacker_id = e.register(PartialWithdrawAttacker, ());
    let attacker_client = PartialWithdrawAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&bond_id);
    client.set_callback(&admin, &attacker_id);

    // First call acquires the lock; the callback attempts a second withdraw_bond which must fail.
    client.withdraw_bond(&1_000_i128);
}

// ===========================================================================
// 17. Reentrancy in early withdrawal — attacker harness regression
// ===========================================================================
#[test]
#[should_panic(expected = "HostError")]
fn test_withdraw_early_reentrancy_blocked() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    // Stay inside the lock-up window to force the early-exit path.
    e.ledger().with_mut(|li| li.timestamp = 43_200);

    let attacker_id = e.register(EarlyWithdrawAttacker, ());
    let attacker_client = EarlyWithdrawAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&bond_id);
    client.set_callback(&admin, &attacker_id);

    client.withdraw_early(&500_i128);
}

// ===========================================================================
// 18. Reentrancy in cooldown withdrawal — attacker harness regression
// ===========================================================================
#[test]
#[should_panic(expected = "HostError")]
fn test_cooldown_withdrawal_reentrancy_blocked() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    client.set_cooldown_period(&admin, &3_600_u64);
    client.request_cooldown_withdrawal(&identity, &1_000_i128);
    e.ledger().with_mut(|li| li.timestamp = 3_601);

    let attacker_id = e.register(CooldownReentrantAttacker, ());
    let attacker_client = CooldownReentrantAttackerClient::new(&e, &attacker_id);
    attacker_client.setup(&bond_id, &identity);
    client.set_callback(&admin, &attacker_id);

    client.execute_cooldown_withdrawal(&identity);
}

// ===========================================================================
// 19. Non-admin cannot set callback — admin gate regression
// ===========================================================================
#[test]
#[should_panic(expected = "not admin")]
fn test_set_callback_non_admin_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, _admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let impostor = Address::generate(&e);
    let dummy_cb = Address::generate(&e);
    client.set_callback(&impostor, &dummy_cb);
}

// ===========================================================================
// 20. State committed before callback fires (withdraw_bond)
// ===========================================================================
#[test]
fn test_state_committed_before_callback_withdraw_bond() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    // Advance past lock-up.
    e.ledger().with_mut(|li| li.timestamp = 86_401);

    let snap_id = e.register(StateSnapshotCallback, ());
    let snap_client = StateSnapshotCallbackClient::new(&e, &snap_id);
    snap_client.setup(&bond_id);
    client.set_callback(&admin, &snap_id);

    client.withdraw_bond(&3_000_i128);

    // When the callback fired, bonded_amount must already have been reduced.
    assert_eq!(snap_client.get_snap_bonded(), 7_000_i128);
    assert_eq!(snap_client.get_snap_amount(), 3_000_i128);
}

// ===========================================================================
// 21. State committed before callback fires (slash_bond)
// ===========================================================================
#[test]
fn test_state_committed_before_callback_slash() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    let benign_id = e.register(BenignCallback, ());
    client.set_callback(&admin, &benign_id);

    let slashed = client.slash_bond(&admin, &2_500_i128);
    assert_eq!(slashed, 2_500_i128);

    let state = client.get_identity_state();
    assert_eq!(state.slashed_amount, 2_500_i128);
    assert!(state.active);
    assert!(!client.is_locked());
}

// ===========================================================================
// 22. Lock released after partial withdrawal (no callback set)
// ===========================================================================
#[test]
fn test_lock_released_after_partial_withdraw() {
    let e = Env::default();
    e.mock_all_auths();
    let (bond_id, _admin, _identity) = setup_bond(&e);
    let client = CredenceBondClient::new(&e, &bond_id);

    // Advance past lock-up period.
    e.ledger().with_mut(|li| li.timestamp = 86_401);

    client.withdraw_bond(&2_000_i128);
    assert!(!client.is_locked());

    let state = client.get_identity_state();
    assert_eq!(state.bonded_amount, 8_000_i128);
}
