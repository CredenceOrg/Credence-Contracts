#![allow(clippy::disallowed_macros)]
#![cfg(test)]

use dispute_resolution::{
    DataKey, DisputeError, DisputeResolutionContract, DisputeStatus, STORAGE_TTL_THRESHOLD,
};
use soroban_sdk::{
    symbol_short,
    testutils::{
        storage::{Instance as InstanceTestutils, Persistent as PersistentTestutils},
        Address as _, Events,
    },
    vec, Address, Env, IntoVal, TryFromVal,
};

fn setup() -> (
    Env,
    dispute_resolution::DisputeResolutionContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(DisputeResolutionContract, ());
    let client = dispute_resolution::DisputeResolutionContractClient::new(&env, &contract_id);
    (env, client)
}

#[test]
fn test_create_dispute_returns_id_with_open_status() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.id, id);
    assert_eq!(dispute.creator, creator);
    assert_eq!(dispute.status, DisputeStatus::Open);
    assert_eq!(dispute.resolver, resolver);
}

#[test]
fn test_get_dispute_not_found_returns_error() {
    let (_, client) = setup();
    let err = client.try_get_dispute(&999_u64).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::DisputeNotFound);
}

#[test]
fn test_get_dispute_found_after_create() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);
    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
}

#[test]
fn test_close_transitions_open_to_closed() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
    assert_eq!(dispute.resolver, resolver);
}

#[test]
fn test_double_close_returns_already_closed() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();
    let err = client.try_close(&resolver, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::AlreadyClosed);

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
}

#[test]
fn test_close_by_non_resolver_returns_unauthorized() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let attacker = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    let err = client.try_close(&attacker, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
}

#[test]
fn test_close_nonexistent_dispute_returns_not_found() {
    let (env, client) = setup();
    let resolver = Address::generate(&env);
    let err = client.try_close(&resolver, &42_u64).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::DisputeNotFound);
}

#[test]
fn test_resolve_transitions_open_to_resolved() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);
    assert_eq!(dispute.resolver, resolver);
}

#[test]
fn test_resolve_already_resolved_returns_already_closed() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();
    let err = client.try_resolve(&resolver, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::AlreadyClosed);

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);
}

#[test]
fn test_resolve_closed_dispute_returns_already_closed() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();
    let err = client.try_resolve(&resolver, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::AlreadyClosed);

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
}

#[test]
fn test_resolve_by_non_resolver_returns_unauthorized() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let attacker = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    let err = client.try_resolve(&attacker, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
}

#[test]
fn test_resolve_nonexistent_dispute_returns_not_found() {
    let (env, client) = setup();
    let resolver = Address::generate(&env);
    let err = client
        .try_resolve(&resolver, &999_u64)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, DisputeError::DisputeNotFound);
}

#[test]
fn test_close_resolved_dispute_succeeds() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();
    client.try_close(&resolver, &id).unwrap().unwrap();

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
}

#[test]
fn test_multiple_independent_disputes() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver_a = Address::generate(&env);
    let resolver_b = Address::generate(&env);

    let id_a = client.create_dispute(&creator, &resolver_a);
    let id_b = client.create_dispute(&creator, &resolver_b);

    client.try_close(&resolver_a, &id_a).unwrap().unwrap();

    let dispute_b = client.try_get_dispute(&id_b).unwrap().unwrap();
    assert_eq!(dispute_b.status, DisputeStatus::Open);
    assert_eq!(dispute_b.resolver, resolver_b);

    let dispute_a = client.try_get_dispute(&id_a).unwrap().unwrap();
    assert_eq!(dispute_a.status, DisputeStatus::Closed);
    assert_eq!(dispute_a.resolver, resolver_a);
}

#[test]
fn test_close_records_require_auth_for_resolver() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| addr == &resolver),
        "require_auth was not recorded for resolver during close"
    );
}

#[test]
fn test_resolve_records_require_auth_for_resolver() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| addr == &resolver),
        "require_auth was not recorded for resolver during resolve"
    );
}

#[test]
fn test_error_discriminants_match_spec() {
    assert_eq!(DisputeError::DisputeNotFound as u32, 1);
    assert_eq!(DisputeError::AlreadyClosed as u32, 2);
    assert_eq!(DisputeError::Unauthorized as u32, 3);
}

#[test]
fn test_boundary_ids_are_valid_storage_keys() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);

    let err_min = client.try_get_dispute(&u64::MIN).unwrap_err().unwrap();
    assert_eq!(err_min, DisputeError::DisputeNotFound);

    let err_max = client.try_get_dispute(&u64::MAX).unwrap_err().unwrap();
    assert_eq!(err_max, DisputeError::DisputeNotFound);

    let _id = client.create_dispute(&creator, &resolver);
}

#[test]
fn test_resolver_a_cannot_close_resolver_b_dispute() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver_a = Address::generate(&env);
    let resolver_b = Address::generate(&env);

    let id_b = client.create_dispute(&creator, &resolver_b);

    let err = client.try_close(&resolver_a, &id_b).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);

    let dispute = client.try_get_dispute(&id_b).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Open);
}

#[test]
fn test_terminal_state_never_reverts() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let attacker = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    let _ = client.try_close(&resolver, &id);
    let _ = client.try_close(&attacker, &id);

    let dispute = client.try_get_dispute(&id).unwrap().unwrap();
    assert_eq!(dispute.status, DisputeStatus::Closed);
    assert_eq!(dispute.resolver, resolver);
}

#[test]
fn test_resolver_field_preserved_through_lifecycle() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().resolver,
        resolver
    );

    client.try_resolve(&resolver, &id).unwrap().unwrap();
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().resolver,
        resolver
    );

    client.try_close(&resolver, &id).unwrap().unwrap();
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().resolver,
        resolver
    );
}

#[test]
fn test_create_dispute_emits_created_event() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    let events = env.events().all();
    let found = events.iter().any(|(_, topics, data)| {
        let expected_topics = vec![
            &env,
            symbol_short!("created").into_val(&env),
            id.into_val(&env),
        ];
        if topics != expected_topics {
            return false;
        }
        let decoded = <(Address, Address)>::try_from_val(&env, &data);
        decoded == Ok((creator.clone(), resolver.clone()))
    });
    assert!(found, "expected a 'created' event for dispute id {id}");
}

#[test]
fn test_close_emits_closed_event() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_close(&resolver, &id).unwrap().unwrap();

    let events = env.events().all();
    let found = events.iter().any(|(_, topics, _data)| {
        let expected = vec![
            &env,
            symbol_short!("closed").into_val(&env),
            id.into_val(&env),
        ];
        topics == expected
    });
    assert!(found, "expected a 'closed' event for dispute id {id}");
}

#[test]
fn test_resolve_emits_resolved_event() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.try_resolve(&resolver, &id).unwrap().unwrap();

    let events = env.events().all();
    let found = events.iter().any(|(_, topics, _data)| {
        let expected = vec![
            &env,
            symbol_short!("resolved").into_val(&env),
            id.into_val(&env),
        ];
        topics == expected
    });
    assert!(found, "expected a 'resolved' event for dispute id {id}");
}

#[test]
fn test_get_dispute_does_not_mutate_state() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    let d1 = client.try_get_dispute(&id).unwrap().unwrap();
    let d2 = client.try_get_dispute(&id).unwrap().unwrap();

    assert_eq!(d1, d2);
    assert_eq!(d1.status, DisputeStatus::Open);
}

#[test]
fn test_full_lifecycle_open_resolved_closed() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Open
    );

    client.try_resolve(&resolver, &id).unwrap().unwrap();
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Resolved
    );

    client.try_close(&resolver, &id).unwrap().unwrap();
    assert_eq!(
        client.try_get_dispute(&id).unwrap().unwrap().status,
        DisputeStatus::Closed
    );

    assert_eq!(
        client.try_close(&resolver, &id).unwrap_err().unwrap(),
        DisputeError::AlreadyClosed
    );
    assert_eq!(
        client.try_resolve(&resolver, &id).unwrap_err().unwrap(),
        DisputeError::AlreadyClosed
    );
}

#[test]
fn test_failed_calls_leave_state_unchanged() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let attacker = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    let before = client.try_get_dispute(&id).unwrap().unwrap();

    let _ = client.try_close(&attacker, &id);
    let _ = client.try_resolve(&attacker, &id);

    let after = client.try_get_dispute(&id).unwrap().unwrap();

    assert_eq!(
        before, after,
        "storage must be unchanged after failed calls"
    );
}

#[test]
fn create_requires_creator_auth() {
    let env = Env::default();
    let contract_id = env.register(DisputeResolutionContract, ());
    let client = dispute_resolution::DisputeResolutionContractClient::new(&env, &contract_id);
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);

    assert!(client.try_create_dispute(&creator, &resolver).is_err());

    env.mock_all_auths();
    assert_eq!(
        client.try_get_dispute(&0_u64).unwrap_err().unwrap(),
        DisputeError::DisputeNotFound
    );
}

#[test]
fn ids_are_sequential() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id0 = client.create_dispute(&creator, &resolver);
    let id1 = client.create_dispute(&creator, &resolver);
    let id2 = client.create_dispute(&creator, &resolver);

    assert_eq!((id0, id1, id2), (0, 1, 2));
    assert_eq!(client.get_dispute(&1).creator, creator);
}

#[test]
fn records_in_persistent_storage() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    env.as_contract(&client.address, || {
        assert!(env.storage().persistent().has(&DataKey::Dispute(id)));
    });
}

#[test]
fn ttl_extended_on_access() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.resolve(&resolver, &id);

    let key = DataKey::Dispute(id);
    let (persistent_ttl, instance_ttl) = env.as_contract(&client.address, || {
        (
            PersistentTestutils::get_ttl(&env.storage().persistent(), &key),
            InstanceTestutils::get_ttl(&env.storage().instance()),
        )
    });

    assert!(persistent_ttl >= STORAGE_TTL_THRESHOLD);
    assert!(instance_ttl >= STORAGE_TTL_THRESHOLD);
}

#[test]
fn non_resolver_gets_unauthorized_on_closed() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);
    let stranger = Address::generate(&env);
    let id = client.create_dispute(&creator, &resolver);

    client.close(&resolver, &id);

    let err = client.try_close(&stranger, &id).unwrap_err().unwrap();
    assert_eq!(err, DisputeError::Unauthorized);

    let resolve_err = client.try_resolve(&stranger, &id).unwrap_err().unwrap();
    assert_eq!(resolve_err, DisputeError::Unauthorized);
}

#[test]
fn instance_cost_independent_of_dispute_count() {
    let (env, client) = setup();
    let creator = Address::generate(&env);
    let resolver = Address::generate(&env);

    client.create_dispute(&creator, &resolver);
    let _ = client.get_dispute(&0);
    let r_after_1 = env.cost_estimate().resources();

    for _ in 1..200 {
        client.create_dispute(&creator, &resolver);
    }

    let _ = client.get_dispute(&0);
    let r_after_200 = env.cost_estimate().resources();

    assert_eq!(r_after_1.read_entries, r_after_200.read_entries);
    assert_eq!(r_after_1.read_bytes, r_after_200.read_bytes);

    let _ = client.get_dispute(&199);
    let cost_id_199 = env.cost_estimate().resources().instructions;
    let diff = cost_id_199.abs_diff(r_after_200.instructions);
    assert!(
        diff * 20 <= r_after_200.instructions as u64,
        "cost_id_0={}, cost_id_199={cost_id_199}",
        r_after_200.instructions
    );
}
