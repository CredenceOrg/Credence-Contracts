use crate::test_helpers;
use soroban_sdk::Env;

#[test]
fn test_create_bond() {
    let e = Env::default();
    let (client, _admin, identity, _token_id, _bond_id) = test_helpers::setup_with_token(&e);

    let bond = client.create_bond_with_rolling(&identity, &1000_i128, &86400_u64, &false, &0_u64);

    assert!(bond.active);
    assert_eq!(bond.bonded_amount, 1000_i128);
    assert_eq!(bond.slashed_amount, 0);
    assert_eq!(bond.identity, identity);
}
#[cfg(test)]
mod test_admin_transfer {
    use crate::CredenceBond;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_propose_and_accept_admin() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, CredenceBond);
        let client = crate::CredenceBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let new_admin = Address::generate(&e);

        client.initialize(&admin, &None);

        // Propose new admin
        client.propose_admin(&admin, &new_admin);

        // Fast-forward ledger past timelock
        e.ledger().with_mut(|l| {
            l.timestamp = l.timestamp + 86_401;
        });

        // Accept as new admin
        client.accept_admin(&new_admin);
    }

    #[test]
    #[should_panic]
    fn test_only_admin_can_propose() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, CredenceBond);
        let client = crate::CredenceBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let rogue = Address::generate(&e);
        let new_admin = Address::generate(&e);

        client.initialize(&admin, &None);
        client.propose_admin(&rogue, &new_admin); // should panic
    }

    #[test]
    #[should_panic]
    fn test_only_pending_admin_can_accept() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, CredenceBond);
        let client = crate::CredenceBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let new_admin = Address::generate(&e);
        let rogue = Address::generate(&e);

        client.initialize(&admin, &None);
        client.propose_admin(&admin, &new_admin);

        e.ledger().with_mut(|l| {
            l.timestamp = l.timestamp + 86_401;
        });

        client.accept_admin(&rogue); // should panic
    }

    #[test]
    #[should_panic]
    fn test_cannot_accept_before_timelock() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, CredenceBond);
        let client = crate::CredenceBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        let new_admin = Address::generate(&e);

        client.initialize(&admin, &None);
        client.propose_admin(&admin, &new_admin);

        // Do NOT advance time past timelock
        client.accept_admin(&new_admin); // should panic
    }

    #[test]
    #[should_panic]
    fn test_cannot_propose_same_admin() {
        let e = Env::default();
        e.mock_all_auths();
        let contract_id = e.register_contract(None, CredenceBond);
        let client = crate::CredenceBondClient::new(&e, &contract_id);

        let admin = Address::generate(&e);
        client.initialize(&admin, &None);
        client.propose_admin(&admin, &admin); // should panic
    }
}
