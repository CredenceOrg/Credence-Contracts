use crate::test_helpers;
use soroban_sdk::Env;

#[test]
fn test_create_bond() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(&e);

    let bond = client.create_bond(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    assert!(bond.active);
    assert_eq!(bond.bonded_amount, 1000_i128);
    assert_eq!(bond.slashed_amount, 0);
    assert_eq!(bond.identity, identity);
}
