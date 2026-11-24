use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Env, String};

#[test]
fn get_disputes_by_creator_pages_in_creation_order() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let creator = Address::generate(&e);
    let other_creator = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    let mut ids = Vec::new(&e);
    for _ in 0..3 {
        let id = client.create_dispute(&creator, &String::from_str(&e, "Test"), &3600);
        ids.push_back(id);
        client.cancel_dispute(&creator, &id, &None);
    }

    let other_id = client.create_dispute(&other_creator, &String::from_str(&e, "Other"), &3600);
    let (page, next) = client.get_disputes_by_creator(&creator, &0, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0), ids.get(0));
    assert_eq!(page.get(1), ids.get(1));
    assert_eq!(next, Some(2));

    let (last_page, next) = client.get_disputes_by_creator(&creator, &2, &2);
    assert_eq!(last_page.len(), 1);
    assert_eq!(last_page.get(0), ids.get(2));
    assert_eq!(next, None);

    let (other_page, next) = client.get_disputes_by_creator(&other_creator, &0, &10);
    assert_eq!(other_page.len(), 1);
    assert_eq!(other_page.get(0), Some(other_id));
    assert_eq!(next, None);
}

#[test]
fn get_disputes_by_creator_rejects_empty_and_out_of_range_cursors() {
    let e = Env::default();
    e.mock_all_auths();

    let admin = Address::generate(&e);
    let creator = Address::generate(&e);
    let contract_id = e.register(CredenceArbitration, ());
    let client = CredenceArbitrationClient::new(&e, &contract_id);
    client.initialize(&admin);

    let empty_err = client
        .try_get_disputes_by_creator(&creator, &0, &10)
        .unwrap_err()
        .unwrap();
    assert_eq!(empty_err, ArbitrationError::CursorOutOfRange);

    let id = client.create_dispute(&creator, &String::from_str(&e, "Test"), &3600);
    client.cancel_dispute(&creator, &id, &None);
    let end_err = client
        .try_get_disputes_by_creator(&creator, &1, &10)
        .unwrap_err()
        .unwrap();
    assert_eq!(end_err, ArbitrationError::CursorOutOfRange);
}
