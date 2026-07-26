//! Tests that exercise the reusable [`crate::test_invariants`] library.
//!
//! Every state-changing contract call is followed by
//! [`assert_all_invariants`], demonstrating the intended usage pattern and
//! providing the 20+ call sites required by the task. Edge cases are covered for
//! the post-slash, post-renew, and post-withdraw-request states.

#![cfg(test)]

use crate::test_invariants::{
    assert_all_invariants, assert_bond_invariants, assert_notice_period_bounded,
    assert_slashed_within_bonded, assert_withdrawal_request_requires_rolling, load_bond,
};
use crate::{CredenceBond, CredenceBondClient, IdentityBond};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String};
use proptest::prelude::*;

struct Ctx<'a> {
    env: Env,
    client: CredenceBondClient<'a>,
    contract: Address,
    admin: Address,
    identity: Address,
}

fn setup() -> Ctx<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract);
    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    client.initialize(&admin, &None);
    Ctx {
        env,
        client,
        contract,
        admin,
        identity,
    }
}

fn advance(env: &Env, by: u64) {
    let mut info = env.ledger().get();
    info.timestamp = info.timestamp.saturating_add(by);
    info.sequence_number = info.sequence_number.saturating_add(1);
    env.ledger().set(info);
}

// ---------------------------------------------------------------------------
// Happy-path lifecycle: invariants asserted after EVERY mutating call.
// ---------------------------------------------------------------------------

#[test]
fn invariants_hold_through_full_lifecycle() {
    let ctx = setup();

    // Site 1: after create
    ctx.client
        .create_bond(&ctx.identity, &10_000, &1_000, &false, &0);
    assert_all_invariants(&ctx.env, &ctx.contract);

    // Site 2: after top_up
    ctx.client.top_up(&identity, &5_000);
    assert_all_invariants(&ctx.env, &ctx.contract);

    // Site 3: after extend_duration
    ctx.client.extend_duration(&identity, &500);
    assert_all_invariants(&ctx.env, &ctx.contract);

    // Site 4: after second top_up
    ctx.client.top_up(&identity, &1);
    assert_all_invariants(&ctx.env, &ctx.contract);

    // Site 5: after slash
    advance(&ctx.env, 10);
    ctx.client.slash(&ctx.admin, &2_000);
    assert_all_invariants(&ctx.env, &ctx.contract);

    // Site 6: after withdraw (post lock-up)
    advance(&ctx.env, 5_000);
    ctx.client.withdraw(&identity, &100);
    assert_all_invariants(&ctx.env, &ctx.contract);
}

// ---------------------------------------------------------------------------
// Edge case: post-slash invariants.
// ---------------------------------------------------------------------------

#[test]
fn invariants_hold_after_slash_to_full_amount() {
    let ctx = setup();
    ctx.client
        .create_bond(&ctx.identity, &1_000, &1_000, &false, &0);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 7

    advance(&ctx.env, 1);
    // Slash the entire bonded amount: slashed_amount == bonded_amount (boundary of I2).
    ctx.client.slash(&ctx.admin, &1_000);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 8

    let bond = load_bond(&ctx.env, &ctx.contract);
    assert_eq!(bond.slashed_amount, 1_000);
    assert_slashed_within_bonded(&bond); // Site 9 (explicit I2)
}

#[test]
fn invariants_hold_after_incremental_slashes() {
    let ctx = setup();
    ctx.client
        .create_bond(&ctx.identity, &900, &1_000, &false, &0);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 10

    advance(&ctx.env, 1);
    ctx.client.slash(&ctx.admin, &300);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 11

    ctx.client.slash(&ctx.admin, &300);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 12

    ctx.client.slash(&ctx.admin, &300);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 13
}

// ---------------------------------------------------------------------------
// Edge case: post-withdraw-request invariants (rolling bonds).
// ---------------------------------------------------------------------------

#[test]
fn invariants_hold_after_withdraw_request() {
    let ctx = setup();
    ctx.client
        .create_bond(&ctx.identity, &5_000, &1_000, &true, &100);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 14

    advance(&ctx.env, 50); // ensure a non-zero "requested at" timestamp
    ctx.client.request_withdrawal(&identity);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 15

    let bond = load_bond(&ctx.env, &ctx.contract);
    assert!(bond.withdrawal_requested_at != 0);
    // I3 must hold: request only allowed because is_rolling == true.
    assert_withdrawal_request_requires_rolling(&bond); // Site 16
}

#[test]
fn invariants_hold_after_withdraw_request_then_slash() {
    let ctx = setup();
    ctx.client
        .create_bond(&ctx.identity, &5_000, &1_000, &true, &100);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 17

    advance(&ctx.env, 50);
    ctx.client.request_withdrawal(&identity);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 18

    advance(&ctx.env, 1);
    ctx.client.slash(&ctx.admin, &1_000);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 19
}

// ---------------------------------------------------------------------------
// Edge case: post-renew invariants (rolling bonds).
// ---------------------------------------------------------------------------

#[test]
fn invariants_hold_after_renew() {
    let ctx = setup();
    ctx.client
        .create_bond(&ctx.identity, &5_000, &1_000, &true, &100);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 20

    // Advance past the period end so renew_if_rolling renews the bond.
    advance(&ctx.env, 2_000);
    ctx.client.renew_if_rolling(&identity);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 21

    let bond = load_bond(&ctx.env, &ctx.contract);
    // After renewal withdrawal_requested_at is reset and the bond is still rolling.
    assert_eq!(bond.withdrawal_requested_at, 0);
    assert!(bond.is_rolling);
    assert_notice_period_bounded(&bond); // Site 22 (explicit I6)
}

#[test]
fn invariants_hold_after_request_then_renew_is_noop() {
    let ctx = setup();
    ctx.client
        .create_bond(&ctx.identity, &5_000, &1_000, &true, &100);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 23

    advance(&ctx.env, 50);
    ctx.client.request_withdrawal(&identity);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 24

    // With a pending request, renew_if_rolling is a no-op; invariants still hold.
    advance(&ctx.env, 2_000);
    ctx.client.renew_if_rolling(&identity);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 25
}

// ---------------------------------------------------------------------------
// Attestation invariants (I1, I7).
// ---------------------------------------------------------------------------

#[test]
fn invariants_hold_after_attestations() {
    let ctx = setup();
    ctx.client
        .create_bond(&ctx.identity, &5_000, &1_000, &false, &0);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 26

    let attester = Address::generate(&ctx.env);
    ctx.client.register_attester(&attester);
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 27

    ctx.client.add_attestation(
        &attester,
        &ctx.identity,
        &String::from_str(&ctx.env, "kyc-passed"),
        &0,
    );
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 28

    ctx.client.add_attestation(
        &attester,
        &ctx.identity,
        &String::from_str(&ctx.env, "kyc-renewed"),
        &1,
    );
    assert_all_invariants(&ctx.env, &ctx.contract); // Site 29
}

// ---------------------------------------------------------------------------
// Pure-state unit tests of individual invariant helpers.
// ---------------------------------------------------------------------------

fn sample_bond(env: &Env) -> IdentityBond {
    IdentityBond {
        identity: Address::generate(env),
        bonded_amount: 1_000,
        bond_start: 0,
        bond_duration: 1_000,
        slashed_amount: 0,
        active: true,
        is_rolling: false,
        withdrawal_requested_at: 0,
        notice_period_duration: 0,
    }
}

#[test]
fn bond_invariants_pass_for_well_formed_bond() {
    let env = Env::default();
    let bond = sample_bond(&env);
    assert_bond_invariants(&bond); // Site 30
}

#[test]
#[should_panic(expected = "INVARIANT I2 VIOLATED")]
fn slashed_over_bonded_is_detected() {
    let env = Env::default();
    let mut bond = sample_bond(&env);
    bond.slashed_amount = bond.bonded_amount + 1;
    assert_slashed_within_bonded(&bond);
}

#[test]
#[should_panic(expected = "INVARIANT I3 VIOLATED")]
fn withdrawal_request_on_non_rolling_is_detected() {
    let env = Env::default();
    let mut bond = sample_bond(&env);
    bond.is_rolling = false;
    bond.withdrawal_requested_at = 42;
    assert_withdrawal_request_requires_rolling(&bond);
}

#[test]
#[should_panic(expected = "INVARIANT I6 VIOLATED")]
fn oversized_notice_period_is_detected() {
    let env = Env::default();
    let mut bond = sample_bond(&env);
    bond.is_rolling = true;
    bond.notice_period_duration = bond.bond_duration + 1;
    assert_notice_period_bounded(&bond);
}

#[derive(Clone, Debug)]
enum Action {
    Deposit(i128),
    Withdraw(i128),
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        // Happy paths
        (1..10_000_i128).prop_map(Action::Deposit),
        (1..10_000_i128).prop_map(Action::Withdraw),
        // Sad paths (negative, zero)
        (-1000..=0_i128).prop_map(Action::Deposit),
        (-1000..=0_i128).prop_map(Action::Withdraw),
        // Overflow paths
        (i128::MAX - 1000..=i128::MAX).prop_map(Action::Deposit),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_random_deposit_withdraw_invariants(actions in proptest::collection::vec(action_strategy(), 1..20)) {
        let ctx = setup();
        
        // Initial bond creation so we can deposit/withdraw
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ctx.client.create_bond(&ctx.identity, &1000, &86400, &false, &0);
        }));
        
        assert_all_invariants(&ctx.env, &ctx.contract);
        
        for action in actions {
            // Advance ledger to allow withdrawals (since bond_duration is 86400)
            advance(&ctx.env, 100_000);
            
            match action {
                Action::Deposit(amount) => {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        ctx.client.top_up(&ctx.identity, &amount);
                    }));
                }
                Action::Withdraw(amount) => {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        ctx.client.withdraw(&ctx.identity, &amount);
                    }));
                }
            }
            
            // The invariant must hold regardless of whether the action succeeded or panicked
            assert_all_invariants(&ctx.env, &ctx.contract);
        }
    }
}

