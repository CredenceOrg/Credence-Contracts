#![cfg(test)]

//! Regression tests for the bond lifecycle state-transition invariants (#1273).
//!
//! Every mutating lifecycle entrypoint must reject a bond whose `active ==
//! false` (closed via `withdraw_bond` or `liquidate`). These tests prove the
//! invariant at the actual contract boundary: legal transitions succeed,
//! stale/repeated/out-of-order transitions on a closed bond panic and leave no
//! partial state.

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

/// Setup a bond contract with a funded, approved identity.
fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address, Address) {
    crate::test_helpers::setup_with_token(e)
}

/// Create a fixed-duration bond for `identity`.
fn create_fixed_bond(client: &CredenceBondClient<'_>, identity: &Address) {
    client.create_bond(
        identity,
        &1_000_i128,
        &credence_math::Timestamp::SECONDS_PER_DAY,
        &false,
        &0_u64,
    );
}

#[test]
fn lifecycle_legal_transition_matrix_succeeds() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);

    // Active bond: all mutating lifecycle operations are legal.
    let _ = client.withdraw(&identity, &500_i128);
    let _ = client.top_up(&identity, &500_i128);
    let _ = client.extend_duration(&identity, &60_u64);
    let _ = client.request_cooldown_withdrawal(&identity, &100_i128);
    let _ = client.cancel_cooldown(&identity);

    let bond = client.get_identity_state(&identity);
    assert!(bond.active, "bond must still be active after legal transitions");
}

#[test]
#[should_panic(expected = "HostError")]
fn top_up_rejected_after_withdraw_bond() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);

    // Close the bond via withdraw_bond (advance past lock-up first).
    e.ledger().with_mut(|li| {
        li.timestamp = li
            .timestamp
            .saturating_add(2 * credence_math::Timestamp::SECONDS_PER_DAY);
    });
    let _ = client.withdraw_bond(&identity);

    // A closed bond must reject top_up.
    let _ = client.top_up(&identity, &100_i128);
}

#[test]
#[should_panic(expected = "HostError")]
fn extend_duration_rejected_after_withdraw_bond() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);

    e.ledger().with_mut(|li| {
        li.timestamp = li
            .timestamp
            .saturating_add(2 * credence_math::Timestamp::SECONDS_PER_DAY);
    });
    let _ = client.withdraw_bond(&identity);

    let _ = client.extend_duration(&identity, &60_u64);
}

#[test]
#[should_panic(expected = "HostError")]
fn withdraw_rejected_after_withdraw_bond() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);

    e.ledger().with_mut(|li| {
        li.timestamp = li
            .timestamp
            .saturating_add(2 * credence_math::Timestamp::SECONDS_PER_DAY);
    });
    let _ = client.withdraw_bond(&identity);

    // Repeated withdrawal on the closed bond must panic.
    let _ = client.withdraw(&identity, &100_i128);
}

#[test]
#[should_panic(expected = "HostError")]
fn request_cooldown_rejected_after_withdraw_bond() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);

    e.ledger().with_mut(|li| {
        li.timestamp = li
            .timestamp
            .saturating_add(2 * credence_math::Timestamp::SECONDS_PER_DAY);
    });
    let _ = client.withdraw_bond(&identity);

    let _ = client.request_cooldown_withdrawal(&identity, &100_i128);
}

#[test]
#[should_panic(expected = "HostError")]
fn execute_cooldown_rejected_after_withdraw_bond() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);
    let _ = client.request_cooldown_withdrawal(&identity, &100_i128);

    e.ledger().with_mut(|li| {
        li.timestamp = li
            .timestamp
            .saturating_add(2 * credence_math::Timestamp::SECONDS_PER_DAY);
    });
    let _ = client.withdraw_bond(&identity);

    // Executing the now-stale cooldown request must panic.
    let _ = client.execute_cooldown_withdrawal(&identity);
}

#[test]
#[should_panic(expected = "HostError")]
fn cancel_cooldown_rejected_after_withdraw_bond() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);
    let _ = client.request_cooldown_withdrawal(&identity, &100_i128);

    e.ledger().with_mut(|li| {
        li.timestamp = li
            .timestamp
            .saturating_add(2 * credence_math::Timestamp::SECONDS_PER_DAY);
    });
    let _ = client.withdraw_bond(&identity);

    let _ = client.cancel_cooldown(&identity);
}

#[test]
#[should_panic(expected = "HostError")]
fn top_up_rejected_after_liquidate() {
    let e = Env::default();
    let (client, admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);

    // Admin liquidates the bond (expired non-rolling).
    e.ledger().with_mut(|li| {
        li.timestamp = li
            .timestamp
            .saturating_add(2 * credence_math::Timestamp::SECONDS_PER_DAY);
    });
    let _ = client.liquidate(&admin, &identity);

    let _ = client.top_up(&identity, &100_i128);
}

#[test]
#[should_panic(expected = "HostError")]
fn extend_duration_rejected_after_liquidate() {
    let e = Env::default();
    let (client, admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);

    e.ledger().with_mut(|li| {
        li.timestamp = li
            .timestamp
            .saturating_add(2 * credence_math::Timestamp::SECONDS_PER_DAY);
    });
    let _ = client.liquidate(&admin, &identity);

    let _ = client.extend_duration(&identity, &60_u64);
}

#[test]
fn withdraw_bond_then_recreate_is_legal() {
    // The lifecycle is per-bond: after a closed bond, a *new* create_bond
    // (fresh identity) is the only legal transition back to Active.
    let e = Env::default();
    let (client, _admin, identity, _token_id, _contract_id) = setup(&e);

    create_fixed_bond(&client, &identity);
    e.ledger().with_mut(|li| {
        li.timestamp = li
            .timestamp
            .saturating_add(2 * credence_math::Timestamp::SECONDS_PER_DAY);
    });
    let _ = client.withdraw_bond(&identity);

    // Re-create on a *different* identity must succeed.
    let identity2 = Address::generate(&e);
    create_fixed_bond(&client, &identity2);
    let bond = client.get_identity_state(&identity2);
    assert!(bond.active);
}
