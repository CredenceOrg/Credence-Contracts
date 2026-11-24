//! Event schema regression tests for dispute-resolution lifecycle events.

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env, Symbol, TryFromVal,
};

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(DisputeResolutionContract, ());
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    (env, contract_id, creator, resolver)
}

fn assert_event_schema(
    env: &Env,
    event_index: u32,
    name: &str,
    id: u64,
    expected_data: (Address, Address),
) {
    let events = env.events().all();
    let event = events.get(event_index).expect("event exists");
    let topics = event.1;
    assert_eq!(topics.len(), 2, "{name} must have two topics");

    let event_name = Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap();
    assert_eq!(event_name, Symbol::new(env, name));
    let event_id = u64::try_from_val(env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(event_id, id);

    let data: (Address, Address) = <(Address, Address)>::try_from_val(env, &event.2).unwrap();
    assert_eq!(data, expected_data);
}

#[test]
fn created_event_schema_is_pinned() {
    let (env, contract_id, creator, resolver) = setup();
    let id = env.as_contract(&contract_id, || {
        DisputeResolutionContract::create_dispute(
            env.clone(),
            creator.clone(),
            resolver.clone(),
        )
    });

    assert_eq!(env.events().all().len(), 1);
    assert_event_schema(
        &env,
        0,
        "created",
        id,
        (creator, resolver),
    );
}

#[test]
fn resolved_event_schema_is_pinned() {
    let (env, contract_id, creator, resolver) = setup();
    let id = env.as_contract(&contract_id, || {
        DisputeResolutionContract::create_dispute(
            env.clone(),
            creator.clone(),
            resolver.clone(),
        )
    });

    env.as_contract(&contract_id, || {
        DisputeResolutionContract::resolve(env.clone(), resolver.clone(), id).unwrap();
    });

    assert_eq!(env.events().all().len(), 2);
    assert_event_schema(
        &env,
        1,
        "resolved",
        id,
        (creator, resolver),
    );
}

#[test]
fn closed_event_schema_is_pinned() {
    let (env, contract_id, creator, resolver) = setup();
    let id = env.as_contract(&contract_id, || {
        DisputeResolutionContract::create_dispute(
            env.clone(),
            creator.clone(),
            resolver.clone(),
        )
    });

    env.as_contract(&contract_id, || {
        DisputeResolutionContract::close(env.clone(), resolver.clone(), id).unwrap();
    });

    assert_eq!(env.events().all().len(), 2);
    assert_event_schema(
        &env,
        1,
        "closed",
        id,
        (creator, resolver),
    );
}

#[test]
fn failed_transitions_emit_no_events() {
    let (env, contract_id, creator, resolver) = setup();
    let id = env.as_contract(&contract_id, || {
        DisputeResolutionContract::create_dispute(
            env.clone(),
            creator,
            resolver.clone(),
        )
    });
    let stranger = Address::generate(&env);

    env.as_contract(&contract_id, || {
        assert_eq!(
            DisputeResolutionContract::resolve(env.clone(), stranger.clone(), id),
            Err(DisputeError::Unauthorized)
        );
        assert_eq!(env.events().all().len(), 1);

        DisputeResolutionContract::resolve(env.clone(), resolver.clone(), id).unwrap();
        assert_eq!(
            DisputeResolutionContract::resolve(env.clone(), resolver.clone(), id),
            Err(DisputeError::AlreadyClosed)
        );
        assert_eq!(
            DisputeResolutionContract::close(env.clone(), stranger, id),
            Err(DisputeError::Unauthorized)
        );
        assert_eq!(env.events().all().len(), 2);

        DisputeResolutionContract::close(env.clone(), resolver.clone(), id).unwrap();
        assert_eq!(
            DisputeResolutionContract::close(env.clone(), resolver.clone(), id),
            Err(DisputeError::AlreadyClosed)
        );
        assert_eq!(env.events().all().len(), 3);
    });
}
