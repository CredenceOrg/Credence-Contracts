use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Env, String};

fn setup_contract(e: &Env) -> (CredenceArbitrationClient<'_>, Address, Address) {
    e.mock_all_auths();
    let admin = Address::generate(e);
    let arb = Address::generate(e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(e, &contract_id);
    client.initialize(&admin);
    client.register_arbitrator(&arb, &10);
    (client, admin, arb)
}

/// Before any dispute is created the counter must be 0.
#[test]
fn dispute_count_zero_at_init() {
    let e = Env::default();
    let (client, _admin, _arb) = setup_contract(&e);
    assert_eq!(client.get_dispute_count(), 0);
}

/// After creating one dispute the counter must be 1.
#[test]
fn dispute_count_one_after_create() {
    let e = Env::default();
    let (client, _admin, _arb) = setup_contract(&e);

    let creator = Address::generate(&e);
    let desc = String::from_str(&e, "first dispute");
    client.create_dispute(&creator, &desc, &3600);

    assert_eq!(client.get_dispute_count(), 1);
}

/// Counter reflects total *created* disputes, not active ones.
/// Creating two disputes and cancelling one must still return 2.
#[test]
fn dispute_count_reflects_total_created_not_active() {
    let e = Env::default();
    let (client, _admin, _arb) = setup_contract(&e);

    let creator1 = Address::generate(&e);
    let creator2 = Address::generate(&e);
    let desc = String::from_str(&e, "dispute");

    let id1 = client.create_dispute(&creator1, &desc, &3600);
    let _id2 = client.create_dispute(&creator2, &desc, &3600);

    // Cancel the first dispute.
    client.cancel_dispute(&creator1, &id1);

    // Counter must still be 2 — it tracks total created.
    assert_eq!(client.get_dispute_count(), 2);
}

/// Counter increases monotonically with each create.
#[test]
fn dispute_count_increases_monotonically() {
    let e = Env::default();
    let (client, _admin, _arb) = setup_contract(&e);

    for i in 1u64..=5 {
        let creator = Address::generate(&e);
        let desc = String::from_str(&e, "d");
        client.create_dispute(&creator, &desc, &3600);
        assert_eq!(client.get_dispute_count(), i);
    }
}
