//! Tests for the `liquidate` entrypoint and its supporting treasury
//! configuration entrypoints (issue #366).
//!
//! These tests are fully self-contained — they do not depend on
//! `crate::test_helpers` because the `liquidate` entrypoint never touches
//! token integration. That keeps this test module independent of any
//! pre-existing infrastructure gaps in the wider test harness.
//!
//! Coverage:
//! - Eligibility:
//!   - fully-slashed bonds (success)
//!   - expired non-rolling bonds (success)
//!   - healthy in-progress bonds (rejected)
//!   - partially-slashed bonds (rejected)
//!   - rolling bonds even when their period has ended (rejected)
//!   - one second before lockup end (rejected)
//! - Idempotency:
//!   - double-liquidate on the freshly liquidated bond (rejected)
//!   - liquidating a bond that exited through `withdraw_bond` (rejected)
//! - Authorization:
//!   - non-admin caller (rejected)
//! - Side effects:
//!   - `IdentityBond.active` flips to `false`
//!   - `slashed_amount` / `bonded_amount` are preserved
//!   - `is_liquidated` flag returns `true`
//! - Setters:
//!   - `set_liquidation_treasury` rejects non-admin callers
//!   - `get_liquidation_treasury` returns `None` when unset
//!   - treasury round-trip: `get` returns the most recently set address

use crate::test_helpers::{self, advance_ledger_sequence};
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

// -----------------------------------------------------------------
// Test setup helpers (self-contained — no test_helpers dependency)
// -----------------------------------------------------------------

/// Register the bond contract and configure the liquidation treasury.
/// Returns (client, admin, identity, treasury_address).
fn setup_with_treasury(e: &Env) -> (CredenceBondClient<'_>, Address, Address, Address) {
    let (client, admin, identity) = setup(e);
    let treasury = Address::generate(e);
    client.set_liquidation_treasury(&admin, &treasury);
    let slash_treasury = Address::generate(e);
    client.set_slash_treasury(&admin, &slash_treasury);
    (client, admin, identity, treasury)
}

/// Register the bond contract without configuring a liquidation treasury.
fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let (client, admin, identity, _token, _bond_id) = test_helpers::setup_with_token(e);
    let slash_treasury = Address::generate(e);
    client.set_slash_treasury(&admin, &slash_treasury);
    (client, admin, identity)
}

/// Create a bond at a deterministic ledger timestamp so tests can derive
/// `bond_start + bond_duration` precisely and then advance past that threshold.
fn make_bond(
    e: &Env,
    client: &CredenceBondClient,
    identity: &Address,
    amount: i128,
    duration: u64,
    is_rolling: bool,
    notice: u64,
) {
    e.ledger().with_mut(|li| li.timestamp = 1_000);
    client.create_bond(identity, &amount, &duration, &is_rolling, &notice);
    advance_ledger_sequence(e);
}

// -----------------------------------------------------------------
// Treasury setter / getter
// -----------------------------------------------------------------

#[test]
fn set_liquidation_treasury_round_trip() {
    let e = Env::default();
    let (client, admin, _identity, _treasury) = setup_with_treasury(&e);
    let new_treasury = Address::generate(&e);

    assert_ne!(
        client.get_liquidation_treasury(),
        Some(new_treasury.clone())
    );
    client.set_liquidation_treasury(&admin, &new_treasury);
    assert_eq!(
        client.get_liquidation_treasury(),
        Some(new_treasury.clone())
    );
}

#[test]
fn get_liquidation_treasury_unset_returns_none() {
    let e = Env::default();
    let (client, _admin, _identity) = setup(&e);
    assert_eq!(client.get_liquidation_treasury(), None);
}

#[test]
#[should_panic]
fn set_liquidation_treasury_unauthorized_rejected() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let stranger = Address::generate(&e);
    let treasury = Address::generate(&e);

    client.set_liquidation_treasury(&stranger, &treasury);
}

// -----------------------------------------------------------------
// Happy paths
// -----------------------------------------------------------------

#[test]
fn liquidate_fully_slashed_succeeds() {
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    client.slash(&admin, &1_000_i128); // fully slash

    let bond = client.liquidate(&admin);
    assert!(!bond.active, "liquidated bonds must have active=false");
    assert_eq!(bond.bonded_amount, 1_000, "bonded_amount is preserved");
    assert_eq!(
        bond.slashed_amount, 1_000,
        "slashed_amount is preserved for audit"
    );
    assert!(client.is_liquidated(&identity));
}

#[test]
fn liquidate_expired_unrenewed_succeeds() {
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    // bond_start = 1_000, duration = 86_400 → ends at 87_400.
    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    e.ledger().with_mut(|li| li.timestamp = 87_500);

    let bond = client.liquidate(&admin);
    assert!(!bond.active);
    assert_eq!(bond.bonded_amount, 1_000);
    assert_eq!(bond.slashed_amount, 0, "no slashing occurred");
    assert!(client.is_liquidated(&identity));
}

#[test]
fn liquidate_expired_at_exact_boundary_succeeds() {
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    e.ledger().with_mut(|li| li.timestamp = 87_400); // exactly bond_start + duration

    let bond = client.liquidate(&admin);
    assert!(!bond.active);
    assert!(client.is_liquidated(&identity));
}

#[test]
fn liquidate_preserves_slashed_and_bonded_amounts() {
    // Partial slash + lockup expiry → expired_unrenewed path with a non-zero
    // residual. The on-chain state must stay coherent so off-chain replays
    // can derive the residual sweep amount.
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    client.slash(&admin, &700_i128); // partial slash

    e.ledger().with_mut(|li| li.timestamp = 87_500);
    let bond = client.liquidate(&admin);

    assert_eq!(bond.bonded_amount, 1_000);
    assert_eq!(bond.slashed_amount, 700);
    assert!(!bond.active);
    assert!(client.is_liquidated(&identity));
}

#[test]
fn liquidate_without_treasury_still_marks_bond_inactive() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    // No `set_liquidation_treasury` call: treasury is `None`.

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    client.slash(&admin, &400_i128);
    e.ledger().with_mut(|li| li.timestamp = 87_500);

    let bond = client.liquidate(&admin);
    assert!(!bond.active);
    assert!(client.is_liquidated(&identity));
}

// -----------------------------------------------------------------
// Negative paths (eligibility)
// -----------------------------------------------------------------
// Note: bare `#[should_panic]` is used for unauthorized/no-bond paths to
// keep tests independent of the SDK's SCErrorCode format. Eligibility
// rejections come from a literal `panic!("bond is not eligible for ...")`
// and are matched by the expected substring below.

#[test]
#[should_panic(expected = "bond is not eligible for liquidation")]
fn liquidate_one_second_before_expiry_rejected() {
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    e.ledger().with_mut(|li| li.timestamp = 87_399);

    client.liquidate(&admin);
}

#[test]
#[should_panic(expected = "bond is not eligible for liquidation")]
fn liquidate_healthy_bond_rejected() {
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    // No slash, not past lockup.
    client.liquidate(&admin);
}

#[test]
#[should_panic(expected = "bond is not eligible for liquidation")]
fn liquidate_partially_slashed_bond_rejected() {
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    client.slash(&admin, &500_i128); // partial slash
    client.liquidate(&admin);
}

#[test]
#[should_panic(expected = "bond is not eligible for liquidation")]
fn liquidate_rolling_bond_past_lockup_rejected() {
    // Rolling bonds auto-renew on `renew_if_rolling`. The "expired_unrenewed"
    // path therefore excludes them. The bond below still passes eligibility
    // from `renew_if_rolling`'s perspective, so liquidate must refuse to act.
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, true, 100);
    e.ledger().with_mut(|li| li.timestamp = 87_500);

    client.liquidate(&admin);
}

#[test]
#[should_panic]
fn liquidate_no_bond_rejected() {
    let e = Env::default();
    let (client, admin, _identity, _treasury) = setup_with_treasury(&e);

    client.liquidate(&admin);
}

#[test]
#[should_panic]
fn liquidate_non_admin_rejected() {
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    client.slash(&admin, &1_000_i128);

    let stranger = Address::generate(&e);
    client.liquidate(&stranger);
}

// -----------------------------------------------------------------
// Idempotency / replay-protection
// -----------------------------------------------------------------

#[test]
#[should_panic]
fn liquidate_twice_rejected() {
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    client.slash(&admin, &1_000_i128);
    client.liquidate(&admin);
    // Second liquidation must be rejected with BondNotActive.
    client.liquidate(&admin);
}

#[test]
fn liquidate_after_withdraw_bond_does_not_set_liquidated_flag() {
    // A bond that exited through `withdraw_bond` has `active = false` but no
    // `Liquidated` flag is set — the flag is reserved for the liquidate path.
    let e = Env::default();
    let (client, _admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    e.ledger().with_mut(|li| li.timestamp = 87_500);
    let _withdraw_amount = client.withdraw_bond(&identity);

    assert!(
        !client.is_liquidated(&identity),
        "withdraw_bond path must not set the Liquidated flag"
    );
}

#[test]
#[should_panic]
fn liquidate_after_withdraw_bond_panics() {
    // Companion to the above: after `withdraw_bond` flips `active = false`,
    // a subsequent `liquidate` is rejected (it cannot differentiate "withdrew"
    // from "liquidated" without the flag, so it conservatively rejects any
    // inactive bond).
    let e = Env::default();
    let (client, admin, identity, _treasury) = setup_with_treasury(&e);

    make_bond(&e, &client, &identity, 1_000_i128, 86_400_u64, false, 0);
    e.ledger().with_mut(|li| li.timestamp = 87_500);
    let _withdraw_amount = client.withdraw_bond(&identity);

    client.liquidate(&admin);
}
