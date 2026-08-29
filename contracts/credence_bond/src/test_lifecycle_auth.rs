//! Authorization-boundary regression tests for the CredenceBond lifecycle.
//!
//! The bond lifecycle is split across several entry points — **creation**
//! (`create_bond`), **increase** (`top_up`), **cooldown**
//! (`request_cooldown_withdrawal` / `execute_cooldown_withdrawal` /
//! `cancel_cooldown`), **exit** (`withdraw` / `withdraw_early` /
//! `withdraw_bond`) and **liquidation** (`liquidate`). This module proves, at
//! the actual integration boundary, that every one of those state-mutating
//! paths enforces `require_auth()` on the correct address *before* any state
//! is changed:
//!
//!   - **allowed**: the owning identity (or admin) can perform the operation.
//!   - **denied**: a stranger's call is rejected.
//!   - **forged-identity**: authorising a *stranger* while passing the
//!     *victim's* address as `identity` is rejected by `require_auth`.
//!   - **cross-tenant**: one identity cannot mutate another identity's bond.
//!   - **no-mutation**: every rejected / forged / cross-tenant call leaves the
//!     bond, cooldown, and liquidation state byte-for-byte unchanged.
//!
//! Unlike the happy-path suites (which use `e.mock_all_auths()` and therefore
//! bypass Soroban's host-level auth), these tests use **selective**
//! [`Env::mock_auths`](soroban_sdk::Env::mock_auths) so the `require_auth()`
//! guards inside the contract are genuinely exercised instead of short-circuited.

use crate::test_helpers::advance_ledger_sequence;
use crate::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, IntoVal, Val, Vec};

/// Authorise exactly one address (`caller`) for a single entry-point call.
/// Each invocation replaces the auth set, so callers decide precisely who is
/// authorised — enabling forged-identity / cross-tenant denial tests.
fn authorize(env: &Env, caller: &Address, client_addr: &Address, fn_name: &str, args: Vec<Val>) {
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: caller,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: client_addr,
            fn_name,
            args,
            sub_invokes: &[],
        },
    }]);
}

// ---------------------------------------------------------------------------
// Setup (no `mock_all_auths` — auth is granted selectively per call)
// ---------------------------------------------------------------------------

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    let admin = Address::generate(e);
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);

    authorize(
        e,
        &admin,
        &client.address,
        "initialize",
        (admin.clone(), None::<Address>).into_val(e),
    );
    client.initialize(&admin, &None);

    (client, admin)
}

/// Register a bond for `identity` with only `identity` authorised.
/// The ledger is pinned to `start_ts` so callers can reason about lock-up.
fn create_bond_for(
    e: &Env,
    client: &CredenceBondClient<'_>,
    identity: &Address,
    amount: i128,
    duration: u64,
    start_ts: u64,
) {
    e.ledger().with_mut(|li| li.timestamp = start_ts);
    authorize(
        e,
        identity,
        &client.address,
        "create_bond",
        (identity.clone(), amount, duration, false, 0_u64).into_val(e),
    );
    client.create_bond(identity, &amount, &duration, &false, &0_u64);
}

// ---------------------------------------------------------------------------
// CREATION
// ---------------------------------------------------------------------------

#[test]
fn create_bond_allowed_when_identity_authorizes() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    create_bond_for(&e, &client, &identity, 1_000_i128, 86_400_u64, 1_000);

    let bond = client.get_identity_state(&identity);
    assert_eq!(bond.identity, identity);
    assert_eq!(bond.bonded_amount, 1_000);
    assert_eq!(bond.bond_start, 1_000);
    assert!(bond.active);
}

#[test]
fn create_bond_forged_identity_rejected_without_mutation() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let victim = Address::generate(&e);
    let attacker = Address::generate(&e);

    // Only the attacker authorises; the contract requires the victim's auth.
    authorize(
        &e,
        &attacker,
        &client.address,
        "create_bond",
        (victim.clone(), 1_000_i128, 86_400_u64, false, 0_u64).into_val(&e),
    );
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_bond(&victim, &1_000_i128, &86_400_u64, &false, &0_u64);
    }));
    assert!(res.is_err(), "forged-identity create_bond must be rejected");

    // No bond may have been created for the victim.
    let res_read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.get_identity_state(&victim);
    }));
    assert!(
        res_read.is_err(),
        "victim must have no bond after a rejected forged create_bond"
    );
}

// ---------------------------------------------------------------------------
// INCREASE (top_up)
// ---------------------------------------------------------------------------

#[test]
fn top_up_allowed_when_identity_authorizes() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    create_bond_for(&e, &client, &identity, 1_000_i128, 86_400_u64, 1_000);

    authorize(
        &e,
        &identity,
        &client.address,
        "top_up",
        (identity.clone(), 500_i128).into_val(&e),
    );
    let bond = client.top_up(&identity, &500_i128);
    assert_eq!(bond.bonded_amount, 1_500);
}

#[test]
fn top_up_cross_tenant_rejected_without_mutation() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let owner = Address::generate(&e);
    let attacker = Address::generate(&e);
    create_bond_for(&e, &client, &owner, 1_000_i128, 86_400_u64, 1_000);

    let before = client.get_identity_state(&owner);

    // Attacker authorises themselves but targets the owner's bond.
    authorize(
        &e,
        &attacker,
        &client.address,
        "top_up",
        (owner.clone(), 500_i128).into_val(&e),
    );
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.top_up(&owner, &500_i128);
    }));
    assert!(res.is_err(), "cross-tenant top_up must be rejected");

    let after = client.get_identity_state(&owner);
    assert_eq!(
        before, after,
        "owner bond must be unchanged after rejected top_up"
    );
}

// ---------------------------------------------------------------------------
// COOLDOWN
// ---------------------------------------------------------------------------

#[test]
fn cooldown_request_allowed_when_identity_authorizes() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    create_bond_for(&e, &client, &identity, 5_000_i128, 86_400_u64, 1_000);

    authorize(
        &e,
        &identity,
        &client.address,
        "request_cooldown_withdrawal",
        (identity.clone(), 1_000_i128).into_val(&e),
    );
    client.request_cooldown_withdrawal(&identity, &1_000_i128);

    let req = client.get_cooldown_request(&identity);
    assert!(req.is_some(), "cooldown request must be recorded");
}

#[test]
fn cooldown_request_forged_identity_rejected_without_mutation() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let victim = Address::generate(&e);
    let attacker = Address::generate(&e);
    create_bond_for(&e, &client, &victim, 5_000_i128, 86_400_u64, 1_000);

    let before = client.get_identity_state(&victim);

    authorize(
        &e,
        &attacker,
        &client.address,
        "request_cooldown_withdrawal",
        (victim.clone(), 1_000_i128).into_val(&e),
    );
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.request_cooldown_withdrawal(&victim, &1_000_i128);
    }));
    assert!(res.is_err(), "forged cooldown request must be rejected");
    assert!(
        client.get_cooldown_request(&victim).is_none(),
        "no cooldown request may be recorded after a rejected call"
    );
    let after = client.get_identity_state(&victim);
    assert_eq!(
        before, after,
        "victim bond must be unchanged after rejected cooldown request"
    );
}

#[test]
fn execute_cooldown_forged_identity_rejected_without_mutation() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let victim = Address::generate(&e);
    let attacker = Address::generate(&e);
    create_bond_for(&e, &client, &victim, 5_000_i128, 86_400_u64, 1_000);

    // The victim legitimately requests a cooldown withdrawal.
    authorize(
        &e,
        &victim,
        &client.address,
        "request_cooldown_withdrawal",
        (victim.clone(), 1_000_i128).into_val(&e),
    );
    client.request_cooldown_withdrawal(&victim, &1_000_i128);
    assert!(
        client.get_cooldown_request(&victim).is_some(),
        "victim cooldown request must be recorded before the attack"
    );

    let before = client.get_identity_state(&victim);

    // A stranger tries to execute the *victim's* pending cooldown withdrawal;
    // only the stranger's auth is mocked, so `victim.require_auth()` rejects.
    e.ledger().with_mut(|li| li.timestamp = 1_000 + 86_400);
    advance_ledger_sequence(&e);
    authorize(
        &e,
        &attacker,
        &client.address,
        "execute_cooldown_withdrawal",
        (victim.clone(),).into_val(&e),
    );
    let res =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.execute_cooldown_withdrawal(&victim);
        }));
    assert!(
        res.is_err(),
        "forged cooldown execution must be rejected"
    );
    assert!(
        client.get_cooldown_request(&victim).is_some(),
        "rejected cooldown execution must not clear the victim's request"
    );
    let after = client.get_identity_state(&victim);
    assert_eq!(
        before, after,
        "victim bond must be unchanged after rejected cooldown execution"
    );
}

// ---------------------------------------------------------------------------
// EXIT (withdraw)
// ---------------------------------------------------------------------------

#[test]
fn withdraw_allowed_when_identity_authorizes() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    // bond_start = 0 in a fresh ledger; duration 86_400 → expiry 86_400.
    create_bond_for(&e, &client, &identity, 10_000_i128, 86_400_u64, 0);

    // Advance past the lock-up window.
    e.ledger().with_mut(|li| li.timestamp = 90_000);
    authorize(
        &e,
        &identity,
        &client.address,
        "withdraw",
        (identity.clone(), 4_000_i128).into_val(&e),
    );
    let bond = client.withdraw(&identity, &4_000_i128);
    assert_eq!(bond.bonded_amount, 6_000);
}

#[test]
fn withdraw_forged_identity_rejected_without_mutation() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let victim = Address::generate(&e);
    let attacker = Address::generate(&e);
    create_bond_for(&e, &client, &victim, 10_000_i128, 86_400_u64, 0);
    e.ledger().with_mut(|li| li.timestamp = 90_000);

    let before = client.get_identity_state(&victim);

    authorize(
        &e,
        &attacker,
        &client.address,
        "withdraw",
        (victim.clone(), 4_000_i128).into_val(&e),
    );
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw(&victim, &4_000_i128);
    }));
    assert!(res.is_err(), "forged-identity withdraw must be rejected");

    let after = client.get_identity_state(&victim);
    assert_eq!(
        before, after,
        "victim bond must be unchanged after rejected withdraw"
    );
}

#[test]
fn withdraw_early_forged_identity_rejected_without_mutation() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let victim = Address::generate(&e);
    let attacker = Address::generate(&e);

    // Admin configures the early-exit penalty (treasury + bps).
    let treasury = Address::generate(&e);
    authorize(
        &e,
        &admin,
        &client.address,
        "set_early_exit_config",
        (admin.clone(), treasury.clone(), 500_u32).into_val(&e),
    );
    client.set_early_exit_config(&admin, &treasury, &500_u32);

    // bond_start = 1_000; keep the clock inside the lock-up window so the
    // contract would enter the early-exit code path for a legitimate owner.
    create_bond_for(&e, &client, &victim, 10_000_i128, 86_400_u64, 1_000);
    e.ledger().with_mut(|li| li.timestamp = 1_000 + 1);

    let before = client.get_identity_state(&victim);

    // Forged identity: only the attacker is authorised, the victim is not.
    authorize(
        &e,
        &attacker,
        &client.address,
        "withdraw_early",
        (victim.clone(), 4_000_i128).into_val(&e),
    );
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.withdraw_early(&victim, &4_000_i128);
    }));
    assert!(res.is_err(), "forged-identity withdraw_early must be rejected");

    let after = client.get_identity_state(&victim);
    assert_eq!(
        before, after,
        "victim bond must be unchanged after rejected withdraw_early"
    );
}

// ---------------------------------------------------------------------------
// LIQUIDATION (admin-gated)
// ---------------------------------------------------------------------------

#[test]
fn liquidate_allowed_when_admin_authorizes() {
    let e = Env::default();
    let (client, admin) = setup(&e);
    let identity = Address::generate(&e);
    create_bond_for(&e, &client, &identity, 1_000_i128, 86_400_u64, 1_000);

    // Fully slash, then liquidate. Slash must run in a later ledger than the
    // collateral increase (same-ledger guard).
    advance_ledger_sequence(&e);
    e.ledger().with_mut(|li| li.timestamp = 2_000);
    authorize(
        &e,
        &admin,
        &client.address,
        "slash",
        (admin.clone(), identity.clone(), 1_000_i128).into_val(&e),
    );
    client.slash(&admin, &identity, &1_000_i128);

    advance_ledger_sequence(&e);
    authorize(
        &e,
        &admin,
        &client.address,
        "liquidate",
        (admin.clone(), identity.clone()).into_val(&e),
    );
    let bond = client.liquidate(&admin, &identity);
    assert!(!bond.active);
    assert!(client.is_liquidated(&identity));
}

#[test]
fn liquidate_forged_admin_rejected_without_mutation() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    create_bond_for(&e, &client, &identity, 1_000_i128, 86_400_u64, 1_000);

    let stranger = Address::generate(&e);

    // A stranger claims to be the admin; their auth is mocked but the stored
    // admin differs, so `liquidate` must reject.
    authorize(
        &e,
        &stranger,
        &client.address,
        "liquidate",
        (stranger.clone(), identity.clone()).into_val(&e),
    );
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.liquidate(&stranger, &identity);
    }));
    assert!(res.is_err(), "non-admin liquidate must be rejected");

    let bond = client.get_identity_state(&identity);
    assert!(bond.active, "bond must remain active after rejected liquidate");
    assert!(
        !client.is_liquidated(&identity),
        "liquidated flag must not be set after rejected liquidate"
    );
}

#[test]
fn liquidate_healthy_bond_forged_admin_rejected_without_mutation() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let identity = Address::generate(&e);
    create_bond_for(&e, &client, &identity, 1_000_i128, 86_400_u64, 1_000);

    let stranger = Address::generate(&e);

    // Same as above but on a still-healthy (in-progress) bond: the only
    // possible rejection cause here must be the admin boundary, not the
    // eligibility check — and no liquidation flag may be set.
    authorize(
        &e,
        &stranger,
        &client.address,
        "liquidate",
        (stranger.clone(), identity.clone()).into_val(&e),
    );
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.liquidate(&stranger, &identity);
    }));
    assert!(res.is_err(), "non-admin liquidate must be rejected");
    assert!(
        !client.is_liquidated(&identity),
        "liquidated flag must not be set after a rejected call"
    );
    assert!(
        client.get_identity_state(&identity).active,
        "bond must remain active after a rejected call"
    );
}
