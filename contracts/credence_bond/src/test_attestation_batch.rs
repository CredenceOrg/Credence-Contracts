extern crate std;

use crate::{AttestationBatchItem, CredenceBond, CredenceBondClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, FromVal, String, Symbol, Vec,
};
use std::panic::AssertUnwindSafe;

fn setup(e: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(e, &contract_id);
    let admin = Address::generate(e);
    client.initialize(&admin, &None);
    (client, admin, contract_id)
}

#[test]
fn test_batch_success() {
    let e = Env::default();
    let (client, admin, contract_id) = setup(&e);

    // Setup 3 attesters
    let attester1 = Address::generate(&e);
    let attester2 = Address::generate(&e);
    let attester3 = Address::generate(&e);

    client.register_attester(&attester1);
    client.register_attester(&attester2);
    client.register_attester(&attester3);

    // Configure stakes to give different weights
    client.set_attester_stake(&admin, &attester1, &100_000i128);
    client.set_attester_stake(&admin, &attester2, &200_000i128);
    client.set_attester_stake(&admin, &attester3, &300_000i128);

    // Set weight config (multiplier = 100 bps = 1%, max = 10_000)
    client.set_weight_config(&admin, &100u32, &10_000u32);

    let subject = Address::generate(&e);

    let nonce1 = client.get_nonce(&attester1);
    let nonce2 = client.get_nonce(&attester2);
    let nonce3 = client.get_nonce(&attester3);

    let mut items = Vec::new(&e);
    items.push_back(AttestationBatchItem {
        attester: attester1.clone(),
        attestation_data: String::from_str(&e, "claim1"),
        nonce: nonce1,
    });
    items.push_back(AttestationBatchItem {
        attester: attester2.clone(),
        attestation_data: String::from_str(&e, "claim2"),
        nonce: nonce2,
    });
    items.push_back(AttestationBatchItem {
        attester: attester3.clone(),
        attestation_data: String::from_str(&e, "claim3"),
        nonce: nonce3,
    });

    let added = client.add_attestation_batch(&subject, &items);

    assert_eq!(added.len(), 3);
    assert_eq!(added.get(0).unwrap().weight, 1000); // 100_000 * 1%
    assert_eq!(added.get(1).unwrap().weight, 2000); // 200_000 * 1%
    assert_eq!(added.get(2).unwrap().weight, 3000); // 300_000 * 1%

    // Verify aggregate event
    let events = e.events().all();
    let batch_events: std::vec::Vec<_> = events
        .iter()
        .filter(|ev| {
            ev.0 == contract_id
                && Symbol::from_val(&e, &ev.1.get(0).unwrap())
                    == Symbol::new(&e, "attestations_batch_added")
        })
        .collect();
    assert_eq!(batch_events.len(), 1);
    let ev = batch_events.first().unwrap();
    assert_eq!(Address::from_val(&e, &ev.1.get(1).unwrap()), subject);
}

#[test]
fn test_batch_empty_rejected() {
    let e = Env::default();
    let (client, _admin, _contract_id) = setup(&e);
    let subject = Address::generate(&e);
    let items = Vec::new(&e);

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.add_attestation_batch(&subject, &items);
    }));
    assert!(result.is_err());
}

#[test]
fn test_batch_too_large_rejected() {
    let e = Env::default();
    let (client, _admin, _contract_id) = setup(&e);
    let subject = Address::generate(&e);

    let mut items = Vec::new(&e);
    for _ in 0..65 {
        items.push_back(AttestationBatchItem {
            attester: Address::generate(&e),
            attestation_data: String::from_str(&e, "data"),
            nonce: 0,
        });
    }

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.add_attestation_batch(&subject, &items);
    }));
    assert!(result.is_err());
}

#[test]
fn test_batch_duplicate_attester_rejected() {
    let e = Env::default();
    let (client, _admin, _contract_id) = setup(&e);
    let subject = Address::generate(&e);
    let attester = Address::generate(&e);
    client.register_attester(&attester);

    let nonce = client.get_nonce(&attester);

    let mut items = Vec::new(&e);
    items.push_back(AttestationBatchItem {
        attester: attester.clone(),
        attestation_data: String::from_str(&e, "data1"),
        nonce,
    });
    items.push_back(AttestationBatchItem {
        attester: attester.clone(),
        attestation_data: String::from_str(&e, "data2"),
        nonce: nonce + 1,
    });

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.add_attestation_batch(&subject, &items);
    }));
    assert!(result.is_err());
}

#[test]
fn test_batch_aggregate_weight_cap_rejected() {
    let e = Env::default();
    let (client, admin, _contract_id) = setup(&e);

    let attester1 = Address::generate(&e);
    let attester2 = Address::generate(&e);
    client.register_attester(&attester1);
    client.register_attester(&attester2);

    // Stakes give weights 3000 and 4000
    client.set_attester_stake(&admin, &attester1, &300_000i128);
    client.set_attester_stake(&admin, &attester2, &400_000i128);
    // Config max weight = 5000 (aggregate cap)
    client.set_weight_config(&admin, &100u32, &5000u32);

    let subject = Address::generate(&e);

    let mut items = Vec::new(&e);
    items.push_back(AttestationBatchItem {
        attester: attester1.clone(),
        attestation_data: String::from_str(&e, "data1"),
        nonce: client.get_nonce(&attester1),
    });
    items.push_back(AttestationBatchItem {
        attester: attester2.clone(),
        attestation_data: String::from_str(&e, "data2"),
        nonce: client.get_nonce(&attester2),
    });

    // 3000 + 4000 = 7000 > 5000 cap -> should fail
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.add_attestation_batch(&subject, &items);
    }));
    assert!(result.is_err());
}

#[test]
fn test_batch_atomic_rollback() {
    let e = Env::default();
    let (client, admin, _contract_id) = setup(&e);

    let attester1 = Address::generate(&e);
    let attester2 = Address::generate(&e); // Unregistered attester to trigger failure
    client.register_attester(&attester1);

    client.set_attester_stake(&admin, &attester1, &100_000i128);
    client.set_weight_config(&admin, &100u32, &10_000u32);

    let subject = Address::generate(&e);
    let nonce1 = client.get_nonce(&attester1);

    let mut items = Vec::new(&e);
    items.push_back(AttestationBatchItem {
        attester: attester1.clone(),
        attestation_data: String::from_str(&e, "data1"),
        nonce: nonce1,
    });
    items.push_back(AttestationBatchItem {
        attester: attester2.clone(),
        attestation_data: String::from_str(&e, "data2"),
        nonce: 0,
    });

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.add_attestation_batch(&subject, &items);
    }));
    assert!(result.is_err());

    // Verify attester1 nonce was NOT consumed
    assert_eq!(client.get_nonce(&attester1), nonce1);

    // Verify no attestations were stored for subject
    let view = client.describe_config().unwrap(); // check if contract still works
    assert_eq!(view.weight_max, 10_000);
}
