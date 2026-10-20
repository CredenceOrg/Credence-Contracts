//! Acceptance tests for issue #1614 — `set_callback` must be admin-gated.
//!
//! Before the fix, `set_callback(addr)` performed no authorization at all, so any
//! account could point the test hook at a contract whose `on_withdraw` panics and
//! permanently revert withdrawals, slashing and fee collection for every user.
//!
//! These are integration tests (not `#[cfg(test)]` modules) on purpose: the
//! callback hook is reachable from the crate's public surface, so the tests drive
//! it exactly the way a hostile caller would — through the generated client.
//!
//! Everything asserted here is observable on-chain:
//! * the typed `ContractError` returned by the guard,
//! * the single `require_auth` entry the guard records,
//! * the `DataKey::Callback` slot (set / cleared),
//! * the `callback_set` / `callback_cleared` events.

use credence_bond::{CredenceBond, CredenceBondClient, DataKey};
use credence_errors::ContractError;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Error, InvokeError, Symbol, TryFromVal, Val,
};

/// A callback target whose hooks always panic. Installing it is enough to brick
/// `withdraw_bond`, `slash_bond` and `collect_fees` — which is exactly what made
/// the missing guard a denial of service.
#[contract]
pub struct PanickingCallback;

#[contractimpl]
impl PanickingCallback {
    pub fn on_withdraw(_e: Env, _amount: i128) {
        panic!("callback panicked on purpose");
    }

    pub fn on_slash(_e: Env, _amount: i128) {
        panic!("callback panicked on purpose");
    }

    pub fn on_collect(_e: Env, _fees: i128) {
        panic!("callback panicked on purpose");
    }
}

fn setup() -> (Env, CredenceBondClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);
    (env, client, admin, contract_id)
}

/// Read the callback slot straight out of instance storage.
fn stored_callback(env: &Env, contract_id: &Address) -> Option<Address> {
    env.as_contract(contract_id, || {
        env.storage().instance().get(&DataKey::Callback)
    })
}

fn dec<T: TryFromVal<Env, Val>>(env: &Env, val: &Val) -> T {
    T::try_from_val(env, val).expect("event field failed to decode")
}

/// Assert that a `try_*` call failed with a specific contract error.
///
/// The generated client reports the error as a `soroban_sdk::Error`, because
/// `ContractError` is defined in the shared `credence_errors` crate rather than
/// in this contract, so the wire code is compared instead of the Rust variant.
fn assert_contract_error(res: Result<Error, InvokeError>, expected: ContractError) {
    let expected_code = expected as u32;
    match res.map(InvokeError::from) {
        Ok(InvokeError::Contract(code)) => assert_eq!(
            code, expected_code,
            "returned contract error code does not match the expected variant"
        ),
        other => panic!(
            "expected contract error code {}, got {:?}",
            expected_code, other
        ),
    }
}

/// Assert that a `try_*` call failed in the host rather than in the contract —
/// i.e. the stored-admin guard's `require_auth` was not satisfied.
fn assert_host_auth_failure(res: Result<Error, InvokeError>) {
    let err = res.expect("expected the call to fail authorization");
    assert_eq!(
        InvokeError::from(err),
        InvokeError::Abort,
        "expected a host authorization failure, not a contract error"
    );
}

/// Collect `callback_set` payloads from the events of the most recent call.
fn callback_set_payloads(env: &Env) -> std::vec::Vec<(Option<Address>, Option<Address>)> {
    let topic = Symbol::new(env, "callback_set");
    let mut out = std::vec::Vec::new();
    for (_contract, topics, data) in env.events().all().iter() {
        if dec::<Symbol>(env, &topics.get(0).expect("event has no topic")) == topic {
            out.push(dec(env, &data));
        }
    }
    out
}

/// Collect `callback_cleared` payloads from the events of the most recent call.
fn callback_cleared_payloads(env: &Env) -> std::vec::Vec<Option<Address>> {
    let topic = Symbol::new(env, "callback_cleared");
    let mut out = std::vec::Vec::new();
    for (_contract, topics, data) in env.events().all().iter() {
        if dec::<Symbol>(env, &topics.get(0).expect("event has no topic")) == topic {
            out.push(dec(env, &data));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Acceptance criteria
// ---------------------------------------------------------------------------

/// `try_set_callback(&mallory, &cb)` returns `ContractError::NotAdmin` and the
/// callback stays unset — an arbitrary account can no longer install a hook.
#[test]
fn set_callback_requires_admin() {
    let (env, client, _admin, contract_id) = setup();
    let mallory = Address::generate(&env);
    let cb = env.register(PanickingCallback, ());

    let err = client.try_set_callback(&mallory, &cb).unwrap_err();

    assert_contract_error(err, ContractError::NotAdmin);
    assert_eq!(
        stored_callback(&env, &contract_id),
        None,
        "the rejected call must not touch the callback slot"
    );
}

/// With no mocked auths, the admin-less call fails in the host — proving the
/// entry point really performs `admin.require_auth()`.
#[test]
fn set_callback_requires_signature() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &None);

    // Disable auth mocking: from here on a missing `require_auth` is an error.
    env.set_auths(&[]);

    let cb = env.register(PanickingCallback, ());
    let res = client.try_set_callback(&admin, &cb);

    assert!(res.is_err(), "set_callback must require the admin signature");
    assert_host_auth_failure(res.unwrap_err());
    assert_eq!(
        stored_callback(&env, &contract_id),
        None,
        "a call that fails authorization must not write storage"
    );
}

/// The guard performs exactly one `require_auth` for the stored admin.
#[test]
fn set_callback_records_a_single_admin_auth() {
    let (env, client, admin, _contract_id) = setup();
    let cb = env.register(PanickingCallback, ());

    client.set_callback(&admin, &cb);

    let auths = env.auths();
    assert_eq!(auths.len(), 1, "exactly one authorization is expected");
    assert_eq!(auths.get(0).unwrap().0, admin);
}

/// The DoS recovery path: with a panicking hook installed `withdraw_bond`
/// reverts; after `clear_callback` the withdrawal goes through untouched.
#[test]
fn clear_callback_restores_withdraw() {
    let (env, client, admin, _contract_id) = setup();
    let identity = Address::generate(&env);

    let amount = 1_000_000_000_000_000_000_i128;
    client.create_bond(&identity, &amount, &86_400_u64, &false, &0_u64);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86_401_u64);

    let cb = env.register(PanickingCallback, ());
    client.set_callback(&admin, &cb);

    let attempt = client.try_withdraw_bond(&identity);
    assert!(
        attempt.is_err(),
        "the panicking hook must revert withdraw_bond"
    );    let bond = client.get_identity_state(&identity);
    assert!(
        bond.active,
        "the reverted withdrawal must not deactivate the bond"
    );
    assert_eq!(bond.bonded_amount, amount);

    client.clear_callback(&admin);

    let withdrawn = client.withdraw_bond(&identity);
    assert_eq!(withdrawn, amount, "the full bonded amount is withdrawable");
    assert!(!client.get_identity_state(&identity).active);
}

/// `callback_set(old, new)` then `callback_cleared(old)` are published with the
/// addresses involved.
#[test]
fn callback_events_emitted() {
    let (env, client, admin, _contract_id) = setup();
    let cb = env.register(PanickingCallback, ());

    // Each top-level invocation starts a fresh event log in the test host, so
    // the assertions are taken directly after the call that emits them.
    client.set_callback(&admin, &cb);
    assert_eq!(
        callback_set_payloads(&env),
        std::vec![(None, Some(cb.clone()))],
        "callback_set must carry (old = None, new = Some(cb))"
    );

    client.clear_callback(&admin);
    assert_eq!(
        callback_cleared_payloads(&env),
        std::vec![Some(cb)],
        "callback_cleared must carry the removed callback"
    );
}

/// A second `set_callback` reports the previously installed address as `old`,
/// so indexers can follow hook changes without replaying storage.
#[test]
fn callback_set_reports_previous_callback() {
    let (env, client, admin, _contract_id) = setup();
    let first = env.register(PanickingCallback, ());
    let second = env.register(PanickingCallback, ());

    client.set_callback(&admin, &first);
    assert_eq!(
        callback_set_payloads(&env),
        std::vec![(None, Some(first.clone()))],
        "the first install reports old = None"
    );

    client.set_callback(&admin, &second);
    assert_eq!(
        callback_set_payloads(&env),
        std::vec![(Some(first), Some(second))],
        "a replacement reports the previously installed callback as old"
    );
}

/// `clear_callback` is admin-gated too — otherwise anyone could silently disable
/// the hook an admin installed.
#[test]
fn clear_callback_requires_admin() {
    let (env, client, admin, contract_id) = setup();
    let cb = env.register(PanickingCallback, ());
    client.set_callback(&admin, &cb);

    let mallory = Address::generate(&env);
    let err = client.try_clear_callback(&mallory).unwrap_err();

    assert_contract_error(err, ContractError::NotAdmin);
    assert_eq!(
        stored_callback(&env, &contract_id),
        Some(cb),
        "the rejected call must leave the hook in place"
    );
}
