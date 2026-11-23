//! Tests for the grace-window read view and admin-gated setter (issue #655).
//!
//! Covers: default (unset) returns 0; set-then-read round-trips; the setter
//! emits a `param_updated` event carrying `(old, new)`; and a non-admin setter
//! is rejected with `NotAdmin`.

use crate::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, IntoVal, Symbol};

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    (client, admin)
}

#[test]
fn default_grace_window_is_zero() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    assert_eq!(client.get_grace_window(), 0);
}

#[test]
fn set_then_read_round_trips() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_grace_window(&admin, &300u64);
    assert_eq!(client.get_grace_window(), 300);

    // Overwrite with a different value.
    client.set_grace_window(&admin, &0u64);
    assert_eq!(client.get_grace_window(), 0);
}

#[test]
fn setter_emits_param_updated_with_old_and_new() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_grace_window(&admin, &120u64);

    // The grace-window update is published as a `param_updated` event whose
    // data payload is the (old, new) pair. Old was 0 (unset), new is 120.
    let events = e.events().all();
    let found = events.iter().any(|(_id, topics, data)| {
        let is_param = topics
            .get(0)
            .map(|t| t == Symbol::new(&e, "param_updated").into_val(&e))
            .unwrap_or(false);
        let is_grace = topics
            .get(1)
            .map(|t| t == Symbol::new(&e, "grace_window").into_val(&e))
            .unwrap_or(false);
        is_param && is_grace && data == (0i128, 120i128).into_val(&e)
    });
    assert!(found, "expected param_updated(grace_window) with (0, 120)");
}

#[test]
fn setter_event_carries_previous_value() {
    let e = Env::default();
    let (client, admin) = setup(&e);

    client.set_grace_window(&admin, &50u64);
    client.set_grace_window(&admin, &75u64);

    // The most recent event must carry old = 50 (the prior stored value), new = 75.
    let events = e.events().all();
    let last = events.last().expect("an event was emitted");
    let (_id, _topics, data) = last;
    assert_eq!(data, (50i128, 75i128).into_val(&e));
}

#[test]
fn non_admin_setter_is_rejected() {
    let e = Env::default();
    let (client, _admin) = setup(&e);
    let intruder = Address::generate(&e);

    // mock_all_auths authorizes the require_auth, but the stored-admin equality
    // check must still reject a non-admin caller with NotAdmin.
    let res = client.try_set_grace_window(&intruder, &100u64);
    assert_eq!(
        res,
        Err(Ok(soroban_sdk::Error::from_contract_error(
            credence_errors::ContractError::NotAdmin as u32
        )))
    );

    // State unchanged.
    assert_eq!(client.get_grace_window(), 0);
}
