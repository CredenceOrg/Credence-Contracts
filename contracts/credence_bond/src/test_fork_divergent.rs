#![cfg(test)]

use crate::fork_divergent::{CredenceBond, CredenceBondClient};
use crate::BondTier;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use std::panic::AssertUnwindSafe;

fn setup_contract(env: &Env) -> (CredenceBondClient<'_>, Address, Address) {
    let contract_address = env.register(CredenceBond, ());
    let client = CredenceBondClient::new(env, &contract_address);
    let admin = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin);

    (client, contract_address, admin)
}

#[test]
fn test_get_tier_boundaries() {
    let env = Env::default();
    let (client, _, _) = setup_contract(&env);
    let identity = Address::generate(&env);

    // Initial state: No bond exists -> should panic (Recovery)
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.get_tier(&identity);
    }));
    assert!(result.is_err());

    // Create bond with amount 0 (Boundary)
    client.create_bond(&identity, &0_i128, &1000_u64, &false, &0_u64);
    assert_eq!(client.get_tier(&identity), BondTier::Bronze);

    // Top up to 1 (Boundary)
    client.top_up(&identity, &1_i128);
    assert_eq!(client.get_tier(&identity), BondTier::Gold);

    // Top up beyond 1
    client.top_up(&identity, &100_i128);
    assert_eq!(client.get_tier(&identity), BondTier::Gold);

    // Create another identity with a huge amount
    let identity2 = Address::generate(&env);
    client.create_bond(&identity2, &1000000_i128, &1000_u64, &false, &0_u64);
    assert_eq!(client.get_tier(&identity2), BondTier::Gold);
}

#[test]
fn test_slash_recovery_and_bounds() {
    let env = Env::default();
    let (client, _, _) = setup_contract(&env);
    let identity = Address::generate(&env);

    // Slash with no bond (Recovery)
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.slash(&identity, &100_i128);
    }));
    assert!(result.is_err());

    // Create bond with amount 1000
    client.create_bond(&identity, &1000_i128, &1000_u64, &false, &0_u64);

    // Slash 0 (Boundary)
    let bond = client.slash(&identity, &0_i128);
    assert_eq!(bond.slashed_amount, 0);

    // Slash partial amount
    let bond = client.slash(&identity, &500_i128);
    assert_eq!(bond.slashed_amount, 500);

    // Slash exactly remaining amount (Boundary)
    let bond = client.slash(&identity, &500_i128);
    assert_eq!(bond.slashed_amount, 1000);

    // Slash beyond bonded amount (Recovery/Boundary)
    let bond = client.slash(&identity, &500_i128);
    assert_eq!(bond.slashed_amount, 1000, "Slashed amount should be capped at bonded_amount");
}

#[test]
fn test_top_up_recovery() {
    let env = Env::default();
    let (client, _, _) = setup_contract(&env);
    let identity = Address::generate(&env);

    // Top up with no bond (Recovery)
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        client.top_up(&identity, &100_i128);
    }));
    assert!(result.is_err());

    // Create bond
    client.create_bond(&identity, &1000_i128, &1000_u64, &false, &0_u64);

    // Top up 0 (Boundary)
    let bond = client.top_up(&identity, &0_i128);
    assert_eq!(bond.bonded_amount, 1000);

    // Top up negative
    let bond = client.top_up(&identity, &-500_i128);
    assert_eq!(bond.bonded_amount, 500);

    // Top up positive
    let bond = client.top_up(&identity, &2000_i128);
    assert_eq!(bond.bonded_amount, 2500);
}
