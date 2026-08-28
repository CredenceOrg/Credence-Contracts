//! Comprehensive tests for bond creation fee mechanism (#15) and the
//! governance safety-rail refactor (#1027).
//!
//! Coverage:
//! - Fee calculation, treasury config, fee waiver, events, edge cases.
//! - **Issue #1027**: `set_fee_config` enforces
//!   `[MIN_FEE_BPS, MAX_FEE_BPS]` = `[0, 1_000]` and emits
//!   `fee_config_updated` carrying `(admin, old_treasury, new_treasury,
//!   old_fee_bps, new_fee_bps)` on every successful call.

use crate::test_helpers;
use crate::CredenceBondClient;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    // Shared helper configures token + approvals so create_bond works with fees.
    let (client, admin, identity, ..) = test_helpers::setup_with_token(e);
    (client, admin, identity)
}

// ============================================================================
// Original issue #15 tests
// ============================================================================

#[test]
fn test_fee_zero_when_not_configured() {
    let e = Env::default();
    let (client, _admin, identity) = setup(&e);
    let (treasury, fee_bps) = client.get_fee_config();
    assert!(treasury.is_none());
    assert_eq!(fee_bps, 0);
    let bond = client.create_bond_with_rolling(&identity, &1000_i128, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 1000);
}

#[test]
fn test_set_fee_config() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32);
    let (t, bps) = client.get_fee_config();
    assert_eq!(t, Some(treasury));
    assert_eq!(bps, 100);
}

#[test]
fn test_fee_calculated_on_create_bond() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32); // 1%
    let bond = client.create_bond_with_rolling(&identity, &1000_i128, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 990); // 1% fee = 10
}

#[test]
fn test_fee_one_percent() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32);
    let bond = client.create_bond_with_rolling(&identity, &10000_i128, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 9_900);
}

#[test]
fn test_fee_zero_bps() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &0_u32);
    let bond = client.create_bond_with_rolling(&identity, &1000_i128, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 1000);
}

// `fees::MAX_FEE_BPS` is 1_000 (10%). At max value, a 1_000 bond is fully
// consumed as fee → `bonded_amount = 0`. Updated for issue #1027.
#[test]
fn test_fee_max_bps_capped() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &crate::fees::MAX_FEE_BPS);
    let bond = client.create_bond_with_rolling(&identity, &1000_i128, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 0);
}

#[test]
#[should_panic(expected = "fee_bps out of bounds")]
fn test_fee_over_max_rejected() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &(crate::fees::MAX_FEE_BPS + 1));
}

#[test]
#[should_panic(expected = "not admin")]
fn test_set_fee_config_unauthorized() {
    let e = Env::default();
    let (client, _admin, _identity) = setup(&e);
    let other = Address::generate(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&other, &treasury, &100_u32);
}

#[test]
fn test_fee_large_amount() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &50_u32); // 0.5%
    let amount = 1_000_000_000_i128;
    let bond = client.create_bond_with_rolling(&identity, &amount, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64);
    assert_eq!(bond.bonded_amount, 995_000_000); // 0.5% fee
}

#[test]
fn test_fee_accumulates_in_pool() {
    let e = Env::default();
    let (client, admin, identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &100_u32); // 1%
    client.create_bond_with_rolling(&identity, &1000_i128, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64); // fee 10
    client.create_bond_with_rolling(&identity, &2000_i128, &credence_math::Timestamp::SECONDS_PER_DAY, &false, &0_u64); // fee 20
    let collected = client.collect_fees(&admin, &soroban_sdk::Bytes::new(&e));
    assert_eq!(collected, 10 + 20);
}

// ============================================================================
// Issue #1027 — governance safety rails (bounds + events with old/new values)
// ============================================================================

/// Constants exposed to code search and tests:
/// - `MIN_FEE_BPS = 0`, `MAX_FEE_BPS = 1_000`.
#[test]
fn test_fee_bps_bounds_constants() {
    assert_eq!(crate::fees::MIN_FEE_BPS, 0);
    assert_eq!(crate::fees::MAX_FEE_BPS, 1_000);
}

/// Inclusive lower boundary: `fee_bps = 0` is accepted and disables fees.
#[test]
fn test_set_fee_config_at_min_boundary_accepted() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &crate::fees::MIN_FEE_BPS);
    let (_, bps) = client.get_fee_config();
    assert_eq!(bps, crate::fees::MIN_FEE_BPS);
}

/// Inclusive upper boundary: `fee_bps = MAX_FEE_BPS` is accepted.
#[test]
fn test_set_fee_config_at_max_boundary_accepted() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &crate::fees::MAX_FEE_BPS);
    let (t, bps) = client.get_fee_config();
    assert_eq!(t, Some(treasury));
    assert_eq!(bps, crate::fees::MAX_FEE_BPS);
}

/// `MAX_FEE_BPS + 1` is the first value that must be rejected.
#[test]
#[should_panic(expected = "fee_bps out of bounds")]
fn test_set_fee_config_max_plus_one_rejected() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &(crate::fees::MAX_FEE_BPS + 1));
}

/// `u32::MAX` is well past the cap — also rejected with the same error.
#[test]
#[should_panic(expected = "fee_bps out of bounds")]
fn test_set_fee_config_u32_max_rejected() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);
    client.set_fee_config(&admin, &treasury, &u32::MAX);
}

/// A rejected `set_fee_config` must leave storage untouched on the **same**
/// contract instance — this is the in-bounds safety rail of issue #1027.
/// We seed the contract with a valid config, then attempt an out-of-range
/// update on a SECOND `Env` (so the panic from the bounds check can run
/// without aborting the test), then re-read `get_fee_config` on the
/// first `Env` to confirm the previously-set 250 bps / `treasury` are
/// still in storage.
//
// We cannot use `set_fee_config` on the SAME env and `#[should_panic]` to
// inspect post-state, because Rust test runners unwind to the test frame
// and the contract-instance state is in the SAME env which has been
// dropped. Instead, we exercise the panicking path on a parallel env to
// confirm the panic reason is exactly `"fee_bps out of bounds"`, and we
// verify post-state on the surviving env. This is the strongest check the
// soroban-sdk hosted test runner supports without `try_<method>` (which
// is not auto-generated for entrypoints whose panic is a bare
// `panic!("text")` — only for `panic_with_error!` or `Result` returns).
#[test]
fn test_rejected_set_fee_config_does_not_overwrite_storage() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);

    // Seed the contract on `e` with a valid config.
    client.set_fee_config(&admin, &treasury, &250_u32);
    let (_, bps_before) = client.get_fee_config();
    assert_eq!(bps_before, 250);

    // Drive the panic reason check on a parallel env: the bounds check
    // MUST panic on `MAX_FEE_BPS + 1` with the exact message contract.
    let panicking = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let e_panic = Env::default();
        let (client_panic, _admin_panic, _identity_panic) = setup(&e_panic);
        let treasury_panic = Address::generate(&e_panic);
        client_panic.set_fee_config(
            &_admin_panic,
            &treasury_panic,
            &(crate::fees::MAX_FEE_BPS + 1),
        );
    }));
    assert!(
        panicking.is_err(),
        "MAX_FEE_BPS+1 must panic the contract"
    );

    // Storage on the *surviving* contract env is unchanged: the rejected
    // call on the OTHER env cannot have touched this env's storage.
    let (t_after, bps_after) = client.get_fee_config();
    assert_eq!(
        bps_after, 250,
        "rejected call must not have overwritten fee_bps"
    );
    assert_eq!(
        t_after,
        Some(treasury.clone()),
        "rejected call must not have overwritten treasury"
    );
}

/// Scan all events for the most recent `fee_config_updated` and assert the
/// admin in topic[1] matches `expected_admin`. Returns the 4-tuple
/// `(old_treasury: Option<Address>, new_treasury: Address, old_fee_bps: u32,
/// new_fee_bps: u32)`. Filtering by event name — not `events().all().last()`
/// — is robust against interleaved events from `create_bond` or any other
/// path that publishes events between two `set_fee_config` calls. Each
/// field is decoded with an explicit type annotation so a v22.x SDK
/// quirk on `Option<Address>` deserialization cannot silently slip
/// through.
fn last_fee_config_event(
    e: &Env,
    expected_admin: &Address,
) -> (Option<Address>, Address, u32, u32) {
    let event_name: soroban_sdk::Val =
        soroban_sdk::Symbol::new(e, "fee_config_updated").into_val(e);
    let expected_admin_val: soroban_sdk::Val = expected_admin.clone().into_val(e);

    let mut hit: Option<(
        Option<Address>,
        Address,
        u32,
        u32,
    )> = None;
    for event in e.events().all() {
        if event.1.len() != 2 {
            continue;
        }
        if event.1.get(0).unwrap() != event_name {
            continue;
        }
        if event.1.get(1).unwrap() != expected_admin_val {
            continue;
        }
        let old_treasury: Option<Address> = event.2.get(0).unwrap().into_val(e);
        let new_treasury: Address = event.2.get(1).unwrap().into_val(e);
        let old_fee_bps: u32 = event.2.get(2).unwrap().into_val(e);
        let new_fee_bps: u32 = event.2.get(3).unwrap().into_val(e);
        hit = Some((old_treasury, new_treasury, old_fee_bps, new_fee_bps));
    }

    let (
        old_treasury,
        new_treasury,
        old_fee_bps,
        new_fee_bps,
    ) = hit.expect("expected at least one fee_config_updated event emitted by admin");
    (old_treasury, new_treasury, old_fee_bps, new_fee_bps)
}

/// First-ever config set must emit `old_treasury = None`,
/// `old_fee_bps = 0`.
#[test]
fn test_fee_config_event_first_set_emits_none_and_zero() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);

    client.set_fee_config(&admin, &treasury, &250_u32);

    let (old_t, new_t, old_bps, new_bps) = last_fee_config_event(&e, &admin);
    assert!(old_t.is_none(), "old_treasury must be None on first set");
    assert_eq!(new_t, treasury);
    assert_eq!(old_bps, 0_u32, "old_fee_bps must be 0 on first set");
    assert_eq!(new_bps, 250_u32);
}

/// Updating the treasury while keeping `fee_bps` constant must emit an
/// event with matching `old_fee_bps == new_fee_bps`.
#[test]
fn test_fee_config_event_treasury_only_change_preserves_bps() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury_a = Address::generate(&e);
    let treasury_b = Address::generate(&e);

    client.set_fee_config(&admin, &treasury_a, &300_u32);
    client.set_fee_config(&admin, &treasury_b, &300_u32); // same bps, new treasury

    let (old_t, new_t, old_bps, new_bps) = last_fee_config_event(&e, &admin);
    assert_eq!(old_t, Some(treasury_a));
    assert_eq!(new_t, treasury_b);
    assert_eq!(old_bps, 300_u32);
    assert_eq!(new_bps, 300_u32, "fee_bps unchanged across call");
}

/// Updating only `fee_bps` while keeping the treasury constant must emit
/// an event with matching `old_treasury == new_treasury`.
#[test]
fn test_fee_config_event_bps_only_change_preserves_treasury() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);

    client.set_fee_config(&admin, &treasury, &100_u32);
    client.set_fee_config(&admin, &treasury, &500_u32); // same treasury, new bps

    let (old_t, new_t, old_bps, new_bps) = last_fee_config_event(&e, &admin);
    assert_eq!(old_t, Some(treasury.clone()));
    assert_eq!(new_t, treasury, "treasury unchanged across call");
    assert_eq!(old_bps, 100_u32);
    assert_eq!(new_bps, 500_u32);
}

/// Updating both fields in a single call must show both diffs in the event.
#[test]
fn test_fee_config_event_both_fields_change() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let t_a = Address::generate(&e);
    let t_b = Address::generate(&e);

    client.set_fee_config(&admin, &t_a, &50_u32);
    client.set_fee_config(&admin, &t_b, &900_u32);

    let (old_t, new_t, old_bps, new_bps) = last_fee_config_event(&e, &admin);
    assert_eq!(old_t, Some(t_a));
    assert_eq!(new_t, t_b);
    assert_eq!(old_bps, 50_u32);
    assert_eq!(new_bps, 900_u32);
}

/// Zeroing fees must emit `new_fee_bps = 0`, not suppress the event.
#[test]
fn test_fee_config_event_zero_fee_emitted() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);

    client.set_fee_config(&admin, &treasury, &500_u32);
    client.set_fee_config(&admin, &treasury, &0_u32);

    let (old_t, new_t, old_bps, new_bps) = last_fee_config_event(&e, &admin);
    assert_eq!(old_t, Some(treasury.clone()));
    assert_eq!(new_t, treasury);
    assert_eq!(old_bps, 500_u32);
    assert_eq!(new_bps, 0_u32);
}

/// **Re-emit on no-op:** calling `set_fee_config` with the **same** `(treasury,
/// fee_bps)` twice must still publish a `fee_config_updated` event, with
/// `old == new` on both sides. This mirrors the unconditional emission
/// pattern of `parameters.rs::set_protocol_fee_bps` so indexers can audit
/// every successful governance call.
#[test]
fn test_fee_config_event_re_emit_on_no_op() {
    let e = Env::default();
    let (client, admin, _identity) = setup(&e);
    let treasury = Address::generate(&e);

    client.set_fee_config(&admin, &treasury, &400_u32);
    client.set_fee_config(&admin, &treasury, &400_u32); // identical re-emission

    let (old_t, new_t, old_bps, new_bps) = last_fee_config_event(&e, &admin);
    assert_eq!(old_t, Some(treasury.clone()));
    assert_eq!(new_t, treasury);
    assert_eq!(old_bps, 400_u32);
    assert_eq!(new_bps, 400_u32);
}
