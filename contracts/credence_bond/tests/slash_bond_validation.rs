//! Tests for `CredenceBond::slash_bond` input validation, checked arithmetic,
//! and event emission (issue #1039).
//!
//! `slash_bond` is the reentrancy-guarded, idempotency-salted admin
//! entrypoint (distinct from the simpler `slash()` wrapper documented in
//! `docs/slashing.md`). Before this fix it accepted any `i128` — including
//! negative and zero amounts — with unchecked addition, and never emitted
//! `bond_slashed`, so indexers and the reputation engine could not observe
//! slashes performed through this path.

use credence_bond::{CredenceBond, CredenceBondClient};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Bytes, Env, Symbol, TryFromVal};

/// One token in the contract's normalized 18-decimal accounting. `create_bond`
/// enforces a minimum bond amount of `1 * 10^18` outside of the crate's own
/// `#[cfg(test)]` builds (see `validation::MIN_BOND_AMOUNT`), so this
/// integration test — which links `credence_bond` as a normal dependency —
/// must use realistically scaled amounts rather than small literals like `1_000`.
const ONE_TOKEN: i128 = 1_000_000_000_000_000_000;

struct Fixture {
    env: Env,
    client: CredenceBondClient<'static>,
    admin: Address,
    identity: Address,
}

fn setup(bonded_amount: i128) -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let identity = Address::generate(&env);
    client.initialize(&admin, &None);
    client.create_bond(&identity, &bonded_amount, &3_600_u64, &false, &0_u64);
    Fixture {
        env,
        client,
        admin,
        identity,
    }
}

fn no_salt(env: &Env) -> Bytes {
    Bytes::new(env)
}

/// Decode the last `bond_slashed` event's data payload as `(identity, slash_amount, total_slashed)`.
fn last_bond_slashed_event(env: &Env) -> (Address, i128, i128) {
    let topic = Symbol::new(env, "bond_slashed");
    let mut result = None;
    for (_contract, topics, data) in env.events().all().iter() {
        if let Some(topic0_val) = topics.get(0) {
            if let Ok(topic0) = Symbol::try_from_val(env, &topic0_val) {
                if topic0 == topic {
                    result = Some(<(Address, i128, i128)>::try_from_val(env, &data).unwrap());
                }
            }
        }
    }
    result.expect("no bond_slashed event was emitted")
}

#[test]
#[should_panic]
fn slash_bond_rejects_negative_amount() {
    let f = setup(1_000 * ONE_TOKEN);
    f.client
        .slash_bond(&f.admin, &f.identity, &-1_i128, &no_salt(&f.env));
}

#[test]
#[should_panic]
fn slash_bond_rejects_zero_amount() {
    let f = setup(1_000 * ONE_TOKEN);
    f.client
        .slash_bond(&f.admin, &f.identity, &0_i128, &no_salt(&f.env));
}

#[test]
fn slash_bond_accepts_positive_amount_and_emits_event() {
    let f = setup(1_000 * ONE_TOKEN);
    let total = f
        .client
        .slash_bond(&f.admin, &f.identity, &(300 * ONE_TOKEN), &no_salt(&f.env));
    assert_eq!(total, 300 * ONE_TOKEN);

    // Events must be inspected before any further contract call — the test
    // host's event log only retains the most recent invocation's events.
    let (identity, slash_amount, total_slashed) = last_bond_slashed_event(&f.env);
    assert_eq!(identity, f.identity);
    assert_eq!(slash_amount, 300 * ONE_TOKEN);
    assert_eq!(total_slashed, 300 * ONE_TOKEN);

    let bond = f.client.get_identity_state(&f.identity);
    assert_eq!(bond.slashed_amount, 300 * ONE_TOKEN);
    assert_eq!(bond.bonded_amount, 1_000 * ONE_TOKEN);
}

#[test]
fn slash_bond_event_reflects_cumulative_total_across_calls() {
    let f = setup(1_000 * ONE_TOKEN);
    f.client
        .slash_bond(&f.admin, &f.identity, &(200 * ONE_TOKEN), &no_salt(&f.env));
    f.client
        .slash_bond(&f.admin, &f.identity, &(150 * ONE_TOKEN), &no_salt(&f.env));

    let (identity, slash_amount, total_slashed) = last_bond_slashed_event(&f.env);
    assert_eq!(identity, f.identity);
    assert_eq!(
        slash_amount,
        150 * ONE_TOKEN,
        "event carries the amount just slashed, not the cumulative total"
    );
    assert_eq!(total_slashed, 350 * ONE_TOKEN);

    let bond = f.client.get_identity_state(&f.identity);
    assert_eq!(bond.slashed_amount, 350 * ONE_TOKEN);
}

#[test]
fn slash_bond_allows_slashing_exactly_up_to_bonded_amount() {
    let f = setup(1_000 * ONE_TOKEN);
    let total = f.client.slash_bond(
        &f.admin,
        &f.identity,
        &(1_000 * ONE_TOKEN),
        &no_salt(&f.env),
    );
    assert_eq!(total, 1_000 * ONE_TOKEN);

    let bond = f.client.get_identity_state(&f.identity);
    assert_eq!(bond.slashed_amount, bond.bonded_amount);
}

#[test]
#[should_panic]
fn slash_bond_rejects_amount_exceeding_bonded_amount() {
    let f = setup(1_000 * ONE_TOKEN);
    f.client.slash_bond(
        &f.admin,
        &f.identity,
        &(1_000 * ONE_TOKEN + 1),
        &no_salt(&f.env),
    );
}

#[test]
#[should_panic]
fn slash_bond_rejects_cumulative_amount_exceeding_bonded_amount() {
    let f = setup(1_000 * ONE_TOKEN);
    f.client
        .slash_bond(&f.admin, &f.identity, &(700 * ONE_TOKEN), &no_salt(&f.env));
    // Available balance is now only 300 * ONE_TOKEN; this must be rejected
    // rather than silently capped, matching the over-slash guard's existing
    // semantics.
    f.client.slash_bond(
        &f.admin,
        &f.identity,
        &(300 * ONE_TOKEN + 1),
        &no_salt(&f.env),
    );
}

#[test]
fn slash_bond_releases_reentrancy_lock_after_rejected_call() {
    let f = setup(1_000 * ONE_TOKEN);

    // A rejected (zero-amount) call must not leave the reentrancy lock held —
    // a subsequent valid call has to succeed.
    let salt = no_salt(&f.env);
    assert!(f
        .client
        .try_slash_bond(&f.admin, &f.identity, &0_i128, &salt)
        .is_err());

    let total = f
        .client
        .slash_bond(&f.admin, &f.identity, &(100 * ONE_TOKEN), &no_salt(&f.env));
    assert_eq!(total, 100 * ONE_TOKEN);
}

#[test]
#[should_panic]
fn slash_bond_rejects_non_admin_caller() {
    let f = setup(1_000 * ONE_TOKEN);
    let stranger = Address::generate(&f.env);
    f.client.slash_bond(
        &stranger,
        &f.identity,
        &(100 * ONE_TOKEN),
        &no_salt(&f.env),
    );
}
