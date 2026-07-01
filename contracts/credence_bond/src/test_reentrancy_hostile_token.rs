//! Hostile-token fault injection for guarded fund-moving bond paths.
//!
//! These tests install `ChaosToken` as the bond token, arm its transfer hook,
//! and make the token attempt to re-enter the bond contract while the outer
//! operation is mid-transfer.

use crate::chaos_token::{ChaosToken, ChaosTokenClient};
use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{Address, Env, Symbol, Vec};

const INITIAL_BOND: i128 = 10_000;
const OUTER_AMOUNT: i128 = 1_000;
const REENTER_AMOUNT: i128 = 100;
const DURATION: u64 = 86_400;

struct HostileSetup<'a> {
    client: CredenceBondClient<'a>,
    admin: Address,
    identity: Address,
    contract_id: Address,
    token: ChaosTokenClient<'a>,
}

fn setup_hostile_token_bond(e: &Env) -> HostileSetup<'_> {
    e.mock_all_auths();
    e.ledger().with_mut(|li| {
        li.timestamp = 1_000;
    });

    let (client, admin, identity, _old_token, contract_id) = test_helpers::setup_with_token(e);
    let token_id = e.register(ChaosToken, ());
    let token = ChaosTokenClient::new(e, &token_id);
    token.initialize();
    token.mint(&identity, &100_000);

    let mut accepted = Vec::new(e);
    accepted.push_back(token_id.clone());
    client.set_accepted_tokens(&admin, &accepted);
    client.set_token(&admin, &token_id);

    client.create_bond(&identity, &INITIAL_BOND, &DURATION);
    assert_invariants(e, &contract_id);

    HostileSetup {
        client,
        admin,
        identity,
        contract_id,
        token,
    }
}

fn assert_invariants(e: &Env, contract_id: &Address) {
    e.as_contract(contract_id, || {
        crate::invariants::assert_self_consistent(e);
    });
    crate::test_invariants::assert_all_invariants(e, contract_id);
}

fn arm_attack(
    e: &Env,
    setup: &HostileSetup<'_>,
    method: &str,
    reentry_amount: i128,
) -> (i128, i128, i128) {
    let before_identity = setup.token.balance(&setup.identity);
    let before_contract = setup.token.balance(&setup.contract_id);
    let before_treasury = setup.token.balance(&setup.admin);

    setup.token.set_reentry_attack(
        &setup.contract_id,
        &Symbol::new(e, method),
        &setup.identity,
        &setup.admin,
        &reentry_amount,
    );

    (before_identity, before_contract, before_treasury)
}

fn assert_attack_rejected(setup: &HostileSetup<'_>) {
    assert!(setup.token.attack_attempted(), "hostile token did not re-enter");
    assert!(
        setup.token.attack_rejected(),
        "reentrant bond call was not rejected"
    );
    assert!(!setup.client.is_locked(), "reentrancy lock was left stuck");
}

/// Attack vector: a withdrawal payout token transfer re-enters `withdraw`.
#[test]
fn hostile_token_reentry_into_withdraw_is_rejected() {
    let e = Env::default();
    let setup = setup_hostile_token_bond(&e);
    setup.client.set_slash_treasury(&setup.admin, &setup.admin);
    e.ledger().with_mut(|li| {
        li.timestamp = 1_000 + DURATION + 1;
    });
    test_helpers::advance_ledger_sequence(&e);

    let (before_identity, before_contract, _) =
        arm_attack(&e, &setup, "withdraw", REENTER_AMOUNT);
    setup.client.slash(&setup.admin, &OUTER_AMOUNT);

    assert_attack_rejected(&setup);
    assert_invariants(&e, &setup.contract_id);
    assert_eq!(setup.token.balance(&setup.identity), before_identity);
    assert_eq!(
        setup.token.balance(&setup.contract_id),
        before_contract - OUTER_AMOUNT
    );
}

/// Attack vector: an early-exit treasury/user payout re-enters `withdraw_early`.
#[test]
fn hostile_token_reentry_into_withdraw_early_is_rejected() {
    let e = Env::default();
    let setup = setup_hostile_token_bond(&e);
    setup
        .client
        .set_early_exit_config(&setup.admin, &setup.admin, &1_000_u32);

    let (before_identity, before_contract, _) =
        arm_attack(&e, &setup, "withdraw_early", REENTER_AMOUNT);
    setup.client.withdraw_early(&setup.identity, &OUTER_AMOUNT);

    assert_attack_rejected(&setup);
    assert_invariants(&e, &setup.contract_id);
    assert!(
        setup.token.balance(&setup.identity) > before_identity,
        "identity should receive the outer net early-exit payout"
    );
    assert_eq!(
        setup.token.balance(&setup.contract_id),
        before_contract - OUTER_AMOUNT
    );
}

/// Attack vector: a slash-to-treasury token transfer re-enters `slash`.
#[test]
fn hostile_token_reentry_into_slash_is_rejected() {
    let e = Env::default();
    let setup = setup_hostile_token_bond(&e);
    setup.client.set_slash_treasury(&setup.admin, &setup.admin);
    test_helpers::advance_ledger_sequence(&e);

    let (_, before_contract, before_treasury) =
        arm_attack(&e, &setup, "slash", REENTER_AMOUNT);
    setup.client.slash(&setup.admin, &OUTER_AMOUNT);

    assert_attack_rejected(&setup);
    assert_invariants(&e, &setup.contract_id);
    let bond = setup.client.get_identity_state();
    assert_eq!(bond.slashed_amount, OUTER_AMOUNT);
    assert_eq!(
        setup.token.balance(&setup.contract_id),
        before_contract - OUTER_AMOUNT
    );
    assert_eq!(
        setup.token.balance(&setup.admin),
        before_treasury + OUTER_AMOUNT
    );
}

/// Attack vector: an allowance-based top-up pull re-enters `top_up`.
#[test]
fn hostile_token_reentry_into_top_up_is_rejected() {
    let e = Env::default();
    let setup = setup_hostile_token_bond(&e);

    let (before_identity, before_contract, _) =
        arm_attack(&e, &setup, "top_up", REENTER_AMOUNT);
    setup.client.top_up(&setup.identity, &OUTER_AMOUNT);

    assert_attack_rejected(&setup);
    assert_invariants(&e, &setup.contract_id);
    let bond = setup.client.get_identity_state();
    assert_eq!(bond.bonded_amount, INITIAL_BOND + OUTER_AMOUNT);
    assert_eq!(
        setup.token.balance(&setup.identity),
        before_identity - OUTER_AMOUNT
    );
    assert_eq!(
        setup.token.balance(&setup.contract_id),
        before_contract + OUTER_AMOUNT
    );
}

/// Attack vector: a withdrawal payout token transfer re-enters `collect_fees`.
#[test]
fn hostile_token_reentry_into_collect_fees_is_rejected() {
    let e = Env::default();
    let setup = setup_hostile_token_bond(&e);
    setup
        .client
        .set_early_exit_config(&setup.admin, &setup.admin, &1_000_u32);
    setup.client.deposit_fees(&500_i128);

    let (_, before_contract, _) = arm_attack(&e, &setup, "collect_fees", 0);
    setup.client.withdraw_early(&setup.identity, &OUTER_AMOUNT);

    assert_attack_rejected(&setup);
    assert_invariants(&e, &setup.contract_id);
    assert_eq!(
        setup.token.balance(&setup.contract_id),
        before_contract - OUTER_AMOUNT
    );
    assert_eq!(setup.client.collect_fees(&setup.admin), 500_i128);
}
