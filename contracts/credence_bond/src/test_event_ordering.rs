#![cfg(test)]

use crate::{test_helpers, CredenceBond, CredenceBondClient};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, FromVal, Symbol,
};
use proptest::prelude::*;
use crate::batch::BatchBondParams;

fn setup() -> (Env, CredenceBondClient<'static>, Address, Address, Address) {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    let identity = Address::generate(&e);

    client.initialize(&admin, &None);

    let token_addr = e.register(test_helpers::MockStellarAsset, ());
    let token_admin_client = StellarAssetClient::new(&e, &token_addr);
    token_admin_client.mint(&identity, &100_000_i128);
    let token_client = TokenClient::new(&e, &token_addr);
    token_client.approve(&identity, &contract_id, &100_000_i128, &99999_u32);
    client.set_token(&admin, &token_addr);

    (e, client, admin, identity, contract_id)
}

fn event_name(e: &Env, event: &soroban_sdk::ContractEvent) -> Symbol {
    Symbol::from_val(e, &event.1.get(0).unwrap())
}

#[test]
fn create_bond_emits_events_in_order() {
    let (e, client, _admin, identity, contract_id) = setup();

    client.create_bond_with_rolling(&identity, &10_000_i128, &86400_u64, &false, &0_u64);

    let events = e.events().all();

    let our_events: Vec<_> = events
        .iter()
        .filter(|ev| ev.0 == contract_id)
        .collect();

    assert_eq!(our_events.len(), 2, "expected 2 events from create_bond");

    let first = &our_events[0];
    let second = &our_events[1];

    assert_eq!(event_name(&e, first), Symbol::new(&e, "bond_created"));
    assert_eq!(event_name(&e, second), Symbol::new(&e, "bond_created_v2"));
}

#[test]
fn withdraw_emits_events_in_order() {
    let (e, client, _admin, identity, _contract_id) = setup();

    client.create_bond_with_rolling(&identity, &10_000_i128, &86400_u64, &false, &0_u64);

    let mut ledger_info = e.ledger().get();
    ledger_info.timestamp += 86401;
    e.ledger().set(ledger_info);

    client.withdraw(&identity, &3_000_i128);

    let events = e.events().all();

    let contract_id = events.iter().find(|ev| ev.0 == e.current_contract()).unwrap().0;
    // Re-collect by contract_id
    let bond_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            let name = Symbol::from_val(&e, &ev.1.get(0).unwrap());
            name == Symbol::new(&e, "bond_withdrawn") || name == Symbol::new(&e, "bond_withdrawn_v2")
        })
        .collect();

    assert_eq!(bond_events.len(), 2, "expected 2 withdraw events");

    assert_eq!(
        event_name(&e, &bond_events[0]),
        Symbol::new(&e, "bond_withdrawn")
    );
    assert_eq!(
        event_name(&e, &bond_events[1]),
        Symbol::new(&e, "bond_withdrawn_v2")
    );
}

#[test]
fn top_up_emits_events_in_order() {
    let (e, client, _admin, identity, _contract_id) = setup();

    client.create_bond_with_rolling(&identity, &10_000_i128, &86400_u64, &false, &0_u64);

    client.top_up(&identity, &5_000_i128);

    let events = e.events().all();

    let increase_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            let name = Symbol::from_val(&e, &ev.1.get(0).unwrap());
            name == Symbol::new(&e, "bond_increased") || name == Symbol::new(&e, "bond_increased_v2")
        })
        .collect();

    assert_eq!(increase_events.len(), 2, "expected 2 increase events");

    assert_eq!(
        event_name(&e, &increase_events[0]),
        Symbol::new(&e, "bond_increased")
    );
    assert_eq!(
        event_name(&e, &increase_events[1]),
        Symbol::new(&e, "bond_increased_v2")
    );
}

#[test]
fn multi_event_tx_ordering_create_bond() {
    let (e, client, _admin, identity, contract_id) = setup();

    client.create_bond_with_rolling(&identity, &10_000_i128, &86400_u64, &false, &0_u64);

    let events = e.events().all();

    let contract_events: Vec<_> = events
        .iter()
        .filter(|ev| ev.0 == contract_id)
        .collect();

    let expected_order = [
        Symbol::new(&e, "bond_created"),
        Symbol::new(&e, "bond_created_v2"),
    ];

    for (i, expected_name) in expected_order.iter().enumerate() {
        let actual_name = event_name(&e, &contract_events[i]);
        assert_eq!(
            actual_name, *expected_name,
            "event at position {} should be {:?} but got {:?}",
            i, expected_name, actual_name
        );
    }
}

#[test]
fn create_bond_no_tier_events_when_tier_unchanged() {
    let (e, client, _admin, identity, contract_id) = setup();

    client.create_bond_with_rolling(&identity, &10_000_i128, &86400_u64, &false, &0_u64);

    let events = e.events().all();

    let tier_events: Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && (event_name(&e, ev) == Symbol::new(&e, "tier_changed")
                    || event_name(&e, ev) == Symbol::new(&e, "tier_changed_v2"))
        })
        .collect();

    assert!(
        tier_events.is_empty(),
        "should not emit tier events when tier does not change"
    );
}

#[test]
fn sad_path_no_events_on_panic() {
    let e = Env::default();
    e.mock_all_auths();

    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let identity = Address::generate(&e);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_bond_with_rolling(&identity, &10_000_i128, &86400_u64, &false, &0_u64);
    }));
    assert!(result.is_err(), "create_bond without token should fail");

    let events = e.events().all();

    let our_events: Vec<_> = events.iter().filter(|ev| ev.0 == contract_id).collect();
    assert!(
        our_events.is_empty(),
        "no events should be emitted on a panicking transaction"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_multi_event_tx_ordering_matches_emission(batch_size in 1..=10u32) {
        let (e, client, _admin, _identity, contract_id) = setup();

        let mut params_list = soroban_sdk::Vec::new(&e);
        let mut expected_order = std::vec::Vec::new();

        for _ in 0..batch_size {
            let identity = Address::generate(&e);
            
            // Large amount to guarantee a tier change from Bronze -> something higher
            let amount = 10_000_000_000_000_000_000_000_i128; 
            params_list.push_back(BatchBondParams {
                identity,
                amount,
                duration: 86400,
                is_rolling: false,
                notice_period_duration: 0,
            });
            
            // Expected events per bond:
            expected_order.push(Symbol::new(&e, "tier_changed"));
            expected_order.push(Symbol::new(&e, "tier_changed_v2"));
        }
        expected_order.push(Symbol::new(&e, "batch_bonds_created"));
        
        client.create_batch_bonds(&params_list);
        
        let events = e.events().all();
        let contract_events: std::vec::Vec<_> = events
            .iter()
            .filter(|ev| ev.0 == contract_id)
            .collect();
            
        assert_eq!(
            contract_events.len(),
            expected_order.len(),
            "event count mismatch"
        );
        
        for (i, expected_name) in expected_order.iter().enumerate() {
            let actual_name = event_name(&e, &contract_events[i]);
            assert_eq!(
                actual_name, *expected_name,
                "event at position {} should be {:?} but got {:?}",
                i, expected_name, actual_name
            );
        }
    }
}

#[test]
fn sad_path_no_events_on_batch_panic() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);
    let admin = Address::generate(&e);
    client.initialize(&admin, &None);

    let mut params_list = soroban_sdk::Vec::new(&e);
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&e),
        amount: 1000,
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });
    params_list.push_back(BatchBondParams {
        identity: Address::generate(&e),
        amount: -100, // Invalid
        duration: 86400,
        is_rolling: false,
        notice_period_duration: 0,
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_batch_bonds(&params_list);
    }));
    assert!(result.is_err());

    let events = e.events().all();
    let our_events: std::vec::Vec<_> = events.iter().filter(|ev| ev.0 == contract_id).collect();
    assert!(
        our_events.is_empty(),
        "no events should be emitted on a panicking transaction"
    );
}
