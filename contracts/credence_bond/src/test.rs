use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Env;

#[test]
fn test_create_bond() {
    let e = Env::default();
    e.mock_all_auths();
    let contract_id = e.register(CredenceBond, ());
    let client = CredenceBondClient::new(&e, &contract_id);

    let admin = Address::generate(&e);
    client.initialize(&admin);

    let identity = Address::generate(&e);
    let bond = client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    assert!(bond.active);
    assert_eq!(bond.bonded_amount, 1000_i128);
    assert_eq!(bond.slashed_amount, 0);
    assert_eq!(bond.identity, identity);
}
